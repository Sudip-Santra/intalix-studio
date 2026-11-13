//! Report generation: a SQL Editor query, run again in full, rendered with a report template
//! into PDF and/or Excel.
//!
//! Rows stream from PostgreSQL straight into the Excel writer (constant memory) and, up to the
//! PDF limit, into the PDF table. Files are written to a temporary name next to the target
//! and renamed when complete, so a failed or cancelled report never leaves a half-written file
//! under the chosen name.

pub mod fit;
pub mod format;
pub mod pdf;
pub mod template;
pub mod xlsx;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{AppError, AppResult};
use crate::lang::variables::{OptionsSource, VariableDef, VariableType};
use crate::model::{Cell, ResultColumn};
use crate::pg::query::RowSink;
use format::{ColumnKind, Formatted, Sum, column_kind, format_cell, format_date, summable};
use template::Template;
use xlsx::XlsxReport;

/// Most rows a PDF report holds (about 500 pages at normal density).
pub const PDF_MAX_ROWS: usize = 25_000;
/// Most pages a PDF report may have.
pub const PDF_MAX_PAGES: usize = 600;
/// Most rows an Excel report holds (Excel's own limit is 1,048,576 per sheet).
pub const EXCEL_MAX_ROWS: u64 = 1_000_000;
const MAX_TITLE_CHARS: usize = 120;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportRequest {
    pub title: String,
    pub sql: String,
    #[serde(default)]
    pub variables: Vec<VariableDef>,
    #[serde(default)]
    pub values: BTreeMap<String, Value>,
    /// The template definition (normalised by the UI; checked again here).
    pub template: Value,
    pub pdf: bool,
    pub xlsx: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratedFile {
    /// `pdf` or `xlsx`.
    pub format: &'static str,
    pub path: String,
    pub name: String,
    pub size_bytes: u64,
    pub pages: Option<usize>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportSuccess {
    pub ok: bool,
    pub files: Vec<GeneratedFile>,
    pub rows: u64,
    /// Things the user should know, e.g. a format that was skipped.
    pub warnings: Vec<String>,
    pub duration_ms: u64,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum ReportOutcome {
    Ok(ReportSuccess),
    Failed(crate::model::QueryFailure),
}

/// A column as the report shows it.
#[derive(Debug, Clone)]
pub struct ColumnInfo {
    pub label: String,
    pub kind: ColumnKind,
    pub summable: bool,
}

/// Everything both exporters need, apart from the Excel rows that stream past.
#[derive(Debug, Clone)]
pub struct ReportData {
    pub title: String,
    pub organization: String,
    pub date_text: String,
    pub time_text: String,
    pub year: i32,
    pub month: u8,
    pub day: u8,
    /// Filters shown in the header: (label, value).
    pub parameters: Vec<(String, String)>,
    pub columns: Vec<ColumnInfo>,
    pub pdf_rows: Vec<Vec<Formatted>>,
    /// Exact totals for summable columns.
    pub sums: Vec<Option<Sum>>,
    pub totals_enabled: bool,
    pub totals_label: String,
    pub formats: template::Formats,
}

impl ReportData {
    /// Fills `{title}`, `{organization}`, `{date}` and `{time}`; `{page}` / `{pages}` are left
    /// for the layout.
    pub fn fill_tokens(&self, text: &str) -> String {
        text.replace("{title}", &self.title)
            .replace("{organization}", &self.organization)
            .replace("{date}", &self.date_text)
            .replace("{time}", &self.time_text)
    }

    /// The totals row (label first), or `None` when the template has none or nothing sums.
    pub fn totals_row(&self) -> Option<Vec<Formatted>> {
        if !self.totals_enabled || !self.columns.iter().any(|c| c.summable) {
            return None;
        }
        Some(
            self.columns
                .iter()
                .enumerate()
                .map(|(i, c)| {
                    if i == 0 {
                        return Formatted {
                            text: self.totals_label.clone(),
                            negative: false,
                        };
                    }
                    let total = self
                        .sums
                        .get(i)
                        .and_then(Option::as_ref)
                        .and_then(Sum::text);
                    match total {
                        Some(v) => format_cell(&Cell::Decimal { v }, c.kind, &self.formats),
                        None => Formatted {
                            text: String::new(),
                            negative: false,
                        },
                    }
                })
                .collect(),
        )
    }
}

/// `"start_date"` → `"Start date"`
fn label_from_name(name: &str) -> String {
    let words = name.replace('_', " ");
    let words = words.trim();
    let mut chars = words.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => name.to_owned(),
    }
}

/// A heading for a result column: `order_date` → `Order Date`. Names chosen with
/// `AS "Any Text"` (anything not plain lower-case snake_case) are kept exactly.
pub fn column_label(name: &str) -> String {
    let plain = !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    if !plain {
        return name.to_owned();
    }
    name.split('_')
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut chars = w.chars();
            chars
                .next()
                .map(|f| f.to_uppercase().chain(chars).collect::<String>())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The filters shown in the report header, formatted like the report's values.
fn parameters(
    defs: &[VariableDef],
    values: &BTreeMap<String, Value>,
    t: &Template,
) -> Vec<(String, String)> {
    defs.iter()
        .map(|def| {
            let label = if def.label.trim().is_empty() {
                label_from_name(&def.name)
            } else {
                def.label.trim().to_owned()
            };
            if !def.required {
                return (label, "All".to_owned());
            }
            let raw = values.get(&def.name).unwrap_or(&Value::Null);
            let choice = |v: &str| -> String {
                match &def.options {
                    Some(OptionsSource::List { values }) => values
                        .iter()
                        .find(|o| o.value == v && !o.label.is_empty())
                        .map_or_else(|| v.to_owned(), |o| o.label.clone()),
                    _ => v.to_owned(),
                }
            };
            let value = match def.kind {
                VariableType::Date => raw
                    .as_str()
                    .map(|s| format_date(s, t.formats.date))
                    .unwrap_or_default(),
                VariableType::DateRange => {
                    let part = |k: &str| {
                        raw.get(k)
                            .and_then(Value::as_str)
                            .map(|s| format_date(s, t.formats.date))
                            .unwrap_or_default()
                    };
                    format!("{} – {}", part("from"), part("to"))
                }
                VariableType::MultiEnum => {
                    let items: Vec<String> = raw
                        .as_array()
                        .map(|a| a.iter().filter_map(Value::as_str).map(choice).collect())
                        .unwrap_or_default();
                    if items.len() > 4 {
                        format!("{} +{} more", items[..4].join(", "), items.len() - 4)
                    } else {
                        items.join(", ")
                    }
                }
                VariableType::Enum => raw.as_str().map(choice).unwrap_or_default(),
                VariableType::Text | VariableType::Number => raw
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| raw.to_string()),
            };
            (label, value)
        })
        .collect()
}

/// A safe file name from the report title: no reserved characters, at most 100 characters.
///
/// # Examples
/// - `"Sales: Q1/Q2?"` → `"Sales Q1 Q2"`
/// - `"   "` → `"Report"`
pub fn file_stem(title: &str) -> String {
    let cleaned: String = title
        .chars()
        .map(|c| {
            if c.is_control() || "<>:\"/\\|?*".contains(c) {
                ' '
            } else {
                c
            }
        })
        .collect();
    let collapsed = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed: String = collapsed.chars().take(100).collect();
    let trimmed = trimmed.trim_end_matches(['.', ' ']).to_owned();
    let reserved = ["CON", "PRN", "AUX", "NUL", "COM1", "LPT1"];
    if trimmed.is_empty() || reserved.contains(&trimmed.to_uppercase().as_str()) {
        "Report".to_owned()
    } else {
        trimmed
    }
}

/// The rows of a report flowing from PostgreSQL to the exporters.
pub struct ReportSink {
    pub data: ReportData,
    template: Template,
    xlsx: Option<XlsxReport>,
    want_pdf: bool,
    want_xlsx: bool,
    /// More rows than a PDF can hold.
    pub pdf_overflow: bool,
}

impl ReportSink {
    fn new(data: ReportData, template: Template, want_pdf: bool, want_xlsx: bool) -> Self {
        Self {
            data,
            template,
            // Started once the columns are known.
            xlsx: None,
            want_pdf,
            want_xlsx,
            pdf_overflow: false,
        }
    }
}

impl RowSink for ReportSink {
    fn start(&mut self, columns: &[ResultColumn]) -> AppResult<()> {
        self.data.columns = columns
            .iter()
            .map(|c| {
                let kind = column_kind(c);
                ColumnInfo {
                    label: column_label(&c.name),
                    kind,
                    summable: summable(c, kind),
                }
            })
            .collect();
        self.data.sums = self
            .data
            .columns
            .iter()
            .map(|c| c.summable.then(Sum::default))
            .collect();
        if self.want_xlsx {
            self.xlsx = Some(XlsxReport::start(&self.data, &self.template)?);
        }
        Ok(())
    }

    fn row(&mut self, cells: Vec<Cell>) -> AppResult<bool> {
        for (i, cell) in cells.iter().enumerate() {
            if let (Some(Some(sum)), Some(text)) =
                (self.data.sums.get_mut(i), format::decimal_text(cell))
            {
                sum.add(&text);
            }
        }
        if let Some(x) = self.xlsx.as_mut() {
            x.row(&cells)?;
        }
        if self.want_pdf && !self.pdf_overflow {
            if self.data.pdf_rows.len() >= PDF_MAX_ROWS {
                self.pdf_overflow = true;
            } else {
                let row: Vec<Formatted> = cells
                    .iter()
                    .zip(&self.data.columns)
                    .map(|(cell, c)| format_cell(cell, c.kind, &self.template.formats))
                    .collect();
                self.data.pdf_rows.push(row);
            }
        }
        // Without Excel there is no reason to read past what the PDF can hold.
        Ok(self.xlsx.is_some() || !self.pdf_overflow)
    }
}

/// Validated pieces of a request, ready to run.
pub struct Prepared {
    pub title: String,
    pub template: Template,
    pub sink: ReportSink,
    pub max_rows: u64,
}

/// Checks the request and builds the empty report.
pub fn prepare(request: &ReportRequest) -> AppResult<Prepared> {
    build(request, false)
}

/// Builds the empty report for a preview: PDF only, first rows only.
pub fn prepare_preview(request: &ReportRequest) -> AppResult<Prepared> {
    build(request, true)
}

fn build(request: &ReportRequest, preview: bool) -> AppResult<Prepared> {
    if !preview && !request.pdf && !request.xlsx {
        return Err(AppError::invalid("Choose PDF, Excel or both."));
    }
    let title: String = request.title.trim().chars().take(MAX_TITLE_CHARS).collect();
    let title = if title.is_empty() {
        "Report".to_owned()
    } else {
        title
    };
    let template = Template::from_definition(&request.template);
    let now = time::OffsetDateTime::now_local().unwrap_or_else(|_| time::OffsetDateTime::now_utc());
    let iso = format!(
        "{:04}-{:02}-{:02}",
        now.year(),
        u8::from(now.month()),
        now.day()
    );
    let data = ReportData {
        title: title.clone(),
        organization: template.header.organization.clone(),
        date_text: format_date(&iso, template.formats.date),
        time_text: format!("{:02}:{:02}", now.hour(), now.minute()),
        year: now.year(),
        month: u8::from(now.month()),
        day: now.day(),
        parameters: parameters(&request.variables, &request.values, &template),
        columns: Vec::new(),
        pdf_rows: Vec::new(),
        sums: Vec::new(),
        totals_enabled: template.table.totals_row,
        totals_label: template.table.totals_label.clone(),
        formats: template.formats.clone(),
    };
    let max_rows = if preview {
        PREVIEW_ROWS
    } else if request.xlsx {
        EXCEL_MAX_ROWS
    } else {
        // One extra row tells us the PDF would be too long.
        PDF_MAX_ROWS as u64 + 1
    };
    let sink = if preview {
        ReportSink::new(data, template.clone(), true, false)
    } else {
        ReportSink::new(data, template.clone(), request.pdf, request.xlsx)
    };
    Ok(Prepared {
        title,
        template,
        sink,
        max_rows,
    })
}

/// Target paths for the chosen formats, from the path picked in the Save dialog.
pub fn targets(chosen: &Path, pdf: bool, xlsx: bool) -> (Option<PathBuf>, Option<PathBuf>) {
    let stem = chosen.with_extension("");
    (
        pdf.then(|| stem.with_extension("pdf")),
        xlsx.then(|| stem.with_extension("xlsx")),
    )
}

/// `report.pdf` → `report.pdf.part` in the same folder.
pub fn partial_path(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    name.push(".part");
    path.with_file_name(name)
}

/// Moves a finished temporary file over the target, replacing an older report.
pub fn commit(partial: &Path, target: &Path) -> AppResult<()> {
    if target.exists() {
        std::fs::remove_file(target).map_err(|e| {
            AppError::invalid(format!("{} could not be replaced.", display_name(target))).with_hint(
                format!("If it is open in another program, close it and try again. ({e})"),
            )
        })?;
    }
    std::fs::rename(partial, target).map_err(|e| AppError::internal("saving the report", e))
}

pub fn display_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

pub fn generated(format: &'static str, path: &Path, pages: Option<usize>) -> GeneratedFile {
    GeneratedFile {
        format,
        path: path.to_string_lossy().into_owned(),
        name: display_name(path),
        size_bytes: std::fs::metadata(path).map(|m| m.len()).unwrap_or(0),
        pages,
    }
}

/// A written PDF: its page count and whether long text was shortened to fit.
pub struct WrittenPdf {
    pub pages: usize,
    pub shortened: bool,
}

/// Note for a PDF whose long text was shortened.
pub const SHORTENED_NOTE: &str =
    "Some long text was shortened to fit the PDF columns; the Excel file has it in full.";

/// Writes the PDF to a temporary file and moves it into place.
pub fn write_pdf(data: &ReportData, template: &Template, target: &Path) -> AppResult<WrittenPdf> {
    let laid = pdf::layout(data, template)?;
    let pages = laid.document.pages().len();
    if pages > PDF_MAX_PAGES {
        return Err(AppError::invalid(format!(
            "The PDF would have {pages} pages; the limit is {PDF_MAX_PAGES}."
        ))
        .with_hint("Narrow the filters, or use Excel for large reports."));
    }
    let bytes = pdf::pdf_bytes(&laid.document)?;
    let partial = partial_path(target);
    std::fs::write(&partial, bytes).map_err(|e| {
        AppError::invalid(format!("{} could not be saved.", display_name(target)))
            .with_hint(e.to_string())
    })?;
    commit(&partial, target)?;
    Ok(WrittenPdf {
        pages,
        shortened: laid.shortened,
    })
}

/// Rows a preview reads.
pub const PREVIEW_ROWS: u64 = 200;
/// Pages a preview shows.
pub const PREVIEW_PAGES: usize = 4;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewSuccess {
    pub ok: bool,
    /// PNG page images as data URLs.
    pub pages: Vec<String>,
    /// Pages laid out from the rows read (more than shown when long).
    pub page_count: usize,
    pub rows: u64,
    /// The query has more rows than the preview read.
    pub more_rows: bool,
    pub shortened: bool,
    /// Too many columns for the page width; landscape or a larger paper would help.
    pub crowded: bool,
    pub duration_ms: u64,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum PreviewOutcome {
    Ok(PreviewSuccess),
    Failed(crate::model::QueryFailure),
}

/// A drawn preview.
pub struct PreviewPages {
    pub images: Vec<String>,
    pub page_count: usize,
    pub shortened: bool,
    pub crowded: bool,
}

/// Lays out a preview and draws its first pages.
pub fn preview_pages(data: &ReportData, template: &Template) -> AppResult<PreviewPages> {
    use base64::Engine;
    let laid = pdf::layout(data, template)?;
    let images = pdf::page_images(&laid.document, PREVIEW_PAGES)?;
    let urls = images
        .iter()
        .map(|png| {
            format!(
                "data:image/png;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(png)
            )
        })
        .collect();
    Ok(PreviewPages {
        images: urls,
        page_count: laid.document.pages().len(),
        shortened: laid.shortened,
        crowded: laid.crowded,
    })
}

/// Saves the Excel workbook through a temporary file.
pub fn write_xlsx(x: XlsxReport, data: &ReportData, target: &Path) -> AppResult<()> {
    let partial = partial_path(target);
    let result = x.finish(data, &partial);
    if result.is_err() {
        let _ = std::fs::remove_file(&partial);
    }
    result?;
    commit(&partial, target)
}

impl ReportSink {
    pub fn take_xlsx(&mut self) -> Option<XlsxReport> {
        self.xlsx.take()
    }
}

/// Milliseconds since `started`.
pub fn elapsed(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::variables::OptionItem;
    use serde_json::json;

    fn request(pdf: bool, xlsx: bool) -> ReportRequest {
        ReportRequest {
            title: "  Monthly Sales  ".into(),
            sql: "SELECT 1".into(),
            variables: vec![],
            values: BTreeMap::new(),
            template: json!({}),
            pdf,
            xlsx,
        }
    }

    fn col(name: &str, t: &str) -> ResultColumn {
        ResultColumn {
            name: name.into(),
            pg_type: t.into(),
        }
    }

    #[test]
    fn column_labels() {
        assert_eq!(column_label("order_date"), "Order Date");
        assert_eq!(column_label("amount"), "Amount");
        assert_eq!(column_label("Order Date"), "Order Date");
        assert_eq!(column_label("customerName"), "customerName");
        assert_eq!(column_label("count"), "Count");
    }

    #[test]
    fn file_names_are_safe() {
        assert_eq!(file_stem("Sales: Q1/Q2?"), "Sales Q1 Q2");
        assert_eq!(file_stem("   "), "Report");
        assert_eq!(file_stem("con"), "Report");
        assert_eq!(file_stem("Report."), "Report");
        assert_eq!(file_stem(&"x".repeat(300)).len(), 100);
    }

    #[test]
    fn targets_share_the_chosen_name() {
        let (pdf, xlsx) = targets(Path::new(r"C:\r\Sales.pdf"), true, true);
        assert_eq!(pdf.unwrap(), PathBuf::from(r"C:\r\Sales.pdf"));
        assert_eq!(xlsx.unwrap(), PathBuf::from(r"C:\r\Sales.xlsx"));
        assert_eq!(
            partial_path(Path::new(r"C:\r\Sales.pdf")),
            PathBuf::from(r"C:\r\Sales.pdf.part")
        );
    }

    #[test]
    fn needs_a_format() {
        assert!(prepare(&request(false, false)).is_err());
        let p = prepare(&request(true, false)).unwrap();
        assert_eq!(p.title, "Monthly Sales");
        assert_eq!(p.max_rows, PDF_MAX_ROWS as u64 + 1);
        assert_eq!(
            prepare(&request(false, true)).unwrap().max_rows,
            EXCEL_MAX_ROWS
        );
    }

    #[test]
    fn header_parameters() {
        let def = |name: &str, kind: VariableType, required: bool| VariableDef {
            name: name.into(),
            label: String::new(),
            kind,
            required,
            default: Value::Null,
            options: (kind == VariableType::Enum).then(|| OptionsSource::List {
                values: vec![OptionItem {
                    value: "n".into(),
                    label: "North".into(),
                }],
            }),
            help: String::new(),
        };
        let defs = vec![
            def("period", VariableType::DateRange, true),
            def("region", VariableType::Enum, true),
            def("status", VariableType::MultiEnum, true),
            def("customer", VariableType::Text, false),
        ];
        let values: BTreeMap<String, Value> = [
            (
                "period".to_string(),
                json!({"from": "2026-03-01", "to": "2026-03-31"}),
            ),
            ("region".to_string(), json!("n")),
            ("status".to_string(), json!(["a", "b", "c", "d", "e", "f"])),
            ("customer".to_string(), json!("Acme")),
        ]
        .into_iter()
        .collect();
        let t = Template::from_definition(&json!({}));
        assert_eq!(
            parameters(&defs, &values, &t),
            vec![
                (
                    "Period".to_string(),
                    "01 Mar 2026 – 31 Mar 2026".to_string()
                ),
                ("Region".to_string(), "North".to_string()),
                ("Status".to_string(), "a, b, c, d +2 more".to_string()),
                ("Customer".to_string(), "All".to_string()),
            ]
        );
    }

    #[test]
    fn sink_formats_totals_and_limits_pdf_rows() {
        let mut p = prepare(&request(true, false)).unwrap();
        p.sink
            .start(&[
                col("region", "text"),
                col("units", "int8"),
                col("amount", "numeric"),
            ])
            .unwrap();
        for (r, u, a) in [("North", "3", "10.50"), ("South", "-1", "0.25")] {
            let keep = p
                .sink
                .row(vec![
                    Cell::Text {
                        v: r.into(),
                        truncated: false,
                    },
                    Cell::Int { v: u.into() },
                    Cell::Decimal { v: a.into() },
                ])
                .unwrap();
            assert!(keep);
        }
        let data = &p.sink.data;
        assert_eq!(data.pdf_rows[1][1].text, "-1");
        assert!(data.pdf_rows[1][1].negative);
        assert_eq!(data.pdf_rows[0][2].text, "10.50");
        let totals: Vec<String> = data
            .totals_row()
            .unwrap()
            .into_iter()
            .map(|f| f.text)
            .collect();
        assert_eq!(totals, vec!["Total", "2", "10.75"]);
        assert_eq!(
            data.fill_tokens("{title} for {organization}"),
            "Monthly Sales for Your Company"
        );
    }

    /// Manual check of the PDF layout: writes page images to `INTALIX_REPORT_PREVIEW`.
    /// Run with `cargo test report_preview -- --ignored`.
    #[test]
    #[ignore]
    fn report_preview() {
        let Ok(dir) = std::env::var("INTALIX_REPORT_PREVIEW") else {
            return;
        };
        let dir = PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        let customers = [
            "Northwind Traders",
            "Contoso Pharmaceuticals",
            "Fabrikam Industries",
            "Tailspin Logistics",
            "Wide World Importers",
        ];
        let regions = ["North", "South", "East", "West"];
        for (name, def) in [
            ("corporate", json!({})),
            (
                "modern",
                json!({"theme": {"accent": "#ea580c"}, "header": {"style": "band", "layout": "split"}, "table": {"headerStyle": "tinted", "density": "relaxed", "gridlines": "none"}}),
            ),
            (
                "ledger",
                json!({"page": {"orientation": "landscape", "margins": "narrow"}, "theme": {"accent": "#0f5132", "font": "arial", "headingFont": "arial", "baseSize": 8}, "table": {"gridlines": "all", "density": "compact", "rowNumbers": true, "striped": false}, "formats": {"negative": "parentheses", "currency": "₹", "numbers": "12,34,567.89"}, "watermark": {"text": "DRAFT"}}),
            ),
        ] {
            let mut p = prepare(&ReportRequest {
                title: "Monthly Sales Report".into(),
                variables: vec![VariableDef {
                    name: "period".into(),
                    label: "Period".into(),
                    kind: VariableType::DateRange,
                    required: true,
                    default: Value::Null,
                    options: None,
                    help: String::new(),
                }],
                values: [(
                    "period".to_string(),
                    json!({"from": "2026-03-01", "to": "2026-03-31"}),
                )]
                .into_iter()
                .collect(),
                template: def,
                ..request(true, false)
            })
            .unwrap();
            p.sink
                .start(&[
                    col("invoice", "text"),
                    col("order_date", "date"),
                    col("customer", "text"),
                    col("region", "text"),
                    col("units", "int8"),
                    col("amount", "numeric"),
                    col("change_pct", "numeric"),
                ])
                .unwrap();
            for i in 0..70i64 {
                p.sink
                    .row(vec![
                        Cell::Text {
                            v: format!("INV-{:05}", 1000 + i),
                            truncated: false,
                        },
                        Cell::Date {
                            v: format!("2026-03-{:02}", 1 + i % 28),
                        },
                        Cell::Text {
                            v: customers[(i % 5) as usize].into(),
                            truncated: false,
                        },
                        Cell::Text {
                            v: regions[(i % 4) as usize].into(),
                            truncated: false,
                        },
                        Cell::Int {
                            v: (3 + i * 7 % 40).to_string(),
                        },
                        Cell::Decimal {
                            v: format!("{}.{:02}", 1200 + i * 3719 % 90000, i * 13 % 100),
                        },
                        Cell::Decimal {
                            v: format!("{}.{}", (i * 17 % 30) - 12, i % 10),
                        },
                    ])
                    .unwrap();
            }
            let laid = pdf::layout(&p.sink.data, &p.template).unwrap();
            for (n, png) in pdf::page_images(&laid.document, 2)
                .unwrap()
                .into_iter()
                .enumerate()
            {
                std::fs::write(dir.join(format!("{name}-{}.png", n + 1)), png).unwrap();
            }
            write_pdf(&p.sink.data, &p.template, &dir.join(format!("{name}.pdf"))).unwrap();
        }
    }

    /// Manual check of a wide table (22 columns, long text, unbroken codes), portrait and
    /// landscape. Run with `cargo test wide_report_preview -- --ignored`.
    #[test]
    #[ignore]
    fn wide_report_preview() {
        let Ok(dir) = std::env::var("INTALIX_REPORT_PREVIEW") else {
            return;
        };
        let dir = PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        let names = [
            "id",
            "category_id",
            "name",
            "cat_no",
            "sku_code",
            "formula",
            "weight",
            "status",
            "related_codes",
            "chemical_name",
            "synonym",
            "description",
            "ship_temp",
            "storage",
            "country_of_origin",
            "smiles",
            "image_path",
            "file_path",
            "unit_price",
            "quantity",
            "created_at",
            "updated_at",
        ];
        let types = |n: &str| match n {
            "unit_price" => "numeric",
            "quantity" => "int4",
            "created_at" | "updated_at" => "timestamptz",
            _ => "text",
        };
        for (name, orientation) in [
            ("wide-portrait", "portrait"),
            ("wide-landscape", "landscape"),
        ] {
            let mut p = prepare(&ReportRequest {
                template: json!({"page": {"orientation": orientation}}),
                ..request(true, false)
            })
            .unwrap();
            let cols: Vec<ResultColumn> = names.iter().map(|n| col(n, types(n))).collect();
            p.sink.start(&cols).unwrap();
            for i in 0..40i64 {
                let row = names
                    .iter()
                    .map(|n| {
                        let text = match *n {
                            "id" => format!("67d3{i:04}-b5f8-31aa-9c4e-66b1e9e{i:05}"),
                            "category_id" => format!("13e1b2c4-{i:04}-4f1a-9b8e-2d6f5c7a8b9c"),
                            "name" => "Abacavir Sulfate Impurity".into(),
                            "cat_no" => format!("AVR-A{:06}", 2049 + i),
                            "description" => "Abacavir Sulfate is chemically Bis[(1S,4R)-4-[2-amino-6-(cyclopropylamino)-9H-purin-9-yl]cyclopent-2-enyl]methanol sulfate (as per EP); it is supplied with a detailed certificate of analysis and characterisation data. ".repeat(3),
                            "chemical_name" => "Bis[(1S,4R)-4-[2-amino-6-(cyclopropylamino)-9H-purin-9-yl]cyclopent-2-enyl]methanol sulfate".into(),
                            "smiles" => "[H][C@@]1(CO)C[C@@H](C=C1)N1C=NC2=C1N=C(N)N=C2NC1CC1".into(),
                            "image_path" => format!("/assets/rsi/images/{i}/structure.png"),
                            "file_path" => format!("/assets/rsi/files/{i}/coa.pdf"),
                            "related_codes" => "136470-78-5 (free base); 136777-48-6 (HCl salt)".into(),
                            "unit_price" => format!("{}.50", 1200 + i * 37),
                            "quantity" => (i % 9 + 1).to_string(),
                            "created_at" | "updated_at" => "2025-11-30T10:15:00+05:30".into(),
                            other => format!("{other} {i}"),
                        };
                        match types(n) {
                            "numeric" => Cell::Decimal { v: text },
                            "int4" => Cell::Int { v: text },
                            "timestamptz" => Cell::Timestamptz { v: text },
                            _ => Cell::Text { v: text, truncated: false },
                        }
                    })
                    .collect();
                p.sink.row(row).unwrap();
            }
            let laid = pdf::layout(&p.sink.data, &p.template).unwrap();
            for (n, png) in pdf::page_images(&laid.document, 1)
                .unwrap()
                .into_iter()
                .enumerate()
            {
                std::fs::write(dir.join(format!("{name}-{}.png", n + 1)), png).unwrap();
            }
        }
    }

    /// Lays out a real PDF and Excel file, so a broken layout fails the build's tests.
    #[test]
    fn renders_pdf_and_excel() {
        let mut p = prepare(&ReportRequest {
            template: json!({
                "header": {"style": "band", "layout": "split"},
                "table": {"rowNumbers": true, "gridlines": "all"},
                "formats": {"negative": "red", "currency": "₹"},
                "watermark": {"text": "DRAFT"},
            }),
            ..request(true, true)
        })
        .unwrap();
        p.sink
            .start(&[
                col("customer", "text"),
                col("order_date", "date"),
                col("amount", "numeric"),
            ])
            .unwrap();
        for i in 0..120 {
            p.sink
                .row(vec![
                    Cell::Text {
                        v: format!("Customer {i} # [x] *bold* $ \\ \"q\" हिन्दी"),
                        truncated: false,
                    },
                    Cell::Date {
                        v: "2026-03-07".into(),
                    },
                    Cell::Decimal {
                        v: format!("{}.25", i - 60),
                    },
                ])
                .unwrap();
        }
        let dir = std::env::temp_dir().join(format!("intalix-report-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let pdf_path = dir.join("r.pdf");
        let pages = write_pdf(&p.sink.data, &p.template, &pdf_path)
            .unwrap()
            .pages;
        assert!(pages >= 2, "{pages}");
        let bytes = std::fs::read(&pdf_path).unwrap();
        assert!(bytes.starts_with(b"%PDF"));

        let xlsx_path = dir.join("r.xlsx");
        let x = p.sink.take_xlsx().unwrap();
        write_xlsx(x, &p.sink.data, &xlsx_path).unwrap();
        let bytes = std::fs::read(&xlsx_path).unwrap();
        assert!(bytes.starts_with(b"PK"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
