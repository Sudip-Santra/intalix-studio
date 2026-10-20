//! Read-only statement policy (defence in depth).
//!
//! Layers, from weakest to strongest:
//! 1. This lexer-based check: one statement, of a read-only kind, without locking clauses,
//!    `SELECT INTO`, data-modifying CTEs or server-administration functions.
//! 2. The extended query protocol, which refuses to prepare more than one statement.
//! 3. A `READ ONLY` transaction with a statement timeout, enforced by PostgreSQL itself.
//! 4. The database role's own privileges (documented as the primary control).
//!
//! The lexer understands comments (nested), standard and escape strings, dollar quoting and
//! quoted identifiers, so keywords inside literals can't trick it. It is not a full parser
//! and is never relied on alone.

use crate::error::{AppError, AppResult, ErrorCategory};

#[derive(Debug, Clone, PartialEq, Eq)]
enum Tok {
    /// Lower-cased unquoted identifier or keyword.
    Word(String),
    QuotedIdent,
    Literal,
    Number,
    LParen,
    RParen,
    Semicolon,
    Dot,
    Other,
}

#[derive(Debug, Clone)]
struct Token {
    tok: Tok,
    /// 1-based character position of the token's first character.
    pos: u32,
}

/// Splits SQL into tokens. Unterminated strings or comments are a syntax error.
fn lex(sql: &str) -> AppResult<Vec<Token>> {
    let chars: Vec<char> = sql.chars().collect();
    let mut out = Vec::new();
    let mut i = 0usize;
    let at = |i: usize| chars.get(i).copied();
    let pos = |i: usize| u32::try_from(i + 1).unwrap_or(u32::MAX);

    while let Some(c) = at(i) {
        let start = i;
        // Whitespace
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        // -- line comment
        if c == '-' && at(i + 1) == Some('-') {
            while let Some(ch) = at(i) {
                if ch == '\n' {
                    break;
                }
                i += 1;
            }
            continue;
        }
        // /* block comment */ (PostgreSQL allows nesting)
        if c == '/' && at(i + 1) == Some('*') {
            let mut depth = 0usize;
            loop {
                match (at(i), at(i + 1)) {
                    (Some('/'), Some('*')) => {
                        depth += 1;
                        i += 2;
                    }
                    (Some('*'), Some('/')) => {
                        depth -= 1;
                        i += 2;
                        if depth == 0 {
                            break;
                        }
                    }
                    (Some(_), _) => i += 1,
                    (None, _) => return Err(syntax("Unterminated /* comment.", pos(start))),
                }
            }
            continue;
        }
        // Escape string E'...' (backslash escapes) — check before identifiers.
        if (c == 'e' || c == 'E') && at(i + 1) == Some('\'') {
            i += 2;
            loop {
                match at(i) {
                    Some('\\') => i += 2,
                    Some('\'') if at(i + 1) == Some('\'') => i += 2,
                    Some('\'') => {
                        i += 1;
                        break;
                    }
                    Some(_) => i += 1,
                    None => return Err(syntax("Unterminated string literal.", pos(start))),
                }
            }
            out.push(Token {
                tok: Tok::Literal,
                pos: pos(start),
            });
            continue;
        }
        // Standard string '...' with '' escapes
        if c == '\'' {
            i += 1;
            loop {
                match at(i) {
                    Some('\'') if at(i + 1) == Some('\'') => i += 2,
                    Some('\'') => {
                        i += 1;
                        break;
                    }
                    Some(_) => i += 1,
                    None => return Err(syntax("Unterminated string literal.", pos(start))),
                }
            }
            out.push(Token {
                tok: Tok::Literal,
                pos: pos(start),
            });
            continue;
        }
        // Quoted identifier "..." with "" escapes
        if c == '"' {
            i += 1;
            loop {
                match at(i) {
                    Some('"') if at(i + 1) == Some('"') => i += 2,
                    Some('"') => {
                        i += 1;
                        break;
                    }
                    Some(_) => i += 1,
                    None => return Err(syntax("Unterminated quoted identifier.", pos(start))),
                }
            }
            out.push(Token {
                tok: Tok::QuotedIdent,
                pos: pos(start),
            });
            continue;
        }
        // Dollar-quoted string $tag$...$tag$ (but not a $1 parameter)
        if c == '$' {
            let mut j = i + 1;
            while let Some(ch) = at(j) {
                if ch.is_alphanumeric() || ch == '_' {
                    j += 1;
                } else {
                    break;
                }
            }
            let tag_is_param = j > i + 1 && chars[i + 1..j].iter().all(char::is_ascii_digit);
            if at(j) == Some('$') && !tag_is_param {
                let tag: Vec<char> = chars[i..=j].to_vec();
                i = j + 1;
                loop {
                    if i + tag.len() > chars.len() {
                        return Err(syntax("Unterminated dollar-quoted string.", pos(start)));
                    }
                    if chars[i..i + tag.len()] == tag[..] {
                        i += tag.len();
                        break;
                    }
                    i += 1;
                }
                out.push(Token {
                    tok: Tok::Literal,
                    pos: pos(start),
                });
                continue;
            }
            // Positional parameter like $1
            i = j.max(i + 1);
            out.push(Token {
                tok: Tok::Other,
                pos: pos(start),
            });
            continue;
        }
        // Identifier or keyword
        if c.is_alphabetic() || c == '_' {
            while let Some(ch) = at(i) {
                if ch.is_alphanumeric() || ch == '_' || ch == '$' {
                    i += 1;
                } else {
                    break;
                }
            }
            let word: String = chars[start..i].iter().collect::<String>().to_lowercase();
            out.push(Token {
                tok: Tok::Word(word),
                pos: pos(start),
            });
            continue;
        }
        // Number
        if c.is_ascii_digit() {
            while let Some(ch) = at(i) {
                if ch.is_ascii_alphanumeric() || ch == '.' || ch == '_' {
                    i += 1;
                } else {
                    break;
                }
            }
            out.push(Token {
                tok: Tok::Number,
                pos: pos(start),
            });
            continue;
        }
        let tok = match c {
            '(' => Tok::LParen,
            ')' => Tok::RParen,
            ';' => Tok::Semicolon,
            '.' => Tok::Dot,
            _ => Tok::Other,
        };
        out.push(Token {
            tok,
            pos: pos(start),
        });
        i += 1;
    }
    Ok(out)
}

