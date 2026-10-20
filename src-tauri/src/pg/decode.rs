//! Exact decoding of PostgreSQL binary values into [`Cell`]s.
//!
//! We read the raw wire bytes ourselves instead of converting through Rust number/date types,
//! so `NUMERIC` keeps every digit, `infinity` dates survive, and nothing is silently rounded.
//! Any type we don't understand becomes `Cell::Unsupported` with its name, never a wrong value.

use std::error::Error;
use std::net::{Ipv4Addr, Ipv6Addr};

use tokio_postgres::types::{FromSql, Kind, Type};

use crate::model::Cell;

/// Longest text/JSON value sent to the grid; longer values are cut and flagged as truncated.
pub const MAX_CELL_BYTES: usize = 64 * 1024;

/// Accepts any column type and hands back the raw bytes for [`decode`].
pub struct Raw<'a>(pub Option<&'a [u8]>);

impl<'a> FromSql<'a> for Raw<'a> {
    fn from_sql(_: &Type, raw: &'a [u8]) -> Result<Self, Box<dyn Error + Sync + Send>> {
        Ok(Raw(Some(raw)))
    }
    fn from_sql_null(_: &Type) -> Result<Self, Box<dyn Error + Sync + Send>> {
        Ok(Raw(None))
    }
    fn accepts(_: &Type) -> bool {
        true
    }
}

/// Decodes one value. Malformed bytes produce `Unsupported` rather than an error so a single
/// odd value never fails a whole result set.
pub fn decode(ty: &Type, raw: Option<&[u8]>) -> Cell {
    let Some(bytes) = raw else { return Cell::Null };
    decode_value(ty, bytes).unwrap_or_else(|| Cell::Unsupported {
        pg_type: ty.name().to_owned(),
    })
}

fn decode_value(ty: &Type, b: &[u8]) -> Option<Cell> {
    match ty.kind() {
        Kind::Enum(_) => return Some(text(b)),
        Kind::Domain(inner) => return decode_value(inner, b),
        Kind::Array(elem) => return array_text(elem, b).map(|v| truncate_text(v, false)),
        _ => {}
    }
    let cell = match *ty {
        Type::BOOL => Cell::Bool {
            v: *b.first()? != 0,
        },
        Type::INT2 => Cell::Int {
            v: i16::from_be_bytes(b.try_into().ok()?).to_string(),
        },
        Type::INT4 => Cell::Int {
            v: i32::from_be_bytes(b.try_into().ok()?).to_string(),
        },
        Type::INT8 => Cell::Int {
            v: i64::from_be_bytes(b.try_into().ok()?).to_string(),
        },
        Type::OID | Type::REGCLASS | Type::REGTYPE | Type::REGPROC | Type::XID => Cell::Int {
            v: u32::from_be_bytes(b.try_into().ok()?).to_string(),
        },
        Type::FLOAT4 => float(f64::from(f32::from_be_bytes(b.try_into().ok()?))),
        Type::FLOAT8 => float(f64::from_be_bytes(b.try_into().ok()?)),
        Type::NUMERIC => Cell::Decimal { v: numeric(b)? },
        // money is a 64-bit count of the smallest currency unit; two fraction digits is the
        // PostgreSQL default (lc_monetary). Shown as an exact decimal.
        Type::MONEY => Cell::Decimal {
            v: scaled_i64(i64::from_be_bytes(b.try_into().ok()?), 2),
        },
        Type::TEXT | Type::VARCHAR | Type::BPCHAR | Type::NAME | Type::UNKNOWN | Type::XML => {
            text(b)
        }
        Type::CHAR => text(b),
        Type::DATE => Cell::Date {
            v: date(i32::from_be_bytes(b.try_into().ok()?)),
        },
        Type::TIME => Cell::Time {
            v: time_of_day(i64::from_be_bytes(b.try_into().ok()?)),
        },
        Type::TIMETZ => {
            let micros = i64::from_be_bytes(b.get(0..8)?.try_into().ok()?);
            // PostgreSQL stores the zone as seconds WEST of UTC.
            let west = i32::from_be_bytes(b.get(8..12)?.try_into().ok()?);
            Cell::Time {
                v: format!("{}{}", time_of_day(micros), offset(-west)),
            }
        }
        Type::TIMESTAMP => Cell::Timestamp {
            v: timestamp(i64::from_be_bytes(b.try_into().ok()?)),
        },
        Type::TIMESTAMPTZ => {
            let ts = i64::from_be_bytes(b.try_into().ok()?);
            let v = timestamp(ts);
            Cell::Timestamptz {
                v: if is_infinite_ts(ts) {
                    v
                } else {
                    format!("{v}+00")
                },
            }
        }
        Type::INTERVAL => Cell::Text {
            v: interval(b)?,
            truncated: false,
        },
        Type::UUID => Cell::Uuid { v: uuid(b)? },
        Type::JSON => json(b),
        // jsonb binary format: 1 version byte, then the JSON text.
        Type::JSONB => {
            if *b.first()? != 1 {
                return None;
            }
            json(&b[1..])
        }
        Type::BYTEA => Cell::Bytes { size: b.len() },
        Type::INET | Type::CIDR => Cell::Text {
            v: inet(b)?,
            truncated: false,
        },
        _ if ty.name() == "citext" => text(b),
        _ => return None,
    };
    Some(cell)
}

