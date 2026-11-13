//! Which table column a `{{variable}}` is compared with, so a Multi-select filter can offer
//! that column's distinct values without any setup.
//!
//! Recognised shapes (the column may be qualified by an alias, table or schema):
//! `col IN ({{x}})`, `col NOT IN ({{x}})`, `col = ANY({{x}})`, `col = {{x}}`, `col <> {{x}}`.

use crate::lang::catalog::Catalog;
use crate::lang::lexer::{Kind, Lexed};
use crate::lang::scope::{relations_in_scope, statement_bounds};
use crate::pg::policy::quote_ident;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnRef {
    pub schema: String,
    pub table: String,
    pub column: String,
}

impl ColumnRef {
    /// `schema.table.column`, for display.
    pub fn label(&self) -> String {
        format!("{}.{}.{}", self.schema, self.table, self.column)
    }

    /// Read-only query for the column's distinct non-null values, identifiers quoted.
    pub fn distinct_values_sql(&self, limit: u32) -> String {
        let col = quote_ident(&self.column);
        format!(
            "SELECT DISTINCT {col} FROM {}.{} WHERE {col} IS NOT NULL ORDER BY 1 LIMIT {limit}",
            quote_ident(&self.schema),
            quote_ident(&self.table),
        )
    }
}

/// The column the first reference to `name` is compared with, resolved through the
/// statement's tables and aliases. `None` when the shape or the table isn't recognised.
pub fn compared_column(lexed: &Lexed, catalog: &Catalog, name: &str) -> Option<ColumnRef> {
    let tokens = lexed.significant();
    let at = tokens
        .iter()
        .position(|t| lexed.variable(t).is_some_and(|v| v.name == name))?;

    // Walk left over `(`, ANY/ALL/SOME, IN, NOT and the comparison operator.
    let mut j = at.checked_sub(1)?;
    if tokens[j].kind == Kind::LParen {
        j = j.checked_sub(1)?;
        let w = lexed.word(&tokens[j]);
        if matches!(w.as_deref(), Some("any" | "all" | "some")) {
            j = j.checked_sub(1)?;
            if tokens[j].kind != Kind::Operator {
                return None;
            }
        } else if w.as_deref() == Some("in") {
            if j >= 1 && lexed.word(&tokens[j - 1]).as_deref() == Some("not") {
                j -= 1;
            }
        } else {
            return None;
        }
    } else if tokens[j].kind != Kind::Operator {
        return None;
    }
    let mut end = j.checked_sub(1)?;

    // The column: name, qualifier.name or schema.table.name.
    let mut parts = Vec::new();
    loop {
        let t = &tokens[end];
        if !matches!(t.kind, Kind::Word | Kind::QuotedIdent) {
            return None;
        }
        parts.insert(0, lexed.identifier(t)?);
        if end >= 2 && tokens[end - 1].kind == Kind::Dot && parts.len() < 3 {
            end -= 2;
        } else {
            break;
        }
    }

    let (start, stop) = statement_bounds(&tokens, tokens[at].start);
    let scope = relations_in_scope(lexed, &tokens[start..stop], catalog, tokens[at].start);
    let resolved = match parts.as_slice() {
        [column] => scope
            .iter()
            .find(|e| e.relation.is_some() && e.columns.iter().any(|c| &c.name == column))
            .and_then(|e| Some((e.schema.clone()?, e.relation.clone()?, column.clone()))),
        [qualifier, column] => scope
            .iter()
            .find(|e| &e.name == qualifier)
            .and_then(|e| Some((e.schema.clone()?, e.relation.clone()?, column.clone())))
            .or_else(|| {
                let rel = catalog.find_relation(None, qualifier)?;
                Some((rel.schema.clone(), rel.name.clone(), column.clone()))
            }),
        [schema, table, column] => Some((schema.clone(), table.clone(), column.clone())),
        _ => None,
    }?;
    let (schema, table, column) = resolved;
    // Only real columns of a catalog relation, never a CTE or expression.
    let rel = catalog.find_relation(Some(&schema), &table)?;
    rel.columns
        .iter()
        .any(|c| c.name == column)
        .then_some(ColumnRef {
            schema,
            table,
            column,
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::catalog::{Column, RelKind, Relation};
    use crate::lang::lexer::lex;

    fn catalog() -> Catalog {
        let col = |n: &str| Column {
            name: n.into(),
            data_type: "text".into(),
        };
        Catalog {
            search_path: vec!["public".into()],
            schemas: vec!["public".into(), "sales".into()],
            relations: vec![
                Relation {
                    schema: "public".into(),
                    name: "customers".into(),
                    kind: RelKind::Table,
                    columns: vec![col("id"), col("code"), col("Grade")],
                },
                Relation {
                    schema: "sales".into(),
                    name: "orders".into(),
                    kind: RelKind::Table,
                    columns: vec![col("id"), col("status")],
                },
            ],
            routines: vec![],
        }
    }

    fn found(sql: &str, name: &str) -> Option<String> {
        compared_column(&lex(sql), &catalog(), name).map(|c| c.label())
    }

    #[test]
    fn finds_the_compared_column() {
        assert_eq!(
            found("SELECT * FROM customers WHERE code IN ({{code}})", "code").as_deref(),
            Some("public.customers.code")
        );
        assert_eq!(
            found(
                "SELECT * FROM sales.orders o WHERE o.status = ANY({{s}})",
                "s"
            )
            .as_deref(),
            Some("sales.orders.status")
        );
        assert_eq!(
            found(
                "SELECT * FROM sales.orders WHERE status NOT IN ({{s}})",
                "s"
            )
            .as_deref(),
            Some("sales.orders.status")
        );
        assert_eq!(
            found("SELECT * FROM customers p WHERE p.\"Grade\" = {{g}}", "g").as_deref(),
            Some("public.customers.Grade")
        );
        assert_eq!(
            found(
                "SELECT * FROM customers p JOIN sales.orders o ON o.id = p.id WHERE status IN ({{s}})",
                "s"
            )
            .as_deref(),
            Some("sales.orders.status")
        );
    }

    #[test]
    fn gives_up_when_unsure() {
        assert!(
            found(
                "SELECT * FROM customers WHERE lower(code) IN ({{code}})",
                "code"
            )
            .is_none()
        );
        assert!(found("SELECT * FROM customers WHERE nope IN ({{code}})", "code").is_none());
        assert!(found("SELECT {{code}}", "code").is_none());
        assert!(
            found(
                "WITH c AS (SELECT 1 AS k) SELECT * FROM c WHERE k IN ({{x}})",
                "x"
            )
            .is_none()
        );
    }

    #[test]
    fn values_query_is_quoted() {
        let c = ColumnRef {
            schema: "public".into(),
            table: "Odd \"T\"".into(),
            column: "Grade".into(),
        };
        let sql = c.distinct_values_sql(1000);
        assert_eq!(
            sql,
            "SELECT DISTINCT \"Grade\" FROM \"public\".\"Odd \"\"T\"\"\" WHERE \"Grade\" IS NOT NULL ORDER BY 1 LIMIT 1000"
        );
        assert!(crate::pg::policy::check_read_only(&sql).is_ok());
    }
}
