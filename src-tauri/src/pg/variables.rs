//! Running and checking SQL that contains `{{variables}}`.
//!
//! Planning takes two prepares (parse + analyse; nothing runs) in one READ ONLY transaction:
//!
//! 1. **Infer.** Date variables are written as `(($n::text)::date)`; every other variable is a
//!    bare `$n`, so PostgreSQL infers the type its position needs (an enum column, `integer`,
//!    `text[]` for `= ANY(…)`, …). When PostgreSQL can't decide for one parameter
//!    (`SELECT {{x}}`), that parameter gets its variable type's default and we try again. This
//!    step runs inside a savepoint, because a failed prepare would abort the transaction.
//! 2. **Bind.** Every variable becomes `(($n::text)::<type>)` (or `text[]` for lists) with
//!    the type chosen in step 1, and every parameter is declared as text. The values are sent
//!    as text and converted by PostgreSQL's own input functions, exactly as for a quoted
//!    literal, without any value ever being placed in the SQL text.
//!
//! If step 1 fails for another reason (a typo, an ambiguous operator), step 2 runs with the
//! default types so the user sees PostgreSQL's real error for their query.

use std::collections::BTreeSet;
use std::time::Instant;

use deadpool_postgres::{Object, Transaction};
use serde::Serialize;
use tokio_postgres::Statement;
use tokio_postgres::error::SqlState;
use tokio_postgres::types::{Kind as PgKind, ToSql, Type};

use crate::error::{AppError, AppResult, ErrorCategory};
use crate::jobs::Jobs;
use crate::lang::lexer::Lexed;
use crate::lang::variables::{
    Bound, BoundValues, Compiled, Placeholder, Prepared, Slot, VariableDef, VariableType, compile,
    prepare, skipped_variables,
};
use crate::model::{QueryFailure, QueryOutcome, QueryResultSet, ResultColumn};
use crate::pg::connect::PoolHandle;
use crate::pg::errors::{Phase, map_pg_error};
use crate::pg::policy::{check_read_only, quote_ident};
use crate::pg::query::{
    MAX_ROW_LIMIT, MAX_SQL_BYTES, begin_read_only, elapsed_ms, fetch, on_server, outcome,
};