fn syntax(message: &str, position: u32) -> AppError {
    AppError::new(ErrorCategory::SyntaxError, message).with_position(Some(position))
}

fn rejected(message: impl Into<String>, position: u32) -> AppError {
    AppError::new(ErrorCategory::QueryRejected, message)
        .with_position(Some(position))
        .with_hint(
            "Intalix runs queries read-only. Use SELECT, WITH, VALUES, TABLE, SHOW or EXPLAIN.",
        )
}

const ALLOWED_START: &[&str] = &["select", "with", "values", "table", "show", "explain"];
const DATA_MODIFYING: &[&str] = &["insert", "update", "delete", "merge"];

/// Functions that act on the server rather than read data. Many are already restricted by
/// role privileges; blocking them here is an extra safeguard for over-privileged roles.
const BLOCKED_FUNCTIONS: &[&str] = &[
    "pg_terminate_backend",
    "pg_cancel_backend",
    "pg_reload_conf",
    "pg_rotate_logfile",
    "pg_promote",
    "pg_switch_wal",
    "pg_create_restore_point",
    "pg_backup_start",
    "pg_backup_stop",
    "pg_read_file",
    "pg_read_binary_file",
    "pg_ls_dir",
    "pg_stat_file",
    "pg_file_write",
    "lo_import",
    "lo_export",
    "lo_unlink",
    "dblink",
    "dblink_exec",
    "dblink_connect",
    "dblink_send_query",
    "set_config",
    "pg_advisory_lock",
    "pg_advisory_xact_lock",
];

/// The statement kind that passed the policy, used for logging and future decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatementKind {
    Select,
    With,
    Values,
    Table,
    Show,
    Explain,
}

