//! Optional filters: a condition that uses an empty, not-required variable is skipped.
//!
//! `WHERE cat_no = {{cat_no}} AND active` with `cat_no` empty runs as
//! `WHERE TRUE AND active`. The skipped part is the smallest boolean condition around the
//! reference inside a `WHERE`, `ON` or `HAVING` clause (or a bracketed group in one):
//! conditions are separated by `AND` / `OR`, and the `AND` of `BETWEEN … AND …` belongs to its
//! condition. Function calls, `IN (…)` lists and `ANY(…)` are part of the condition they sit in.
//!
//! A reference outside such a clause (e.g. in the select list or `LIMIT`) can't be skipped;
//! it is bound as NULL instead.

use std::collections::BTreeSet;

use crate::lang::lexer::{Kind, Lexed, Token};

/// Clauses whose content is a boolean condition.
const CONDITION_CLAUSES: &[&str] = &["where", "on", "having"];

/// Words that end a condition clause.
const CLAUSE_END: &[&str] = &[
    "where",
    "group",
    "order",
    "limit",
    "offset",
    "union",
    "except",
    "intersect",
    "window",
    "having",
    "join",
    "left",
    "right",
    "inner",
    "full",
    "cross",
    "natural",
    "returning",
    "for",
    "fetch",
    "select",
    "from",
    "using",
];

/// Words before `(` that make it a grouping bracket rather than a call or list.
const GROUPING_BEFORE: &[&str] = &[
    "and", "or", "not", "where", "on", "having", "when", "then", "else",
];

/// Character ranges (start, end) to replace with `TRUE`, sorted and non-overlapping.
///
/// # Examples
/// - `WHERE a = {{x}} AND b = 1`, x empty → the range of `a = {{x}}`
/// - `WHERE d BETWEEN {{p.from}} AND {{p.to}}`, p empty → `d BETWEEN {{p.from}} AND {{p.to}}`
/// - `SELECT {{x}}` → nothing (not in a condition)
pub fn skipped_conditions(lexed: &Lexed, empty: &BTreeSet<String>) -> Vec<(usize, usize)> {
    if empty.is_empty() {
        return Vec::new();
    }
    let tokens = lexed.significant();
    let mut spans: Vec<(usize, usize)> = Vec::new();
    for (i, t) in tokens.iter().enumerate() {
        let Some(var) = lexed.variable(t) else {
            continue;
        };
        if !empty.contains(&var.name) {
            continue;
        }
        if let Some((a, b)) = condition_around(lexed, &tokens, i) {
            spans.push((tokens[a].start, tokens[b].end));
        }
    }
    // Keep outer spans; drop any span inside another.
    spans.sort_by_key(|&(s, e)| (s, std::cmp::Reverse(e)));
    let mut out: Vec<(usize, usize)> = Vec::new();
    for span in spans {
        match out.last() {
            Some(&(_, end)) if span.0 < end => {}
            _ => out.push(span),
        }
    }
    out
}