fn float(v: f64) -> Cell {
    if v.is_finite() {
        Cell::Float { v }
    } else if v.is_nan() {
        Cell::Decimal { v: "NaN".into() }
    } else if v > 0.0 {
        Cell::Decimal {
            v: "Infinity".into(),
        }
    } else {
        Cell::Decimal {
            v: "-Infinity".into(),
        }
    }
}

fn text(b: &[u8]) -> Cell {
    truncate_text(String::from_utf8_lossy(b).into_owned(), false)
}

fn json(b: &[u8]) -> Cell {
    match truncate_text(String::from_utf8_lossy(b).into_owned(), true) {
        Cell::Text { v, truncated } => Cell::Json { v, truncated },
        other => other,
    }
}

/// Cuts long strings at a character boundary so huge values can't flood the UI.
fn truncate_text(mut s: String, _json: bool) -> Cell {
    if s.len() <= MAX_CELL_BYTES {
        return Cell::Text {
            v: s,
            truncated: false,
        };
    }
    let mut cut = MAX_CELL_BYTES;
    while !s.is_char_boundary(cut) {
        cut -= 1;
    }
    s.truncate(cut);
    Cell::Text {
        v: s,
        truncated: true,
    }
}

/* ---------------------------------------------------------------------------------------- */
/* NUMERIC                                                                                   */
/* ---------------------------------------------------------------------------------------- */

const NUMERIC_POS: u16 = 0x0000;
const NUMERIC_NEG: u16 = 0x4000;
const NUMERIC_NAN: u16 = 0xC000;
const NUMERIC_PINF: u16 = 0xD000;
const NUMERIC_NINF: u16 = 0xF000;

