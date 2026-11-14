//! PDF reports through Typst, embedded as a library.
//!
//! `report.typ` is a fixed layout; the report itself is handed over as a JSON file in a
//! virtual file system (plus the logo image). The compiler can read nothing else: no disk
//! access, no packages, no network.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::OnceLock;

use base64::Engine;
use serde_json::{Value, json};
use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime, Duration};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};
use typst_layout::PagedDocument;

use crate::error::{AppError, AppResult, ErrorCategory};
use crate::report::ReportData;
use crate::report::fit::{self, ColumnStats};
use crate::report::format::{ColumnKind, Formatted};
use crate::report::template::{
    Gridlines, HeaderLayout, HeaderStyle, NegativeStyle, TableHeaderStyle, Template,
};

const LAYOUT: &str = include_str!("report.typ");

/// Fonts bundled with the app. Inter is licensed under the SIL Open Font License
/// (src-tauri/fonts/Inter-LICENSE.txt).
const BUNDLED: &[&[u8]] = &[
    include_bytes!("../../fonts/Inter-Regular.ttf"),
    include_bytes!("../../fonts/Inter-SemiBold.ttf"),
    include_bytes!("../../fonts/Inter-Bold.ttf"),
];

/// Windows fonts used by templates, plus fallbacks for Indian scripts and symbols.
const SYSTEM_FONTS: &[&str] = &[
    "segoeui.ttf",
    "segoeuib.ttf",
    "seguisb.ttf",
    "arial.ttf",
    "arialbd.ttf",
    "calibri.ttf",
    "calibrib.ttf",
    "georgia.ttf",
    "georgiab.ttf",
    "cambria.ttc",
    "cambriab.ttf",
    "Nirmala.ttc",
    "seguisym.ttf",
];

/// Families tried after the template font, for characters it lacks.
const FALLBACKS: &[&str] = &["Segoe UI", "Nirmala UI", "Segoe UI Symbol", "Inter"];

struct Shared {
    library: LazyHash<Library>,
    book: LazyHash<FontBook>,
    fonts: Vec<Font>,
}

/// Loads the fonts ahead of the first report, so the first preview isn't slower.
pub fn warm_up() {
    let _ = shared();
}

/// Fonts and the standard library, loaded once per run of the app.
fn shared() -> &'static Shared {
    static SHARED: OnceLock<Shared> = OnceLock::new();
    SHARED.get_or_init(|| {
        let mut fonts: Vec<Font> = BUNDLED
            .iter()
            .flat_map(|data| Font::iter(Bytes::new(data.to_vec())))
            .collect();
        let windir =
            std::env::var_os("WINDIR").map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from);
        for name in SYSTEM_FONTS {
            // A missing font only means its family falls back to another.
            if let Ok(data) = std::fs::read(windir.join("Fonts").join(name)) {
                fonts.extend(Font::iter(Bytes::new(data)));
            }
        }
        Shared {
            library: LazyHash::new(Library::default()),
            book: LazyHash::new(FontBook::from_fonts(&fonts)),
            fonts,
        }
    })
}

fn file_id(path: &str) -> AppResult<FileId> {
    let vpath = VirtualPath::new(path).map_err(|e| AppError::internal("report file path", e))?;
    Ok(FileId::new(RootedPath::new(VirtualRoot::Project, vpath)))
}

struct ReportWorld {
    shared: &'static Shared,
    main: FileId,
    source: Source,
    files: HashMap<FileId, Bytes>,
    today: Option<Datetime>,
}

impl World for ReportWorld {
    fn library(&self) -> &LazyHash<Library> {
        &self.shared.library
    }
    fn book(&self) -> &LazyHash<FontBook> {
        &self.shared.book
    }
    fn main(&self) -> FileId {
        self.main
    }
    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.main {
            Ok(self.source.clone())
        } else {
            Err(FileError::NotSource)
        }
    }
    fn file(&self, id: FileId) -> FileResult<Bytes> {
        self.files
            .get(&id)
            .cloned()
            .ok_or_else(|| FileError::NotFound(PathBuf::from(format!("{id:?}"))))
    }
    fn font(&self, index: usize) -> Option<Font> {
        self.shared.fonts.get(index).cloned()
    }
    fn today(&self, _offset: Option<Duration>) -> Option<Datetime> {
        self.today
    }
}

/// The logo as (virtual path, bytes) from a template data URL.
fn logo_file(data_url: &str) -> Option<(&'static str, Vec<u8>)> {
    let (path, b64) = [
        ("/logo.png", "data:image/png;base64,"),
        ("/logo.jpg", "data:image/jpeg;base64,"),
        ("/logo.webp", "data:image/webp;base64,"),
    ]
    .into_iter()
    .find_map(|(path, prefix)| Some((path, data_url.strip_prefix(prefix)?)))?;
    let bytes = base64::engine::general_purpose::STANDARD.decode(b64).ok()?;
    Some((path, bytes))
}

