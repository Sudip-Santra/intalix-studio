//! Fits the PDF table to the page width.
//!
//! Each column starts from the width its values need. When the columns don't fit, they shrink
//! towards a minimum, and then the table text gets smaller (down to 6 pt). Text that still
//! doesn't fit wraps: long words get break points, so nothing spills into the next column, and
//! very long text is shortened to a few lines (the Excel file keeps it in full).

use super::format::ColumnKind;

/// Millimetres per point.
const PT_MM: f64 = 0.352_778;
/// Smallest table text size, in points.
pub const MIN_SIZE: f64 = 6.0;
/// The table text never shrinks more than this below the template size just to avoid wrapping.
const SOFT_SHRINK: f64 = 1.5;
/// Most lines a text cell shows in the PDF.
const MAX_LINES: usize = 6;
/// Typical text is given up to this many characters per line before it wraps.
const TEXT_NATURAL_CAP: usize = 36;
/// A break point that shows nothing (for values).
const ZWSP: char = '\u{200B}';
/// A break point that shows a hyphen when used (for headings).
const SHY: char = '\u{00AD}';

/// Average character width as a share of the font size.
fn char_em(kind: ColumnKind) -> f64 {
    match kind {
        ColumnKind::Text | ColumnKind::Bool => 0.55,
        _ => 0.6,
    }
}
const HEADER_EM: f64 = 0.62;

/// Narrowest character width as a share of the font size. A word is only split when even
/// narrow letters couldn't fit it on one line.
const NARROW_EM: f64 = 0.45;

/// What a column's values need.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnStats {
    pub kind: ColumnKind,
    /// Longest word of the heading.
    pub header_word: usize,
    /// Length most values stay within (90th percentile).
    pub typical: usize,
    pub longest: usize,
}

impl ColumnStats {
    pub fn measure<'a>(
        kind: ColumnKind,
        label: &str,
        values: impl Iterator<Item = &'a str>,
    ) -> Self {
        let mut lengths: Vec<usize> = values.map(|v| v.chars().count()).collect();
        lengths.sort_unstable();
        let longest = lengths.last().copied().unwrap_or(0);
        let typical = if lengths.is_empty() {
            0
        } else {
            lengths[(lengths.len() - 1) * 9 / 10]
        };
        Self {
            kind,
            header_word: label
                .split_whitespace()
                .map(|w| w.chars().count())
                .max()
                .unwrap_or(0),
            typical,
            longest,
        }
    }

    /// The heading may break inside a long word (with a hyphen).
    fn header_min(&self) -> usize {
        self.header_word.min(7).max(self.header_word.div_ceil(2))
    }

    /// (value characters, heading characters) per line at the natural width.
    fn natural(&self) -> (usize, usize) {
        let content = match self.kind {
            ColumnKind::Text => self.typical.min(TEXT_NATURAL_CAP),
            _ => self.longest,
        };
        (content.max(2), self.header_word)
    }

    /// (value characters, heading characters) per line at the narrowest width.
    fn minimum(&self) -> (usize, usize) {
        let content = match self.kind {
            // Numbers and dates never wrap.
            ColumnKind::Int | ColumnKind::Money | ColumnKind::Decimal | ColumnKind::Date => {
                self.longest
            }
            // "30 Nov 2025" stays on one line; the time may go below it.
            ColumnKind::Timestamp => self.longest.min(11),
            ColumnKind::Bool => self.longest.min(5),
            ColumnKind::Text => self.typical.min(8),
        };
        (content.max(2), self.header_min())
    }
}

