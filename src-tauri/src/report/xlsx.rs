//! Excel reports through `rust_xlsxwriter`, streamed row by row ("constant memory" mode) so a
//! million-row report never sits in memory.
//!
//! Precision policy: numbers with up to 15 significant digits are written as Excel numbers
//! (Excel's own limit). Longer values are written as text, right-aligned, so financial values
//! are never silently rounded. Text is always written as text: a value starting with `=` is
//! not a formula.
//!
//! Number formats follow the template's decimals, currency and negative style. Digit grouping
//! symbols follow the reader's Excel settings, except Indian grouping, which is built into
//! the format.

use std::path::Path;

use rust_xlsxwriter::{
    Color, ExcelDateTime, Format, FormatAlign, FormatBorder, Formula, Workbook, XlsxError,
};

use crate::error::{AppError, AppResult};
use crate::model::Cell;
use crate::report::ReportData;
use crate::report::format::{ColumnKind, decimal_text, format_cell, is_plain_decimal, ymd};
use crate::report::template::{
    DateFormat, NegativeStyle, NumberStyle, PageSize, TableHeaderStyle, Template,
};

/// Excel's own limit on precise digits.
const MAX_EXACT_DIGITS: usize = 15;
/// Excel's limit on characters in a cell.
const MAX_CELL_CHARS: usize = 32_767;
/// Rows sampled to size the columns.
const WIDTH_SAMPLE_ROWS: u64 = 2_000;

fn color(rgb: crate::report::template::Rgb) -> Color {
    Color::RGB(rgb.as_u32())
}

/// Excel number format for a numeric column.
///
/// # Examples
/// - Money, 2 decimals, `₹`, parentheses → `"₹"#,##0.00;("₹"#,##0.00)`
/// - Int, Indian grouping → `[>=10000000]##\,##\,##\,##0;[>=100000]##\,##\,##0;##,##0`
pub fn number_format(kind: ColumnKind, t: &Template) -> String {
    let f = &t.formats;
    let decimals = if kind == ColumnKind::Int {
        0
    } else {
        f.decimals
    };
    let frac = if decimals == 0 {
        String::new()
    } else {
        format!(".{}", "0".repeat(decimals))
    };
    let currency = if kind == ColumnKind::Money && !f.currency.is_empty() {
        format!("\"{}\"", f.currency.replace('"', ""))
    } else {
        String::new()
    };
    if f.numbers == NumberStyle::Indian {
        // Lakh / crore grouping; negatives fall to the last section and keep a minus sign.
        return format!(
            "[>=10000000]{currency}##\\,##\\,##\\,##0{frac};[>=100000]{currency}##\\,##\\,##0{frac};{currency}##,##0{frac}"
        );
    }
    let base = format!("{currency}#,##0{frac}");
    match f.negative {
        NegativeStyle::Minus => base,
        NegativeStyle::Parentheses => format!("{base};({base})"),
        NegativeStyle::Red => format!("{base};[Red]-{base}"),
    }
}

/// Excel date format matching the template.
pub fn date_format(format: DateFormat, with_time: bool) -> String {
    let date = match format {
        DateFormat::DdMmmYyyy => "dd mmm yyyy",
        DateFormat::Iso => "yyyy-mm-dd",
        DateFormat::DdMmYyyy => "dd/mm/yyyy",
        DateFormat::MmDdYyyy => "mm/dd/yyyy",
        DateFormat::MmmDYyyy => "mmm d, yyyy",
    };
    if with_time {
        format!("{date} hh:mm")
    } else {
        date.to_owned()
    }
}

/// Significant digits in a plain decimal string.
fn significant_digits(v: &str) -> usize {
    let digits: String = v.chars().filter(char::is_ascii_digit).collect();
    let trimmed = digits.trim_start_matches('0').trim_end_matches('0');
    trimmed.len().max(1)
}

/// `13:05:09.25…` → (13, 5, 9.25)
fn hms(time: &str) -> Option<(u16, u8, f64)> {
    let h = time.get(0..2)?.parse().ok()?;
    let m = time.get(3..5)?.parse().ok()?;
    let rest = time.get(6..)?;
    let end = rest
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(rest.len());
    let s = rest[..end].parse().ok()?;
    Some((h, m, s))
}

/// Excel header/footer text: `{page}` → `&P`, `{pages}` → `&N`, `&` escaped.
fn print_text(text: &str) -> String {
    text.replace('&', "&&")
        .replace("{page}", "&P")
        .replace("{pages}", "&N")
}

