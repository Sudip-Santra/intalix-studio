//! Schema metadata from PostgreSQL's system catalogs (PostgreSQL 12 or newer).
//!
//! Every catalog query runs in a READ ONLY transaction with its own statement timeout and a
//! hard row cap, so a very large or slow database can't hang the explorer. Identifiers in
//! generated DDL are quoted by the server (`quote_ident`), which knows the reserved words.

use std::collections::BTreeMap;

use tokio_postgres::types::ToSql;
use tokio_postgres::{Client, Row};

use crate::error::{AppError, AppResult};
use crate::model::{
    ColumnInfo, ConstraintInfo, ConstraintKind, ConstraintRef, DatabaseInfo, ForeignKeyRef,
    IndexInfo, RelationDetail, RelationKind, RelationSummary, RoutineKind, RoutineSummary,
    SchemaObjects, SchemaSummary, SequenceSummary,
};
use crate::pg::errors::{Phase, map_pg_error};

const CATALOG_TIMEOUT: &str = "30s";
/// Upper bounds on catalog result sizes; far above normal, low enough to protect memory.
const MAX_SCHEMAS: i64 = 10_000;
const MAX_OBJECTS: i64 = 20_000;
const MAX_COMPLETION_RELATIONS: i64 = 5_000;

/// Runs one catalog query in a short read-only transaction.
async fn catalog(
    client: &mut Client,
    sql: &str,
    params: &[&(dyn ToSql + Sync)],
) -> AppResult<Vec<Row>> {
    let tx = client
        .build_transaction()
        .read_only(true)
        .start()
        .await
        .map_err(|e| map_pg_error(&e, Phase::Query))?;
    tx.batch_execute(&format!(
        "SET LOCAL statement_timeout = '{CATALOG_TIMEOUT}'"
    ))
    .await
    .map_err(|e| map_pg_error(&e, Phase::Query))?;
    let rows = tx
        .query(sql, params)
        .await
        .map_err(|e| map_pg_error(&e, Phase::Query))?;
    tx.commit()
        .await
        .map_err(|e| map_pg_error(&e, Phase::Query))?;
    Ok(rows)
}

/// Typed column read with a clear internal error if the catalog shape is unexpected.
fn get<'a, T: tokio_postgres::types::FromSql<'a>>(row: &'a Row, idx: usize) -> AppResult<T> {
    row.try_get(idx)
        .map_err(|e| AppError::internal("reading catalog row", e))
}

pub async fn database_info(client: &mut Client) -> AppResult<DatabaseInfo> {
    let rows = catalog(
        client,
        "SELECT current_database()::text, current_setting('server_version'), current_user::text, \
                CASE WHEN has_database_privilege(current_database(), 'CONNECT') \
                     THEN pg_database_size(current_database()) END",
        &[],
    )
    .await?;
    let row = rows
        .first()
        .ok_or_else(|| AppError::internal("database info", "no row"))?;
    Ok(DatabaseInfo {
        name: get(row, 0)?,
        server_version: format!("PostgreSQL {}", get::<String>(row, 1)?),
        current_user: get(row, 2)?,
        size_bytes: get(row, 3)?,
    })
}

pub async fn list_schemas(client: &mut Client) -> AppResult<Vec<SchemaSummary>> {
    let rows = catalog(
        client,
        "SELECT n.nspname::text, pg_get_userbyid(n.nspowner)::text, obj_description(n.oid, 'pg_namespace'), \
                n.nspname IN ('pg_catalog', 'information_schema') AS is_system \
         FROM pg_namespace n \
         WHERE n.nspname NOT LIKE 'pg\\_toast%' AND n.nspname NOT LIKE 'pg\\_temp\\_%' \
           AND has_schema_privilege(n.oid, 'USAGE') \
         ORDER BY is_system, n.nspname \
         LIMIT $1",
        &[&MAX_SCHEMAS],
    )
    .await?;
    rows.iter()
        .map(|r| {
            Ok(SchemaSummary {
                name: get(r, 0)?,
                owner: get(r, 1)?,
                comment: get(r, 2)?,
                is_system: get(r, 3)?,
            })
        })
        .collect()
}