/// How one column is laid out.
#[derive(Debug, Clone, PartialEq)]
pub struct ColumnFit {
    pub width_mm: f64,
    /// Value characters that fit on one line (average letters).
    pub line_chars: usize,
    /// Longest word that may still fit on one line (narrow letters); longer words are split.
    pub word_chars: usize,
    /// Heading characters that fit on one line.
    pub header_chars: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TableFit {
    /// Table text size in points.
    pub size: f64,
    /// Horizontal cell padding in millimetres (each side).
    pub pad_x: f64,
    pub columns: Vec<ColumnFit>,
    /// Even numbers and dates may break (the columns don't fit at the smallest size).
    pub break_all: bool,
}

fn width_mm(chars: (usize, usize), kind: ColumnKind, size: f64, pad_x: f64) -> f64 {
    let content = chars.0 as f64 * char_em(kind);
    let header = chars.1 as f64 * HEADER_EM;
    content.max(header) * size * PT_MM + 2.0 * pad_x
}

/// Column widths and text size for a table `available_mm` wide.
///
/// `row_digits` is the width of the row-number column in digits, when it is shown.
pub fn fit(
    stats: &[ColumnStats],
    available_mm: f64,
    base_size: f64,
    row_digits: Option<usize>,
) -> TableFit {
    let pad_x = if stats.len() > 8 { 1.4 } else { 2.0 };
    let numbers =
        |size: f64| row_digits.map_or(0.0, |d| d.max(1) as f64 * 0.6 * size * PT_MM + 2.0 * pad_x);
    let total = |size: f64, natural: bool| -> f64 {
        numbers(size)
            + stats
                .iter()
                .map(|c| {
                    let chars = if natural { c.natural() } else { c.minimum() };
                    width_mm(chars, c.kind, size, pad_x)
                })
                .sum::<f64>()
    };
    let mut sizes = Vec::new();
    let mut s = base_size;
    while s >= MIN_SIZE - 1e-9 {
        sizes.push(s);
        s -= 0.5;
    }
    if sizes.is_empty() {
        sizes.push(base_size);
    }

    let soft = sizes
        .iter()
        .copied()
        .find(|&s| s >= base_size - SOFT_SHRINK - 1e-9 && total(s, true) <= available_mm);
    let (size, break_all) = match soft {
        Some(s) => (s, false),
        None => match sizes
            .iter()
            .copied()
            .find(|&s| total(s, false) <= available_mm)
        {
            Some(s) => (s, false),
            None => (*sizes.last().unwrap_or(&base_size), true),
        },
    };

    let room = available_mm - numbers(size);
    let mins: Vec<f64> = stats
        .iter()
        .map(|c| width_mm(c.minimum(), c.kind, size, pad_x))
        .collect();
    let nats: Vec<f64> = stats
        .iter()
        .map(|c| {
            width_mm(c.natural(), c.kind, size, pad_x).max(width_mm(
                c.minimum(),
                c.kind,
                size,
                pad_x,
            ))
        })
        .collect();
    let sum_min: f64 = mins.iter().sum();
    let sum_nat: f64 = nats.iter().sum();
    let widths: Vec<f64> = if sum_nat <= room {
        // Everything fits: spread the spare room in proportion.
        let scale = if sum_nat > 0.0 { room / sum_nat } else { 1.0 };
        nats.iter().map(|w| w * scale).collect()
    } else if sum_min <= room {
        // Give each column its minimum, then share the rest by how much more it wants.
        let share = (room - sum_min) / (sum_nat - sum_min);
        mins.iter()
            .zip(&nats)
            .map(|(lo, hi)| lo + (hi - lo) * share)
            .collect()
    } else {
        let scale = if sum_min > 0.0 { room / sum_min } else { 1.0 };
        mins.iter().map(|w| w * scale).collect()
    };

    let columns = stats
        .iter()
        .zip(widths)
        .map(|(c, w)| {
            let inner = (w - 2.0 * pad_x).max(0.5);
            let per_char = |em: f64| ((inner / (em * size * PT_MM)).floor() as usize).max(1);
            ColumnFit {
                width_mm: w,
                line_chars: per_char(char_em(c.kind)),
                word_chars: per_char(NARROW_EM),
                header_chars: per_char(HEADER_EM),
            }
        })
        .collect();
    TableFit {
        size,
        pad_x,
        columns,
        break_all,
    }
}

/// Inserts `mark` between the characters of every word longer than `limit`.
///
/// # Examples
/// - `("ab abcdef", 4, '|')` → `"ab a|b|c|d|e|f"`
fn breakable(text: &str, limit: usize, mark: char) -> String {
    let mut out = String::with_capacity(text.len() + 8);
    for (i, word) in text.split(' ').enumerate() {
        if i > 0 {
            out.push(' ');
        }
        if word.chars().count() > limit {
            for (j, ch) in word.chars().enumerate() {
                if j > 0 {
                    out.push(mark);
                }
                out.push(ch);
            }
        } else {
            out.push_str(word);
        }
    }
    out
}

/// A value ready for the PDF: shortened to a few lines and given break points.
/// Returns the text and whether it was shortened.
///
/// # Examples
/// - a 600-character description in a 30-character column → its first 119 characters + "…"
/// - `"C1=CC=CC=C1N"` in a 6-character column → break points between the characters
pub fn cell_text(text: &str, kind: ColumnKind, col: &ColumnFit, break_all: bool) -> (String, bool) {
    if kind != ColumnKind::Text && !break_all {
        return (text.to_owned(), false);
    }
    let limit = (col.line_chars * MAX_LINES).max(20);
    let (text, shortened) = if kind == ColumnKind::Text && text.chars().count() > limit {
        let cut: String = text.chars().take(limit - 1).collect();
        (format!("{}…", cut.trim_end()), true)
    } else {
        (text.to_owned(), false)
    };
    (breakable(&text, col.word_chars, ZWSP), shortened)
}

/// A column heading. The layout hyphenates long words; words far too long for that also get
/// break points (shown with a hyphen).
pub fn header_text(label: &str, col: &ColumnFit) -> String {
    breakable(label, col.header_chars * 2, SHY)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stats(kind: ColumnKind, label: &str, values: &[&str]) -> ColumnStats {
        ColumnStats::measure(kind, label, values.iter().copied())
    }

    #[test]
    fn measures_columns() {
        let s = stats(
            ColumnKind::Text,
            "Customer Name",
            &["a", "abc", "abcdefghij"],
        );
        assert_eq!(s.header_word, 8);
        assert_eq!(s.longest, 10);
        assert_eq!(s.typical, 3);
    }

    #[test]
    fn few_columns_keep_the_template_size() {
        let cols = vec![
            stats(ColumnKind::Text, "Customer", &["Northwind Traders"]),
            stats(ColumnKind::Money, "Amount", &["₹ 12,34,567.00"]),
        ];
        let f = fit(&cols, 180.0, 9.0, None);
        assert_eq!(f.size, 9.0);
        assert!(!f.break_all);
        let total: f64 = f.columns.iter().map(|c| c.width_mm).sum();
        assert!((total - 180.0).abs() < 0.01, "{total}");
    }

    #[test]
    fn many_columns_shrink_but_numbers_stay_whole() {
        let long = "x".repeat(400);
        let mut cols: Vec<ColumnStats> = (0..20)
            .map(|i| {
                stats(
                    ColumnKind::Text,
                    &format!("Column {i}"),
                    &[long.as_str(), "short value"],
                )
            })
            .collect();
        cols.push(stats(ColumnKind::Money, "Amount", &["1,23,45,678.00"]));
        let f = fit(&cols, 277.0, 9.0, Some(4));
        assert!(f.size < 9.0);
        assert!(!f.break_all);
        let amount = f.columns.last().unwrap();
        assert!(amount.line_chars >= 14, "{amount:?}");
        let total: f64 = f.columns.iter().map(|c| c.width_mm).sum();
        assert!(total <= 277.0, "{total}");
    }

    #[test]
    fn impossible_tables_break_everything() {
        let cols: Vec<ColumnStats> = (0..60)
            .map(|_| stats(ColumnKind::Money, "Amount", &["1,23,45,678.00"]))
            .collect();
        let f = fit(&cols, 190.0, 9.0, None);
        assert_eq!(f.size, MIN_SIZE);
        assert!(f.break_all);
    }

    #[test]
    fn long_text_is_shortened_and_breakable() {
        let col = ColumnFit {
            width_mm: 20.0,
            line_chars: 6,
            word_chars: 7,
            header_chars: 5,
        };
        let (text, short) = cell_text(&"word ".repeat(40), ColumnKind::Text, &col, false);
        assert!(short);
        assert!(text.ends_with('…'));
        // 35 characters, less the trailing space, plus "…".
        assert_eq!(text.chars().count(), 35);

        let (smiles, short) = cell_text("C1=CC=CC=C1N", ColumnKind::Text, &col, false);
        assert!(!short);
        assert_eq!(smiles.replace(ZWSP, ""), "C1=CC=CC=C1N");
        assert!(smiles.contains(ZWSP));

        let (n, _) = cell_text("1,23,45,678.00", ColumnKind::Money, &col, false);
        assert_eq!(n, "1,23,45,678.00");
        assert_eq!(
            header_text("Temperature", &col).replace(SHY, ""),
            "Temperature"
        );
        assert_eq!(header_text("Status", &col), "Status");
        assert_eq!(breakable("ab abcdef", 4, '|'), "ab a|b|c|d|e|f");
    }
}
