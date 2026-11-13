//! Query variables: `{{name}}` placeholders that become filters.
//!
//! A variable is never pasted into the SQL as text. [`compile`] replaces every reference with
//! a numbered PostgreSQL parameter (`$1`, `$2`, …) and the value travels separately, bound by
//! the driver. A value therefore can't change the shape of the query, whatever it contains.
//!
//! Every parameter is sent as text (or a text array) and converted by PostgreSQL with an
//! explicit cast, e.g. `(($1::text)::date)`. The target type comes from the variable type
//! (dates) or from what PostgreSQL inferred for that position (see `pg::variables`). This is
//! the same conversion PostgreSQL applies to a typed literal, so `{{start}}` behaves like
//! `'2026-01-31'` would, without the quoting risks.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{AppError, AppResult, ErrorCategory};
use crate::lang::lexer::{Kind, Lexed, lex};
use crate::lang::optional::skipped_conditions;

pub const MAX_VARIABLES: usize = 50;
const MAX_NAME_LEN: usize = 63;
const MAX_LABEL_CHARS: usize = 120;
const MAX_HELP_CHARS: usize = 500;
const MAX_OPTIONS: usize = 1000;
const MAX_OPTION_CHARS: usize = 500;
const MAX_TEXT_VALUE_CHARS: usize = 10_000;
const MAX_NUMBER_CHARS: usize = 100;
pub const MAX_LOOKUP_SQL_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VariableType {
    Text,
    Number,
    Date,
    DateRange,
    Enum,
    MultiEnum,
}

impl VariableType {
    fn label(self) -> &'static str {
        match self {
            Self::Text => "Text",
            Self::Number => "Number",
            Self::Date => "Date",
            Self::DateRange => "Date range",
            Self::Enum => "Enum",
            Self::MultiEnum => "Multi-select enum",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OptionItem {
    pub value: String,
    #[serde(default)]
    pub label: String,
}

/// Where an enum's choices come from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "camelCase")]
pub enum OptionsSource {
    /// A fixed list typed by the author.
    List { values: Vec<OptionItem> },
    /// A read-only lookup query; column 1 is the value, optional column 2 the label.
    Query { sql: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VariableDef {
    pub name: String,
    #[serde(default)]
    pub label: String,
    #[serde(rename = "type")]
    pub kind: VariableType,
    #[serde(default)]
    pub required: bool,
    /// Same shape as a run value (see [`resolve_values`]); `null` for none.
    #[serde(default)]
    pub default: Value,
    #[serde(default)]
    pub options: Option<OptionsSource>,
    #[serde(default)]
    pub help: String,
}

impl VariableDef {
    fn display(&self) -> &str {
        if self.label.trim().is_empty() {
            &self.name
        } else {
            self.label.trim()
        }
    }
}

/// Which half of a date range a reference means.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Part {
    Whole,
    From,
    To,
}

impl Part {
    pub fn as_suffix(self) -> Option<&'static str> {
        match self {
            Part::Whole => None,
            Part::From => Some("from"),
            Part::To => Some("to"),
        }
    }
}

/// `true` for names like `start_date`: lower-case ASCII letters, digits and `_`, not
/// starting with a digit, at most 63 characters.
pub fn valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_lowercase() || c == '_')
        && name.len() <= MAX_NAME_LEN
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// Checks variable definitions before they are stored.
pub fn validate_definitions(defs: &[VariableDef]) -> AppResult<()> {
    if defs.len() > MAX_VARIABLES {
        return Err(AppError::invalid(format!(
            "A query can have at most {MAX_VARIABLES} variables."
        )));
    }
    let mut seen = std::collections::BTreeSet::new();
    for def in defs {
        if !valid_name(&def.name) {
            return Err(AppError::invalid(format!(
                "\"{}\" is not a valid variable name.",
                def.name
            ))
            .with_hint("Use lower-case letters, digits and _, starting with a letter."));
        }
        if !seen.insert(def.name.as_str()) {
            return Err(AppError::invalid(format!(
                "The variable {} is defined twice.",
                def.name
            )));
        }
        let invalid = |message: String| Err(AppError::invalid(message));
        if def.label.chars().count() > MAX_LABEL_CHARS || def.label.chars().any(char::is_control) {
            return invalid(format!(
                "The label of {} is too long or not valid.",
                def.name
            ));
        }
        if def.help.chars().count() > MAX_HELP_CHARS {
            return invalid(format!("The help text of {} is too long.", def.name));
        }
        let enum_like = matches!(def.kind, VariableType::Enum | VariableType::MultiEnum);
        match (&def.options, enum_like) {
            (None, false) => {}
            (Some(_), false) => {
                return invalid(format!(
                    "{} is a {} variable and can't have a list of choices.",
                    def.name,
                    def.kind.label()
                ));
            }
            (None, true) if def.kind == VariableType::Enum => {
                return invalid(format!("Add the choices for {}.", def.display()));
            }
            // Multi-select without a list offers the distinct values of its column.
            (None, true) => {}
            (Some(OptionsSource::List { values }), true) => {
                if values.is_empty() {
                    return invalid(format!("Add at least one choice for {}.", def.display()));
                }
                if values.len() > MAX_OPTIONS {
                    return invalid(format!(
                        "{} has more than {MAX_OPTIONS} choices. Use a lookup query instead.",
                        def.display()
                    ));
                }
                let mut values_seen = std::collections::BTreeSet::new();
                for item in values {
                    if item.value.is_empty()
                        || item.value.chars().count() > MAX_OPTION_CHARS
                        || item.label.chars().count() > MAX_OPTION_CHARS
                    {
                        return invalid(format!(
                            "A choice of {} is empty or too long.",
                            def.display()
                        ));
                    }
                    if !values_seen.insert(item.value.as_str()) {
                        return invalid(format!(
                            "{} lists the choice \"{}\" twice.",
                            def.display(),
                            item.value
                        ));
                    }
                }
            }
            (Some(OptionsSource::Query { sql }), true) => {
                if sql.trim().is_empty() {
                    return invalid(format!("Write the lookup query for {}.", def.display()));
                }
                if sql.len() > MAX_LOOKUP_SQL_BYTES {
                    return invalid(format!(
                        "The lookup query of {} is too long.",
                        def.display()
                    ));
                }
            }
        }
        if !def.default.is_null() {
            resolve_one(def, &def.default).map_err(|e| {
                AppError::invalid(format!("Default value of {}: {}", def.display(), e.message))
            })?;
        }
    }
    Ok(())
}

/* ------------------------------------------------------------------------------------------
 * Values
 * ---------------------------------------------------------------------------------------- */

/// A value ready to bind. `None` is SQL NULL (an optional filter left empty).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Bound {
    Text(Option<String>),
    TextArray(Option<Vec<String>>),
}