/// Decodes PostgreSQL's base-10000 NUMERIC wire format into an exact decimal string.
///
/// # Examples (see tests)
/// - digits `[1, 2345]`, weight 1, dscale 2 → `"12345.00"`
/// - digits `[1234]`, weight -1, dscale 4, negative → `"-0.1234"`
fn numeric(b: &[u8]) -> Option<String> {
    let u16_at = |i: usize| {
        b.get(i..i + 2)
            .and_then(|s| s.try_into().ok())
            .map(u16::from_be_bytes)
    };
    let i16_at = |i: usize| {
        b.get(i..i + 2)
            .and_then(|s| s.try_into().ok())
            .map(i16::from_be_bytes)
    };
    let ndigits = usize::try_from(i16_at(0)?).ok()?;
    let weight = i32::from(i16_at(2)?);
    let sign = u16_at(4)?;
    let dscale = usize::from(u16_at(6)?);

    match sign {
        NUMERIC_NAN => return Some("NaN".into()),
        NUMERIC_PINF => return Some("Infinity".into()),
        NUMERIC_NINF => return Some("-Infinity".into()),
        NUMERIC_POS | NUMERIC_NEG => {}
        _ => return None,
    }

    let mut digits = Vec::with_capacity(ndigits);
    for k in 0..ndigits {
        let d = u16_at(8 + k * 2)?;
        if d > 9999 {
            return None;
        }
        digits.push(d);
    }

    // Integer part: base-10000 groups 0..=weight.
    let mut int_part = String::new();
    if weight >= 0 {
        for g in 0..=weight {
            let d = usize::try_from(g)
                .ok()
                .and_then(|g| digits.get(g))
                .copied()
                .unwrap_or(0);
            if int_part.is_empty() {
                int_part.push_str(&d.to_string());
            } else {
                int_part.push_str(&format!("{d:04}"));
            }
        }
    }
    if int_part.is_empty() {
        int_part.push('0');
    }

    // Fraction part: groups after the weight, then cut/pad to dscale decimal digits.
    let mut frac = String::new();
    if dscale > 0 {
        let first_frac_group = weight + 1;
        let mut g = first_frac_group;
        while frac.len() < dscale {
            let d = if g < 0 {
                0
            } else {
                usize::try_from(g)
                    .ok()
                    .and_then(|g| digits.get(g))
                    .copied()
                    .unwrap_or(0)
            };
            frac.push_str(&format!("{d:04}"));
            g += 1;
        }
        frac.truncate(dscale);
    }

    let negative = sign == NUMERIC_NEG && (int_part != "0" || frac.chars().any(|c| c != '0'));
    let mut out = String::new();
    if negative {
        out.push('-');
    }
    out.push_str(&int_part);
    if dscale > 0 {
        out.push('.');
        out.push_str(&frac);
    }
    Some(out)
}

/// `value / 10^scale` as an exact decimal string.
fn scaled_i64(value: i64, scale: u32) -> String {
    let negative = value < 0;
    let abs = value.unsigned_abs();
    let divisor = 10u64.pow(scale);
    let s = format!(
        "{}.{:0width$}",
        abs / divisor,
        abs % divisor,
        width = scale as usize
    );
    if negative { format!("-{s}") } else { s }
}

/* ---------------------------------------------------------------------------------------- */
/* Dates and times                                                                           */
/* ---------------------------------------------------------------------------------------- */

/// Days between 1970-01-01 and PostgreSQL's epoch 2000-01-01.
const PG_EPOCH_DAYS: i64 = 10_957;
const MICROS_PER_DAY: i64 = 86_400_000_000;

/// Gregorian date from days since 1970-01-01 (Howard Hinnant's algorithm).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// "2026-01-31", "0044-03-15 BC", or "infinity".
fn date(days_since_2000: i32) -> String {
    match days_since_2000 {
        i32::MAX => "infinity".into(),
        i32::MIN => "-infinity".into(),
        d => format_ymd(i64::from(d) + PG_EPOCH_DAYS),
    }
}

fn format_ymd(days_since_1970: i64) -> String {
    let (y, m, d) = civil_from_days(days_since_1970);
    if y <= 0 {
        // Astronomical year 0 is 1 BC.
        format!("{:04}-{m:02}-{d:02} BC", 1 - y)
    } else {
        format!("{y:04}-{m:02}-{d:02}")
    }
}

/// "13:05:09" or "13:05:09.25" (trailing zeros trimmed, like PostgreSQL).
fn time_of_day(micros: i64) -> String {
    let secs = micros.div_euclid(1_000_000);
    let frac = micros.rem_euclid(1_000_000);
    let (h, m, s) = (secs / 3600, (secs / 60) % 60, secs % 60);
    let mut out = format!("{h:02}:{m:02}:{s:02}");
    if frac != 0 {
        let f = format!("{frac:06}");
        out.push('.');
        out.push_str(f.trim_end_matches('0'));
    }
    out
}

fn offset(east_secs: i32) -> String {
    let sign = if east_secs < 0 { '-' } else { '+' };
    let abs = east_secs.unsigned_abs();
    let (h, m, s) = (abs / 3600, (abs / 60) % 60, abs % 60);
    match (m, s) {
        (0, 0) => format!("{sign}{h:02}"),
        (_, 0) => format!("{sign}{h:02}:{m:02}"),
        _ => format!("{sign}{h:02}:{m:02}:{s:02}"),
    }
}