/// Checks that `sql` is exactly one statement the read-only policy allows.
///
/// # Examples (as exercised by the tests)
/// - `"SELECT * FROM t;"` → `Ok(Select)`
/// - `"DELETE FROM t"` → `Err(QueryRejected)` at position 1
/// - `"SELECT 1; SELECT 2"` → `Err(QueryRejected)` ("Only one statement…")
/// - `"SELECT 'DROP TABLE x'"` → `Ok(Select)` (keywords inside strings are ignored)
pub fn check_read_only(sql: &str) -> AppResult<StatementKind> {
    let tokens = lex(sql)?;
    let mut statements: Vec<&[Token]> = tokens
        .split(|t| t.tok == Tok::Semicolon)
        .filter(|s| !s.is_empty())
        .collect();

    if statements.is_empty() {
        return Err(
            AppError::new(ErrorCategory::SyntaxError, "There is no SQL to run.")
                .with_hint("Type a query first."),
        );
    }
    if statements.len() > 1 {
        let second = statements[1].first().map_or(1, |t| t.pos);
        return Err(
            rejected("Only one statement can run at a time.", second).with_hint(
                "Remove the extra statements, or select the one you want and run just that.",
            ),
        );
    }
    let statement = statements.remove(0);
    check_statement(statement)
}

fn check_statement(tokens: &[Token]) -> AppResult<StatementKind> {
    // Allow leading parentheses: (SELECT 1) UNION (SELECT 2)
    let first = tokens.iter().find(|t| t.tok != Tok::LParen);
    let Some(first) = first else {
        return Err(AppError::new(
            ErrorCategory::SyntaxError,
            "There is no SQL to run.",
        ));
    };
    let Tok::Word(keyword) = &first.tok else {
        return Err(rejected(
            "This statement isn't one Intalix can run.",
            first.pos,
        ));
    };
    if !ALLOWED_START.contains(&keyword.as_str()) {
        return Err(rejected(
            format!(
                "{} statements are not allowed. Intalix runs queries read-only.",
                keyword.to_uppercase()
            ),
            first.pos,
        ));
    }

    let kind = match keyword.as_str() {
        "select" => StatementKind::Select,
        "with" => StatementKind::With,
        "values" => StatementKind::Values,
        "table" => StatementKind::Table,
        "show" => StatementKind::Show,
        _ => StatementKind::Explain,
    };

    if kind == StatementKind::Explain {
        let rest = explain_target(tokens);
        check_statement(rest)?;
        return Ok(StatementKind::Explain);
    }

    check_body(tokens, kind)?;
    Ok(kind)
}

/// Skips `EXPLAIN [ANALYZE] [VERBOSE] [(options)]` and returns the explained statement.
fn explain_target(tokens: &[Token]) -> &[Token] {
    let mut i = 1; // past EXPLAIN
    loop {
        match tokens.get(i).map(|t| &t.tok) {
            Some(Tok::Word(w)) if w == "analyze" || w == "analyse" || w == "verbose" => i += 1,
            Some(Tok::LParen) => {
                let mut depth = 0usize;
                while let Some(t) = tokens.get(i) {
                    match t.tok {
                        Tok::LParen => depth += 1,
                        Tok::RParen => {
                            depth -= 1;
                            if depth == 0 {
                                i += 1;
                                break;
                            }
                        }
                        _ => {}
                    }
                    i += 1;
                }
            }
            _ => break,
        }
    }
    tokens.get(i..).unwrap_or(&[])
}