/// Resolved values keyed by variable name and part.
pub type BoundValues = BTreeMap<(String, Part), Bound>;

/// Checks the values entered for a run and converts them for binding.
///
/// Value shapes (JSON): text, number, date and enum take a string; multi-select enum takes
/// an array of strings; a date range takes `{ "from": "YYYY-MM-DD", "to": "YYYY-MM-DD" }`.
/// Empty strings, empty arrays and `null` mean "no value", which is allowed only for optional
/// variables and binds NULL.
///
/// # Examples
/// - date `start`, value `"2026-02-30"` → error "Start: 2026-02-30 is not a real date."
/// - optional enum `region`, value `null` → `Text(None)` (NULL)
/// - date range `period`, `{"from":"2026-01-01","to":"2026-01-31"}` →
///   `(period, From) = "2026-01-01"`, `(period, To) = "2026-01-31"`
pub fn resolve_values(
    defs: &[VariableDef],
    values: &BTreeMap<String, Value>,
) -> AppResult<BoundValues> {
    let mut out = BoundValues::new();
    for def in defs {
        let raw = values.get(&def.name).unwrap_or(&Value::Null);
        let resolved = resolve_one(def, raw).map_err(|e| {
            AppError::new(
                ErrorCategory::InvalidInput,
                format!("{}: {}", def.display(), e.message),
            )
            .with_hint(
                e.hint
                    .unwrap_or_else(|| "Fix the filter value and run again.".into()),
            )
        })?;
        for (part, bound) in resolved {
            out.insert((def.name.clone(), part), bound);
        }
    }
    Ok(out)
}