/// How a variable reference was typed, for the editor's check panel.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParameterInfo {
    /// `region`, or `period.from` for half of a range.
    pub reference: String,
    /// The PostgreSQL type the value is converted to, e.g. `date`, `order_status[]`.
    pub pg_type: String,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum CheckOutcome {
    Ok(CheckSuccess),
    Failed(QueryFailure),
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckSuccess {
    pub ok: bool,
    pub columns: Vec<ResultColumn>,
    pub parameters: Vec<ParameterInfo>,
    pub duration_ms: u64,
}

struct Plan {
    compiled: Compiled,
    statement: Statement,
    parameters: Vec<ParameterInfo>,
}

fn pg_error(e: &tokio_postgres::Error) -> AppError {
    map_pg_error(e, Phase::Query)
}

/// The quoted type name to cast to, e.g. `"pg_catalog"."int4"`.
fn type_name(ty: &Type) -> String {
    format!("{}.{}", quote_ident(ty.schema()), quote_ident(ty.name()))
}

/// The type a variable gets when PostgreSQL can't infer one.
fn default_type(kind: VariableType) -> Type {
    match kind {
        VariableType::Date | VariableType::DateRange => Type::DATE,
        VariableType::Number => Type::NUMERIC,
        VariableType::Text | VariableType::Enum => Type::TEXT,
        VariableType::MultiEnum => Type::TEXT_ARRAY,
    }
}

fn is_numeric(ty: &Type) -> bool {
    [
        Type::INT2,
        Type::INT4,
        Type::INT8,
        Type::NUMERIC,
        Type::FLOAT4,
        Type::FLOAT8,
        Type::MONEY,
    ]
    .contains(ty)
}

/// Dates always use `date`; their meaning doesn't depend on the context.
fn fixed(slot: &Slot) -> bool {
    matches!(slot.kind, VariableType::Date | VariableType::DateRange)
}

/// Chooses the final type for a slot from what PostgreSQL inferred.
fn choose_type(slot: &Slot, inferred: Option<&Type>) -> AppResult<Type> {
    let at = Some(slot.source_position);
    let Some(ty) = inferred.filter(|t| **t != Type::UNKNOWN) else {
        return Ok(default_type(slot.kind));
    };
    let is_array = matches!(ty.kind(), PgKind::Array(_));
    match slot.kind {
        VariableType::Date | VariableType::DateRange => Ok(Type::DATE),
        VariableType::Number => Ok(if is_numeric(ty) {
            ty.clone()
        } else {
            Type::NUMERIC
        }),
        VariableType::MultiEnum if is_array => Ok(ty.clone()),
        VariableType::MultiEnum => Err(AppError::new(
            ErrorCategory::QueryRejected,
            format!(
                "{{{{{}}}}} holds a list of values, but it is used as a single value.",
                slot.variable
            ),
        )
        .with_hint(format!(
            "Write column IN ({{{{{0}}}}}) to match any of the chosen values.",
            slot.variable
        ))
        .with_position(at)),
        VariableType::Text | VariableType::Enum if is_array => Err(AppError::new(
            ErrorCategory::QueryRejected,
            format!(
                "{{{{{}}}}} is a single value, but it is used as a list.",
                slot.variable
            ),
        )
        .with_hint("Compare with = instead, or change the variable to Multi-select enum.")
        .with_position(at)),
        VariableType::Text | VariableType::Enum => Ok(ty.clone()),
    }
}

fn declared(slot: &Slot) -> Type {
    if slot.is_array() {
        Type::TEXT_ARRAY
    } else {
        Type::TEXT
    }
}

fn policy(compiled: &Compiled) -> AppResult<()> {
    check_read_only(&compiled.sql)
        .map(|_| ())
        .map_err(|e| compiled.map_error(e))
}

/// Pass 1 in a savepoint: returns the inferred type of each slot, or `None` when inference
/// failed for a reason other than an undecidable parameter.
async fn infer(
    tx: &mut Transaction<'_>,
    lexed: &Lexed,
    slots: &[Slot],
    skipped: &[(usize, usize)],
) -> AppResult<Option<Vec<Option<Type>>>> {
    // Some slots get their default type up front: dates always, and any slot PostgreSQL
    // couldn't decide on a previous attempt. Typed slots are numbered first because a
    // prepare can only declare the leading parameter types.
    let mut forced: Vec<bool> = slots.iter().map(fixed).collect();
    for _attempt in 0..=slots.len() {
        let mut order: Vec<usize> = (0..slots.len()).filter(|&i| forced[i]).collect();
        order.extend((0..slots.len()).filter(|&i| !forced[i]));
        let ordered: Vec<Slot> = order.iter().map(|&i| slots[i].clone()).collect();
        let placeholders: Vec<Placeholder> = order
            .iter()
            .map(|&i| {
                if forced[i] {
                    Placeholder::Cast(type_name(&default_type(slots[i].kind)))
                } else {
                    Placeholder::Infer
                }
            })
            .collect();
        let types: Vec<Type> = order
            .iter()
            .filter(|&&i| forced[i])
            .map(|&i| declared(&slots[i]))
            .collect();
        let compiled = compile(lexed, &ordered, &placeholders, skipped)?;
        policy(&compiled)?;

        let savepoint = tx
            .savepoint("intalix_infer")
            .await
            .map_err(|e| pg_error(&e))?;
        let prepared = savepoint.prepare_typed(&compiled.sql, &types).await;
        let result = match prepared {
            Ok(statement) => {
                let mut inferred: Vec<Option<Type>> = vec![None; slots.len()];
                for (n, &i) in order.iter().enumerate() {
                    inferred[i] = statement.params().get(n).cloned();
                }
                Some(Ok(inferred))
            }
            Err(e) if e.code() == Some(&SqlState::INDETERMINATE_DATATYPE) => {
                // "could not determine data type of parameter $N": default that one, retry.
                let n = e
                    .as_db_error()
                    .and_then(|d| d.message().rsplit('$').next()?.trim().parse::<usize>().ok());
                match n.and_then(|n| order.get(n.checked_sub(1)?)) {
                    Some(&i) if !forced[i] => {
                        forced[i] = true;
                        None
                    }
                    _ => Some(Err(())),
                }
            }
            Err(e) if e.code() == Some(&SqlState::QUERY_CANCELED) => {
                return Err(compiled.map_error(pg_error(&e)));
            }
            Err(_) => Some(Err(())),
        };
        savepoint.rollback().await.map_err(|e| pg_error(&e))?;
        match result {
            Some(Ok(inferred)) => return Ok(Some(inferred)),
            Some(Err(())) => return Ok(None),
            None => continue,
        }
    }
    Ok(None)
}

/// Plans `sql`: compiles the variables, prepares the final statement and checks its shape.
/// Conditions using a variable in `empty` are skipped (see `lang::optional`).
async fn plan(
    tx: &mut Transaction<'_>,
    sql: &str,
    defs: &[VariableDef],
    empty: &BTreeSet<String>,
) -> AppResult<Plan> {
    if sql.len() > MAX_SQL_BYTES {
        return Err(AppError::invalid(
            "This query is too large to run (over 1 MB).",
        ));
    }
    let Prepared {
        lexed,
        slots,
        skipped,
    } = prepare(sql, defs, empty)?;

    let types: Vec<Type> = if slots.is_empty() {
        Vec::new()
    } else {
        match infer(tx, &lexed, &slots, &skipped).await? {
            Some(inferred) => slots
                .iter()
                .zip(&inferred)
                .map(|(slot, ty)| choose_type(slot, ty.as_ref()))
                .collect::<AppResult<_>>()?,
            None => slots.iter().map(|s| default_type(s.kind)).collect(),
        }
    };

    let placeholders: Vec<Placeholder> = types
        .iter()
        .map(|t| Placeholder::Cast(type_name(t)))
        .collect();
    let compiled = compile(&lexed, &slots, &placeholders, &skipped)?;
    policy(&compiled)?;
    let declared: Vec<Type> = slots.iter().map(declared).collect();
    let statement = tx
        .prepare_typed(&compiled.sql, &declared)
        .await
        .map_err(|e| compiled.map_error(pg_error(&e)))?;
    if statement.params().len() != slots.len() {
        return Err(AppError::internal(
            "planning variables",
            "parameter count mismatch",
        ));
    }
    let parameters = slots
        .iter()
        .zip(&types)
        .map(|(slot, ty)| ParameterInfo {
            reference: slot.reference(),
            pg_type: match ty.kind() {
                PgKind::Array(member) => format!("{}[]", member.name()),
                _ => ty.name().to_owned(),
            },
        })
        .collect();
    Ok(Plan {
        compiled,
        statement,
        parameters,
    })
}

/// Parses and analyses the query on the server without running it.
pub async fn check(
    handle: &PoolHandle,
    jobs: &Jobs,
    request_id: &str,
    sql: &str,
    defs: &[VariableDef],
) -> AppResult<CheckOutcome> {
    let started = Instant::now();
    let timeout_secs = handle.query_timeout_secs;
    let result = on_server(handle, jobs, request_id, async |client: &mut Object| {
        let mut tx = begin_read_only(client, timeout_secs).await?;
        // The check covers the whole query: nothing is skipped.
        let plan = plan(&mut tx, sql, defs, &BTreeSet::new()).await?;
        let columns = plan
            .statement
            .columns()
            .iter()
            .map(|c| ResultColumn {
                name: c.name().to_owned(),
                pg_type: c.type_().name().to_owned(),
            })
            .collect();
        let parameters = plan.parameters;
        drop(plan.statement);
        if let Err(error) = tx.rollback().await {
            tracing::debug!(%error, "rollback after check failed");
        }
        Ok((columns, parameters))
    })
    .await?;
    Ok(match result {
        Ok((columns, parameters)) => CheckOutcome::Ok(CheckSuccess {
            ok: true,
            columns,
            parameters,
            duration_ms: elapsed_ms(started),
        }),
        Err(error) => CheckOutcome::Failed(QueryFailure::from_error(error, elapsed_ms(started))),
    })
}

/// Runs the query with the given (already validated) values.
pub async fn run(
    handle: &PoolHandle,
    jobs: &Jobs,
    request_id: &str,
    sql: &str,
    defs: &[VariableDef],
    values: &BoundValues,
    row_limit: u32,
) -> AppResult<QueryOutcome> {
    let started = Instant::now();
    let row_limit = row_limit.clamp(1, MAX_ROW_LIMIT);
    let timeout_secs = handle.query_timeout_secs;
    let result = on_server(handle, jobs, request_id, async |client: &mut Object| {
        let mut tx = begin_read_only(client, timeout_secs).await?;
        let plan = plan(&mut tx, sql, defs, &skipped_variables(defs)).await?;
        let owned: Vec<Box<dyn ToSql + Sync + Send>> = plan
            .compiled
            .slots
            .iter()
            .map(|slot| -> Box<dyn ToSql + Sync + Send> {
                match values.get(&(slot.variable.clone(), slot.part)) {
                    Some(Bound::Text(v)) => Box::new(v.clone()),
                    Some(Bound::TextArray(v)) => Box::new(v.clone()),
                    // Validation resolves every defined variable; NULL is the safe fallback.
                    None if slot.is_array() => Box::new(None::<Vec<String>>),
                    None => Box::new(None::<String>),
                }
            })
            .collect();
        let params: Vec<&(dyn ToSql + Sync)> = owned
            .iter()
            .map(|b| b.as_ref() as &(dyn ToSql + Sync))
            .collect();
        let result: AppResult<QueryResultSet> = fetch(&tx, &plan.statement, &params, row_limit)
            .await
            .map_err(|e| plan.compiled.map_error(e));
        drop(params);
        drop(plan);
        if let Err(error) = tx.rollback().await {
            tracing::debug!(%error, "rollback after query failed");
        }
        result
    })
    .await?;
    Ok(outcome(result, started))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slot(kind: VariableType) -> Slot {
        Slot {
            variable: "v".into(),
            part: crate::lang::variables::Part::Whole,
            kind,
            source_position: 5,
        }
    }

    #[test]
    fn chooses_types_from_inference() {
        let enum_like = Type::TEXT_ARRAY;
        assert_eq!(
            choose_type(&slot(VariableType::Text), None).unwrap(),
            Type::TEXT
        );
        assert_eq!(
            choose_type(&slot(VariableType::Text), Some(&Type::UNKNOWN)).unwrap(),
            Type::TEXT
        );
        assert_eq!(
            choose_type(&slot(VariableType::Enum), Some(&Type::INT4)).unwrap(),
            Type::INT4
        );
        assert_eq!(
            choose_type(&slot(VariableType::Number), Some(&Type::TEXT)).unwrap(),
            Type::NUMERIC
        );
        assert_eq!(
            choose_type(&slot(VariableType::Number), Some(&Type::INT8)).unwrap(),
            Type::INT8
        );
        assert_eq!(
            choose_type(&slot(VariableType::Date), Some(&Type::TIMESTAMPTZ)).unwrap(),
            Type::DATE
        );
        assert_eq!(
            choose_type(&slot(VariableType::MultiEnum), Some(&enum_like)).unwrap(),
            Type::TEXT_ARRAY
        );
        let err = choose_type(&slot(VariableType::MultiEnum), Some(&Type::TEXT)).unwrap_err();
        assert!(err.hint.unwrap().contains("IN ("));
        assert_eq!(err.position, Some(5));
        assert!(choose_type(&slot(VariableType::Enum), Some(&Type::INT4_ARRAY)).is_err());
    }

    #[test]
    fn type_names_are_quoted_and_qualified() {
        assert_eq!(type_name(&Type::INT4), "\"pg_catalog\".\"int4\"");
        assert_eq!(type_name(&Type::TEXT_ARRAY), "\"pg_catalog\".\"_text\"");
    }
}
