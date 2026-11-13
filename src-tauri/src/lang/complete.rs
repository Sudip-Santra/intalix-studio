//! Context-aware completions and signature help for PostgreSQL.
//!
//! The context comes from the tokens around the cursor: after `FROM`/`JOIN` it suggests
//! tables, after `alias.` the columns of that relation, after `::` data types, after `{{`
//! variables, and elsewhere columns in scope, functions and the keywords that fit.

use std::collections::BTreeSet;

use serde::Serialize;

use crate::lang::catalog::Catalog;
use crate::lang::docs::{self, FnKind};
use crate::lang::lexer::{Kind, Lexed, Token, lex};
use crate::lang::scope::{ScopeEntry, ctes, current_clause, relations_in_scope, statement_bounds};
use crate::lang::variables::{VariableDef, VariableType};

/// Upper bound on items returned per request; the editor ranks and filters further.
const MAX_ITEMS: usize = 300;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    Keyword,
    Function,
    Table,
    View,
    Column,
    Schema,
    Type,
    Variable,
    Alias,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionItem {
    pub label: String,
    pub kind: ItemKind,
    pub detail: Option<String>,
    pub info: Option<String>,
    /// Text to insert instead of the label.
    pub apply: Option<String>,
    /// When true, `apply` is a snippet: `${}` marks where the cursor goes.
    pub snippet: bool,
    /// Higher ranks first.
    pub boost: i32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Completions {
    /// UTF-16 offset where the replaced text starts.
    pub from: usize,
    pub items: Vec<CompletionItem>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SignatureHelp {
    pub name: String,
    pub signatures: Vec<String>,
    /// Parameters of the first signature, for highlighting.
    pub parameters: Vec<String>,
    pub active_parameter: usize,
    pub description: String,
}

fn item(label: impl Into<String>, kind: ItemKind, boost: i32) -> CompletionItem {
    CompletionItem {
        label: label.into(),
        kind,
        detail: None,
        info: None,
        apply: None,
        snippet: false,
        boost,
    }
}

impl CompletionItem {
    fn detail(mut self, d: impl Into<String>) -> Self {
        self.detail = Some(d.into());
        self
    }
    fn info(mut self, i: impl Into<String>) -> Self {
        let i = i.into();
        if !i.is_empty() {
            self.info = Some(i);
        }
        self
    }
    fn apply(mut self, a: impl Into<String>) -> Self {
        self.apply = Some(a.into());
        self
    }
    fn snippet(mut self, a: impl Into<String>) -> Self {
        self.apply = Some(a.into());
        self.snippet = true;
        self
    }
}

/// `true` when every character of `needle` appears in `hay` in order (case-insensitive).
fn fuzzy(hay: &str, needle: &str) -> bool {
    let mut hay = hay.chars().flat_map(char::to_lowercase);
    needle
        .chars()
        .flat_map(char::to_lowercase)
        .all(|n| hay.any(|h| h == n))
}

fn token_at(tokens: &[Token], cursor: usize) -> Option<&Token> {
    tokens.iter().find(|t| t.start < cursor && cursor <= t.end)
}

/// Whether the cursor sits inside a string or comment, where nothing is suggested.
fn in_literal(lexed: &Lexed, cursor: usize) -> bool {
    let Some(t) = token_at(&lexed.tokens, cursor) else {
        return false;
    };
    let unterminated = lexed.errors.iter().any(|e| e.start == t.start);
    match t.kind {
        Kind::LineComment => true,
        Kind::BlockComment | Kind::String | Kind::QuotedIdent => cursor < t.end || unterminated,
        _ => false,
    }
}

/// Suggestions for the cursor position (a UTF-16 offset).
pub fn complete(
    sql: &str,
    cursor_utf16: usize,
    variables: Option<&[VariableDef]>,
    catalog: &Catalog,
) -> Completions {
    let lexed = lex(sql);
    let cursor = lexed.char_index(cursor_utf16);
    let empty = |from: usize| Completions {
        from: lexed.to_utf16(from),
        items: Vec::new(),
    };
    if in_literal(&lexed, cursor) {
        return empty(cursor);
    }
    let chars = &lexed.chars;

    // Word being typed.
    let mut from = cursor;
    while from > 0
        && chars
            .get(from - 1)
            .is_some_and(|c| c.is_alphanumeric() || *c == '_' || *c == '$')
    {
        from -= 1;
    }
    let prefix: String = chars[from..cursor].iter().collect();

    // {{ variable
    if let Some(items) = variable_context(&lexed, from, cursor, variables) {
        return finish(&lexed, from, &prefix, items);
    }

    let tokens = lexed.significant();
    let before: Vec<Token> = tokens.iter().filter(|t| t.end <= from).copied().collect();
    let prev = before.last();

    // qualifier.  → columns / relations of the qualifier
    if prev.map(|t| t.kind) == Some(Kind::Dot) {
        let mut parts: Vec<String> = Vec::new();
        let mut i = before.len() - 1;
        while i > 0 {
            let name_token = &before[i - 1];
            let Some(name) = lexed.identifier(name_token) else {
                break;
            };
            parts.insert(0, name);
            if i >= 2 && before[i - 2].kind == Kind::Dot {
                i -= 2;
            } else {
                break;
            }
        }
        let (start, end) = statement_bounds(&tokens, cursor);
        let statement = &tokens[start..end];
        let items = qualified(&lexed, statement, catalog, &parts, cursor);
        return finish(&lexed, from, &prefix, items);
    }

    // ::type
    if prev.map(|t| t.kind) == Some(Kind::Cast) || in_cast_as(&lexed, &before) {
        let items = docs::TYPES
            .iter()
            .map(|(name, info)| item(*name, ItemKind::Type, 0).info(*info))
            .collect();
        return finish(&lexed, from, &prefix, items);
    }

    let (start, end) = statement_bounds(&tokens, cursor);
    let statement = &tokens[start..end];
    let clause = current_clause(&lexed, statement, from);
    let prev_word = prev.and_then(|t| lexed.word(t));
    let mut items = Vec::new();

    let wants_relation = matches!(
        prev_word.as_deref(),
        Some("from" | "join" | "only" | "lateral" | "table")
    ) || (prev.map(|t| t.kind) == Some(Kind::Comma)
        && clause.as_deref() == Some("from"));

    if wants_relation {
        relation_items(&lexed, statement, catalog, &mut items);
        return finish(&lexed, from, &prefix, items);
    }

    // Words that are almost always followed by a fixed keyword: ORDER → BY, IS → NULL …
    if let Some(w) = prev_word.as_deref()
        && matches!(
            w,
            "group"
                | "order"
                | "partition"
                | "is"
                | "nulls"
                | "left"
                | "right"
                | "full"
                | "inner"
                | "cross"
                | "union"
        )
    {
        for kw in docs::keywords_after(w) {
            items.push(keyword(kw, 4));
        }
        return finish(&lexed, from, &prefix, items);
    }

    // Right after a table reference in FROM/JOIN: alias or the next clause.
    let after_name = prev.is_some_and(|t| {
        t.kind == Kind::QuotedIdent
            || (t.kind == Kind::Word && !prev_word.as_deref().is_some_and(docs::is_reserved))
    });
    if after_name && matches!(clause.as_deref(), Some("from" | "join")) {
        for kw in docs::AFTER_RELATION {
            items.push(keyword(kw, 3));
        }
        return finish(&lexed, from, &prefix, items);
    }

    // Expressions: columns in scope, aliases, functions, variables, keywords.
    let scope = relations_in_scope(&lexed, statement, catalog, cursor);
    column_items(&scope, &mut items);
    for entry in &scope {
        items.push(
            item(entry.name.clone(), ItemKind::Alias, 1)
                .detail(entry.relation.as_ref().map_or(
                    entry.detail.clone(),
                    |r| match &entry.schema {
                        Some(s) => format!("{s}.{r}"),
                        None => r.clone(),
                    },
                ))
                .apply(docs::ident(&entry.name)),
        );
    }
    function_items(catalog, &mut items);
    if let Some(defs) = variables {
        let after_operator = prev.is_some_and(|t| t.kind == Kind::Operator)
            || matches!(
                prev_word.as_deref(),
                Some("in" | "between" | "and" | "any" | "like" | "ilike")
            );
        for (label, def) in variable_labels(defs) {
            items.push(
                item(
                    label.clone(),
                    ItemKind::Variable,
                    if after_operator { 4 } else { 1 },
                )
                .detail(format!("{{{{{label}}}}} · {}", type_label(def.kind)))
                .info(def.help.clone())
                .apply(format!("{{{{{label}}}}}")),
            );
        }
    }
    keyword_items(prev_word.as_deref(), prev, &mut items);
    finish(&lexed, from, &prefix, items)
}

fn type_label(kind: VariableType) -> &'static str {
    match kind {
        VariableType::Text => "Text",
        VariableType::Number => "Number",
        VariableType::Date => "Date",
        VariableType::DateRange => "Date range",
        VariableType::Enum => "Enum",
        VariableType::MultiEnum => "Multi-select",
    }
}

/// Labels for each variable reference: `name`, or `name.from` / `name.to` for ranges.
fn variable_labels(defs: &[VariableDef]) -> Vec<(String, &VariableDef)> {
    let mut out = Vec::new();
    for def in defs {
        if def.kind == VariableType::DateRange {
            out.push((format!("{}.from", def.name), def));
            out.push((format!("{}.to", def.name), def));
        } else {
            out.push((def.name.clone(), def));
        }
    }
    out
}

/// Inside `{{ …` (possibly with part of a name typed).
fn variable_context(
    lexed: &Lexed,
    from: usize,
    cursor: usize,
    variables: Option<&[VariableDef]>,
) -> Option<Vec<CompletionItem>> {
    let chars = &lexed.chars;
    // Allow a dot inside the name: {{period.f
    let mut start = from;
    if start >= 1 && chars.get(start - 1) == Some(&'.') {
        start -= 1;
        while start > 0
            && chars
                .get(start - 1)
                .is_some_and(|c| c.is_alphanumeric() || *c == '_')
        {
            start -= 1;
        }
    }
    let mut j = start;
    while j > 0 && matches!(chars.get(j - 1), Some(' ' | '\t')) {
        j -= 1;
    }
    if !(j >= 2 && chars[j - 2] == '{' && chars[j - 1] == '{') {
        return None;
    }
    // Without variables (explorer tabs) there is nothing to offer here.
    let Some(defs) = variables else {
        return Some(Vec::new());
    };
    // Don't add closing braces if they are already there.
    let mut k = cursor;
    while chars
        .get(k)
        .is_some_and(|c| c.is_alphanumeric() || *c == '_' || *c == '.')
    {
        k += 1;
    }
    while matches!(chars.get(k), Some(' ' | '\t')) {
        k += 1;
    }
    let closed = chars.get(k) == Some(&'}') && chars.get(k + 1) == Some(&'}');
    let typed_dot = start < from;
    let items = variable_labels(defs)
        .into_iter()
        .map(|(label, def)| {
            // After "{{period." only the part after the dot is replaced.
            let insert = if typed_dot {
                label
                    .split_once('.')
                    .map_or(label.clone(), |(_, p)| p.to_owned())
            } else {
                label.clone()
            };
            let mut it = item(insert.clone(), ItemKind::Variable, 5)
                .detail(type_label(def.kind))
                .info(if def.label.is_empty() {
                    def.help.clone()
                } else {
                    def.label.clone()
                });
            if !closed {
                it = it.apply(format!("{insert}}}}}"));
            }
            it
        })
        .collect();
    Some(items)
}

/// `CAST(x AS |`
fn in_cast_as(lexed: &Lexed, before: &[Token]) -> bool {
    if before.last().and_then(|t| lexed.word(t)).as_deref() != Some("as") {
        return false;
    }
    let mut depth = 0i32;
    for (i, t) in before.iter().enumerate().rev() {
        match t.kind {
            Kind::RParen => depth += 1,
            Kind::LParen if depth == 0 => {
                return i
                    .checked_sub(1)
                    .and_then(|p| before.get(p))
                    .and_then(|f| lexed.word(f))
                    .as_deref()
                    == Some("cast");
            }
            Kind::LParen => depth -= 1,
            _ => {}
        }
    }
    false
}

fn qualified(
    lexed: &Lexed,
    statement: &[Token],
    catalog: &Catalog,
    parts: &[String],
    cursor: usize,
) -> Vec<CompletionItem> {
    let mut items = Vec::new();
    match parts {
        [one] => {
            // An alias or relation in scope …
            let scope = relations_in_scope(lexed, statement, catalog, cursor);
            if let Some(entry) = scope.iter().find(|e| &e.name == one) {
                column_items(std::slice::from_ref(entry), &mut items);
            } else if let Some(rel) = catalog.find_relation(None, one) {
                for col in &rel.columns {
                    items.push(column_item(&col.name, &col.data_type, &rel.name, 5));
                }
            }
            // … and/or a schema.
            for rel in catalog.relations.iter().filter(|r| &r.schema == one) {
                items.push(relation_item(rel, false, 3));
            }
            for routine in catalog.routines.iter().filter(|r| &r.schema == one) {
                items.push(
                    item(routine.name.clone(), ItemKind::Function, 1)
                        .detail(format!("({}) → {}", routine.arguments, routine.result))
                        .snippet(format!("{}(${{}})", docs::ident(&routine.name))),
                );
            }
        }
        [.., schema, table] => {
            if let Some(rel) = catalog.find_relation(Some(schema), table) {
                for col in &rel.columns {
                    items.push(column_item(&col.name, &col.data_type, &rel.name, 5));
                }
            }
        }
        [] => {}
    }
    items
}

fn column_item(name: &str, data_type: &str, owner: &str, boost: i32) -> CompletionItem {
    let detail = if data_type.is_empty() {
        owner.to_owned()
    } else {
        format!("{data_type} · {owner}")
    };
    item(name, ItemKind::Column, boost)
        .detail(detail)
        .apply(docs::ident(name))
}

fn column_items(scope: &[ScopeEntry], items: &mut Vec<CompletionItem>) {
    // With several relations in scope, qualify columns that appear in more than one.
    let mut counts = std::collections::HashMap::<&str, usize>::new();
    for entry in scope {
        for col in &entry.columns {
            *counts.entry(col.name.as_str()).or_default() += 1;
        }
    }
    for entry in scope {
        for col in &entry.columns {
            let mut it = column_item(&col.name, &col.data_type, &entry.name, 5);
            if counts.get(col.name.as_str()).copied().unwrap_or(0) > 1 {
                it.label = format!("{}.{}", entry.name, col.name);
                it.apply = Some(format!(
                    "{}.{}",
                    docs::ident(&entry.name),
                    docs::ident(&col.name)
                ));
            }
            items.push(it);
        }
    }
}

fn relation_item(
    rel: &crate::lang::catalog::Relation,
    qualify: bool,
    boost: i32,
) -> CompletionItem {
    let kind = match rel.kind {
        crate::lang::catalog::RelKind::Table | crate::lang::catalog::RelKind::ForeignTable => {
            ItemKind::Table
        }
        _ => ItemKind::View,
    };
    let (label, apply) = if qualify {
        (
            format!("{}.{}", rel.schema, rel.name),
            format!("{}.{}", docs::ident(&rel.schema), docs::ident(&rel.name)),
        )
    } else {
        (rel.name.clone(), docs::ident(&rel.name))
    };
    let columns: Vec<&str> = rel
        .columns
        .iter()
        .take(12)
        .map(|c| c.name.as_str())
        .collect();
    let more = rel.columns.len().saturating_sub(columns.len());
    let info = if columns.is_empty() {
        String::new()
    } else if more > 0 {
        format!("{} … and {more} more", columns.join(", "))
    } else {
        columns.join(", ")
    };
    item(label, kind, boost)
        .detail(format!("{} · {}", rel.kind.label(), rel.schema))
        .info(info)
        .apply(apply)
}

fn relation_items(
    lexed: &Lexed,
    statement: &[Token],
    catalog: &Catalog,
    items: &mut Vec<CompletionItem>,
) {
    for cte in ctes(lexed, statement).entries {
        let info = cte
            .columns
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        items.push(
            item(cte.name.clone(), ItemKind::Table, 6)
                .detail("CTE")
                .info(info)
                .apply(docs::ident(&cte.name)),
        );
    }
    for rel in &catalog.relations {
        let visible = catalog.in_search_path(&rel.schema);
        items.push(relation_item(rel, !visible, if visible { 3 } else { 2 }));
    }
    for schema in &catalog.schemas {
        items.push(
            item(schema.clone(), ItemKind::Schema, 1)
                .detail("schema")
                .apply(format!("{}.", docs::ident(schema))),
        );
    }
}

fn function_items(catalog: &Catalog, items: &mut Vec<CompletionItem>) {
    for f in docs::FUNCTIONS {
        let boost = match f.kind {
            FnKind::Aggregate => 2,
            _ => 1,
        };
        let detail = f.signatures.first().map_or("", |s| s).to_owned();
        let mut it = item(f.name, ItemKind::Function, boost)
            .detail(detail)
            .info(f.description);
        if f.kind != FnKind::Constant {
            let body = if f.name == "count" { "count(${*})" } else { "" };
            it = if body.is_empty() {
                it.snippet(format!("{}(${{}})", f.name))
            } else {
                it.snippet(body)
            };
        }
        items.push(it);
    }
    // The user's own functions, once per name.
    let mut seen = BTreeSet::new();
    for r in &catalog.routines {
        if !seen.insert((r.schema.as_str(), r.name.as_str())) {
            continue;
        }
        let qualify = !catalog.in_search_path(&r.schema);
        let (label, call) = if qualify {
            (
                format!("{}.{}", r.schema, r.name),
                format!("{}.{}", docs::ident(&r.schema), docs::ident(&r.name)),
            )
        } else {
            (r.name.clone(), docs::ident(&r.name))
        };
        items.push(
            item(label, ItemKind::Function, 1)
                .detail(format!("({}) → {}", r.arguments, r.result))
                .info(if r.aggregate {
                    "Aggregate function"
                } else {
                    "Function"
                })
                .snippet(format!("{call}(${{}})")),
        );
    }
}

fn keyword(kw: &str, boost: i32) -> CompletionItem {
    item(kw, ItemKind::Keyword, boost)
}

fn keyword_items(prev_word: Option<&str>, prev: Option<&Token>, items: &mut Vec<CompletionItem>) {
    let mut ranked: Vec<(&str, i32)> = Vec::new();
    for kw in prev_word.map_or(&[][..], docs::keywords_after) {
        ranked.push((kw, 4));
    }
    let after_value = prev.is_some_and(|t| {
        matches!(
            t.kind,
            Kind::Word
                | Kind::QuotedIdent
                | Kind::Number
                | Kind::String
                | Kind::RParen
                | Kind::Variable
        )
    }) && !prev_word.is_some_and(docs::is_reserved);
    if after_value {
        for kw in docs::AFTER_EXPRESSION {
            ranked.push((kw, 3));
        }
    }
    for kw in docs::KEYWORDS {
        ranked.push((kw, 0));
    }
    let mut seen = BTreeSet::new();
    for (kw, boost) in ranked {
        if seen.insert(kw) {
            items.push(keyword(kw, boost));
        }
    }
}

/// Filters by the typed prefix, keeps the best-ranked items and converts the start offset.
fn finish(lexed: &Lexed, from: usize, prefix: &str, mut items: Vec<CompletionItem>) -> Completions {
    if !prefix.is_empty() {
        items.retain(|i| fuzzy(&i.label, prefix));
    }
    // Stable: equal boosts keep their contextual order.
    items.sort_by_key(|i| std::cmp::Reverse(i.boost));
    let mut seen = BTreeSet::new();
    items.retain(|i| seen.insert((i.label.clone(), i.kind as u8)));
    items.truncate(MAX_ITEMS);
    Completions {
        from: lexed.to_utf16(from),
        items,
    }
}

/// Signature help for the function call around the cursor, if any.
///
/// # Examples
/// - `"SELECT date_trunc('month', |"` → `date_trunc`, active parameter 1
/// - `"SELECT coalesce(a, b"` with cursor at the end → `coalesce`, active parameter 1
pub fn signature(sql: &str, cursor_utf16: usize, catalog: &Catalog) -> Option<SignatureHelp> {
    let lexed = lex(sql);
    let cursor = lexed.char_index(cursor_utf16);
    if in_literal(&lexed, cursor) {
        return None;
    }
    let tokens: Vec<Token> = lexed
        .significant()
        .into_iter()
        .filter(|t| t.end <= cursor)
        .collect();
    let mut depth = 0i32;
    let mut commas = 0usize;
    let mut open = None;
    for (i, t) in tokens.iter().enumerate().rev() {
        match t.kind {
            Kind::RParen | Kind::RBracket => depth += 1,
            Kind::LParen | Kind::LBracket if depth > 0 => depth -= 1,
            Kind::LParen => {
                open = Some(i);
                break;
            }
            Kind::LBracket => return None,
            Kind::Comma if depth == 0 => commas += 1,
            Kind::Semicolon => return None,
            _ => {}
        }
    }
    let open = open?;
    let name_token = tokens.get(open.checked_sub(1)?)?;
    let name = lexed.identifier(name_token)?;
    if name_token.kind == Kind::Word && docs::is_reserved(&name) && docs::function(&name).is_none()
    {
        return None;
    }
    let schema = open
        .checked_sub(3)
        .filter(|_| tokens.get(open - 2).map(|t| t.kind) == Some(Kind::Dot))
        .and_then(|i| tokens.get(i))
        .and_then(|t| lexed.identifier(t));

    if schema.is_none()
        && let Some(f) = docs::function(&name)
    {
        let first = f.signatures.first().copied().unwrap_or_default();
        return Some(SignatureHelp {
            name: f.name.to_owned(),
            signatures: f.signatures.iter().map(|s| (*s).to_owned()).collect(),
            parameters: docs::signature_params(first),
            active_parameter: commas,
            description: f.description.to_owned(),
        });
    }
    let routines: Vec<_> = catalog.routines_named(schema.as_deref(), &name).collect();
    let first = routines.first()?;
    let signatures: Vec<String> = routines
        .iter()
        .take(8)
        .map(|r| format!("{}({}) → {}", r.name, r.arguments, r.result))
        .collect();
    let parameters = docs::signature_params(signatures.first().map_or("", String::as_str));
    Some(SignatureHelp {
        name: first.name.clone(),
        signatures,
        parameters,
        active_parameter: commas,
        description: format!("Function in schema {}.", first.schema),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::catalog::{Column, RelKind, Relation, Routine};
    use serde_json::Value;

    fn catalog() -> Catalog {
        let col = |n: &str, t: &str| Column {
            name: n.into(),
            data_type: t.into(),
        };
        Catalog {
            search_path: vec!["public".into()],
            schemas: vec!["public".into(), "sales".into()],
            relations: vec![
                Relation {
                    schema: "public".into(),
                    name: "customers".into(),
                    kind: RelKind::Table,
                    columns: vec![col("id", "integer"), col("name", "text")],
                },
                Relation {
                    schema: "sales".into(),
                    name: "orders".into(),
                    kind: RelKind::Table,
                    columns: vec![
                        col("id", "integer"),
                        col("customer_id", "integer"),
                        col("Order Date", "date"),
                    ],
                },
            ],
            routines: vec![Routine {
                schema: "sales".into(),
                name: "fiscal_year".into(),
                arguments: "d date".into(),
                result: "integer".into(),
                aggregate: false,
            }],
        }
    }

    fn at(sql_with_cursor: &str) -> (String, usize) {
        let cursor = sql_with_cursor.find('|').unwrap();
        (sql_with_cursor.replace('|', ""), cursor)
    }

    fn labels(sql_with_cursor: &str) -> Vec<String> {
        let (sql, cursor) = at(sql_with_cursor);
        let defs = vec![VariableDef {
            name: "period".into(),
            label: "Period".into(),
            kind: VariableType::DateRange,
            required: false,
            default: Value::Null,
            options: None,
            help: String::new(),
        }];
        complete(&sql, cursor, Some(&defs), &catalog())
            .items
            .into_iter()
            .map(|i| i.label)
            .collect()
    }

    #[test]
    fn suggests_relations_after_from() {
        let l = labels("SELECT * FROM |");
        assert!(l.contains(&"customers".to_string()));
        assert!(l.contains(&"sales.orders".to_string()));
        assert!(l.contains(&"sales".to_string()));
        assert!(!l.contains(&"SELECT".to_string()));
        let l = labels("SELECT * FROM cu|");
        assert_eq!(l.first().map(String::as_str), Some("customers"));
    }

    #[test]
    fn suggests_columns_through_aliases_and_schemas() {
        let l = labels("SELECT o.| FROM sales.orders o");
        assert_eq!(l, vec!["id", "customer_id", "Order Date"]);
        let (sql, cursor) = at("SELECT o.| FROM sales.orders o");
        let items = complete(&sql, cursor, None, &catalog()).items;
        let quoted = items.iter().find(|i| i.label == "Order Date").unwrap();
        assert_eq!(quoted.apply.as_deref(), Some("\"Order Date\""));

        let l = labels("SELECT * FROM sales.|");
        assert!(l.contains(&"orders".to_string()));
        assert!(l.contains(&"fiscal_year".to_string()));
        let l = labels("SELECT sales.orders.| FROM sales.orders");
        assert_eq!(l.len(), 3);
    }

    #[test]
    fn columns_in_scope_rank_first_and_shared_names_are_qualified() {
        let l = labels("SELECT | FROM sales.orders o JOIN customers c ON c.id = o.customer_id");
        assert!(l.iter().take(5).any(|x| x == "o.id"));
        assert!(l.contains(&"c.id".to_string()));
        assert!(l.contains(&"customer_id".to_string()));
        assert!(l.contains(&"count".to_string()));
    }

    #[test]
    fn suggests_types_after_casts() {
        let l = labels("SELECT a::|");
        assert!(l.contains(&"date".to_string()));
        let l = labels("SELECT CAST(a AS |");
        assert!(l.contains(&"numeric".to_string()));
    }

    #[test]
    fn suggests_variables_inside_braces() {
        let (sql, cursor) = at("SELECT * FROM t WHERE d >= {{|");
        let defs = vec![VariableDef {
            name: "period".into(),
            label: String::new(),
            kind: VariableType::DateRange,
            required: false,
            default: Value::Null,
            options: None,
            help: String::new(),
        }];
        let c = complete(&sql, cursor, Some(&defs), &catalog());
        assert_eq!(c.items[0].label, "period.from");
        assert_eq!(c.items[0].apply.as_deref(), Some("period.from}}"));

        let (sql, cursor) = at("WHERE d >= {{period.|}}");
        let c = complete(&sql, cursor, Some(&defs), &catalog());
        assert_eq!(c.items[0].label, "from");
        assert!(c.items[0].apply.is_none());

        // Explorer tabs have no variables.
        let (sql, cursor) = at("SELECT {{|");
        assert!(complete(&sql, cursor, None, &catalog()).items.is_empty());
    }

    #[test]
    fn nothing_inside_strings_or_comments() {
        assert!(labels("SELECT 'abc|'").is_empty());
        assert!(labels("SELECT 1 -- note|").is_empty());
        assert!(labels("SELECT 'unterminated|").is_empty());
    }

    #[test]
    fn keywords_follow_context() {
        let l = labels("SELECT * FROM customers |");
        assert_eq!(l.first().map(String::as_str), Some("WHERE"));
        let l = labels("SELECT * FROM customers ORDER |");
        assert_eq!(l.first().map(String::as_str), Some("BY"));
    }

    #[test]
    fn signature_help() {
        let (sql, cursor) = at("SELECT date_trunc('month', |) FROM t");
        let s = signature(&sql, cursor, &catalog()).unwrap();
        assert_eq!(s.name, "date_trunc");
        assert_eq!(s.active_parameter, 1);
        assert_eq!(s.parameters, vec!["field text", "source timestamp"]);

        let (sql, cursor) = at("SELECT coalesce(a, f(x, y), |");
        assert_eq!(
            signature(&sql, cursor, &catalog())
                .unwrap()
                .active_parameter,
            2
        );

        let (sql, cursor) = at("SELECT sales.fiscal_year(|");
        let s = signature(&sql, cursor, &catalog()).unwrap();
        assert_eq!(s.signatures, vec!["fiscal_year(d date) → integer"]);

        let (sql, cursor) = at("SELECT * FROM t WHERE a IN (|");
        assert!(signature(&sql, cursor, &catalog()).is_none());
        let (sql, cursor) = at("SELECT (|");
        assert!(signature(&sql, cursor, &catalog()).is_none());
    }

    #[test]
    fn utf16_cursor() {
        let sql = "SELECT '😀' FROM ";
        let cursor = sql.encode_utf16().count();
        let c = complete(sql, cursor, None, &catalog());
        assert_eq!(c.from, cursor);
        assert!(c.items.iter().any(|i| i.label == "customers"));
    }
}