fn is_infinite_ts(ts: i64) -> bool {
    ts == i64::MAX || ts == i64::MIN
}

/// "2026-01-31 13:05:09.5" from microseconds since 2000-01-01 00:00.
fn timestamp(micros_since_2000: i64) -> String {
    match micros_since_2000 {
        i64::MAX => "infinity".into(),
        i64::MIN => "-infinity".into(),
        t => {
            let days = t.div_euclid(MICROS_PER_DAY);
            let in_day = t.rem_euclid(MICROS_PER_DAY);
            let date = format_ymd(days + PG_EPOCH_DAYS);
            // Keep "BC" at the end, as PostgreSQL prints it.
            match date.strip_suffix(" BC") {
                Some(d) => format!("{d} {} BC", time_of_day(in_day)),
                None => format!("{date} {}", time_of_day(in_day)),
            }
        }
    }
}

/// PostgreSQL-style interval text, e.g. "1 year 2 mons 3 days 04:05:06".
fn interval(b: &[u8]) -> Option<String> {
    let micros = i64::from_be_bytes(b.get(0..8)?.try_into().ok()?);
    let days = i32::from_be_bytes(b.get(8..12)?.try_into().ok()?);
    let months = i32::from_be_bytes(b.get(12..16)?.try_into().ok()?);
    let mut parts = Vec::new();
    let (years, mons) = (months / 12, months % 12);
    let plural =
        |n: i32, one: &str, many: &str| format!("{n} {}", if n.abs() == 1 { one } else { many });
    if years != 0 {
        parts.push(plural(years, "year", "years"));
    }
    if mons != 0 {
        parts.push(plural(mons, "mon", "mons"));
    }
    if days != 0 {
        parts.push(plural(days, "day", "days"));
    }
    if micros != 0 || parts.is_empty() {
        let sign = if micros < 0 { "-" } else { "" };
        parts.push(format!("{sign}{}", time_of_day(micros.abs())));
    }
    Some(parts.join(" "))
}

/* ---------------------------------------------------------------------------------------- */
/* Other types                                                                               */
/* ---------------------------------------------------------------------------------------- */

fn uuid(b: &[u8]) -> Option<String> {
    let bytes: [u8; 16] = b.try_into().ok()?;
    Some(uuid::Uuid::from_bytes(bytes).hyphenated().to_string())
}

/// inet/cidr binary: family, bits, is_cidr, length, address bytes.
fn inet(b: &[u8]) -> Option<String> {
    let (family, bits, is_cidr, len) =
        (*b.first()?, *b.get(1)?, *b.get(2)?, usize::from(*b.get(3)?));
    let addr = b.get(4..4 + len)?;
    let (text, max_bits) = match family {
        2 => (
            Ipv4Addr::from(<[u8; 4]>::try_from(addr).ok()?).to_string(),
            32,
        ),
        3 => (
            Ipv6Addr::from(<[u8; 16]>::try_from(addr).ok()?).to_string(),
            128,
        ),
        _ => return None,
    };
    Some(if is_cidr != 0 || bits != max_bits {
        format!("{text}/{bits}")
    } else {
        text
    })
}

/// Arrays render in PostgreSQL's text form, e.g. `{1,2,NULL}` or `{{a,b},{c,d}}`.
fn array_text(elem: &Type, b: &[u8]) -> Option<String> {
    let i32_at = |i: usize| {
        b.get(i..i + 4)
            .and_then(|s| s.try_into().ok())
            .map(i32::from_be_bytes)
    };
    let ndim = usize::try_from(i32_at(0)?).ok()?;
    if ndim == 0 {
        return Some("{}".into());
    }
    if ndim > 6 {
        return None;
    }
    let mut dims = Vec::with_capacity(ndim);
    for d in 0..ndim {
        dims.push(usize::try_from(i32_at(12 + d * 8)?).ok()?);
    }
    let mut offset = 12 + ndim * 8;
    let total: usize = dims.iter().product();
    let mut values = Vec::with_capacity(total);
    for _ in 0..total {
        let len = i32_at(offset)?;
        offset += 4;
        if len < 0 {
            values.push("NULL".to_owned());
            continue;
        }
        let len = usize::try_from(len).ok()?;
        let bytes = b.get(offset..offset + len)?;
        offset += len;
        values.push(element_text(&decode(elem, Some(bytes))));
    }
    Some(nest(&values, &dims))
}

