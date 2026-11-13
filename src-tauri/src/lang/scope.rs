//! Lightweight structure of a statement for hints: which relations are in scope (with their
//! aliases), which CTEs exist, and which clause the cursor is in. It reads tokens, not a full
//! grammar, so it is forgiving of half-written SQL.

use crate::lang::catalog::{Catalog, Column};
use crate::lang::lexer::{Kind, Lexed, Token};

/// A relation reachable from the cursor, e.g. `sales.orders o` or a CTE.
#[derive(Debug, Clone, PartialEq)]
pub struct ScopeEntry {
    /// What the SQL calls it: the alias, or the relation name without schema.
    pub name: String,
    pub schema: Option<String>,
    pub relation: Option<String>,
    /// Known output columns (from the catalog, or read from a CTE / subquery select list).
    pub columns: Vec<Column>,
    /// What it is, for display ("table", "view", "CTE", "subquery").
    pub detail: String,
}

/// Words that end a FROM item, so they are never taken as an alias.
const NOT_ALIAS: &[&str] = &[
    "where",
    "join",
    "left",
    "right",
    "inner",
    "full",
    "cross",
    "natural",
    "on",
    "using",
    "group",
    "order",
    "limit",
    "offset",
    "union",
    "except",
    "intersect",
    "window",
    "having",
    "fetch",
    "for",
    "returning",
    "tablesample",
    "lateral",
    "as",
    "outer",
    "select",
    "with",
    "values",
];

/// Clause keywords, used to find which clause the cursor is in.
const CLAUSES: &[&str] = &[
    "select",
    "from",
    "where",
    "group",
    "having",
    "order",
    "limit",
    "offset",
    "join",
    "on",
    "using",
    "with",
    "window",
    "values",
    "returning",
    "partition",
];

/// Index range of the statement (in `tokens`) that contains `cursor`.
pub fn statement_bounds(tokens: &[Token], cursor: usize) -> (usize, usize) {
    let mut start = 0;
    for (i, t) in tokens.iter().enumerate() {
        if t.kind == Kind::Semicolon {
            if t.end <= cursor {
                start = i + 1;
            } else {
                return (start, i);
            }
        }
    }
    (start, tokens.len())
}