fn resolve_one(def: &VariableDef, raw: &Value) -> AppResult<Vec<(Part, Bound)>> {
    let missing = || -> AppResult<Vec<(Part, Bound)>> {
        if def.required {
            return Err(AppError::invalid("enter a value. This filter is required."));
        }
        Ok(match def.kind {
            VariableType::DateRange => vec![
                (Part::From, Bound::Text(None)),
                (Part::To, Bound::Text(None)),
            ],
            VariableType::MultiEnum => vec![(Part::Whole, Bound::TextArray(None))],
            _ => vec![(Part::Whole, Bound::Text(None))],
        })
    };
    // Not required means the filter is switched off: whatever was typed is ignored.
    if !def.required {
        return missing();
    }

    match def.kind {
        VariableType::DateRange => {
            if raw.is_null() {
                return missing();
            }
            let Some(obj) = raw.as_object() else {
                return Err(AppError::invalid("expected a from and to date."));
            };
            let get = |key: &str| {
                obj.get(key)
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .unwrap_or("")
            };
            let (from, to) = (get("from"), get("to"));
            if from.is_empty() && to.is_empty() {
                return missing();
            }
            if from.is_empty() || to.is_empty() {
                return Err(AppError::invalid("choose both the start and the end date."));
            }
            check_date(from)?;
            check_date(to)?;
            if from > to {
                return Err(AppError::invalid("the start date is after the end date."));
            }
            Ok(vec![
                (Part::From, Bound::Text(Some(from.to_owned()))),
                (Part::To, Bound::Text(Some(to.to_owned()))),
            ])
        }
        VariableType::MultiEnum => {
            let items: Vec<String> = match raw {
                Value::Null => Vec::new(),
                Value::Array(items) => items
                    .iter()
                    .map(|v| {
                        v.as_str()
                            .map(str::to_owned)
                            .ok_or_else(|| AppError::invalid("expected a list of choices."))
                    })
                    .collect::<AppResult<_>>()?,
                _ => return Err(AppError::invalid("expected a list of choices.")),
            };
            if items.is_empty() {
                return missing();
            }
            if items.len() > MAX_OPTIONS {
                return Err(AppError::invalid(format!(
                    "choose at most {MAX_OPTIONS} values."
                )));
            }
            for item in &items {
                check_choice(def, item)?;
            }
            Ok(vec![(Part::Whole, Bound::TextArray(Some(items)))])
        }
        _ => {
            let text = match raw {
                Value::Null => "",
                Value::String(s) => s.as_str(),
                // Numbers typed into JSON by hand are accepted for Number variables.
                Value::Number(_) if def.kind == VariableType::Number => {
                    return single(check_number(&raw.to_string())?);
                }
                _ => return Err(AppError::invalid("expected a single value.")),
            };
            let trimmed = if def.kind == VariableType::Text {
                text
            } else {
                text.trim()
            };
            if trimmed.is_empty() {
                return missing();
            }
            match def.kind {
                VariableType::Text => {
                    if trimmed.chars().count() > MAX_TEXT_VALUE_CHARS {
                        return Err(AppError::invalid("the text is too long."));
                    }
                    if trimmed.contains('\0') {
                        return Err(AppError::invalid("the text contains a null character."));
                    }
                    single(trimmed.to_owned())
                }
                VariableType::Number => single(check_number(trimmed)?),
                VariableType::Date => {
                    check_date(trimmed)?;
                    single(trimmed.to_owned())
                }
                _ => {
                    check_choice(def, trimmed)?;
                    single(trimmed.to_owned())
                }
            }
        }
    }
}

fn single(value: String) -> AppResult<Vec<(Part, Bound)>> {
    Ok(vec![(Part::Whole, Bound::Text(Some(value)))])
}

fn check_choice(def: &VariableDef, value: &str) -> AppResult<()> {
    if value.is_empty() || value.chars().count() > MAX_OPTION_CHARS || value.contains('\0') {
        return Err(AppError::invalid("a choice is empty or too long."));
    }
    // Lookup-query choices can't be verified without running the query; they are still
    // bound safely as parameters.
    if let Some(OptionsSource::List { values }) = &def.options
        && !values.iter().any(|o| o.value == value)
    {
        return Err(AppError::invalid(format!(
            "\"{value}\" is not one of the choices."
        )));
    }
    Ok(())
}

/// Plain decimal numbers: `42`, `-3.5`, `.5`, `1e6`. Thousands separators are rejected so
/// `1,234` is never misread.
fn check_number(text: &str) -> AppResult<String> {
    let bad = || {
        AppError::invalid(format!("\"{text}\" is not a number."))
            .with_hint("Use digits with an optional decimal point, e.g. 1500 or 12.75.")
    };
    if text.len() > MAX_NUMBER_CHARS {
        return Err(bad());
    }
    let body = text.strip_prefix(['+', '-']).unwrap_or(text);
    let (mantissa, exponent) = match body.split_once(['e', 'E']) {
        Some((m, e)) => (m, Some(e)),
        None => (body, None),
    };
    let (int, frac) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let digits = |s: &str| s.chars().all(|c| c.is_ascii_digit());
    let mantissa_ok = digits(int) && digits(frac) && !(int.is_empty() && frac.is_empty());
    let exponent_ok = exponent.is_none_or(|e| {
        let e = e.strip_prefix(['+', '-']).unwrap_or(e);
        !e.is_empty() && e.len() <= 4 && digits(e)
    });
    if mantissa_ok && exponent_ok {
        Ok(text.to_owned())
    } else {
        Err(bad())
    }
}

