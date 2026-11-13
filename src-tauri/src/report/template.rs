//! The report template as the exporters use it.
//!
//! Mirrors `src/features/templates/model.ts`. Like the UI's `normalizeDefinition`, every
//! field falls back to its default when it is missing or invalid, so a damaged or older
//! template still produces a report.

use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub struct Template {
    pub page_size: PageSize,
    pub landscape: bool,
    /// Margin in millimetres.
    pub margin_mm: f64,
    pub accent: Rgb,
    pub text: Rgb,
    pub muted: Rgb,
    pub font: FontId,
    pub heading_font: FontId,
    /// Body size in points.
    pub base_size: u32,
    pub header: Header,
    pub table: Table,
    pub formats: Formats,
    pub footer: Footer,
    pub watermark: Watermark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageSize {
    A4,
    Letter,
    Legal,
}

impl PageSize {
    /// Portrait width and height in millimetres.
    pub fn mm(self) -> (f64, f64) {
        match self {
            Self::A4 => (210.0, 297.0),
            Self::Letter => (215.9, 279.4),
            Self::Legal => (215.9, 355.6),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontId {
    Inter,
    Segoe,
    Arial,
    Calibri,
    Georgia,
    Cambria,
}

impl FontId {
    /// Family name as installed (Inter is bundled with the app).
    pub fn family(self) -> &'static str {
        match self {
            Self::Inter => "Inter",
            Self::Segoe => "Segoe UI",
            Self::Arial => "Arial",
            Self::Calibri => "Calibri",
            Self::Georgia => "Georgia",
            Self::Cambria => "Cambria",
        }
    }

    /// Font name for Excel, which uses fonts installed on the reader's computer.
    pub fn excel(self) -> &'static str {
        match self {
            // Inter is rarely installed; Segoe UI is the closest Windows font.
            Self::Inter | Self::Segoe => "Segoe UI",
            other => other.family(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Header {
    pub visible: bool,
    pub layout: HeaderLayout,
    pub style: HeaderStyle,
    pub show_logo: bool,
    /// `data:image/...;base64,...`
    pub logo: Option<String>,
    /// Logo height in points.
    pub logo_pt: f64,
    pub organization: String,
    pub title: String,
    pub subtitle: String,
    pub show_parameters: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderLayout {
    Left,
    Center,
    Split,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderStyle {
    Plain,
    Rule,
    Band,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Table {
    pub header_style: TableHeaderStyle,
    pub striped: bool,
    pub gridlines: Gridlines,
    /// Vertical cell padding in millimetres.
    pub pad_mm: f64,
    pub row_numbers: bool,
    pub totals_row: bool,
    pub totals_label: String,
    pub uppercase_headers: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableHeaderStyle {
    Filled,
    Tinted,
    Underline,
    Plain,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gridlines {
    None,
    Horizontal,
    All,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Formats {
    pub date: DateFormat,
    pub numbers: NumberStyle,
    pub decimals: usize,
    pub negative: NegativeStyle,
    pub currency: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DateFormat {
    DdMmmYyyy,
    Iso,
    DdMmYyyy,
    MmDdYyyy,
    MmmDYyyy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumberStyle {
    Western,
    Indian,
    European,
    Space,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NegativeStyle {
    Minus,
    Parentheses,
    Red,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Footer {
    pub visible: bool,
    pub rule: bool,
    pub left: String,
    pub center: String,
    pub right: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Watermark {
    pub text: String,
    /// Percent, 3-25.
    pub opacity: u32,
}

/// A colour as `#rrggbb` components.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    pub fn parse(hex: &str) -> Option<Self> {
        let h = hex.strip_prefix('#')?;
        if h.len() != 6 || !h.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        let n = u32::from_str_radix(h, 16).ok()?;
        Some(Self((n >> 16) as u8, (n >> 8) as u8, n as u8))
    }

    pub fn hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.0, self.1, self.2)
    }

    pub fn as_u32(self) -> u32 {
        (u32::from(self.0) << 16) | (u32::from(self.1) << 8) | u32::from(self.2)
    }

    /// `percent`% of this colour mixed with white, like CSS `color-mix(… p%, white)`.
    pub fn tint(self, percent: u32) -> Self {
        let p = f64::from(percent.min(100)) / 100.0;
        let mix = |c: u8| (f64::from(c) * p + 255.0 * (1.0 - p)).round() as u8;
        Self(mix(self.0), mix(self.1), mix(self.2))
    }

    fn luminance(self) -> f64 {
        let lin = |c: u8| {
            let c = f64::from(c) / 255.0;
            if c <= 0.03928 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * lin(self.0) + 0.7152 * lin(self.1) + 0.0722 * lin(self.2)
    }

    fn contrast(self, other: Self) -> f64 {
        let (a, b) = (self.luminance(), other.luminance());
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    /// White or near-black, whichever reads better on this colour (as in the preview).
    pub fn readable_text(self) -> Self {
        let white = Self(255, 255, 255);
        let dark = Self(20, 20, 22);
        if self.contrast(white) >= self.contrast(dark) {
            white
        } else {
            dark
        }
    }
}

/* ------------------------------------------------------------------------------------------
 * Reading a definition
 * ---------------------------------------------------------------------------------------- */

struct Section<'a>(Option<&'a Value>);

impl<'a> Section<'a> {
    fn of(root: &'a Value, key: &str) -> Self {
        Self(root.get(key).filter(|v| v.is_object()))
    }
    fn get(&self, key: &str) -> Option<&'a Value> {
        self.0.and_then(|o| o.get(key))
    }
    fn bool(&self, key: &str, default: bool) -> bool {
        self.get(key).and_then(Value::as_bool).unwrap_or(default)
    }
    fn text(&self, key: &str, default: &str, max: usize) -> String {
        match self.get(key).and_then(Value::as_str) {
            Some(s) if s.chars().count() <= max => s.to_owned(),
            _ => default.to_owned(),
        }
    }
    fn one_of<T: Copy>(&self, key: &str, options: &[(&str, T)], default: T) -> T {
        let v = self.get(key).and_then(Value::as_str);
        options
            .iter()
            .find(|(name, _)| Some(*name) == v)
            .map_or(default, |(_, t)| *t)
    }
    fn int(&self, key: &str, min: i64, max: i64, default: i64) -> i64 {
        match self.get(key).and_then(Value::as_i64) {
            Some(n) if (min..=max).contains(&n) => n,
            _ => default,
        }
    }
    fn color(&self, key: &str, default: Rgb) -> Rgb {
        self.get(key)
            .and_then(Value::as_str)
            .and_then(Rgb::parse)
            .unwrap_or(default)
    }
}

const FONTS: &[(&str, FontId)] = &[
    ("inter", FontId::Inter),
    ("segoe", FontId::Segoe),
    ("arial", FontId::Arial),
    ("calibri", FontId::Calibri),
    ("georgia", FontId::Georgia),
    ("cambria", FontId::Cambria),
];

/// Logos must be embedded raster images (the same rule as when templates are saved).
fn valid_logo(s: &str) -> bool {
    [
        "data:image/png;base64,",
        "data:image/jpeg;base64,",
        "data:image/webp;base64,",
    ]
    .iter()
    .any(|p| s.starts_with(p))
        && s.len() <= 1536 * 1024
}

impl Template {
    /// Reads a template definition. Never fails: invalid parts take their defaults.
    pub fn from_definition(def: &Value) -> Self {
        let page = Section::of(def, "page");
        let theme = Section::of(def, "theme");
        let header = Section::of(def, "header");
        let table = Section::of(def, "table");
        let formats = Section::of(def, "formats");
        let footer = Section::of(def, "footer");
        let watermark = Section::of(def, "watermark");

        let logo_px = header.one_of(
            "logoSize",
            &[("small", 30.0), ("medium", 42.0), ("large", 58.0)],
            42.0,
        );
        Self {
            page_size: page.one_of(
                "size",
                &[
                    ("A4", PageSize::A4),
                    ("Letter", PageSize::Letter),
                    ("Legal", PageSize::Legal),
                ],
                PageSize::A4,
            ),
            landscape: page.one_of(
                "orientation",
                &[("portrait", false), ("landscape", true)],
                false,
            ),
            margin_mm: page.one_of(
                "margins",
                &[("narrow", 7.0), ("normal", 11.0), ("wide", 16.0)],
                11.0,
            ),
            accent: theme.color("accent", Rgb(0x1f, 0x3a, 0x5f)),
            text: theme.color("text", Rgb(0x1c, 0x1c, 0x1f)),
            muted: theme.color("muted", Rgb(0x6b, 0x6b, 0x74)),
            font: theme.one_of("font", FONTS, FontId::Inter),
            heading_font: theme.one_of("headingFont", FONTS, FontId::Inter),
            base_size: u32::try_from(theme.int("baseSize", 7, 13, 9)).unwrap_or(9),
            header: Header {
                visible: header.bool("visible", true),
                layout: header.one_of(
                    "layout",
                    &[
                        ("left", HeaderLayout::Left),
                        ("center", HeaderLayout::Center),
                        ("split", HeaderLayout::Split),
                    ],
                    HeaderLayout::Left,
                ),
                style: header.one_of(
                    "style",
                    &[
                        ("plain", HeaderStyle::Plain),
                        ("rule", HeaderStyle::Rule),
                        ("band", HeaderStyle::Band),
                    ],
                    HeaderStyle::Rule,
                ),
                show_logo: header.bool("showLogo", true),
                logo: header
                    .get("logo")
                    .and_then(Value::as_str)
                    .filter(|s| valid_logo(s))
                    .map(str::to_owned),
                // The preview sizes logos in CSS pixels (96 per inch).
                logo_pt: logo_px * 0.75,
                organization: header.text("organization", "Your Company", 120),
                title: header.text("title", "{title}", 160),
                subtitle: header.text("subtitle", "Generated {date}", 200),
                show_parameters: header.bool("showParameters", true),
            },
            table: Table {
                header_style: table.one_of(
                    "headerStyle",
                    &[
                        ("filled", TableHeaderStyle::Filled),
                        ("tinted", TableHeaderStyle::Tinted),
                        ("underline", TableHeaderStyle::Underline),
                        ("plain", TableHeaderStyle::Plain),
                    ],
                    TableHeaderStyle::Filled,
                ),
                striped: table.bool("striped", true),
                gridlines: table.one_of(
                    "gridlines",
                    &[
                        ("none", Gridlines::None),
                        ("horizontal", Gridlines::Horizontal),
                        ("all", Gridlines::All),
                    ],
                    Gridlines::Horizontal,
                ),
                pad_mm: table.one_of(
                    "density",
                    &[("compact", 1.1), ("normal", 1.9), ("relaxed", 2.8)],
                    1.9,
                ),
                row_numbers: table.bool("rowNumbers", false),
                totals_row: table.bool("totalsRow", true),
                totals_label: table.text("totalsLabel", "Total", 40),
                uppercase_headers: table.bool("uppercaseHeaders", false),
            },
            formats: Formats {
                date: formats.one_of(
                    "date",
                    &[
                        ("DD MMM YYYY", DateFormat::DdMmmYyyy),
                        ("YYYY-MM-DD", DateFormat::Iso),
                        ("DD/MM/YYYY", DateFormat::DdMmYyyy),
                        ("MM/DD/YYYY", DateFormat::MmDdYyyy),
                        ("MMM D, YYYY", DateFormat::MmmDYyyy),
                    ],
                    DateFormat::DdMmmYyyy,
                ),
                numbers: formats.one_of(
                    "numbers",
                    &[
                        ("1,234,567.89", NumberStyle::Western),
                        ("12,34,567.89", NumberStyle::Indian),
                        ("1.234.567,89", NumberStyle::European),
                        ("1 234 567,89", NumberStyle::Space),
                    ],
                    NumberStyle::Western,
                ),
                decimals: usize::try_from(formats.int("decimals", 0, 4, 2)).unwrap_or(2),
                negative: formats.one_of(
                    "negative",
                    &[
                        ("minus", NegativeStyle::Minus),
                        ("parentheses", NegativeStyle::Parentheses),
                        ("red", NegativeStyle::Red),
                    ],
                    NegativeStyle::Minus,
                ),
                currency: formats.text("currency", "", 4),
            },
            footer: Footer {
                visible: footer.bool("visible", true),
                rule: footer.one_of("style", &[("plain", false), ("rule", true)], true),
                left: footer.text("left", "{organization} · Confidential", 200),
                center: footer.text("center", "", 200),
                right: footer.text("right", "Page {page} of {pages}", 200),
            },
            watermark: Watermark {
                text: watermark.text("text", "", 30).trim().to_owned(),
                opacity: u32::try_from(watermark.int("opacity", 3, 25, 8)).unwrap_or(8),
            },
        }
    }

    /// Page width and height in millimetres, after orientation.
    pub fn page_mm(&self) -> (f64, f64) {
        let (w, h) = self.page_size.mm();
        if self.landscape { (h, w) } else { (w, h) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn empty_definition_gives_defaults() {
        let t = Template::from_definition(&json!({}));
        assert_eq!(t.page_size, PageSize::A4);
        assert!(!t.landscape);
        assert_eq!(t.margin_mm, 11.0);
        assert_eq!(t.font, FontId::Inter);
        assert_eq!(t.table.header_style, TableHeaderStyle::Filled);
        assert_eq!(t.formats.decimals, 2);
        assert_eq!(t.footer.right, "Page {page} of {pages}");
        assert_eq!(t.header.logo_pt, 31.5);
    }

    #[test]
    fn invalid_values_fall_back() {
        let t = Template::from_definition(&json!({
            "page": {"size": "A3", "orientation": "landscape"},
            "theme": {"accent": "red", "baseSize": 40, "font": "georgia"},
            "header": {"logo": "data:image/svg+xml;base64,AAAA", "title": 5},
            "formats": {"decimals": 9, "negative": "parentheses"},
        }));
        assert_eq!(t.page_size, PageSize::A4);
        assert!(t.landscape);
        assert_eq!(t.page_mm(), (297.0, 210.0));
        assert_eq!(t.accent, Rgb(0x1f, 0x3a, 0x5f));
        assert_eq!(t.base_size, 9);
        assert_eq!(t.font, FontId::Georgia);
        assert!(t.header.logo.is_none());
        assert_eq!(t.header.title, "{title}");
        assert_eq!(t.formats.decimals, 2);
        assert_eq!(t.formats.negative, NegativeStyle::Parentheses);
    }

    #[test]
    fn colours() {
        let navy = Rgb::parse("#1f3a5f").unwrap();
        assert_eq!(navy.readable_text(), Rgb(255, 255, 255));
        assert_eq!(Rgb(250, 250, 250).readable_text(), Rgb(20, 20, 22));
        assert_eq!(Rgb(0, 0, 0).tint(50).hex(), "#808080");
        assert_eq!(navy.tint(100), navy);
        assert!(Rgb::parse("#12345").is_none());
    }
}