/// Splits footer text into literal parts and `{page}` / `{pages}` tokens.
///
/// # Examples
/// - `"Page {page} of {pages}"` → `["Page ", "{page}", " of ", "{pages}"]`
fn footer_parts(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find('{') {
        let token = ["{page}", "{pages}"]
            .into_iter()
            .find(|t| rest[i..].starts_with(t));
        match token {
            Some(t) => {
                if i > 0 {
                    out.push(rest[..i].to_owned());
                }
                out.push(t.to_owned());
                rest = &rest[i + t.len()..];
            }
            None => {
                out.push(rest[..=i].to_owned());
                rest = &rest[i + 1..];
            }
        }
    }
    if !rest.is_empty() {
        out.push(rest.to_owned());
    }
    out
}

fn initials(name: &str) -> String {
    let words: Vec<&str> = name.split_whitespace().collect();
    let s: String = match words.as_slice() {
        [a, b, ..] => a.chars().take(1).chain(b.chars().take(1)).collect(),
        [a] => a.chars().take(2).collect(),
        [] => String::new(),
    };
    if s.is_empty() {
        "IS".to_owned()
    } else {
        s.to_uppercase()
    }
}

/// The JSON document `report.typ` reads, whether long text was shortened to fit, and whether
/// the columns were too many for the page.
fn report_json(data: &ReportData, t: &Template, logo: Option<&str>) -> (Value, bool, bool) {
    let (w, h) = t.page_mm();
    let colors = json!({
        "text": t.text.hex(),
        "muted": t.muted.hex(),
        "accent": t.accent.hex(),
        "onAccent": t.accent.readable_text().hex(),
        "line": t.text.tint(14).hex(),
        "stripe": t.accent.tint(5).hex(),
        "tint7": t.accent.tint(7).hex(),
        "tint12": t.accent.tint(12).hex(),
        "tint45": t.accent.tint(45).hex(),
        "footerRule": t.text.tint(22).hex(),
        "negative": "#c81e1e",
    });
    let totals_cells = data.totals_row();
    let fonts = |primary: &'static str| -> Vec<&'static str> {
        std::iter::once(primary)
            .chain(FALLBACKS.iter().copied().filter(|f| *f != primary))
            .collect()
    };
    // Fit the table to the page width.
    let numbered = t.table.row_numbers;
    let stats: Vec<ColumnStats> = data
        .columns
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let values = data
                .pdf_rows
                .iter()
                .filter_map(|r| r.get(i))
                .map(|f| f.text.as_str());
            let totals = totals_cells
                .as_ref()
                .and_then(|r| r.get(i))
                .map(|f| f.text.as_str());
            ColumnStats::measure(c.kind, &c.label, values.chain(totals))
        })
        .collect();
    let row_digits = numbered.then(|| data.pdf_rows.len().max(1).to_string().len());
    let fit = fit::fit(
        &stats,
        w - 2.0 * t.margin_mm,
        f64::from(t.base_size),
        row_digits,
    );
    let mut shortened = false;
    let mut cell = |c: &Formatted, i: usize| -> Value {
        let kind = data.columns.get(i).map_or(ColumnKind::Text, |c| c.kind);
        let (text, short) = match fit.columns.get(i) {
            Some(col) => fit::cell_text(&c.text, kind, col, fit.break_all),
            None => (c.text.clone(), false),
        };
        shortened |= short;
        if c.negative {
            json!([text])
        } else {
            json!(text)
        }
    };
    let rows: Vec<Value> = data
        .pdf_rows
        .iter()
        .map(|r| Value::Array(r.iter().enumerate().map(|(i, c)| cell(c, i)).collect()))
        .collect();
    let totals = totals_cells.as_ref().map(|cells| {
        cells
            .iter()
            .enumerate()
            .map(|(i, c)| cell(c, i))
            .collect::<Vec<_>>()
    });
    let columns: Vec<Value> = data
        .columns
        .iter()
        .zip(&fit.columns)
        .map(|(c, f)| {
            json!({
                "label": fit::header_text(&c.label, f),
                "right": c.kind.numeric(),
                "width": f.width_mm,
            })
        })
        .collect();

    let json = json!({
        "title": data.title,
        "page": {"w": w, "h": h, "margin": t.margin_mm},
        "font": fonts(t.font.family()),
        "headingFont": fonts(t.heading_font.family()),
        "size": t.base_size,
        "colors": colors,
        "header": {
            "visible": t.header.visible,
            "layout": match t.header.layout {
                HeaderLayout::Left => "left",
                HeaderLayout::Center => "center",
                HeaderLayout::Split => "split",
            },
            "style": match t.header.style {
                HeaderStyle::Plain => "plain",
                HeaderStyle::Rule => "rule",
                HeaderStyle::Band => "band",
            },
            "showLogo": t.header.show_logo,
            "logo": logo,
            "logoSize": t.header.logo_pt,
            "monogram": initials(&t.header.organization),
            "title": data.fill_tokens(&t.header.title),
            "subtitle": data.fill_tokens(&t.header.subtitle),
            "showParameters": t.header.show_parameters,
            "parameters": data.parameters.iter().map(|(l, v)| json!([l, v])).collect::<Vec<_>>(),
        },
        "continued": data.fill_tokens(&t.header.title),
        "table": {
            "style": match t.table.header_style {
                TableHeaderStyle::Filled => "filled",
                TableHeaderStyle::Tinted => "tinted",
                TableHeaderStyle::Underline => "underline",
                TableHeaderStyle::Plain => "plain",
            },
            "striped": t.table.striped,
            "grid": match t.table.gridlines {
                Gridlines::None => "none",
                Gridlines::Horizontal => "horizontal",
                Gridlines::All => "all",
            },
            "pad": t.table.pad_mm,
            "padX": fit.pad_x,
            "size": fit.size,
            "rowNumbers": t.table.row_numbers,
            "uppercase": t.table.uppercase_headers,
            "columns": columns,
            "rows": rows,
            "totals": totals,
            "empty": "No rows match this report's filters.",
        },
        "footer": {
            "visible": t.footer.visible,
            "rule": t.footer.rule,
            "left": footer_parts(&data.fill_tokens(&t.footer.left)),
            "center": footer_parts(&data.fill_tokens(&t.footer.center)),
            "right": footer_parts(&data.fill_tokens(&t.footer.right)),
        },
        "watermark": {"text": t.watermark.text, "opacity": t.watermark.opacity},
        "negRed": t.formats.negative == NegativeStyle::Red,
    });
    (json, shortened, fit.break_all)
}