/// `YYYY-MM-DD` and a real calendar date (years 1-9999).
fn check_date(text: &str) -> AppResult<()> {
    let bad = || {
        AppError::invalid(format!("\"{text}\" is not a date."))
            .with_hint("Pick a date, or type it as YYYY-MM-DD.")
    };
    let b = text.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return Err(bad());
    }
    let num = |s: &str| s.parse::<u32>().ok();
    let (Some(y), Some(m), Some(d)) = (num(&text[0..4]), num(&text[5..7]), num(&text[8..10]))
    else {
        return Err(bad());
    };
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let days = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return Err(bad()),
    };
    if y == 0 || d == 0 || d > days {
        return Err(AppError::invalid(format!("{text} is not a real date.")));
    }
    Ok(())
}

/* ------------------------------------------------------------------------------------------
 * Compiling {{variables}} into parameters
 * ---------------------------------------------------------------------------------------- */

/// One numbered parameter in the compiled SQL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slot {
    pub variable: String,
    pub part: Part,
    pub kind: VariableType,
    /// 1-based character position of the first reference in the source SQL.
    pub source_position: u32,
}

impl Slot {
    pub fn is_array(&self) -> bool {
        self.kind == VariableType::MultiEnum
    }

    /// `period.from`, or just `region`.
    pub fn reference(&self) -> String {
        match self.part.as_suffix() {
            Some(s) => format!("{}.{s}", self.variable),
            None => self.variable.clone(),
        }
    }
}

/// How each slot is written into the SQL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Placeholder {
    /// `$n` with no cast, so PostgreSQL infers the type from the context.
    Infer,
    /// `(($n::text)::<type>)`, or `(($n::text[])::<type>)` for arrays. `<type>` must be a
    /// safely quoted type name.
    Cast(String),
}

/// A replaced span: `source` characters became `output` characters.
#[derive(Debug, Clone, Copy)]
struct Segment {
    source_start: usize,
    source_end: usize,
    output_start: usize,
    output_end: usize,
}

#[derive(Debug, Clone)]
pub struct Compiled {
    pub sql: String,
    /// Parameter `$n` is `slots[n - 1]`.
    pub slots: Vec<Slot>,
    segments: Vec<Segment>,
}

impl Compiled {
    /// Maps a 1-based character position in the compiled SQL back to the source SQL. A
    /// position inside a replaced placeholder points at the variable reference.
    pub fn source_position(&self, output_position: u32) -> u32 {
        let p = (output_position as usize).saturating_sub(1);
        let mut delta: isize = 0;
        for seg in &self.segments {
            if p < seg.output_start {
                break;
            }
            if p < seg.output_end {
                return u32::try_from(seg.source_start + 1).unwrap_or(u32::MAX);
            }
            delta = seg.output_end as isize - seg.source_end as isize;
        }
        let source = (p as isize - delta).max(0) as usize;
        u32::try_from(source + 1).unwrap_or(u32::MAX)
    }

    pub fn map_error(&self, error: AppError) -> AppError {
        let position = error.position.map(|p| self.source_position(p));
        error.with_position(position)
    }
}

/// The distinct slots of `sql`, in order of first appearance. Fails on references that don't
/// match the definitions (unknown names, `.from` on a non-range, a bare range, `$1`).
/// References inside `skipped` ranges are checked but get no slot.
pub fn slots(
    lexed: &Lexed,
    defs: &[VariableDef],
    skipped: &[(usize, usize)],
) -> AppResult<Vec<Slot>> {
    let mut slots: Vec<Slot> = Vec::new();
    for token in &lexed.tokens {
        let position = u32::try_from(token.start + 1).unwrap_or(u32::MAX);
        if token.kind == Kind::Param {
            return Err(reference_error(
                "Use {{variables}} instead of $1-style parameters.",
                position,
            ));
        }
        let Some(var) = lexed.variable(token) else {
            continue;
        };
        let part = reference_part(&var.name, var.part.as_deref(), defs)
            .map_err(|message| reference_error(&message, position))?;
        let Some(def) = defs.iter().find(|d| d.name == var.name) else {
            continue; // reference_part already rejected unknown names
        };
        if in_skipped(skipped, token.start) {
            continue;
        }
        if !slots
            .iter()
            .any(|s| s.variable == var.name && s.part == part)
        {
            slots.push(Slot {
                variable: var.name,
                part,
                kind: def.kind,
                source_position: position,
            });
        }
    }
    Ok(slots)
}