pub async fn list_objects(client: &mut Client, schema: &str) -> AppResult<SchemaObjects> {
    let relations = catalog(
        client,
        "SELECT c.relname::text, c.relkind::text, \
                CASE WHEN c.reltuples < 0 THEN NULL ELSE c.reltuples::bigint END, \
                CASE WHEN c.relkind IN ('r', 'm') THEN pg_total_relation_size(c.oid) END, \
                obj_description(c.oid, 'pg_class') \
         FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
         WHERE n.nspname = $1 AND c.relkind IN ('r', 'p', 'v', 'm', 'f') AND NOT c.relispartition \
         ORDER BY c.relname \
         LIMIT $2",
        &[&schema, &MAX_OBJECTS],
    )
    .await?;
    let routines = catalog(
        client,
        "SELECT p.proname::text, p.prokind::text, pg_get_function_arguments(p.oid), \
                CASE WHEN p.prokind <> 'p' THEN pg_get_function_result(p.oid) END \
         FROM pg_proc p JOIN pg_namespace n ON n.oid = p.pronamespace \
         WHERE n.nspname = $1 AND p.prokind IN ('f', 'p', 'a') \
         ORDER BY p.proname, 3 \
         LIMIT $2",
        &[&schema, &MAX_OBJECTS],
    )
    .await?;
    let sequences = catalog(
        client,
        "SELECT c.relname::text, format_type(s.seqtypid, NULL), \
                (SELECT tn.nspname || '.' || t.relname || '.' || a.attname \
                   FROM pg_depend d \
                   JOIN pg_class t ON t.oid = d.refobjid \
                   JOIN pg_namespace tn ON tn.oid = t.relnamespace \
                   JOIN pg_attribute a ON a.attrelid = t.oid AND a.attnum = d.refobjsubid \
                  WHERE d.classid = 'pg_class'::regclass AND d.objid = c.oid \
                    AND d.refclassid = 'pg_class'::regclass AND d.deptype IN ('a', 'i') \
                  LIMIT 1) \
         FROM pg_class c \
         JOIN pg_namespace n ON n.oid = c.relnamespace \
         JOIN pg_sequence s ON s.seqrelid = c.oid \
         WHERE n.nspname = $1 AND c.relkind = 'S' \
         ORDER BY c.relname \
         LIMIT $2",
        &[&schema, &MAX_OBJECTS],
    )
    .await?;

    Ok(SchemaObjects {
        relations: relations
            .iter()
            .filter_map(|r| relation_summary(r, schema, 0).transpose())
            .collect::<AppResult<_>>()?,
        routines: routines
            .iter()
            .map(|r| {
                let kind = match get::<String>(r, 1)?.as_str() {
                    "p" => RoutineKind::Procedure,
                    "a" => RoutineKind::Aggregate,
                    _ => RoutineKind::Function,
                };
                Ok(RoutineSummary {
                    schema: schema.to_owned(),
                    name: get(r, 0)?,
                    kind,
                    arguments: get(r, 2)?,
                    returns: get(r, 3)?,
                })
            })
            .collect::<AppResult<_>>()?,
        sequences: sequences
            .iter()
            .map(|r| {
                Ok(SequenceSummary {
                    schema: schema.to_owned(),
                    name: get(r, 0)?,
                    data_type: get(r, 1)?,
                    owned_by: get(r, 2)?,
                })
            })
            .collect::<AppResult<_>>()?,
    })
}

/// Reads name, relkind, estimate, size, comment starting at column `offset`.
fn relation_summary(r: &Row, schema: &str, offset: usize) -> AppResult<Option<RelationSummary>> {
    let relkind: String = get(r, offset + 1)?;
    let Some(kind) = relkind
        .bytes()
        .next()
        .and_then(|b| RelationKind::from_relkind(b as i8))
    else {
        return Ok(None);
    };
    Ok(Some(RelationSummary {
        schema: schema.to_owned(),
        name: get(r, offset)?,
        kind,
        estimated_rows: get(r, offset + 2)?,
        size_bytes: get(r, offset + 3)?,
        comment: get(r, offset + 4)?,
    }))
}

