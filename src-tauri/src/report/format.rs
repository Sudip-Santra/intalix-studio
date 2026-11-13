//! How report cells are displayed: a Rust port of `src/features/templates/format.ts`, so the
//! PDF matches the template preview, plus column classification and exact totals.
//!
//! Numbers are formatted from their exact decimal text, never through floating point, so
//! `numeric`, `bigint` and `money` values keep every digit.

use crate::model::{Cell, ResultColumn};
use crate::report::template::{DateFormat, Formats, NegativeStyle, NumberStyle};

/// How a column is shown and whether it is totalled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnKind {
    Text,
    Int,
    /// A numeric column that holds an amount: template decimals and currency symbol.
    Money,
    Decimal,
    Date,
    Timestamp,
    Bool,
}

impl ColumnKind {
    pub fn numeric(self) -> bool {
        matches!(self, Self::Int | Self::Money | Self::Decimal)
    }
}

const MONEY_WORDS: &[&str] = &[
    "amount",
    "amt",
    "price",
    "cost",
    "total",
    "revenue",
    "sales",
    "balance",
    "value",
    "fee",
    "fees",
    "salary",
    "pay",
    "paid",
    "due",
    "tax",
    "net",
    "gross",
    "discount",
    "income",
    "expense",
    "expenses",
    "budget",
    "profit",
    "margin",
    "rate",
    "mrp",
    "invoice_value",
];

/// Words that mark a number as an identifier, code or ratio: adding these up means nothing.
const ID_WORDS: &[&str] = &[
    "id",
    "no",
    "num",
    "number",
    "code",
    "year",
    "month",
    "day",
    "phone",
    "mobile",
    "zip",
    "pin",
    "pincode",
    "postcode",
    "rank",
    "seq",
    "sequence",
    "version",
    "pct",
    "percent",
    "percentage",
    "ratio",
    "rate",
    "avg",
    "average",
    "mean",
    "change",
    "growth",
    "score",
    "rating",
    "age",
];

fn words(name: &str) -> Vec<String> {
    name.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Classifies a result column by PostgreSQL type and, for numbers, by name.
///
/// # Examples
/// - `("int8", "units")` → `Int`
/// - `("numeric", "total_amount")` → `Money`
/// - `("numeric", "change_pct")` → `Decimal`
/// - `("timestamptz", "created_at")` → `Timestamp`
pub fn column_kind(column: &ResultColumn) -> ColumnKind {
    match column.pg_type.as_str() {
        "int2" | "int4" | "int8" | "oid" => ColumnKind::Int,
        "money" => ColumnKind::Money,
        "numeric" => {
            if words(&column.name)
                .iter()
                .any(|w| MONEY_WORDS.contains(&w.as_str()))
            {
                ColumnKind::Money
            } else {
                ColumnKind::Decimal
            }
        }
        "float4" | "float8" => ColumnKind::Decimal,
        "date" => ColumnKind::Date,
        "timestamp" | "timestamptz" => ColumnKind::Timestamp,
        "bool" => ColumnKind::Bool,
        _ => ColumnKind::Text,
    }
}

/// Whether a numeric column gets a total (identifiers, codes and years don't).
pub fn summable(column: &ResultColumn, kind: ColumnKind) -> bool {
    let w = words(&column.name);
    let last = w.last().map(String::as_str).unwrap_or_default();
    // `orderid`, `customerId` are identifiers; `paid` is an amount.
    let glued_id = last.ends_with("id") && !MONEY_WORDS.contains(&last);
    kind.numeric() && !w.iter().any(|x| ID_WORDS.contains(&x.as_str())) && !glued_id
}

/* ------------------------------------------------------------------------------------------
 * Numbers
 * ---------------------------------------------------------------------------------------- */

fn separators(style: NumberStyle) -> (&'static str, &'static str, bool) {
    match style {
        NumberStyle::Western => (",", ".", false),
        NumberStyle::Indian => (",", ".", true),
        NumberStyle::European => (".", ",", false),
        NumberStyle::Space => (" ", ",", false),
    }
}

/// Adds one to a string of digits: `"199"` → `"200"`.
fn increment_digits(digits: &str) -> String {
    let mut out: Vec<u8> = digits.bytes().collect();
    for i in (0..out.len()).rev() {
        if out[i] == b'9' {
            out[i] = b'0';
        } else {
            out[i] += 1;
            return String::from_utf8(out).unwrap_or_default();
        }
    }
    format!("1{}", String::from_utf8(out).unwrap_or_default())
}

/// Rounds an unsigned decimal string half away from zero.
///
/// # Examples
/// - `("12.345", 2)` → `("12", "35")`
/// - `("9.995", 2)` → `("10", "00")`
pub fn round_decimal(abs: &str, decimals: usize) -> (String, String) {
    let (raw_int, raw_frac) = abs.split_once('.').unwrap_or((abs, ""));
    let trimmed = raw_int.trim_start_matches('0');
    let int = if trimmed.is_empty() { "0" } else { trimmed };
    if raw_frac.len() <= decimals {
        return (int.to_owned(), format!("{raw_frac:0<decimals$}"));
    }
    let kept = format!("{int}{}", &raw_frac[..decimals]);
    let round_up = raw_frac.as_bytes()[decimals] >= b'5';
    let rounded = if round_up {
        increment_digits(&kept)
    } else {
        kept
    };
    let int_len = rounded.len() - decimals;
    let int = if int_len == 0 {
        "0".to_owned()
    } else {
        rounded[..int_len].to_owned()
    };
    (int, rounded[int_len..].to_owned())
}

/// Groups integer digits: `"1234567"` → `"1,234,567"`, or `"12,34,567"` for Indian grouping.
pub fn group_digits(int: &str, separator: &str, indian: bool) -> String {
    if int.len() <= 3 {
        return int.to_owned();
    }
    let (rest, last3) = int.split_at(int.len() - 3);
    let size = if indian { 2 } else { 3 };
    let mut groups: Vec<&str> = Vec::new();
    let mut end = rest.len();
    while end > 0 {
        let start = end.saturating_sub(size);
        groups.insert(0, &rest[start..end]);
        end = start;
    }
    groups.push(last3);
    groups.join(separator)
}

/// A formatted value and whether it is negative (for red negatives).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Formatted {
    pub text: String,
    pub negative: bool,
}