fn check_body(tokens: &[Token], kind: StatementKind) -> AppResult<()> {
    let mut depth: i32 = 0;
    for (i, t) in tokens.iter().enumerate() {
        match &t.tok {
            Tok::LParen => depth += 1,
            Tok::RParen => depth -= 1,
            Tok::Word(word) => {
                let prev = i.checked_sub(1).and_then(|p| tokens.get(p)).map(|t| &t.tok);
                let next = tokens.get(i + 1).map(|t| &t.tok);
                let after_dot = prev == Some(&Tok::Dot);

                // Data-modifying statements anywhere they can appear in a WITH query:
                // as the main statement (depth 0) or as a CTE body "AS (INSERT ...".
                if kind == StatementKind::With
                    && DATA_MODIFYING.contains(&word.as_str())
                    && !after_dot
                {
                    let starts_cte = prev == Some(&Tok::LParen)
                        && tokens.get(i.wrapping_sub(2)).is_some_and(
                            |t| matches!(&t.tok, Tok::Word(w) if w == "as" || w == "materialized"),
                        );
                    if depth == 0 || starts_cte {
                        return Err(rejected(
                            format!(
                                "{} inside WITH is not allowed. Intalix runs queries read-only.",
                                word.to_uppercase()
                            ),
                            t.pos,
                        ));
                    }
                }

                // SELECT ... INTO new_table creates a table.
                if word == "into"
                    && depth == 0
                    && matches!(kind, StatementKind::Select | StatementKind::With)
                {
                    return Err(
                        rejected("SELECT INTO creates a table and is not allowed.", t.pos)
                            .with_hint("Remove INTO to just read the rows."),
                    );
                }

                // Row-locking clauses: FOR UPDATE / FOR NO KEY UPDATE / FOR SHARE / FOR KEY SHARE.
                if word == "for"
                    && let Some(Tok::Word(n)) = next
                    && matches!(n.as_str(), "update" | "share" | "no" | "key")
                {
                    return Err(rejected(
                        "Row-locking clauses (FOR UPDATE / FOR SHARE) are not allowed.",
                        t.pos,
                    ));
                }

                // Server-administration functions: name immediately followed by "(".
                if next == Some(&Tok::LParen) && BLOCKED_FUNCTIONS.contains(&word.as_str()) {
                    return Err(rejected(
                        format!("{word}() acts on the server and is not allowed."),
                        t.pos,
                    )
                    .with_hint("Intalix only reads data. Ask your DBA if you need this."));
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// Quotes an identifier for safe interpolation into SQL (used for preview queries).
///
/// # Examples
/// - `quote_ident("orders")` → `"orders"`
/// - `quote_ident("Order \"Items\"")` → `"Order ""Items"""`
pub fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(sql: &str) -> StatementKind {
        match check_read_only(sql) {
            Ok(kind) => kind,
            Err(e) => panic!("expected {sql:?} to pass, got {e:?}"),
        }
    }

    fn rejected_at(sql: &str) -> (ErrorCategory, Option<u32>) {
        match check_read_only(sql) {
            Ok(kind) => panic!("expected {sql:?} to be rejected, got {kind:?}"),
            Err(e) => (e.category, e.position),
        }
    }

    #[test]
    fn allows_read_only_statements() {
        assert_eq!(ok("SELECT * FROM sales.orders"), StatementKind::Select);
        assert_eq!(ok("select 1;"), StatementKind::Select);
        assert_eq!(ok("  -- comment\n  SELECT 1;  \n"), StatementKind::Select);
        assert_eq!(ok("(SELECT 1) UNION (SELECT 2)"), StatementKind::Select);
        assert_eq!(
            ok("WITH a AS (SELECT 1) SELECT * FROM a"),
            StatementKind::With
        );
        assert_eq!(ok("VALUES (1), (2)"), StatementKind::Values);
        assert_eq!(ok("TABLE sales.orders"), StatementKind::Table);
        assert_eq!(ok("SHOW server_version"), StatementKind::Show);
        assert_eq!(ok("EXPLAIN SELECT 1"), StatementKind::Explain);
        assert_eq!(
            ok("EXPLAIN (ANALYZE, BUFFERS) SELECT 1"),
            StatementKind::Explain
        );
        assert_eq!(
            ok("EXPLAIN ANALYZE VERBOSE SELECT 1"),
            StatementKind::Explain
        );
    }

    #[test]
    fn rejects_writes_and_ddl() {
        for sql in [
            "DELETE FROM t",
            "insert into t values (1)",
            "UPDATE t SET a = 1",
            "DROP TABLE t",
            "TRUNCATE t",
            "CREATE TABLE x (a int)",
            "ALTER TABLE t ADD c int",
            "GRANT SELECT ON t TO x",
            "COPY t TO '/tmp/x'",
            "CALL p()",
            "DO $$ BEGIN END $$",
            "SET search_path = x",
            "VACUUM t",
        ] {
            assert_eq!(rejected_at(sql).0, ErrorCategory::QueryRejected, "{sql}");
        }
        assert_eq!(
            rejected_at("  DELETE FROM t"),
            (ErrorCategory::QueryRejected, Some(3))
        );
    }

    #[test]
    fn rejects_multiple_statements() {
        assert_eq!(
            rejected_at("SELECT 1; SELECT 2"),
            (ErrorCategory::QueryRejected, Some(11))
        );
        assert_eq!(
            rejected_at("SELECT 1; DROP TABLE t;").0,
            ErrorCategory::QueryRejected
        );
    }

    #[test]
    fn ignores_keywords_inside_literals_and_comments() {
        ok("SELECT 'DROP TABLE x; DELETE FROM y'");
        ok("SELECT E'it\\'s; DELETE' AS s");
        ok("SELECT $$; DELETE FROM t;$$");
        ok("SELECT $tag$ ; UPDATE $tag$");
        ok("SELECT \"delete\" FROM t");
        ok("SELECT 1 /* ; DROP TABLE t; /* nested */ still comment */");
        ok("SELECT 1 -- ; DELETE FROM t");
    }

    #[test]
    fn rejects_data_modifying_ctes() {
        assert_eq!(
            rejected_at("WITH d AS (DELETE FROM t RETURNING *) SELECT * FROM d").0,
            ErrorCategory::QueryRejected
        );
        assert_eq!(
            rejected_at("WITH d AS MATERIALIZED (UPDATE t SET a=1 RETURNING *) SELECT 1").0,
            ErrorCategory::QueryRejected
        );
        assert_eq!(
            rejected_at("WITH a AS (SELECT 1) INSERT INTO t SELECT * FROM a").0,
            ErrorCategory::QueryRejected
        );
        // A qualified column called "update" is fine.
        ok("WITH a AS (SELECT 1 AS x) SELECT a.update FROM a");
    }

    #[test]
    fn rejects_select_into_and_locking() {
        assert_eq!(
            rejected_at("SELECT * INTO newt FROM t").0,
            ErrorCategory::QueryRejected
        );
        assert_eq!(
            rejected_at("SELECT * FROM t FOR UPDATE").0,
            ErrorCategory::QueryRejected
        );
        assert_eq!(
            rejected_at("SELECT * FROM t FOR NO KEY UPDATE").0,
            ErrorCategory::QueryRejected
        );
        assert_eq!(
            rejected_at("SELECT * FROM t FOR SHARE").0,
            ErrorCategory::QueryRejected
        );
        // INTO inside a function call argument list is not SELECT INTO.
        ok("SELECT array_agg(x) FROM (SELECT 1 AS x) s");
    }

    #[test]
    fn rejects_server_functions() {
        assert_eq!(
            rejected_at("SELECT pg_terminate_backend(123)").0,
            ErrorCategory::QueryRejected
        );
        assert_eq!(
            rejected_at("SELECT PG_READ_FILE('/etc/passwd')").0,
            ErrorCategory::QueryRejected
        );
        assert_eq!(
            rejected_at("SELECT pg_catalog.pg_reload_conf()").0,
            ErrorCategory::QueryRejected
        );
        // A column with the same name is not a call.
        ok("SELECT t.lo_import FROM t");
    }

    #[test]
    fn rejects_dangerous_explain_targets() {
        assert_eq!(
            rejected_at("EXPLAIN ANALYZE DELETE FROM t").0,
            ErrorCategory::QueryRejected
        );
        assert_eq!(
            rejected_at("EXPLAIN (ANALYZE) UPDATE t SET a = 1").0,
            ErrorCategory::QueryRejected
        );
    }

    #[test]
    fn reports_unterminated_literals() {
        assert_eq!(rejected_at("SELECT 'abc").0, ErrorCategory::SyntaxError);
        assert_eq!(rejected_at("SELECT $$abc").0, ErrorCategory::SyntaxError);
        assert_eq!(rejected_at("SELECT /* abc").0, ErrorCategory::SyntaxError);
        assert_eq!(rejected_at("SELECT \"abc").0, ErrorCategory::SyntaxError);
    }

    #[test]
    fn empty_sql_is_a_syntax_error() {
        assert_eq!(rejected_at("   ").0, ErrorCategory::SyntaxError);
        assert_eq!(
            rejected_at("-- only a comment").0,
            ErrorCategory::SyntaxError
        );
        assert_eq!(rejected_at(";;").0, ErrorCategory::SyntaxError);
    }

    #[test]
    fn positional_parameters_are_not_dollar_quotes() {
        ok("SELECT * FROM t WHERE a = $1");
    }

    #[test]
    fn quote_ident_escapes_quotes() {
        assert_eq!(quote_ident("orders"), "\"orders\"");
        assert_eq!(quote_ident("Order \"Items\""), "\"Order \"\"Items\"\"\"");
    }
}
