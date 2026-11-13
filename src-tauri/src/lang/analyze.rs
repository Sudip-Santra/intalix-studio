//! Instant, offline checks for the editor: everything that can be known without asking the
//! database. Exact grammar and name checks come from the server (`pg::variables::check`),
//! which parses the query with PostgreSQL's own parser without running it.

use serde::Serialize;

use crate::lang::lexer::{Kind, Lexed, Token, lex};
use crate::lang::variables::{VariableDef, reference_part};
use crate::pg::policy::check_read_only;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Error,
    Warning,
}

/// A problem at `from..to` (UTF-16 offsets, as the editor counts).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    pub from: usize,
    pub to: usize,
    pub severity: Severity,
    pub message: String,
    pub hint: Option<String>,
}

/// One `{{name}}` reference found in the SQL.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Reference {
    pub name: String,
    pub part: Option<String>,
    pub from: usize,
    pub to: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Analysis {
    pub diagnostics: Vec<Diagnostic>,
    pub references: Vec<Reference>,
    /// Number of statements (split on `;`).
    pub statements: usize,
}

/// Whether `{{variables}}` are available where this SQL is edited.
#[derive(Debug, Clone, Copy)]
pub enum VariableMode<'a> {
    /// The SQL Editor: references are checked against these definitions.
    Defined(&'a [VariableDef]),
    /// Explorer query tabs, which have no variables.
    NotAvailable,
}

const CLAUSE_AFTER_LIST: &[&str] = &[
    "from",
    "where",
    "group",
    "order",
    "having",
    "limit",
    "offset",
    "union",
    "except",
    "intersect",
    "window",
    "fetch",
];

struct Sink<'a> {
    lexed: &'a Lexed,
    out: Vec<Diagnostic>,
}

impl Sink<'_> {
    fn push(
        &mut self,
        start: usize,
        end: usize,
        severity: Severity,
        message: impl Into<String>,
        hint: Option<&str>,
    ) {
        let end = end
            .max(start + 1)
            .min(self.lexed.chars.len().max(start + 1));
        self.out.push(Diagnostic {
            from: self.lexed.to_utf16(start),
            to: self.lexed.to_utf16(end),
            severity,
            message: message.into(),
            hint: hint.map(str::to_owned),
        });
    }

    fn at(&mut self, token: &Token, severity: Severity, message: impl Into<String>) {
        self.push(token.start, token.end, severity, message, None);
    }
}

/// Checks SQL as it is typed.
///
/// # Examples
/// - `"SELECT (1"` → error "This ( is never closed." at the `(`
/// - `"SELECT a, FROM t"` → error "Remove this comma …" at the `,`
/// - `"DELETE FROM t"` → error from the read-only policy at `DELETE`
/// - `"SELECT 1; SELECT 2"` → warning at the second statement
pub fn analyze(sql: &str, mode: VariableMode<'_>) -> Analysis {
    let lexed = lex(sql);
    let mut sink = Sink {
        lexed: &lexed,
        out: Vec::new(),
    };

    for error in &lexed.errors {
        sink.push(error.start, error.end, Severity::Error, error.message, None);
    }

    let references = check_variables(&lexed, mode, &mut sink);
    let significant = lexed.significant();
    check_brackets(&significant, &mut sink);
    check_lints(&lexed, &significant, &mut sink);

    let statements: Vec<&[Token]> = significant
        .split(|t| t.kind == Kind::Semicolon)
        .filter(|s| !s.is_empty())
        .collect();
    if let Some(second) = statements.get(1).and_then(|s| s.first()) {
        let last = statements
            .last()
            .and_then(|s| s.last())
            .map_or(second.end, |t| t.end);
        sink.push(
            second.start,
            last,
            Severity::Warning,
            "Only one statement runs at a time.",
            Some("Select the statement you want, then run it."),
        );
    }

    // The read-only policy, on each statement, with variables masked by NULL so positions
    // stay the same. Skipped when the text doesn't even lex, to avoid duplicate errors.
    if lexed.errors.is_empty() {
        let masked = masked_text(&lexed);
        for statement in &statements {
            let (Some(first), Some(last)) = (statement.first(), statement.last()) else {
                continue;
            };
            let text: String = masked[first.start..last.end].iter().collect();
            if let Err(error) = check_read_only(&text) {
                let at = first.start + error.position.map_or(0, |p| p.saturating_sub(1) as usize);
                let end = lexed
                    .tokens
                    .iter()
                    .find(|t| t.start == at)
                    .map_or(at + 1, |t| t.end);
                sink.push(
                    at,
                    end,
                    Severity::Error,
                    error.message,
                    error.hint.as_deref(),
                );
            }
        }
    }

    let mut diagnostics = sink.out;
    diagnostics.sort_by_key(|d| (d.from, d.to));
    diagnostics.dedup();
    Analysis {
        diagnostics,
        references,
        statements: statements.len(),
    }
}