impl Formatted {
    fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            negative: false,
        }
    }
}

/// `true` for `-123`, `45.60`, `0.5` (what [`format_decimal`] formats).
pub fn is_plain_decimal(s: &str) -> bool {
    let body = s.strip_prefix('-').unwrap_or(s);
    let (int, frac) = body.split_once('.').unwrap_or((body, "0"));
    !int.is_empty()
        && !frac.is_empty()
        && int.bytes().all(|b| b.is_ascii_digit())
        && frac.bytes().all(|b| b.is_ascii_digit())
}

/// Formats an exact decimal string; anything else (NaN, Infinity) is returned unchanged.
///
/// # Examples
/// - `"-1234.5"`, Western, 2 decimals, parentheses → `"(1,234.50)"`
/// - `"1234567.891"`, Indian, 2 decimals, currency `₹` → `"₹12,34,567.89"`
pub fn format_decimal(value: &str, f: &Formats, decimals: usize, currency: &str) -> Formatted {
    let v = value.trim();
    if !is_plain_decimal(v) {
        return Formatted::plain(value);
    }
    let is_negative = v.starts_with('-');
    let (int, frac) = round_decimal(v.trim_start_matches('-'), decimals);
    // "-0.00" after rounding is just zero.
    let negative = is_negative && format!("{int}{frac}").bytes().any(|b| b != b'0');
    let (group, decimal, indian) = separators(f.numbers);
    let mut body = group_digits(&int, group, indian);
    if !frac.is_empty() {
        body.push_str(decimal);
        body.push_str(&frac);
    }
    let money = format!("{currency}{body}");
    if !negative {
        return Formatted::plain(money);
    }
    Formatted {
        text: if f.negative == NegativeStyle::Parentheses {
            format!("({money})")
        } else {
            format!("-{money}")
        },
        negative: true,
    }
}

/* ------------------------------------------------------------------------------------------
 * Dates
 * ---------------------------------------------------------------------------------------- */

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// `(year, month, day)` from text starting with `YYYY-MM-DD`.
pub fn ymd(iso: &str) -> Option<(i32, u8, u8)> {
    let b = iso.as_bytes();
    if b.len() < 10 || b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    let y = iso.get(0..4)?.parse().ok()?;
    let m: u8 = iso.get(5..7)?.parse().ok()?;
    let d: u8 = iso.get(8..10)?.parse().ok()?;
    ((1..=12).contains(&m) && (1..=31).contains(&d)).then_some((y, m, d))
}

/// Formats `YYYY-MM-DD…` without time-zone shifts; other text is returned unchanged.
///
/// # Examples
/// - `("2026-03-07", DdMmmYyyy)` → `"07 Mar 2026"`
/// - `("2026-03-07", MmmDYyyy)` → `"Mar 7, 2026"`
pub fn format_date(iso: &str, format: DateFormat) -> String {
    let Some((y, m, d)) = ymd(iso) else {
        return iso.to_owned();
    };
    let month = MONTHS[usize::from(m - 1)];
    match format {
        DateFormat::DdMmmYyyy => format!("{d:02} {month} {y}"),
        DateFormat::Iso => format!("{y}-{m:02}-{d:02}"),
        DateFormat::DdMmYyyy => format!("{d:02}/{m:02}/{y}"),
        DateFormat::MmDdYyyy => format!("{m:02}/{d:02}/{y}"),
        DateFormat::MmmDYyyy => format!("{month} {d}, {y}"),
    }
}