fn io_error(e: XlsxError) -> AppError {
    match e {
        XlsxError::IoError(io) => AppError::invalid("The Excel file could not be saved.")
            .with_hint(format!(
                "If the file is open in Excel, close it and try again. ({io})"
            )),
        other => AppError::internal("writing the Excel file", other),
    }
}

struct ColumnFormats {
    plain: Format,
    striped: Format,
    /// Right-aligned text for numbers too precise for Excel.
    exact: Format,
    exact_striped: Format,
}

pub struct XlsxReport {
    workbook: Workbook,
    template: Template,
    kinds: Vec<ColumnKind>,
    formats: Vec<ColumnFormats>,
    header_row: u32,
    next_row: u32,
    widths: Vec<usize>,
    sampled: u64,
}

impl XlsxReport {
    /// Starts the workbook: title, subtitle, filters and the column headings.
    pub fn start(data: &ReportData, template: &Template) -> AppResult<Self> {
        let t = template.clone();
        let mut workbook = Workbook::new();
        let font = t.font.excel();
        let size = f64::from(t.base_size) + 1.5;
        let base = Format::new().set_font_name(font).set_font_size(size);

        let ws = workbook
            .add_worksheet_with_constant_memory()
            .map_err(io_error)?;
        ws.set_name("Report").map_err(io_error)?;

        let ncols = data.columns.len().max(1);
        let last_col = u16::try_from(ncols - 1).unwrap_or(u16::MAX);
        let mut row: u32 = 0;
        let title = data.fill_tokens(&t.header.title);
        if t.header.visible {
            let title_fmt = Format::new()
                .set_font_name(t.heading_font.excel())
                .set_font_size(size * 1.6)
                .set_bold()
                .set_font_color(color(t.text));
            ws.write_string_with_format(row, 0, &title, &title_fmt)
                .map_err(io_error)?;
            row += 1;
            let muted = base.clone().set_font_color(color(t.muted));
            let subtitle = data.fill_tokens(&t.header.subtitle);
            if !subtitle.trim().is_empty() {
                ws.write_string_with_format(row, 0, &subtitle, &muted)
                    .map_err(io_error)?;
                row += 1;
            }
            if t.header.show_parameters && !data.parameters.is_empty() {
                let line = data
                    .parameters
                    .iter()
                    .map(|(l, v)| format!("{l}: {v}"))
                    .collect::<Vec<_>>()
                    .join("    ");
                ws.write_string_with_format(row, 0, &line, &muted)
                    .map_err(io_error)?;
                row += 1;
            }
            row += 1; // blank line before the table
        }

        // Column headings.
        let mut head = base
            .clone()
            .set_bold()
            .set_border_bottom(FormatBorder::Thin);
        head = match t.table.header_style {
            TableHeaderStyle::Filled => head
                .set_background_color(color(t.accent))
                .set_font_color(color(t.accent.readable_text()))
                .set_border_bottom_color(color(t.accent)),
            TableHeaderStyle::Tinted => head
                .set_background_color(color(t.accent.tint(12)))
                .set_font_color(color(t.text))
                .set_border_bottom_color(color(t.accent.tint(45))),
            TableHeaderStyle::Underline => head
                .set_font_color(color(t.text))
                .set_border_bottom_color(color(t.accent)),
            TableHeaderStyle::Plain => head
                .set_font_color(color(t.text))
                .set_border_bottom_color(color(t.text)),
        };
        let header_row = row;
        for (i, c) in data.columns.iter().enumerate() {
            let col = u16::try_from(i).unwrap_or(u16::MAX);
            let fmt = if c.kind.numeric() {
                head.clone().set_align(FormatAlign::Right)
            } else {
                head.clone()
            };
            let label = if t.table.uppercase_headers {
                c.label.to_uppercase()
            } else {
                c.label.clone()
            };
            ws.write_string_with_format(header_row, col, &label, &fmt)
                .map_err(io_error)?;
        }

        // Data formats per column.
        let stripe = color(t.accent.tint(5));
        let line = color(t.text.tint(14));
        let gridded = |f: Format| match t.table.gridlines {
            crate::report::template::Gridlines::None => f,
            crate::report::template::Gridlines::Horizontal => f
                .set_border_bottom(FormatBorder::Thin)
                .set_border_bottom_color(line),
            crate::report::template::Gridlines::All => {
                f.set_border(FormatBorder::Thin).set_border_color(line)
            }
        };
        let formats = data
            .columns
            .iter()
            .map(|c| {
                let mut plain = gridded(base.clone());
                plain = match c.kind {
                    ColumnKind::Int | ColumnKind::Money | ColumnKind::Decimal => {
                        plain.set_num_format(number_format(c.kind, &t))
                    }
                    ColumnKind::Date => plain
                        .set_num_format(date_format(t.formats.date, false))
                        .set_align(FormatAlign::Left),
                    ColumnKind::Timestamp => plain
                        .set_num_format(date_format(t.formats.date, true))
                        .set_align(FormatAlign::Left),
                    ColumnKind::Bool => plain.set_align(FormatAlign::Center),
                    ColumnKind::Text => plain,
                };
                let exact = gridded(base.clone()).set_align(FormatAlign::Right);
                ColumnFormats {
                    striped: plain.clone().set_background_color(stripe),
                    exact_striped: exact.clone().set_background_color(stripe),
                    plain,
                    exact,
                }
            })
            .collect();

        // Page setup for printing, following the template.
        if t.landscape {
            ws.set_landscape();
        }
        ws.set_paper_size(match t.page_size {
            PageSize::A4 => 9,
            PageSize::Letter => 1,
            PageSize::Legal => 5,
        });
        let inch = t.margin_mm / 25.4;
        ws.set_margins(inch, inch, inch + 0.2, inch + 0.2, inch / 2.0, inch / 2.0);
        ws.set_repeat_rows(header_row, header_row)
            .map_err(io_error)?;
        if t.footer.visible {
            ws.set_footer(format!(
                "&L{}&C{}&R{}",
                print_text(&data.fill_tokens(&t.footer.left)),
                print_text(&data.fill_tokens(&t.footer.center)),
                print_text(&data.fill_tokens(&t.footer.right)),
            ));
        }
        ws.set_freeze_panes(header_row + 1, 0).map_err(io_error)?;
        let _ = last_col;

        Ok(Self {
            workbook,
            widths: data
                .columns
                .iter()
                .map(|c| c.label.chars().count().min(40))
                .collect(),
            kinds: data.columns.iter().map(|c| c.kind).collect(),
            formats,
            template: t,
            header_row,
            next_row: header_row + 1,
            sampled: 0,
        })
    }