fn element_text(cell: &Cell) -> String {
    let raw = match cell {
        Cell::Null => return "NULL".into(),
        Cell::Bool { v } => return if *v { "t".into() } else { "f".into() },
        Cell::Int { v } | Cell::Decimal { v } | Cell::Uuid { v } => return v.clone(),
        Cell::Float { v } => return v.to_string(),
        Cell::Text { v, .. } | Cell::Json { v, .. } | Cell::Date { v } | Cell::Time { v } => {
            v.clone()
        }
        Cell::Timestamp { v } | Cell::Timestamptz { v } => v.clone(),
        Cell::Bytes { size } => format!("({size} bytes)"),
        Cell::Unsupported { pg_type } => format!("({pg_type})"),
    };
    // Quote like PostgreSQL when the element contains special characters.
    if raw.is_empty()
        || raw.eq_ignore_ascii_case("null")
        || raw.contains(|c: char| "{},\"\\ ".contains(c))
    {
        format!("\"{}\"", raw.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        raw
    }
}

fn nest(values: &[String], dims: &[usize]) -> String {
    match dims {
        [] => String::new(),
        [_] => format!("{{{}}}", values.join(",")),
        [first, rest @ ..] => {
            let chunk = values.len() / (*first).max(1);
            let inner: Vec<String> = values.chunks(chunk.max(1)).map(|c| nest(c, rest)).collect();
            format!("{{{}}}", inner.join(","))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn numeric_bytes(digits: &[u16], weight: i16, sign: u16, dscale: u16) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&(digits.len() as i16).to_be_bytes());
        b.extend_from_slice(&weight.to_be_bytes());
        b.extend_from_slice(&sign.to_be_bytes());
        b.extend_from_slice(&dscale.to_be_bytes());
        for d in digits {
            b.extend_from_slice(&d.to_be_bytes());
        }
        b
    }

    #[test]
    fn numeric_is_exact() {
        assert_eq!(
            numeric(&numeric_bytes(&[1, 2345], 1, NUMERIC_POS, 2)).as_deref(),
            Some("12345.00")
        );
        assert_eq!(
            numeric(&numeric_bytes(&[1234], -1, NUMERIC_NEG, 4)).as_deref(),
            Some("-0.1234")
        );
        assert_eq!(
            numeric(&numeric_bytes(&[], 0, NUMERIC_POS, 0)).as_deref(),
            Some("0")
        );
        assert_eq!(
            numeric(&numeric_bytes(&[], 0, NUMERIC_POS, 3)).as_deref(),
            Some("0.000")
        );
        assert_eq!(
            numeric(&numeric_bytes(&[5], -2, NUMERIC_POS, 8)).as_deref(),
            Some("0.00000005")
        );
        // 12345678901234567890.12 — beyond f64 precision, must stay exact.
        assert_eq!(
            numeric(&numeric_bytes(
                &[1234, 5678, 9012, 3456, 7890, 1200],
                4,
                NUMERIC_POS,
                2
            ))
            .as_deref(),
            Some("12345678901234567890.12")
        );
        assert_eq!(
            numeric(&numeric_bytes(&[10], 2, NUMERIC_POS, 0)).as_deref(),
            Some("1000000000")
        );
        assert_eq!(
            numeric(&numeric_bytes(&[], 0, NUMERIC_NAN, 0)).as_deref(),
            Some("NaN")
        );
        assert_eq!(
            numeric(&numeric_bytes(&[], 0, NUMERIC_NINF, 0)).as_deref(),
            Some("-Infinity")
        );
        assert_eq!(numeric(&[0, 1]), None);
    }

    #[test]
    fn dates_and_times() {
        assert_eq!(date(0), "2000-01-01");
        assert_eq!(date(9_496), "2025-12-31");
        assert_eq!(date(-1), "1999-12-31");
        assert_eq!(date(i32::MAX), "infinity");
        assert_eq!(date(-730_120), "0001-12-31 BC");
        assert_eq!(date(-730_119), "0001-01-01");
        assert_eq!(time_of_day(0), "00:00:00");
        assert_eq!(time_of_day(47_109_250_000), "13:05:09.25");
        assert_eq!(timestamp(0), "2000-01-01 00:00:00");
        assert_eq!(timestamp(-1), "1999-12-31 23:59:59.999999");
        assert_eq!(timestamp(i64::MIN), "-infinity");
        assert_eq!(offset(19_800), "+05:30");
        assert_eq!(offset(-3600), "-01");
    }

    #[test]
    fn intervals() {
        let mut b = Vec::new();
        b.extend_from_slice(&(4 * 3_600_000_000i64 + 5 * 60_000_000 + 6_000_000).to_be_bytes());
        b.extend_from_slice(&3i32.to_be_bytes());
        b.extend_from_slice(&14i32.to_be_bytes());
        assert_eq!(
            interval(&b).as_deref(),
            Some("1 year 2 mons 3 days 04:05:06")
        );
        let zero = [0u8; 16];
        assert_eq!(interval(&zero).as_deref(), Some("00:00:00"));
    }

    #[test]
    fn scalars_via_decode() {
        assert_eq!(
            decode(&Type::INT8, Some(&(-42i64).to_be_bytes())),
            Cell::Int { v: "-42".into() }
        );
        assert_eq!(decode(&Type::BOOL, Some(&[1])), Cell::Bool { v: true });
        assert_eq!(decode(&Type::TEXT, None), Cell::Null);
        assert_eq!(
            decode(&Type::MONEY, Some(&(-12_345i64).to_be_bytes())),
            Cell::Decimal {
                v: "-123.45".into()
            }
        );
        assert_eq!(
            decode(&Type::FLOAT8, Some(&f64::NAN.to_be_bytes())),
            Cell::Decimal { v: "NaN".into() }
        );
        assert_eq!(
            decode(&Type::JSONB, Some(b"\x01{\"a\":1}")),
            Cell::Json {
                v: "{\"a\":1}".into(),
                truncated: false
            }
        );
        assert_eq!(
            decode(&Type::INET, Some(&[2, 24, 1, 4, 10, 0, 0, 0])),
            Cell::Text {
                v: "10.0.0.0/24".into(),
                truncated: false
            }
        );
        assert_eq!(
            decode(&Type::INT4, Some(&[1, 2])),
            Cell::Unsupported {
                pg_type: "int4".into()
            }
        );
        assert_eq!(
            decode(&Type::TS_VECTOR, Some(&[0])),
            Cell::Unsupported {
                pg_type: "tsvector".into()
            }
        );
    }

    #[test]
    fn long_text_is_truncated_on_a_char_boundary() {
        let long = "é".repeat(MAX_CELL_BYTES); // 2 bytes each
        match decode(&Type::TEXT, Some(long.as_bytes())) {
            Cell::Text { v, truncated } => {
                assert!(truncated);
                assert!(v.len() <= MAX_CELL_BYTES);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn arrays() {
        // int4[] {1,NULL,3}
        let mut b = Vec::new();
        for v in [1i32, 1, 23] {
            b.extend_from_slice(&v.to_be_bytes()); // ndim, has_null, elem oid
        }
        b.extend_from_slice(&3i32.to_be_bytes()); // dim length
        b.extend_from_slice(&1i32.to_be_bytes()); // lower bound
        b.extend_from_slice(&4i32.to_be_bytes());
        b.extend_from_slice(&1i32.to_be_bytes());
        b.extend_from_slice(&(-1i32).to_be_bytes());
        b.extend_from_slice(&4i32.to_be_bytes());
        b.extend_from_slice(&3i32.to_be_bytes());
        assert_eq!(array_text(&Type::INT4, &b).as_deref(), Some("{1,NULL,3}"));
        assert_eq!(
            element_text(&Cell::Text {
                v: "a b".into(),
                truncated: false
            }),
            "\"a b\""
        );
        assert_eq!(
            nest(&["1".into(), "2".into(), "3".into(), "4".into()], &[2, 2]),
            "{{1,2},{3,4}}"
        );
    }
}