/// `2026-03-07 13:05:09.25+00` → date in the template format plus `13:05`.
pub fn format_timestamp(value: &str, format: DateFormat) -> String {
    let date = format_date(value, format);
    match value.get(11..16) {
        Some(hm) if ymd(value).is_some() && hm.as_bytes().get(2) == Some(&b':') => {
            format!("{date} {hm}")
        }
        _ => date,
    }
}

/* ------------------------------------------------------------------------------------------
 * Cells
 * ---------------------------------------------------------------------------------------- */

/// The exact decimal text of a numeric cell, if it has one.
pub fn decimal_text(cell: &Cell) -> Option<String> {
    match cell {
        Cell::Int { v } | Cell::Decimal { v } => Some(v.clone()),
        Cell::Float { v } if v.is_finite() => Some(v.to_string()),
        _ => None,
    }
}

/// Display text for a cell in the report.
pub fn format_cell(cell: &Cell, kind: ColumnKind, f: &Formats) -> Formatted {
    match cell {
        Cell::Null => Formatted::plain(""),
        Cell::Bool { v } => Formatted::plain(if *v { "Yes" } else { "No" }),
        Cell::Bytes { size } => Formatted::plain(format!("({size} bytes)")),
        Cell::Unsupported { pg_type } => Formatted::plain(format!("({pg_type})")),
        Cell::Date { v } => Formatted::plain(format_date(v, f.date)),
        Cell::Timestamp { v } | Cell::Timestamptz { v } => {
            Formatted::plain(format_timestamp(v, f.date))
        }
        Cell::Text { v, .. } | Cell::Json { v, .. } => Formatted::plain(v.clone()),
        Cell::Time { v } | Cell::Uuid { v } => Formatted::plain(v.clone()),
        Cell::Int { .. } | Cell::Decimal { .. } | Cell::Float { .. } => {
            let text = decimal_text(cell).unwrap_or_default();
            match kind {
                ColumnKind::Int => format_decimal(&text, f, 0, ""),
                ColumnKind::Money => format_decimal(&text, f, f.decimals, &f.currency),
                // A whole-number column that isn't an integer type still shows decimals.
                _ => format_decimal(&text, f, f.decimals, ""),
            }
        }
    }
}

/* ------------------------------------------------------------------------------------------
 * Exact totals
 * ---------------------------------------------------------------------------------------- */

/// An exact running sum of decimal strings (up to 38 significant digits). Overflow or a
/// non-numeric value (NaN, Infinity) makes the total unavailable rather than wrong.
#[derive(Debug, Clone, Default)]
pub struct Sum {
    mantissa: i128,
    scale: u32,
    broken: bool,
    any: bool,
}

impl Sum {
    pub fn add(&mut self, value: &str) {
        if self.broken {
            return;
        }
        let v = value.trim();
        if !is_plain_decimal(v) {
            self.broken = true;
            return;
        }
        let negative = v.starts_with('-');
        let body = v.trim_start_matches('-');
        let (int, frac) = body.split_once('.').unwrap_or((body, ""));
        let frac = frac.trim_end_matches('0');
        let Ok(scale) = u32::try_from(frac.len()) else {
            self.broken = true;
            return;
        };
        let digits = format!("{int}{frac}");
        let Ok(mut m) = digits.parse::<i128>() else {
            self.broken = true;
            return;
        };
        if negative {
            m = -m;
        }
        // Bring both to the larger scale.
        let target = self.scale.max(scale);
        let rescale = |x: i128, from: u32| -> Option<i128> {
            10i128
                .checked_pow(target - from)
                .and_then(|p| x.checked_mul(p))
        };
        match (rescale(self.mantissa, self.scale), rescale(m, scale)) {
            (Some(a), Some(b)) => match a.checked_add(b) {
                Some(total) => {
                    self.mantissa = total;
                    self.scale = target;
                    self.any = true;
                }
                None => self.broken = true,
            },
            _ => self.broken = true,
        }
    }