/// A laid-out report.
pub struct Laid {
    pub document: PagedDocument,
    /// Some long text was shortened to fit the page.
    pub shortened: bool,
    /// More columns than the page width holds, even at the smallest text.
    pub crowded: bool,
}

/// The PDF file of a laid-out report.
pub fn pdf_bytes(document: &PagedDocument) -> AppResult<Vec<u8>> {
    typst_pdf::pdf(document, &typst_pdf::PdfOptions::default()).map_err(|errors| {
        let detail = errors
            .iter()
            .map(|e| e.message.to_string())
            .collect::<Vec<_>>()
            .join("; ");
        AppError::new(ErrorCategory::Internal, "The PDF could not be written.").with_hint(detail)
    })
}

/// Page images (PNG) of a laid-out report, at most `limit` pages.
pub fn page_images(document: &PagedDocument, limit: usize) -> AppResult<Vec<Vec<u8>>> {
    let opts = typst_render::RenderOptions {
        pixel_per_pt: 1.5.into(),
        ..Default::default()
    };
    document
        .pages()
        .iter()
        .take(limit)
        .map(|page| {
            typst_render::render(page, &opts)
                .encode_png()
                .map_err(|e| AppError::internal("drawing the preview", e))
        })
        .collect()
}

/// Lays out the report pages.
pub fn layout(data: &ReportData, t: &Template) -> AppResult<Laid> {
    let mut files = HashMap::new();
    let logo_path = match t.header.logo.as_deref().and_then(logo_file) {
        Some((path, bytes)) => {
            files.insert(file_id(path)?, Bytes::new(bytes));
            Some(path)
        }
        None => None,
    };
    let (doc, shortened, crowded) = report_json(data, t, logo_path);
    let json = serde_json::to_vec(&doc).map_err(|e| AppError::internal("preparing the PDF", e))?;
    files.insert(file_id("/report.json")?, Bytes::new(json));

    let main = file_id("/report.typ")?;
    let world = ReportWorld {
        shared: shared(),
        main,
        source: Source::new(main, LAYOUT.to_owned()),
        files,
        today: Datetime::from_ymd(data.year, data.month, data.day),
    };

    let document: PagedDocument = typst::compile(&world).output.map_err(|errors| {
        let detail = errors
            .iter()
            .map(|e| e.message.to_string())
            .collect::<Vec<_>>()
            .join("; ");
        AppError::internal("laying out the PDF", detail)
    })?;
    Ok(Laid {
        document,
        shortened,
        crowded,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_footer_tokens() {
        assert_eq!(
            footer_parts("Page {page} of {pages}"),
            vec!["Page ", "{page}", " of ", "{pages}"]
        );
        assert_eq!(footer_parts("{x} {page}"), vec!["{", "x} ", "{page}"]);
        assert!(footer_parts("").is_empty());
    }

    #[test]
    fn monograms() {
        assert_eq!(initials("Your Company"), "YC");
        assert_eq!(initials("intalix"), "IN");
        assert_eq!(initials("  "), "IS");
    }

    #[test]
    fn decodes_logos() {
        let (path, bytes) = logo_file("data:image/png;base64,iVBORw0KGgo=").unwrap();
        assert_eq!(path, "/logo.png");
        assert_eq!(&bytes[1..4], b"PNG");
        assert!(logo_file("data:image/svg+xml;base64,AAAA").is_none());
        assert!(logo_file("data:image/png;base64,!!").is_none());
    }
}