    /// Appends one data row.
    pub fn row(&mut self, cells: &[Cell]) -> AppResult<()> {
        let row = self.next_row;
        let striped = self.template.table.striped && (row - self.header_row).is_multiple_of(2);
        let sample = self.sampled < WIDTH_SAMPLE_ROWS;
        let ws = self.workbook.worksheet_from_index(0).map_err(io_error)?;
        for (i, cell) in cells.iter().enumerate() {
            let (Some(kind), Some(f)) = (self.kinds.get(i), self.formats.get(i)) else {
                continue;
            };
            let col = u16::try_from(i).unwrap_or(u16::MAX);
            let fmt = if striped { &f.striped } else { &f.plain };
            let exact = if striped { &f.exact_striped } else { &f.exact };
            match cell {
                Cell::Null => {
                    ws.write_blank(row, col, fmt).map_err(io_error)?;
                }
                Cell::Bool { v } => {
                    ws.write_boolean_with_format(row, col, *v, fmt)
                        .map_err(io_error)?;
                }
                Cell::Int { .. } | Cell::Decimal { .. } | Cell::Float { .. } => {
                    let text = decimal_text(cell).unwrap_or_default();
                    let number = text.parse::<f64>().ok().filter(|n| n.is_finite());
                    match number {
                        Some(n)
                            if is_plain_decimal(&text)
                                && significant_digits(&text) <= MAX_EXACT_DIGITS =>
                        {
                            ws.write_number_with_format(row, col, n, fmt)
                                .map_err(io_error)?;
                        }
                        // Too precise for Excel (or NaN / Infinity): keep the exact text.
                        _ => {
                            ws.write_string_with_format(row, col, &text, exact)
                                .map_err(io_error)?;
                        }
                    }
                }
                Cell::Date { v } | Cell::Timestamp { v } | Cell::Timestamptz { v } => {
                    let dt = ymd(v)
                        .filter(|(y, _, _)| (1900..=9999).contains(y))
                        .and_then(|(y, m, d)| {
                            ExcelDateTime::from_ymd(u16::try_from(y).ok()?, m, d).ok()
                        })
                        .and_then(|date| match (kind, v.get(11..).and_then(hms)) {
                            (ColumnKind::Timestamp, Some((h, mi, s))) => {
                                date.and_hms(h, mi, s).ok()
                            }
                            _ => Some(date),
                        });
                    match dt {
                        Some(dt) => {
                            ws.write_datetime_with_format(row, col, &dt, fmt)
                                .map_err(io_error)?;
                        }
                        None => {
                            ws.write_string_with_format(row, col, v, fmt)
                                .map_err(io_error)?;
                        }
                    }
                }
                Cell::Text { v, .. } | Cell::Json { v, .. } => {
                    let text: String = v.chars().take(MAX_CELL_CHARS).collect();
                    ws.write_string_with_format(row, col, &text, fmt)
                        .map_err(io_error)?;
                }
                other => {
                    let text = format_cell(other, *kind, &self.template.formats).text;
                    ws.write_string_with_format(row, col, &text, fmt)
                        .map_err(io_error)?;
                }
            }
            if sample && let Some(w) = self.widths.get_mut(i) {
                let shown = format_cell(cell, *kind, &self.template.formats).text;
                *w = (*w).max(shown.chars().count().min(60));
            }
        }
        self.sampled += 1;
        self.next_row += 1;
        Ok(())
    }