/// Checks one reference against the definitions and returns the part it means.
pub fn reference_part(
    name: &str,
    part: Option<&str>,
    defs: &[VariableDef],
) -> Result<Part, String> {
    let Some(def) = defs.iter().find(|d| d.name == name) else {
        return Err(format!("{{{{{name}}}}} is not defined."));
    };
    match (def.kind, part) {
        (VariableType::DateRange, Some("from")) => Ok(Part::From),
        (VariableType::DateRange, Some("to")) => Ok(Part::To),
        (VariableType::DateRange, None) => Err(format!(
            "{name} is a date range. Use {{{{{name}.from}}}} and {{{{{name}.to}}}}."
        )),
        (VariableType::DateRange, Some(other)) => {
            Err(format!("A date range has .from and .to, not .{other}."))
        }
        (_, None) => Ok(Part::Whole),
        (_, Some(_)) => Err(format!(
            "Only date ranges have .from and .to. Write {{{{{name}}}}}."
        )),
    }
}

/// A `[NOT] IN ({{variable}})` written around a single variable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InList {
    /// Character range from `NOT`/`IN` to the closing `)`.
    pub start: usize,
    pub end: usize,
    pub variable_start: usize,
    pub negated: bool,
}

/// Every `[NOT] IN ( {{x}} )` in the SQL.
///
/// # Examples
/// - `status IN ({{s}})` → one list, not negated
/// - `status NOT IN ({{s}})` → negated, starting at `NOT`
/// - `status IN ({{s}}, 'x')` → none (more than the variable inside)
pub fn in_lists(lexed: &Lexed) -> Vec<InList> {
    let t = lexed.significant();
    let mut out = Vec::new();
    for i in 2..t.len().saturating_sub(1) {
        if t[i].kind != Kind::Variable
            || t[i - 1].kind != Kind::LParen
            || t[i + 1].kind != Kind::RParen
            || lexed.word(&t[i - 2]).as_deref() != Some("in")
        {
            continue;
        }
        let negated = i >= 3 && lexed.word(&t[i - 3]).as_deref() == Some("not");
        let start = if negated {
            t[i - 3].start
        } else {
            t[i - 2].start
        };
        out.push(InList {
            start,
            end: t[i + 1].end,
            variable_start: t[i].start,
            negated,
        });
    }
    out
}

fn in_skipped(skipped: &[(usize, usize)], at: usize) -> bool {
    skipped.iter().any(|&(s, e)| s <= at && at < e)
}

fn reference_error(message: &str, position: u32) -> AppError {
    AppError::new(ErrorCategory::SyntaxError, message).with_position(Some(position))
}

/// Replaces every reference with its parameter, and every `skipped` range (an optional
/// filter left empty) with `TRUE`. `placeholders[i]` decides how slot `i` (numbered `$i+1`)
/// is written.
pub fn compile(
    lexed: &Lexed,
    slots: &[Slot],
    placeholders: &[Placeholder],
    skipped: &[(usize, usize)],
) -> AppResult<Compiled> {
    let mut sql = String::with_capacity(lexed.chars.len() + slots.len() * 24);
    let mut segments = Vec::new();
    let mut output_len = 0usize; // in characters
    let mut copied_to = 0usize;
    let mut pending_skips = skipped.iter().peekable();
    let lists = in_lists(lexed);

    for token in &lexed.tokens {
        // Skipped conditions that start before this token become TRUE.
        while let Some(&&(start, end)) = pending_skips.peek() {
            if start > token.start {
                break;
            }
            pending_skips.next();
            let before: String = lexed.chars[copied_to..start].iter().collect();
            output_len += start - copied_to;
            sql.push_str(&before);
            segments.push(Segment {
                source_start: start,
                source_end: end,
                output_start: output_len,
                output_end: output_len + 4,
            });
            sql.push_str("TRUE");
            output_len += 4;
            copied_to = end;
        }
        if token.start < copied_to {
            continue;
        }
        let Some(var) = lexed.variable(token) else {
            continue;
        };
        let wanted = |s: &Slot| s.variable == var.name && s.part.as_suffix() == var.part.as_deref();
        let Some(index) = slots.iter().position(wanted) else {
            return Err(AppError::internal(
                "compiling variables",
                "reference without a slot",
            ));
        };
        // `col IN ({{list}})` becomes `col = ANY(<list>)` (NOT IN → `<> ALL`).
        let list = lists
            .iter()
            .find(|l| l.variable_start == token.start)
            .filter(|_| slots[index].is_array());
        let (source_start, source_end) =
            list.map_or((token.start, token.end), |l| (l.start, l.end));
        let before: String = lexed.chars[copied_to..source_start].iter().collect();
        output_len += source_start - copied_to;
        sql.push_str(&before);

        let n = index + 1;
        let text = match placeholders.get(index) {
            Some(Placeholder::Cast(ty)) if slots[index].is_array() => {
                format!("(${n}::text[])::{ty}")
            }
            Some(Placeholder::Cast(ty)) => format!("(${n}::text)::{ty}"),
            _ => format!("${n}"),
        };
        // Parentheses keep operator precedence intact, e.g. `-{{x}}` or `{{x}}::int`.
        let text = match list {
            Some(l) if l.negated => format!("<> ALL({text})"),
            Some(_) => format!("= ANY({text})"),
            None => format!("({text})"),
        };
        let len = text.chars().count();
        segments.push(Segment {
            source_start,
            source_end,
            output_start: output_len,
            output_end: output_len + len,
        });
        sql.push_str(&text);
        output_len += len;
        copied_to = source_end;
    }
    let rest: String = lexed.chars[copied_to..].iter().collect();
    sql.push_str(&rest);
    Ok(Compiled {
        sql,
        slots: slots.to_vec(),
        segments,
    })
}