struct RawColumn {
    info: ColumnInfo,
    quoted: String,
    identity: String,
    generated: String,
    raw_default: Option<String>,
}

pub async fn relation_detail(
    client: &mut Client,
    schema: &str,
    name: &str,
) -> AppResult<RelationDetail> {
    let head = catalog(
        client,
        "SELECT c.relname::text, c.relkind::text, \
                CASE WHEN c.reltuples < 0 THEN NULL ELSE c.reltuples::bigint END, \
                CASE WHEN c.relkind IN ('r', 'm') THEN pg_total_relation_size(c.oid) END, \
                obj_description(c.oid, 'pg_class'), \
                c.oid, pg_get_userbyid(c.relowner)::text, \
                quote_ident(n.nspname) || '.' || quote_ident(c.relname), \
                CASE WHEN c.relkind = 'p' THEN pg_get_partkeydef(c.oid) END, \
                CASE WHEN c.relkind IN ('v', 'm') THEN pg_get_viewdef(c.oid, true) END \
         FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
         WHERE n.nspname = $1 AND c.relname = $2 AND c.relkind IN ('r', 'p', 'v', 'm', 'f')",
        &[&schema, &name],
    )
    .await?;
    let row = head.first().ok_or_else(|| {
        AppError::not_found(format!(
            "{schema}.{name} no longer exists or you can't see it."
        ))
    })?;
    let summary = relation_summary(row, schema, 0)?
        .ok_or_else(|| AppError::not_found(format!("{schema}.{name} is not a table or view.")))?;
    let oid: u32 = get(row, 5)?;
    let owner: String = get(row, 6)?;
    let qualified: String = get(row, 7)?;
    let partition_key: Option<String> = get(row, 8)?;
    let view_definition: Option<String> = get(row, 9)?;

    let column_rows = catalog(
        client,
        "SELECT a.attnum, a.attname::text, format_type(a.atttypid, a.atttypmod), NOT a.attnotnull, \
                pg_get_expr(d.adbin, d.adrelid), a.attidentity::text, a.attgenerated::text, \
                col_description(a.attrelid, a.attnum), quote_ident(a.attname) \
         FROM pg_attribute a \
         LEFT JOIN pg_attrdef d ON d.adrelid = a.attrelid AND d.adnum = a.attnum \
         WHERE a.attrelid = $1 AND a.attnum > 0 AND NOT a.attisdropped \
         ORDER BY a.attnum",
        &[&oid],
    )
    .await?;
    let constraint_rows = catalog(
        client,
        "SELECT con.conname::text, con.contype::text, pg_get_constraintdef(con.oid, true), \
                ARRAY(SELECT a.attname::text FROM unnest(con.conkey) WITH ORDINALITY k(attnum, ord) \
                      JOIN pg_attribute a ON a.attrelid = con.conrelid AND a.attnum = k.attnum ORDER BY k.ord), \
                fn.nspname::text, fc.relname::text, \
                ARRAY(SELECT a.attname::text FROM unnest(con.confkey) WITH ORDINALITY k(attnum, ord) \
                      JOIN pg_attribute a ON a.attrelid = con.confrelid AND a.attnum = k.attnum ORDER BY k.ord), \
                quote_ident(con.conname) \
         FROM pg_constraint con \
         LEFT JOIN pg_class fc ON fc.oid = con.confrelid \
         LEFT JOIN pg_namespace fn ON fn.oid = fc.relnamespace \
         WHERE con.conrelid = $1 AND con.contype IN ('p', 'u', 'f', 'c', 'x') \
         ORDER BY CASE con.contype WHEN 'p' THEN 0 WHEN 'u' THEN 1 WHEN 'f' THEN 2 WHEN 'c' THEN 3 ELSE 4 END, \
                  con.conname",
        &[&oid],
    )
    .await?;
    let index_rows = catalog(
        client,
        "SELECT ic.relname::text, am.amname::text, i.indisunique, i.indisprimary, pg_get_indexdef(i.indexrelid), \
                pg_relation_size(i.indexrelid), \
                ARRAY(SELECT pg_get_indexdef(i.indexrelid, k, true) \
                      FROM generate_series(1, i.indnkeyatts::int) k ORDER BY k), \
                EXISTS (SELECT 1 FROM pg_constraint c WHERE c.conindid = i.indexrelid AND c.conrelid = i.indrelid) \
         FROM pg_index i \
         JOIN pg_class ic ON ic.oid = i.indexrelid \
         JOIN pg_am am ON am.oid = ic.relam \
         WHERE i.indrelid = $1 \
         ORDER BY i.indisprimary DESC, ic.relname",
        &[&oid],
    )
    .await?;

    // Constraints first: columns need to know their primary/foreign keys.
    let mut constraints = Vec::with_capacity(constraint_rows.len());
    let mut quoted_constraints = Vec::with_capacity(constraint_rows.len());
    let mut pk_columns: Vec<String> = Vec::new();
    let mut fk_by_column: BTreeMap<String, ForeignKeyRef> = BTreeMap::new();
    for r in &constraint_rows {
        let kind = match get::<String>(r, 1)?.as_str() {
            "p" => ConstraintKind::PrimaryKey,
            "u" => ConstraintKind::Unique,
            "f" => ConstraintKind::ForeignKey,
            "c" => ConstraintKind::Check,
            _ => ConstraintKind::Exclusion,
        };
        let columns: Vec<String> = get(r, 3)?;
        let ref_schema: Option<String> = get(r, 4)?;
        let ref_table: Option<String> = get(r, 5)?;
        let ref_columns: Vec<String> = get(r, 6)?;
        let definition: String = get(r, 2)?;
        if kind == ConstraintKind::PrimaryKey {
            pk_columns.clone_from(&columns);
        }
        let references = match (kind, ref_schema, ref_table) {
            (ConstraintKind::ForeignKey, Some(s), Some(t)) => {
                for (col, ref_col) in columns.iter().zip(&ref_columns) {
                    fk_by_column
                        .entry(col.clone())
                        .or_insert_with(|| ForeignKeyRef {
                            schema: s.clone(),
                            table: t.clone(),
                            column: ref_col.clone(),
                        });
                }
                Some(ConstraintRef {
                    schema: s,
                    table: t,
                    columns: ref_columns,
                })
            }
            _ => None,
        };
        quoted_constraints.push((get::<String>(r, 7)?, definition.clone()));
        constraints.push(ConstraintInfo {
            name: get(r, 0)?,
            kind,
            columns,
            definition,
            references,
        });
    }

    let mut raw_columns = Vec::with_capacity(column_rows.len());
    for r in &column_rows {
        let name: String = get(r, 1)?;
        let identity: String = get(r, 5)?;
        let generated: String = get(r, 6)?;
        let raw_default: Option<String> = get(r, 4)?;
        let shown_default = match (identity.as_str(), generated.as_str()) {
            ("a", _) => Some("generated always as identity".to_owned()),
            ("d", _) => Some("generated by default as identity".to_owned()),
            (_, "s") => raw_default
                .as_ref()
                .map(|e| format!("generated always as ({e}) stored")),
            _ => raw_default.clone(),
        };
        raw_columns.push(RawColumn {
            info: ColumnInfo {
                position: get(r, 0)?,
                is_primary_key: pk_columns.contains(&name),
                foreign_key: fk_by_column.get(&name).cloned(),
                data_type: get(r, 2)?,
                nullable: get(r, 3)?,
                default_value: shown_default,
                comment: get(r, 7)?,
                name,
            },
            quoted: get(r, 8)?,
            identity,
            generated,
            raw_default,
        });
    }

    let mut indexes = Vec::with_capacity(index_rows.len());
    let mut standalone_index_defs = Vec::new();
    for r in &index_rows {
        let definition: String = get(r, 4)?;
        if !get::<bool>(r, 7)? {
            standalone_index_defs.push(definition.clone());
        }
        indexes.push(IndexInfo {
            name: get(r, 0)?,
            method: get(r, 1)?,
            is_unique: get(r, 2)?,
            is_primary: get(r, 3)?,
            definition,
            size_bytes: get(r, 5)?,
            columns: get(r, 6)?,
        });
    }

    let ddl = build_ddl(&DdlInput {
        kind: summary.kind,
        qualified: &qualified,
        columns: &raw_columns,
        constraints: &quoted_constraints,
        partition_key: partition_key.as_deref(),
        view_definition: view_definition.as_deref(),
        standalone_indexes: &standalone_index_defs,
        comment: summary.comment.as_deref(),
    });

    Ok(RelationDetail {
        summary,
        owner,
        columns: raw_columns.into_iter().map(|c| c.info).collect(),
        indexes,
        constraints,
        ddl,
    })
}