    /// Writes the totals row, column widths and filter, then saves the file.
    pub fn finish(mut self, data: &ReportData, path: &Path) -> AppResult<()> {
        let t = self.template.clone();
        let first_data = self.header_row + 1;
        let last_data = self.next_row.saturating_sub(1);
        let has_rows = self.next_row > first_data;
        let ncols = u16::try_from(data.columns.len()).unwrap_or(u16::MAX);
        let ws = self.workbook.worksheet_from_index(0).map_err(io_error)?;

        if has_rows && let Some(totals) = data.totals_row() {
            let row = self.next_row;
            let size = f64::from(t.base_size) + 1.5;
            let mut total_fmt = Format::new()
                .set_font_name(t.font.excel())
                .set_font_size(size)
                .set_bold()
                .set_border_top(FormatBorder::Thin)
                .set_border_top_color(color(t.text));
            if t.table.header_style != TableHeaderStyle::Plain {
                total_fmt = total_fmt.set_background_color(color(t.accent.tint(7)));
            }
            for (i, (column, shown)) in data.columns.iter().zip(totals.iter()).enumerate() {
                let col = u16::try_from(i).unwrap_or(u16::MAX);
                let letter = rust_xlsxwriter::utility::column_number_to_name(col);
                if i > 0 && column.summable && data.sums.get(i).is_some_and(Option::is_some) {
                    let fmt = total_fmt
                        .clone()
                        .set_num_format(number_format(column.kind, &t));
                    let raw = data.sums[i]
                        .as_ref()
                        .and_then(|s| s.text())
                        .unwrap_or_default();
                    let formula = Formula::new(format!(
                        "=SUM({letter}{}:{letter}{})",
                        first_data + 1,
                        last_data + 1
                    ))
                    .set_result(raw);
                    ws.write_formula_with_format(row, col, formula, &fmt)
                        .map_err(io_error)?;
                } else {
                    ws.write_string_with_format(row, col, &shown.text, &total_fmt)
                        .map_err(io_error)?;
                }
            }
        }

        for (i, w) in self.widths.iter().enumerate() {
            let col = u16::try_from(i).unwrap_or(u16::MAX);
            let width = (*w as f64 + 2.0).clamp(8.0, 62.0);
            ws.set_column_width(col, width).map_err(io_error)?;
        }
        if has_rows && ncols > 0 {
            ws.autofilter(self.header_row, 0, last_data, ncols - 1)
                .map_err(io_error)?;
        }
        self.workbook.save(path).map_err(io_error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn number_formats_follow_the_template() {
        let t = Template::from_definition(&json!({
            "formats": {"decimals": 2, "currency": "₹", "negative": "parentheses"}
        }));
        assert_eq!(
            number_format(ColumnKind::Money, &t),
            "\"₹\"#,##0.00;(\"₹\"#,##0.00)"
        );
        assert_eq!(number_format(ColumnKind::Int, &t), "#,##0;(#,##0)");
        let red =
            Template::from_definition(&json!({"formats": {"negative": "red", "decimals": 0}}));
        assert_eq!(
            number_format(ColumnKind::Decimal, &red),
            "#,##0;[Red]-#,##0"
        );
        let indian = Template::from_definition(&json!({"formats": {"numbers": "12,34,567.89"}}));
        assert_eq!(
            number_format(ColumnKind::Int, &indian),
            "[>=10000000]##\\,##\\,##\\,##0;[>=100000]##\\,##\\,##0;##,##0"
        );
    }

    #[test]
    fn helpers() {
        assert_eq!(date_format(DateFormat::MmmDYyyy, true), "mmm d, yyyy hh:mm");
        assert_eq!(significant_digits("-001234.5600"), 6);
        assert_eq!(significant_digits("12345678901234567890"), 19);
        assert_eq!(hms("13:05:09.25+00"), Some((13, 5, 9.25)));
        assert_eq!(
            print_text("A & B · Page {page} of {pages}"),
            "A && B · Page &P of &N"
        );
    }
}