/// The SQL with every variable replaced by `NULL` padded with spaces (same length).
fn masked_text(lexed: &Lexed) -> Vec<char> {
    let mut chars = lexed.chars.clone();
    for token in lexed.tokens.iter().filter(|t| t.kind == Kind::Variable) {
        for (i, c) in chars[token.start..token.end].iter_mut().enumerate() {
            *c = "NULL".chars().nth(i).unwrap_or(' ');
        }
    }
    chars
}

fn check_variables(lexed: &Lexed, mode: VariableMode<'_>, sink: &mut Sink<'_>) -> Vec<Reference> {
    let mut references = Vec::new();
    for token in &lexed.tokens {
        match token.kind {
            Kind::Param => sink.push(
                token.start,
                token.end,
                Severity::Error,
                "Use {{variables}} instead of $1-style parameters.",
                Some("Write a name in double braces, e.g. {{start_date}}, and set its type in the Variables panel."),
            ),
            Kind::Variable => {
                let Some(var) = lexed.variable(token) else {
                    continue;
                };
                match mode {
                    VariableMode::NotAvailable => sink.push(
                        token.start,
                        token.end,
                        Severity::Error,
                        "Variables work in the SQL Editor page.",
                        Some("Here, replace the variable with a value."),
                    ),
                    VariableMode::Defined(defs) => {
                        if let Err(message) = reference_part(&var.name, var.part.as_deref(), defs) {
                            sink.at(token, Severity::Error, message);
                        }
                    }
                }
                references.push(Reference {
                    name: var.name,
                    part: var.part,
                    from: lexed.to_utf16(token.start),
                    to: lexed.to_utf16(token.end),
                });
            }
            Kind::String if matches!(mode, VariableMode::Defined(_)) => {
                let text = lexed.text(token);
                if let Some(start) = text.find("{{")
                    && text[start..].contains("}}")
                {
                    sink.push(
                        token.start,
                        token.end,
                        Severity::Warning,
                        "Variables inside quotes are not replaced.",
                        Some("Remove the quotes: {{name}} is already a value. For a pattern, write '%' || {{name}} || '%'."),
                    );
                }
            }
            _ => {}
        }
    }
    references
}

fn check_brackets(tokens: &[Token], sink: &mut Sink<'_>) {
    let mut stack: Vec<Token> = Vec::new();
    for token in tokens {
        match token.kind {
            Kind::LParen | Kind::LBracket => stack.push(*token),
            Kind::RParen | Kind::RBracket => {
                let opener = if token.kind == Kind::RParen {
                    Kind::LParen
                } else {
                    Kind::LBracket
                };
                match stack.last() {
                    Some(open) if open.kind == opener => {
                        stack.pop();
                    }
                    _ => {
                        let symbol = if token.kind == Kind::RParen { ")" } else { "]" };
                        sink.at(
                            token,
                            Severity::Error,
                            format!("This {symbol} has no matching opening bracket."),
                        );
                    }
                }
            }
            // A statement boundary closes nothing; report what is still open.
            Kind::Semicolon => {
                for open in stack.drain(..) {
                    sink.at(&open, Severity::Error, unclosed(open.kind));
                }
            }
            _ => {}
        }
    }
    for open in stack {
        sink.at(&open, Severity::Error, unclosed(open.kind));
    }
}

fn unclosed(kind: Kind) -> &'static str {
    if kind == Kind::LParen {
        "This ( is never closed."
    } else {
        "This [ is never closed."
    }
}