/// Index of the `)` matching the `(` at `open`.
fn closing(tokens: &[Token], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (i, t) in tokens.iter().enumerate().skip(open) {
        match t.kind {
            Kind::LParen => depth += 1,
            Kind::RParen => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Token range (inclusive) of the condition containing token `at`.
fn condition_around(lexed: &Lexed, tokens: &[Token], at: usize) -> Option<(usize, usize)> {
    // Walk outwards until the reference sits directly in a condition region.
    let (mut lo, mut hi) = (at, at);
    loop {
        // Left: the nearest unmatched "(" or condition clause keyword.
        let mut depth = 0i32;
        let mut left = None;
        for i in (0..lo).rev() {
            let t = &tokens[i];
            match t.kind {
                Kind::RParen => depth += 1,
                Kind::LParen if depth > 0 => depth -= 1,
                Kind::LParen => {
                    left = Some((i, false));
                    break;
                }
                Kind::Semicolon => return None,
                Kind::Word if depth == 0 => {
                    let w = lexed.word(t).unwrap_or_default();
                    if CONDITION_CLAUSES.contains(&w.as_str()) {
                        left = Some((i, true));
                        break;
                    }
                    if CLAUSE_END.contains(&w.as_str()) {
                        return None;
                    }
                }
                _ => {}
            }
        }
        let (start, is_clause) = left?;

        if is_clause {
            // The clause runs to the next clause word, ")" or ";" at this depth.
            let mut depth = 0i32;
            let mut end = tokens.len();
            for (i, t) in tokens.iter().enumerate().skip(hi + 1) {
                match t.kind {
                    Kind::LParen => depth += 1,
                    Kind::RParen if depth > 0 => depth -= 1,
                    Kind::RParen | Kind::Semicolon => {
                        end = i;
                        break;
                    }
                    Kind::Word if depth == 0 => {
                        let w = lexed.word(t).unwrap_or_default();
                        if CLAUSE_END.contains(&w.as_str()) {
                            end = i;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            return term_in(lexed, tokens, start + 1, end, at);
        }

        // An unmatched "(" : a grouping bracket is a condition region of its own; a call,
        // IN list or ANY(…) is part of a bigger condition, so step outside it.
        let close = closing(tokens, start)?;
        let before = start
            .checked_sub(1)
            .and_then(|p| tokens.get(p))
            .map(|t| (t.kind, lexed.word(t).unwrap_or_default()));
        let grouping = match &before {
            None => true,
            Some((Kind::Word, w)) => GROUPING_BEFORE.contains(&w.as_str()),
            Some((Kind::LParen | Kind::Operator | Kind::Comma, _)) => true,
            Some(_) => false,
        };
        // A subquery bracket holds a statement, not a condition.
        let subquery = tokens
            .get(start + 1)
            .and_then(|t| lexed.word(t))
            .is_some_and(|w| w == "select" || w == "with" || w == "values");
        if grouping && !subquery {
            return term_in(lexed, tokens, start + 1, close, at);
        }
        lo = if grouping {
            start
        } else {
            start.saturating_sub(1)
        };
        hi = close;
    }
}

/// Splits `tokens[from..to]` into conditions at bracket depth 0 and returns the one that
/// contains `at`.
fn term_in(
    lexed: &Lexed,
    tokens: &[Token],
    from: usize,
    to: usize,
    at: usize,
) -> Option<(usize, usize)> {
    let mut depth = 0i32;
    let mut term_start = from;
    let mut between = false;
    for (i, t) in tokens.iter().enumerate().take(to).skip(from) {
        match t.kind {
            Kind::LParen => depth += 1,
            Kind::RParen => depth -= 1,
            Kind::Word if depth == 0 => {
                let w = lexed.word(t).unwrap_or_default();
                if w == "between" {
                    between = true;
                } else if w == "and" && between {
                    between = false;
                } else if w == "and" || w == "or" {
                    if (term_start..i).contains(&at) {
                        return (term_start < i).then_some((term_start, i - 1));
                    }
                    term_start = i + 1;
                }
            }
            _ => {}
        }
    }
    ((term_start..to).contains(&at) && term_start < to).then_some((term_start, to - 1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::lexer::lex;

    /// The SQL with skipped conditions replaced by TRUE.
    fn skip(sql: &str, empty: &[&str]) -> String {
        let lexed = lex(sql);
        let names: BTreeSet<String> = empty.iter().map(|s| (*s).to_string()).collect();
        let mut out = String::new();
        let mut at = 0;
        for (s, e) in skipped_conditions(&lexed, &names) {
            out.extend(&lexed.chars[at..s]);
            out.push_str("TRUE");
            at = e;
        }
        out.extend(&lexed.chars[at..]);
        out
    }

    #[test]
    fn skips_simple_conditions() {
        assert_eq!(
            skip("SELECT * FROM t WHERE cat_no = {{cat_no}}", &["cat_no"]),
            "SELECT * FROM t WHERE TRUE"
        );
        assert_eq!(
            skip(
                "SELECT * FROM t WHERE a = {{x}} AND b = 1 ORDER BY a",
                &["x"]
            ),
            "SELECT * FROM t WHERE TRUE AND b = 1 ORDER BY a"
        );
        assert_eq!(
            skip("SELECT * FROM t WHERE b = 1 OR NOT a = {{x}};", &["x"]),
            "SELECT * FROM t WHERE b = 1 OR TRUE;"
        );
        // Not empty → untouched.
        assert_eq!(
            skip("SELECT * FROM t WHERE a = {{x}}", &["y"]),
            "SELECT * FROM t WHERE a = {{x}}"
        );
    }

    #[test]
    fn keeps_between_together() {
        assert_eq!(
            skip(
                "SELECT 1 FROM t WHERE d BETWEEN {{p.from}} AND {{p.to}} AND x = 1",
                &["p"]
            ),
            "SELECT 1 FROM t WHERE TRUE AND x = 1"
        );
    }

    #[test]
    fn steps_out_of_calls_and_lists() {
        assert_eq!(
            skip("SELECT 1 FROM t WHERE s = ANY({{s}}) AND k = 2", &["s"]),
            "SELECT 1 FROM t WHERE TRUE AND k = 2"
        );
        assert_eq!(
            skip(
                "SELECT 1 FROM t WHERE lower(name) LIKE lower({{q}}) GROUP BY 1",
                &["q"]
            ),
            "SELECT 1 FROM t WHERE TRUE GROUP BY 1"
        );
        assert_eq!(
            skip("SELECT 1 FROM t WHERE r IN ({{a}}, 'x')", &["a"]),
            "SELECT 1 FROM t WHERE TRUE"
        );
    }

    #[test]
    fn works_inside_groups_joins_and_subqueries() {
        assert_eq!(
            skip("SELECT 1 FROM t WHERE (a = {{x}} OR b = 2) AND c", &["x"]),
            "SELECT 1 FROM t WHERE (TRUE OR b = 2) AND c"
        );
        assert_eq!(
            skip(
                "SELECT 1 FROM t JOIN u ON u.id = t.id AND u.k = {{x}} WHERE z",
                &["x"]
            ),
            "SELECT 1 FROM t JOIN u ON u.id = t.id AND TRUE WHERE z"
        );
        assert_eq!(
            skip(
                "SELECT 1 FROM t WHERE id IN (SELECT id FROM u WHERE k = {{x}}) AND z",
                &["x"]
            ),
            "SELECT 1 FROM t WHERE id IN (SELECT id FROM u WHERE TRUE) AND z"
        );
        assert_eq!(
            skip("SELECT 1 FROM t WHERE ({{x}} IS NULL OR c = {{x}})", &["x"]),
            "SELECT 1 FROM t WHERE (TRUE OR TRUE)"
        );
    }

    #[test]
    fn leaves_references_outside_conditions() {
        assert_eq!(skip("SELECT {{x}} FROM t", &["x"]), "SELECT {{x}} FROM t");
        assert_eq!(
            skip("SELECT * FROM t LIMIT {{x}}", &["x"]),
            "SELECT * FROM t LIMIT {{x}}"
        );
        assert_eq!(
            skip("SELECT coalesce({{x}}, 1) FROM t", &["x"]),
            "SELECT coalesce({{x}}, 1) FROM t"
        );
    }
}