/// Index just past the matching `)` for the `(` at `open`.
fn skip_parens(tokens: &[Token], open: usize) -> usize {
    let mut depth = 0usize;
    let mut i = open;
    while let Some(t) = tokens.get(i) {
        match t.kind {
            Kind::LParen => depth += 1,
            Kind::RParen => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return i + 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    tokens.len()
}

fn is_name(lexed: &Lexed, t: &Token) -> bool {
    match t.kind {
        Kind::QuotedIdent => true,
        Kind::Word => lexed
            .word(t)
            .is_some_and(|w| !NOT_ALIAS.contains(&w.as_str())),
        _ => false,
    }
}

/// Output column names of a `SELECT …` select list (tokens from SELECT up to the end of the
/// query body). Expressions without a clear name are skipped; `*` is not expanded.
///
/// # Examples
/// - `SELECT id, o.total, sum(x) AS amount, upper(n) label FROM …` → `id, total, amount, label`
pub fn select_list_columns(lexed: &Lexed, tokens: &[Token]) -> Vec<Column> {
    let Some(select) = tokens
        .iter()
        .position(|t| lexed.word(t).as_deref() == Some("select"))
    else {
        return Vec::new();
    };
    let mut items: Vec<Vec<Token>> = vec![Vec::new()];
    let mut depth = 0i32;
    for t in &tokens[select + 1..] {
        match t.kind {
            Kind::LParen => depth += 1,
            Kind::RParen => depth -= 1,
            _ => {}
        }
        if depth < 0 {
            break;
        }
        if depth == 0 {
            if t.kind == Kind::Comma {
                items.push(Vec::new());
                continue;
            }
            if lexed.word(t).is_some_and(|w| {
                matches!(
                    w.as_str(),
                    "from"
                        | "where"
                        | "group"
                        | "order"
                        | "limit"
                        | "union"
                        | "having"
                        | "window"
                        | "into"
                )
            }) {
                break;
            }
        }
        if let Some(item) = items.last_mut() {
            item.push(*t);
        }
    }
    let mut columns = Vec::new();
    for item in items {
        let item: Vec<Token> = item
            .into_iter()
            .skip_while(|t| lexed.word(t).as_deref() == Some("distinct"))
            .collect();
        let name = match item.as_slice() {
            [] => None,
            [only] => lexed.identifier(only).filter(|_| is_name(lexed, only)),
            [.., before, last] => {
                // `expr AS name`, `t.name`, or an implicit alias `expr name`.
                let explicit = lexed.word(before).as_deref() == Some("as");
                let implicit = is_name(lexed, last)
                    && matches!(
                        before.kind,
                        Kind::Dot
                            | Kind::Word
                            | Kind::QuotedIdent
                            | Kind::RParen
                            | Kind::String
                            | Kind::Number
                    );
                if explicit || implicit {
                    lexed.identifier(last)
                } else {
                    None
                }
            }
        };
        if let Some(name) = name {
            columns.push(Column {
                name,
                data_type: String::new(),
            });
        }
    }
    columns
}

/// CTEs defined at the start of a statement.
pub struct Ctes {
    pub entries: Vec<ScopeEntry>,
    /// Token index ranges of each CTE body (inside its parentheses).
    bodies: Vec<(usize, usize)>,
    /// Index of the first token of the main query, after the WITH list.
    main_start: usize,
}

/// Reads `WITH a AS (…), b (x, y) AS (…)` at the start of the statement.
pub fn ctes(lexed: &Lexed, tokens: &[Token]) -> Ctes {
    let mut out = Ctes {
        entries: Vec::new(),
        bodies: Vec::new(),
        main_start: 0,
    };
    let mut i = match tokens.first().and_then(|t| lexed.word(t)) {
        Some(w) if w == "with" => 1,
        _ => return out,
    };
    if tokens.get(i).and_then(|t| lexed.word(t)).as_deref() == Some("recursive") {
        i += 1;
    }
    while let Some(name_token) = tokens.get(i) {
        let Some(name) = lexed.identifier(name_token) else {
            break;
        };
        i += 1;
        let mut explicit: Vec<Column> = Vec::new();
        if tokens.get(i).map(|t| t.kind) == Some(Kind::LParen) {
            let end = skip_parens(tokens, i);
            explicit = tokens[i + 1..end.saturating_sub(1).max(i + 1)]
                .iter()
                .filter_map(|t| lexed.identifier(t))
                .map(|name| Column {
                    name,
                    data_type: String::new(),
                })
                .collect();
            i = end;
        }
        if tokens.get(i).and_then(|t| lexed.word(t)).as_deref() != Some("as") {
            break;
        }
        i += 1;
        while tokens
            .get(i)
            .and_then(|t| lexed.word(t))
            .is_some_and(|w| w == "not" || w == "materialized")
        {
            i += 1;
        }
        if tokens.get(i).map(|t| t.kind) != Some(Kind::LParen) {
            break;
        }
        let end = skip_parens(tokens, i);
        let body = (i + 1, end.saturating_sub(1).max(i + 1));
        let columns = if explicit.is_empty() {
            select_list_columns(lexed, &tokens[body.0..body.1])
        } else {
            explicit
        };
        out.entries.push(ScopeEntry {
            name,
            schema: None,
            relation: None,
            columns,
            detail: "CTE".into(),
        });
        out.bodies.push(body);
        i = end;
        out.main_start = i;
        if tokens.get(i).map(|t| t.kind) == Some(Kind::Comma) {
            i += 1;
        } else {
            break;
        }
    }
    out
}

/// Relations in scope at `cursor`: the FROM / JOIN items of the CTE body the cursor is in,
/// or of the main query (subqueries included, which is generous but helpful).
pub fn relations_in_scope(
    lexed: &Lexed,
    statement: &[Token],
    catalog: &Catalog,
    cursor: usize,
) -> Vec<ScopeEntry> {
    let ctes = ctes(lexed, statement);
    let in_body = ctes.bodies.iter().find(|(start, end)| {
        let opened = start
            .checked_sub(1)
            .and_then(|i| statement.get(i))
            .is_some_and(|t| t.end <= cursor);
        let closes = statement.get(*end).is_none_or(|t| cursor <= t.start);
        opened && closes
    });
    let slice = match in_body {
        Some((s, e)) => &statement[*s..*e],
        None => &statement[ctes.main_start.min(statement.len())..],
    };
    from_items(lexed, slice, catalog, &ctes.entries)
}

fn from_items(
    lexed: &Lexed,
    statement: &[Token],
    catalog: &Catalog,
    ctes: &[ScopeEntry],
) -> Vec<ScopeEntry> {
    let mut out: Vec<ScopeEntry> = Vec::new();
    let mut i = 0;
    while i < statement.len() {
        let word = lexed.word(&statement[i]);
        let starts_list = matches!(word.as_deref(), Some("from" | "join"));
        if !starts_list {
            i += 1;
            continue;
        }
        let in_from = word.as_deref() == Some("from");
        i += 1;
        loop {
            while statement
                .get(i)
                .and_then(|t| lexed.word(t))
                .is_some_and(|w| w == "only" || w == "lateral")
            {
                i += 1;
            }
            let Some(first) = statement.get(i) else {
                break;
            };
            let mut entry: Option<ScopeEntry> = None;
            if first.kind == Kind::LParen {
                let end = skip_parens(statement, i);
                let inner = &statement[i + 1..end.saturating_sub(1).max(i + 1)];
                let columns = select_list_columns(lexed, inner);
                i = end;
                entry = Some(ScopeEntry {
                    name: String::new(),
                    schema: None,
                    relation: None,
                    columns,
                    detail: "subquery".into(),
                });
            } else if is_name(lexed, first) {
                let mut parts = vec![lexed.identifier(first).unwrap_or_default()];
                i += 1;
                while statement.get(i).map(|t| t.kind) == Some(Kind::Dot)
                    && statement.get(i + 1).is_some_and(|t| is_name(lexed, t))
                {
                    parts.push(lexed.identifier(&statement[i + 1]).unwrap_or_default());
                    i += 2;
                }
                // A function in FROM, e.g. generate_series(…): skip its arguments.
                if statement.get(i).map(|t| t.kind) == Some(Kind::LParen) {
                    i = skip_parens(statement, i);
                }
                let (schema, name) = match parts.as_slice() {
                    [name] => (None, name.clone()),
                    [.., schema, name] => (Some(schema.clone()), name.clone()),
                    [] => (None, String::new()),
                };
                let cte = schema
                    .is_none()
                    .then(|| ctes.iter().find(|c| c.name == name))
                    .flatten();
                entry = Some(if let Some(cte) = cte {
                    cte.clone()
                } else if let Some(rel) = catalog.find_relation(schema.as_deref(), &name) {
                    ScopeEntry {
                        name: name.clone(),
                        schema: Some(rel.schema.clone()),
                        relation: Some(rel.name.clone()),
                        columns: rel.columns.clone(),
                        detail: rel.kind.label().into(),
                    }
                } else {
                    ScopeEntry {
                        name: name.clone(),
                        schema,
                        relation: Some(name),
                        columns: Vec::new(),
                        detail: "table".into(),
                    }
                });
            }
            let Some(mut entry) = entry else {
                break;
            };
            // Optional alias: [AS] name [(column aliases)]
            if statement.get(i).and_then(|t| lexed.word(t)).as_deref() == Some("as") {
                i += 1;
            }
            if let Some(alias) = statement.get(i).filter(|t| is_name(lexed, t)) {
                entry.name = lexed.identifier(alias).unwrap_or_default();
                i += 1;
                if statement.get(i).map(|t| t.kind) == Some(Kind::LParen) {
                    i = skip_parens(statement, i);
                }
            }
            if !entry.name.is_empty() {
                out.push(entry);
            }
            if in_from && statement.get(i).map(|t| t.kind) == Some(Kind::Comma) {
                i += 1;
                continue;
            }
            break;
        }
    }
    out
}

/// The clause keyword governing `cursor` at the cursor's bracket depth, e.g. `"from"`.
pub fn current_clause(lexed: &Lexed, statement: &[Token], cursor: usize) -> Option<String> {
    let before: Vec<&Token> = statement.iter().filter(|t| t.end <= cursor).collect();
    let mut depth = 0i32;
    for t in before.iter().rev() {
        match t.kind {
            Kind::RParen => depth += 1,
            Kind::LParen => {
                if depth == 0 {
                    return Some("(".into());
                }
                depth -= 1;
            }
            Kind::Word if depth == 0 => {
                if let Some(w) = lexed.word(t)
                    && CLAUSES.contains(&w.as_str())
                {
                    return Some(w);
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::catalog::{RelKind, Relation};
    use crate::lang::lexer::lex;

    fn catalog() -> Catalog {
        Catalog {
            search_path: vec!["public".into()],
            schemas: vec!["public".into(), "sales".into()],
            relations: vec![Relation {
                schema: "sales".into(),
                name: "orders".into(),
                kind: RelKind::Table,
                columns: vec![Column {
                    name: "id".into(),
                    data_type: "integer".into(),
                }],
            }],
            routines: vec![],
        }
    }

    fn scope(sql: &str) -> Vec<(String, String, usize)> {
        let lexed = lex(sql);
        let tokens = lexed.significant();
        relations_in_scope(&lexed, &tokens, &catalog(), sql.len())
            .into_iter()
            .map(|e| (e.name, e.detail, e.columns.len()))
            .collect()
    }

    #[test]
    fn finds_relations_and_aliases() {
        assert_eq!(
            scope("SELECT * FROM sales.orders o JOIN customers AS c ON c.id = o.id, x"),
            vec![
                ("o".into(), "table".into(), 1),
                ("c".into(), "table".into(), 0),
            ]
        );
        assert_eq!(
            scope("SELECT * FROM orders, sales.orders z WHERE"),
            vec![
                ("orders".into(), "table".into(), 1),
                ("z".into(), "table".into(), 1),
            ]
        );
    }

    #[test]
    fn reads_ctes_and_subqueries() {
        let found = scope(
            "WITH totals AS (SELECT id, sum(x) AS amount FROM t GROUP BY id), \
             n (a, b) AS (VALUES (1, 2)) \
             SELECT * FROM totals t JOIN (SELECT 1 AS one) s ON true",
        );
        assert_eq!(
            found,
            vec![
                ("t".into(), "CTE".into(), 2),
                ("s".into(), "subquery".into(), 1)
            ]
        );
        // Inside a CTE body, its own FROM list is the scope.
        let sql = "WITH a AS (SELECT * FROM sales.orders o WHERE ) SELECT * FROM a";
        let lexed = lex(sql);
        let tokens = lexed.significant();
        let cursor = sql.find("WHERE").unwrap() + 6;
        let inner: Vec<_> = relations_in_scope(&lexed, &tokens, &catalog(), cursor)
            .into_iter()
            .map(|e| e.name)
            .collect();
        assert_eq!(inner, vec!["o"]);
    }

    #[test]
    fn select_list_names() {
        let lexed =
            lex("SELECT DISTINCT id, o.total, sum(x) AS amount, upper(n) label, 1 + 2 FROM t");
        let names: Vec<_> = select_list_columns(&lexed, &lexed.significant())
            .into_iter()
            .map(|c| c.name)
            .collect();
        assert_eq!(names, vec!["id", "total", "amount", "label"]);
    }

    #[test]
    fn finds_the_current_clause() {
        let sql = "SELECT a FROM t WHERE (b IN (SELECT c FROM u WHERE ";
        let lexed = lex(sql);
        let tokens = lexed.significant();
        assert_eq!(
            current_clause(&lexed, &tokens, sql.len()).as_deref(),
            Some("where")
        );
        assert_eq!(
            current_clause(&lexed, &tokens, 9).as_deref(),
            Some("select")
        );
        assert_eq!(current_clause(&lexed, &tokens, 23).as_deref(), Some("("));
    }

    #[test]
    fn statement_bounds_split_on_semicolons() {
        let lexed = lex("SELECT 1; SELECT 2; SELECT 3");
        let tokens = lexed.significant();
        assert_eq!(statement_bounds(&tokens, 12), (3, 5));
        assert_eq!(statement_bounds(&tokens, 0), (0, 2));
        assert_eq!(statement_bounds(&tokens, 28), (6, 8));
    }
}