fn check_lints(lexed: &Lexed, tokens: &[Token], sink: &mut Sink<'_>) {
    for (i, token) in tokens.iter().enumerate() {
        let next = tokens.get(i + 1);
        match token.kind {
            Kind::Comma => {
                let dangling = match next {
                    None => true,
                    Some(n) => {
                        matches!(n.kind, Kind::RParen | Kind::Semicolon | Kind::Comma)
                            || lexed
                                .word(n)
                                .is_some_and(|w| CLAUSE_AFTER_LIST.contains(&w.as_str()))
                    }
                };
                if dangling {
                    sink.at(
                        token,
                        Severity::Error,
                        "Remove this comma: nothing follows it.",
                    );
                }
            }
            Kind::Operator => {
                let op = lexed.text(token);
                if matches!(op.as_str(), "=" | "<>" | "!=")
                    && next.and_then(|n| lexed.word(n)).as_deref() == Some("null")
                {
                    let not = if op == "=" { "" } else { "NOT " };
                    sink.push(
                        token.start,
                        next.map_or(token.end, |n| n.end),
                        Severity::Warning,
                        format!("A comparison with NULL is never true. Use IS {not}NULL."),
                        None,
                    );
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::variables::VariableType;
    use serde_json::Value;

    fn errors(sql: &str) -> Vec<String> {
        analyze(sql, VariableMode::Defined(&[]))
            .diagnostics
            .into_iter()
            .map(|d| d.message)
            .collect()
    }

    #[test]
    fn clean_sql_has_no_diagnostics() {
        assert!(errors("SELECT a, b FROM t WHERE x IS NULL ORDER BY a;").is_empty());
        assert!(errors("WITH x AS (SELECT 1) SELECT * FROM x").is_empty());
        assert!(errors("").is_empty());
    }

    #[test]
    fn reports_brackets() {
        let d = analyze("SELECT (1", VariableMode::NotAvailable).diagnostics;
        assert_eq!(d.len(), 1);
        assert_eq!((d[0].from, d[0].to), (7, 8));
        assert!(errors("SELECT 1)")[0].contains("no matching"));
        assert!(errors("SELECT a[1 FROM t")[0].contains("[ is never closed"));
    }

    #[test]
    fn reports_dangling_commas_and_null_comparisons() {
        assert!(errors("SELECT a, FROM t")[0].contains("comma"));
        assert!(errors("SELECT coalesce(a,) FROM t")[0].contains("comma"));
        assert!(errors("SELECT a, from_date FROM t").is_empty());
        let d = analyze("SELECT 1 WHERE a = NULL", VariableMode::NotAvailable).diagnostics;
        assert_eq!(d[0].severity, Severity::Warning);
        assert!(d[0].message.contains("IS NULL"));
        assert!(errors("SELECT 1 WHERE a <> null")[0].contains("IS NOT NULL"));
    }

    #[test]
    fn applies_the_read_only_policy_per_statement() {
        let d = analyze("SELECT 1;\nDELETE FROM t", VariableMode::NotAvailable).diagnostics;
        assert!(d.iter().any(|d| d.severity == Severity::Warning));
        let delete = d.iter().find(|d| d.severity == Severity::Error).unwrap();
        assert_eq!((delete.from, delete.to), (10, 16));
        assert!(delete.message.contains("DELETE"));
    }

    #[test]
    fn checks_variable_references() {
        let defs = vec![VariableDef {
            name: "period".into(),
            label: String::new(),
            kind: VariableType::DateRange,
            required: false,
            default: Value::Null,
            options: None,
            help: String::new(),
        }];
        let a = analyze(
            "SELECT 1 WHERE d >= {{period.from}} AND d <= {{period}} AND {{other}} AND $1",
            VariableMode::Defined(&defs),
        );
        assert_eq!(a.references.len(), 3);
        let messages: Vec<_> = a.diagnostics.iter().map(|d| d.message.as_str()).collect();
        assert_eq!(messages.len(), 3, "{messages:?}");
        assert!(messages[0].contains("date range"));
        assert!(messages[1].contains("not defined"));
        assert!(messages[2].contains("$1"));

        // Variables are masked for the policy check, so this stays clean.
        assert!(
            analyze("SELECT {{period.to}}", VariableMode::Defined(&defs))
                .diagnostics
                .is_empty()
        );
        assert!(
            analyze("SELECT {{x}}", VariableMode::NotAvailable).diagnostics[0]
                .message
                .contains("SQL Editor")
        );
    }

    #[test]
    fn warns_about_variables_in_quotes() {
        let a = analyze(
            "SELECT 1 WHERE name LIKE '%{{q}}%'",
            VariableMode::Defined(&[]),
        );
        assert_eq!(a.diagnostics.len(), 1);
        assert_eq!(a.diagnostics[0].severity, Severity::Warning);
        assert!(a.references.is_empty());
    }

    #[test]
    fn offsets_are_utf16() {
        let d = analyze("SELECT '😀', (", VariableMode::NotAvailable).diagnostics;
        assert_eq!(d[0].from, 13);
    }
}