    /// The total as decimal text, or `None` when unavailable or nothing was added.
    pub fn text(&self) -> Option<String> {
        if self.broken || !self.any {
            return None;
        }
        let negative = self.mantissa < 0;
        let digits = self.mantissa.unsigned_abs().to_string();
        let scale = self.scale as usize;
        let padded = format!("{digits:0>width$}", width = scale + 1);
        let (int, frac) = padded.split_at(padded.len() - scale);
        let sign = if negative { "-" } else { "" };
        Some(if frac.is_empty() {
            format!("{sign}{int}")
        } else {
            format!("{sign}{int}.{frac}")
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::template::Template;
    use serde_json::json;

    fn formats(numbers: &str, negative: &str) -> Formats {
        Template::from_definition(&json!({"formats": {"numbers": numbers, "negative": negative}}))
            .formats
    }

    #[test]
    fn rounds_and_groups_like_the_preview() {
        assert_eq!(round_decimal("12.345", 2), ("12".into(), "35".into()));
        assert_eq!(round_decimal("9.995", 2), ("10".into(), "00".into()));
        assert_eq!(round_decimal("0.004", 2), ("0".into(), "00".into()));
        assert_eq!(round_decimal("007", 0), ("7".into(), String::new()));
        assert_eq!(group_digits("1234567", ",", false), "1,234,567");
        assert_eq!(group_digits("1234567", ",", true), "12,34,567");
        assert_eq!(group_digits("123", ",", true), "123");
    }

    #[test]
    fn formats_decimals_exactly() {
        let w = formats("1,234,567.89", "parentheses");
        assert_eq!(
            format_decimal("-1234.5", &w, 2, ""),
            Formatted {
                text: "(1,234.50)".into(),
                negative: true
            }
        );
        let i = formats("12,34,567.89", "minus");
        assert_eq!(
            format_decimal("1234567.891", &i, 2, "₹").text,
            "₹12,34,567.89"
        );
        let e = formats("1.234.567,89", "minus");
        assert_eq!(
            format_decimal("-1234567.5", &e, 2, "").text,
            "-1.234.567,50"
        );
        // Huge exact values keep every digit.
        assert_eq!(
            format_decimal("12345678901234567890.12", &w, 2, "").text,
            "12,345,678,901,234,567,890.12"
        );
        assert!(!format_decimal("-0.001", &w, 2, "").negative);
        assert_eq!(format_decimal("NaN", &w, 2, "").text, "NaN");
    }

    #[test]
    fn formats_dates() {
        assert_eq!(
            format_date("2026-03-07", DateFormat::DdMmmYyyy),
            "07 Mar 2026"
        );
        assert_eq!(
            format_date("2026-03-07", DateFormat::MmmDYyyy),
            "Mar 7, 2026"
        );
        assert_eq!(
            format_date("2026-03-07", DateFormat::DdMmYyyy),
            "07/03/2026"
        );
        assert_eq!(format_date("infinity", DateFormat::Iso), "infinity");
        assert_eq!(
            format_timestamp("2026-01-31 13:05:09.25+00", DateFormat::DdMmmYyyy),
            "31 Jan 2026 13:05"
        );
    }

    #[test]
    fn classifies_columns() {
        let col = |name: &str, t: &str| ResultColumn {
            name: name.into(),
            pg_type: t.into(),
        };
        assert_eq!(column_kind(&col("units", "int8")), ColumnKind::Int);
        assert_eq!(
            column_kind(&col("total_amount", "numeric")),
            ColumnKind::Money
        );
        assert_eq!(
            column_kind(&col("change_pct", "numeric")),
            ColumnKind::Decimal
        );
        assert_eq!(
            column_kind(&col("created_at", "timestamptz")),
            ColumnKind::Timestamp
        );
        assert_eq!(column_kind(&col("name", "text")), ColumnKind::Text);
        assert!(summable(&col("units", "int8"), ColumnKind::Int));
        assert!(!summable(&col("customer_id", "int8"), ColumnKind::Int));
        assert!(!summable(&col("orderid", "int8"), ColumnKind::Int));
        assert!(summable(&col("paid", "numeric"), ColumnKind::Money));
        assert!(!summable(
            &col("change_pct", "numeric"),
            ColumnKind::Decimal
        ));
        assert!(!summable(&col("avg_price", "numeric"), ColumnKind::Money));
        assert!(!summable(&col("Year", "int4"), ColumnKind::Int));
        assert!(!summable(&col("name", "text"), ColumnKind::Text));
    }

    #[test]
    fn sums_exactly() {
        let mut s = Sum::default();
        for v in ["0.1", "0.2", "-0.05", "100"] {
            s.add(v);
        }
        assert_eq!(s.text().as_deref(), Some("100.25"));
        let mut neg = Sum::default();
        neg.add("-0.5");
        assert_eq!(neg.text().as_deref(), Some("-0.5"));
        let mut big = Sum::default();
        // i128 holds about 1.7 × 10^38.
        big.add("100000000000000000000000000000000000000");
        big.add("100000000000000000000000000000000000000");
        assert_eq!(big.text(), None);
        let mut nan = Sum::default();
        nan.add("NaN");
        assert_eq!(nan.text(), None);
        assert_eq!(Sum::default().text(), None);
    }
}