/// Lexed SQL with its slots and the conditions skipped because their variable is empty.
pub struct Prepared {
    pub lexed: Lexed,
    pub slots: Vec<Slot>,
    pub skipped: Vec<(usize, usize)>,
}

/// Lexes `sql`, finds the conditions to skip for the `empty` variables, and lists the slots.
pub fn prepare(sql: &str, defs: &[VariableDef], empty: &BTreeSet<String>) -> AppResult<Prepared> {
    let lexed = lex(sql);
    if let Some(error) = lexed.errors.first() {
        let position = u32::try_from(error.start + 1).unwrap_or(u32::MAX);
        return Err(reference_error(error.message, position));
    }
    let skipped = skipped_conditions(&lexed, empty);
    let slots = slots(&lexed, defs, &skipped)?;
    Ok(Prepared {
        lexed,
        slots,
        skipped,
    })
}

/// Variables whose filter is switched off (not required): their conditions are skipped,
/// whatever value they hold.
pub fn skipped_variables(defs: &[VariableDef]) -> BTreeSet<String> {
    defs.iter()
        .filter(|d| !d.required)
        .map(|d| d.name.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn def(name: &str, kind: VariableType) -> VariableDef {
        let options = matches!(kind, VariableType::Enum | VariableType::MultiEnum).then(|| {
            OptionsSource::List {
                values: vec![
                    OptionItem {
                        value: "north".into(),
                        label: "North".into(),
                    },
                    OptionItem {
                        value: "south".into(),
                        label: String::new(),
                    },
                ],
            }
        });
        VariableDef {
            name: name.into(),
            label: String::new(),
            kind,
            required: false,
            default: Value::Null,
            options,
            help: String::new(),
        }
        .required()
    }

    impl VariableDef {
        fn required(mut self) -> Self {
            self.required = true;
            self
        }
        fn optional(mut self) -> Self {
            self.required = false;
            self
        }
    }

    fn defs() -> Vec<VariableDef> {
        vec![
            def("start", VariableType::Date),
            def("period", VariableType::DateRange),
            def("region", VariableType::Enum),
            def("status", VariableType::MultiEnum),
            def("min", VariableType::Number),
        ]
    }

    #[test]
    fn compiles_references_into_parameters() {
        let sql = "SELECT * FROM t WHERE d >= {{start}} AND r = {{ Region }} \
                   AND p BETWEEN {{period.from}} AND {{period.to}} AND d2 >= {{start}}";
        let Prepared { lexed, slots, .. } = prepare(sql, &defs(), &BTreeSet::new()).unwrap();
        assert_eq!(slots.len(), 4);
        let placeholders = vec![
            Placeholder::Cast("\"pg_catalog\".\"date\"".into()),
            Placeholder::Infer,
            Placeholder::Cast("\"pg_catalog\".\"date\"".into()),
            Placeholder::Cast("\"pg_catalog\".\"date\"".into()),
        ];
        let compiled = compile(&lexed, &slots, &placeholders, &[]).unwrap();
        assert_eq!(
            compiled.sql,
            "SELECT * FROM t WHERE d >= (($1::text)::\"pg_catalog\".\"date\") AND r = ($2) \
             AND p BETWEEN (($3::text)::\"pg_catalog\".\"date\") AND (($4::text)::\"pg_catalog\".\"date\") \
             AND d2 >= (($1::text)::\"pg_catalog\".\"date\")"
        );
    }

    #[test]
    fn variables_in_strings_and_comments_are_left_alone() {
        let sql = "SELECT '{{start}}' -- {{nope}}\n";
        let Prepared { lexed, slots, .. } = prepare(sql, &defs(), &BTreeSet::new()).unwrap();
        assert!(slots.is_empty());
        assert_eq!(compile(&lexed, &slots, &[], &[]).unwrap().sql, sql);
    }

    #[test]
    fn rejects_bad_references() {
        let err = |sql: &str| prepare(sql, &defs(), &BTreeSet::new()).err().unwrap();
        assert!(err("SELECT {{missing}}").message.contains("not defined"));
        assert!(err("SELECT {{period}}").message.contains("date range"));
        assert!(
            err("SELECT {{start.from}}")
                .message
                .contains("Only date ranges")
        );
        assert!(
            err("SELECT {{period.middle}}")
                .message
                .contains(".from and .to")
        );
        assert!(err("SELECT $1").message.contains("$1"));
        assert_eq!(err("SELECT 1, {{missing}}").position, Some(11));
    }

    #[test]
    fn maps_positions_back_to_the_source() {
        let sql = "SELECT {{min}} + x FROM t WHERE bad";
        let Prepared { lexed, slots, .. } = prepare(sql, &defs(), &BTreeSet::new()).unwrap();
        let compiled = compile(
            &lexed,
            &slots,
            &[Placeholder::Cast("\"pg_catalog\".\"numeric\"".into())],
            &[],
        )
        .unwrap();
        let out_bad = compiled.sql.find("bad").unwrap() as u32 + 1;
        assert_eq!(
            compiled.source_position(out_bad),
            sql.find("bad").unwrap() as u32 + 1
        );
        // Inside the placeholder → the reference itself.
        assert_eq!(compiled.source_position(10), 8);
        // Before any placeholder → unchanged.
        assert_eq!(compiled.source_position(3), 3);
    }

    #[test]
    fn in_lists_become_any() {
        let sql = "SELECT 1 FROM t WHERE s IN ({{status}}) AND r NOT IN ( {{status}} ) AND bad";
        let Prepared {
            lexed,
            slots,
            skipped,
        } = prepare(sql, &defs(), &BTreeSet::new()).unwrap();
        let compiled = compile(
            &lexed,
            &slots,
            &[Placeholder::Cast("\"pg_catalog\".\"_text\"".into())],
            &skipped,
        )
        .unwrap();
        assert_eq!(
            compiled.sql,
            "SELECT 1 FROM t WHERE s = ANY(($1::text[])::\"pg_catalog\".\"_text\") \
             AND r <> ALL(($1::text[])::\"pg_catalog\".\"_text\") AND bad"
        );
        let out_bad = compiled.sql.find("bad").unwrap() as u32 + 1;
        assert_eq!(
            compiled.source_position(out_bad),
            sql.find("bad").unwrap() as u32 + 1
        );
        // A single-value variable inside IN (…) is left as written.
        let Prepared { lexed, slots, .. } = prepare(
            "SELECT 1 WHERE r IN ({{region}})",
            &defs(),
            &BTreeSet::new(),
        )
        .unwrap();
        let compiled = compile(&lexed, &slots, &[Placeholder::Infer], &[]).unwrap();
        assert_eq!(compiled.sql, "SELECT 1 WHERE r IN (($1))");
    }

    #[test]
    fn empty_optional_variables_skip_their_condition() {
        let sql = "SELECT * FROM t WHERE r = {{region}} AND amount >= {{min}} AND bad";
        let empty: BTreeSet<String> = ["region".to_string()].into_iter().collect();
        let Prepared {
            lexed,
            slots,
            skipped,
        } = prepare(sql, &defs(), &empty).unwrap();
        // Only {{min}} still needs a parameter.
        assert_eq!(slots.len(), 1);
        assert_eq!(slots[0].variable, "min");
        let compiled = compile(&lexed, &slots, &[Placeholder::Infer], &skipped).unwrap();
        assert_eq!(
            compiled.sql,
            "SELECT * FROM t WHERE TRUE AND amount >= ($1) AND bad"
        );
        // Errors after the skipped part still point at the source.
        let out_bad = compiled.sql.find("bad").unwrap() as u32 + 1;
        assert_eq!(
            compiled.source_position(out_bad),
            sql.find("bad").unwrap() as u32 + 1
        );

        let mut d = defs();
        d[2] = d[2].clone().optional();
        let found = skipped_variables(&d);
        assert!(found.contains("region") && !found.contains("min"));
    }

    #[test]
    fn resolves_and_validates_values() {
        let d = defs();
        let values: BTreeMap<String, Value> = [
            ("start".to_string(), json!("2024-02-29")),
            (
                "period".to_string(),
                json!({"from": "2026-01-01", "to": "2026-01-31"}),
            ),
            ("region".to_string(), json!("north")),
            ("status".to_string(), json!(["north", "south"])),
            ("min".to_string(), json!(" -12.5 ")),
        ]
        .into_iter()
        .collect();
        let bound = resolve_values(&d, &values).unwrap();
        assert_eq!(
            bound[&("period".to_string(), Part::To)],
            Bound::Text(Some("2026-01-31".into()))
        );
        assert_eq!(
            bound[&("min".to_string(), Part::Whole)],
            Bound::Text(Some("-12.5".into()))
        );
        assert_eq!(
            bound[&("status".to_string(), Part::Whole)],
            Bound::TextArray(Some(vec!["north".into(), "south".into()]))
        );

        // Switched-off (not required) variables bind NULL and ignore their value, even an
        // invalid one.
        let off: Vec<VariableDef> = d.iter().cloned().map(VariableDef::optional).collect();
        let junk: BTreeMap<String, Value> = [("start".to_string(), json!("not a date"))]
            .into_iter()
            .collect();
        assert!(resolve_values(&off, &junk).is_ok());
        let empty = resolve_values(&off, &BTreeMap::new()).unwrap();
        assert_eq!(
            empty[&("status".to_string(), Part::Whole)],
            Bound::TextArray(None)
        );
        assert_eq!(
            empty[&("start".to_string(), Part::Whole)],
            Bound::Text(None)
        );
    }

    #[test]
    fn rejects_bad_values() {
        let check = |kind: VariableType, value: Value| {
            let d = vec![def("v", kind)];
            let values = [("v".to_string(), value)].into_iter().collect();
            resolve_values(&d, &values)
        };
        assert!(check(VariableType::Date, json!("2026-02-29")).is_err());
        assert!(check(VariableType::Date, json!("2026-13-01")).is_err());
        assert!(check(VariableType::Date, json!("01/02/2026")).is_err());
        assert!(check(VariableType::Number, json!("1,234")).is_err());
        assert!(check(VariableType::Number, json!("12abc")).is_err());
        assert!(check(VariableType::Number, json!("1e5")).is_ok());
        assert!(check(VariableType::Enum, json!("west")).is_err());
        assert!(check(VariableType::MultiEnum, json!(["north", "west"])).is_err());
        assert!(check(VariableType::MultiEnum, json!("north")).is_err());
        assert!(
            check(
                VariableType::DateRange,
                json!({"from": "2026-02-01", "to": "2026-01-01"})
            )
            .is_err()
        );
        assert!(check(VariableType::DateRange, json!({"from": "2026-02-01"})).is_err());

        let mut required = def("v", VariableType::Text);
        required.required = true;
        let err = resolve_values(&[required], &BTreeMap::new()).unwrap_err();
        assert!(err.message.contains("required"));
    }

    #[test]
    fn validates_definitions() {
        assert!(validate_definitions(&defs()).is_ok());
        let mut bad = def("Start", VariableType::Text);
        assert!(validate_definitions(&[bad.clone()]).is_err());
        bad.name = "1x".into();
        assert!(validate_definitions(&[bad]).is_err());
        assert!(
            validate_definitions(&[def("a", VariableType::Text), def("a", VariableType::Date)])
                .is_err()
        );
        let mut no_options = def("e", VariableType::Enum);
        no_options.options = None;
        assert!(validate_definitions(&[no_options]).is_err());
        let mut bad_default = def("d", VariableType::Date);
        bad_default.default = json!("tomorrow");
        assert!(validate_definitions(&[bad_default]).is_err());
        let mut text_with_options = def("t", VariableType::Text);
        text_with_options.options = Some(OptionsSource::Query {
            sql: "SELECT 1".into(),
        });
        assert!(validate_definitions(&[text_with_options]).is_err());
    }

    #[test]
    fn names() {
        assert!(valid_name("start_date"));
        assert!(valid_name("_x1"));
        assert!(!valid_name("Start"));
        assert!(!valid_name("1st"));
        assert!(!valid_name("a-b"));
        assert!(!valid_name(""));
        assert!(!valid_name(&"a".repeat(64)));
    }
}