struct DdlInput<'a> {
    kind: RelationKind,
    qualified: &'a str,
    columns: &'a [RawColumn],
    /// (quoted name, definition)
    constraints: &'a [(String, String)],
    partition_key: Option<&'a str>,
    view_definition: Option<&'a str>,
    standalone_indexes: &'a [String],
    comment: Option<&'a str>,
}

fn quote_literal(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// Reconstructs a readable CREATE statement. PostgreSQL has no built-in for tables, so this
/// is assembled from catalog pieces the server already formatted (types, defaults, constraints).
fn build_ddl(input: &DdlInput<'_>) -> String {
    let q = input.qualified;
    let mut sections: Vec<String> = Vec::new();
    let kind_word = match input.kind {
        RelationKind::View => "VIEW",
        RelationKind::MaterializedView => "MATERIALIZED VIEW",
        RelationKind::ForeignTable => "FOREIGN TABLE",
        _ => "TABLE",
    };

    match input.kind {
        RelationKind::View => {
            sections.push(format!(
                "CREATE OR REPLACE VIEW {q} AS\n{}",
                input.view_definition.unwrap_or("").trim_end()
            ));
        }
        RelationKind::MaterializedView => {
            sections.push(format!(
                "CREATE MATERIALIZED VIEW {q} AS\n{}",
                input.view_definition.unwrap_or("").trim_end()
            ));
        }
        _ => {
            let width = input
                .columns
                .iter()
                .map(|c| c.quoted.len())
                .max()
                .unwrap_or(0);
            let mut lines: Vec<String> = input
                .columns
                .iter()
                .map(|c| {
                    let mut line = format!("    {:width$} {}", c.quoted, c.info.data_type);
                    match (c.identity.as_str(), c.generated.as_str()) {
                        ("a", _) => line.push_str(" GENERATED ALWAYS AS IDENTITY"),
                        ("d", _) => line.push_str(" GENERATED BY DEFAULT AS IDENTITY"),
                        (_, "s") => {
                            if let Some(expr) = &c.raw_default {
                                line.push_str(&format!(" GENERATED ALWAYS AS ({expr}) STORED"));
                            }
                        }
                        _ => {
                            if let Some(default) = &c.raw_default {
                                line.push_str(&format!(" DEFAULT {default}"));
                            }
                        }
                    }
                    if !c.info.nullable {
                        line.push_str(" NOT NULL");
                    }
                    line
                })
                .collect();
            for (name, definition) in input.constraints {
                lines.push(format!("    CONSTRAINT {name} {definition}"));
            }
            let mut create = format!(
                "CREATE {}{q} (\n{}\n)",
                if input.kind == RelationKind::ForeignTable {
                    "FOREIGN TABLE "
                } else {
                    "TABLE "
                },
                lines.join(",\n")
            );
            if let Some(key) = input.partition_key {
                create.push_str(&format!(" PARTITION BY {key}"));
            }
            create.push(';');
            if input.kind == RelationKind::ForeignTable {
                create.push_str("\n-- Server and options are not shown.");
            }
            sections.push(create);
        }
    }

    if !input.standalone_indexes.is_empty() {
        sections.push(
            input
                .standalone_indexes
                .iter()
                .map(|d| format!("{d};"))
                .collect::<Vec<_>>()
                .join("\n"),
        );
    }

    let mut comments = Vec::new();
    if let Some(c) = input.comment {
        comments.push(format!(
            "COMMENT ON {kind_word} {q} IS {};",
            quote_literal(c)
        ));
    }
    for c in input.columns {
        if let Some(text) = &c.info.comment {
            comments.push(format!(
                "COMMENT ON COLUMN {q}.{} IS {};",
                c.quoted,
                quote_literal(text)
            ));
        }
    }
    if !comments.is_empty() {
        sections.push(comments.join("\n"));
    }
    sections.join("\n\n") + "\n"
}

pub async fn completion_catalog(
    client: &mut Client,
) -> AppResult<BTreeMap<String, BTreeMap<String, Vec<String>>>> {
    let rows = catalog(
        client,
        "SELECT n.nspname::text, c.relname::text, array_agg(a.attname::text ORDER BY a.attnum) \
         FROM pg_class c \
         JOIN pg_namespace n ON n.oid = c.relnamespace \
         JOIN pg_attribute a ON a.attrelid = c.oid AND a.attnum > 0 AND NOT a.attisdropped \
         WHERE c.relkind IN ('r', 'p', 'v', 'm', 'f') AND NOT c.relispartition \
           AND n.nspname NOT IN ('pg_catalog', 'information_schema') AND n.nspname NOT LIKE 'pg\\_toast%' \
           AND has_schema_privilege(n.oid, 'USAGE') \
         GROUP BY n.nspname, c.relname \
         ORDER BY n.nspname, c.relname \
         LIMIT $1",
        &[&MAX_COMPLETION_RELATIONS],
    )
    .await?;
    let mut out: BTreeMap<String, BTreeMap<String, Vec<String>>> = BTreeMap::new();
    for r in &rows {
        out.entry(get(r, 0)?)
            .or_default()
            .insert(get(r, 1)?, get(r, 2)?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn col(
        quoted: &str,
        ty: &str,
        nullable: bool,
        default: Option<&str>,
        identity: &str,
    ) -> RawColumn {
        RawColumn {
            info: ColumnInfo {
                position: 1,
                name: quoted.trim_matches('"').to_owned(),
                data_type: ty.to_owned(),
                nullable,
                default_value: None,
                is_primary_key: false,
                foreign_key: None,
                comment: None,
            },
            quoted: quoted.to_owned(),
            identity: identity.to_owned(),
            generated: String::new(),
            raw_default: default.map(str::to_owned),
        }
    }

    #[test]
    fn table_ddl_includes_columns_constraints_and_indexes() {
        let columns = [
            col("id", "bigint", false, None, "a"),
            col("\"Order\"", "text", true, Some("'x'::text"), ""),
        ];
        let ddl = build_ddl(&DdlInput {
            kind: RelationKind::Table,
            qualified: "sales.orders",
            columns: &columns,
            constraints: &[("orders_pkey".into(), "PRIMARY KEY (id)".into())],
            partition_key: None,
            view_definition: None,
            standalone_indexes: &["CREATE INDEX i ON sales.orders USING btree (\"Order\")".into()],
            comment: Some("It's orders"),
        });
        assert!(ddl.starts_with("CREATE TABLE sales.orders (\n"));
        assert!(ddl.contains("id      bigint GENERATED ALWAYS AS IDENTITY NOT NULL"));
        assert!(ddl.contains("\"Order\" text DEFAULT 'x'::text,"));
        assert!(ddl.contains("CONSTRAINT orders_pkey PRIMARY KEY (id)\n);"));
        assert!(ddl.contains("CREATE INDEX i ON sales.orders USING btree (\"Order\");"));
        assert!(ddl.contains("COMMENT ON TABLE sales.orders IS 'It''s orders';"));
    }

    #[test]
    fn view_ddl_uses_definition() {
        let ddl = build_ddl(&DdlInput {
            kind: RelationKind::View,
            qualified: "r.v",
            columns: &[],
            constraints: &[],
            partition_key: None,
            view_definition: Some(" SELECT 1;"),
            standalone_indexes: &[],
            comment: None,
        });
        assert_eq!(ddl, "CREATE OR REPLACE VIEW r.v AS\n SELECT 1;\n");
    }
}
