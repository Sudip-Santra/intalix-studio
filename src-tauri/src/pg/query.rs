//! Read-only query execution with row, memory and time limits.

use std::time::{Duration, Instant};

use deadpool_postgres::Object;

use crate::error::{AppError, AppResult, ErrorCategory};
use crate::jobs::Jobs;
use crate::model::{Cell, QueryOutcome, QueryResultSet, QuerySuccess, ResultColumn};
use crate::pg::connect::{PoolHandle, map_pool_error};
use crate::pg::decode::{Raw, decode};
use crate::pg::errors::{Phase, map_pg_error};
use crate::pg::policy::check_read_only;

/// Largest row limit the UI may ask for.
pub const MAX_ROW_LIMIT: u32 = 50_000;
/// Rows fetched per round trip; small enough to stop early when the memory budget is hit.
const FETCH_BATCH: u32 = 500;
/// Approximate cap on result memory sent to the UI.
const MAX_RESULT_BYTES: usize = 32 * 1024 * 1024;
/// Hard ceiling on SQL text accepted from the UI.
pub const MAX_SQL_BYTES: usize = 1024 * 1024;

pub struct QueryRequest<'a> {
    pub request_id: &'a str,
    pub sql: &'a str,
    pub row_limit: u32,
}

/// Validates, runs and times a query. Every failure becomes a `QueryOutcome::Failed`, except
/// cancellation, which is an `Err(Cancelled)` so the UI can treat it as an abort.
pub async fn execute(
    handle: &PoolHandle,
    jobs: &Jobs,
    request: QueryRequest<'_>,
) -> AppResult<QueryOutcome> {
    let started = Instant::now();
    if request.sql.len() > MAX_SQL_BYTES {
        return Ok(QueryOutcome::failed(
            AppError::invalid("This query is too large to run (over 1 MB)."),
            0,
        ));
    }
    if let Err(error) = check_read_only(request.sql) {
        return Ok(QueryOutcome::failed(error, 0));
    }
    let row_limit = request.row_limit.clamp(1, MAX_ROW_LIMIT);
    let timeout_secs = handle.query_timeout_secs;
    let sql = request.sql;
    let result = on_server(handle, jobs, request.request_id, async |client| {
        run(client, sql, row_limit, timeout_secs).await
    })
    .await?;
    Ok(outcome(result, started))
}

/// Milliseconds since `started`, saturating.
pub fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// Wraps a result set (or a reportable failure) into a timed `QueryOutcome`.
pub fn outcome(result: Result<QueryResultSet, AppError>, started: Instant) -> QueryOutcome {
    match result {
        Ok(result) => QueryOutcome::Ok(QuerySuccess {
            ok: true,
            result,
            duration_ms: elapsed_ms(started),
            simulated: false,
        }),
        Err(error) => QueryOutcome::failed(error, elapsed_ms(started)),
    }
}

/// Runs `work` on a pooled connection with cancellation and a safety timeout.
///
/// Returns `Ok(Ok(value))` on success, `Ok(Err(error))` for a failure to show to the user, and
/// `Err(Cancelled)` when the request was cancelled.
pub async fn on_server<T>(
    handle: &PoolHandle,
    jobs: &Jobs,
    request_id: &str,
    work: impl AsyncFnOnce(&mut Object) -> AppResult<T>,
) -> AppResult<Result<T, AppError>> {
    let mut client = handle.pool.get().await.map_err(map_pool_error);
    if jobs.is_cancelled(request_id) {
        return Err(AppError::cancelled());
    }
    let client = match client.as_mut() {
        Ok(c) => c,
        Err(error) => return Ok(Err(error.clone())),
    };
    if !jobs.running_query(request_id, client.cancel_token(), handle.tls.clone()) {
        return Err(AppError::cancelled());
    }

    // Safety net beyond statement_timeout (e.g. a stalled network): cancel server-side too.
    let timeout = Duration::from_secs(u64::from(handle.query_timeout_secs) + 15);
    match tokio::time::timeout(timeout, work(client)).await {
        Ok(Ok(value)) => Ok(Ok(value)),
        Ok(Err(error)) if error.category == ErrorCategory::Cancelled => Err(error),
        Ok(Err(error)) => Ok(Err(error)),
        Err(_) => {
            jobs.cancel(request_id).await;
            Ok(Err(AppError::new(
                ErrorCategory::Timeout,
                "The query did not finish in time and was stopped.",
            )
            .with_hint(
                "Check the network connection, or narrow the query.",
            )))
        }
    }
}

/// Starts a READ ONLY transaction with a statement timeout.
pub async fn begin_read_only(
    client: &mut Object,
    timeout_secs: u32,
) -> AppResult<deadpool_postgres::Transaction<'_>> {
    let pg = |e: tokio_postgres::Error| map_pg_error(&e, Phase::Query);
    let tx = client
        .build_transaction()
        .read_only(true)
        .start()
        .await
        .map_err(pg)?;
    // Integer interpolation only; no user input reaches this statement.
    tx.batch_execute(&format!(
        "SET LOCAL statement_timeout = {}",
        u64::from(timeout_secs) * 1000
    ))
    .await
    .map_err(pg)?;
    Ok(tx)
}

pub(crate) async fn run(
    client: &mut Object,
    sql: &str,
    row_limit: u32,
    timeout_secs: u32,
) -> AppResult<QueryResultSet> {
    let pg = |e: tokio_postgres::Error| map_pg_error(&e, Phase::Query);
    let tx = begin_read_only(client, timeout_secs).await?;

    // The extended protocol refuses to prepare more than one statement: a second, independent
    // guarantee behind the policy check.
    let statement = tx.prepare(sql).await.map_err(pg)?;
    if !statement.params().is_empty() {
        return Err(AppError::new(
            ErrorCategory::QueryRejected,
            "This query has parameters ($1, $2, …) but no values to fill them.",
        )
        .with_hint("Use the SQL Editor page and write variables as {{name}} instead."));
    }
    let result = fetch(&tx, &statement, &[], row_limit).await?;
    // Read-only, so there is nothing to keep; rollback also releases the portal promptly.
    if let Err(error) = tx.rollback().await {
        tracing::debug!(%error, "rollback after query failed");
    }
    Ok(result)
}

/// Binds `params` and reads up to `row_limit` rows, within the row and memory budgets.
pub async fn fetch(
    tx: &deadpool_postgres::Transaction<'_>,
    statement: &tokio_postgres::Statement,
    params: &[&(dyn tokio_postgres::types::ToSql + Sync)],
    row_limit: u32,
) -> AppResult<QueryResultSet> {
    struct Collect {
        columns: Vec<ResultColumn>,
        rows: Vec<Vec<Cell>>,
        bytes: usize,
    }
    impl RowSink for Collect {
        fn start(&mut self, columns: &[ResultColumn]) -> AppResult<()> {
            self.columns = columns.to_vec();
            Ok(())
        }
        fn row(&mut self, cells: Vec<Cell>) -> AppResult<bool> {
            self.bytes += cells.iter().map(cell_size).sum::<usize>();
            self.rows.push(cells);
            // The UI result has a memory budget; reports stream instead.
            Ok(self.bytes <= MAX_RESULT_BYTES)
        }
    }
    let mut sink = Collect {
        columns: Vec::new(),
        rows: Vec::new(),
        bytes: 0,
    };
    let fetched = fetch_each(tx, statement, params, u64::from(row_limit), &mut sink).await?;
    Ok(QueryResultSet {
        columns: sink.columns,
        rows: sink.rows,
        truncated: fetched.truncated,
        row_limit,
    })
}

/// Receives a result one row at a time (reports stream rows straight into their files).
pub trait RowSink: Send {
    fn start(&mut self, columns: &[ResultColumn]) -> AppResult<()>;
    /// Returns `false` to stop reading; the result then counts as truncated.
    fn row(&mut self, cells: Vec<Cell>) -> AppResult<bool>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fetched {
    pub rows: u64,
    /// More rows existed than were delivered.
    pub truncated: bool,
}

/// Binds `params` and delivers up to `max_rows` rows to `sink`, in batches.
pub async fn fetch_each(
    tx: &deadpool_postgres::Transaction<'_>,
    statement: &tokio_postgres::Statement,
    params: &[&(dyn tokio_postgres::types::ToSql + Sync)],
    max_rows: u64,
    sink: &mut dyn RowSink,
) -> AppResult<Fetched> {
    let pg = |e: tokio_postgres::Error| map_pg_error(&e, Phase::Query);
    let columns: Vec<ResultColumn> = statement
        .columns()
        .iter()
        .map(|c| ResultColumn {
            name: c.name().to_owned(),
            pg_type: c.type_().name().to_owned(),
        })
        .collect();
    let types: Vec<_> = statement
        .columns()
        .iter()
        .map(|c| c.type_().clone())
        .collect();
    sink.start(&columns)?;

    let portal = tx.bind(statement, params).await.map_err(pg)?;
    let mut delivered = 0u64;
    let mut truncated = false;

    // Fetch max + 1 rows in batches; the extra row tells us whether more exist.
    'fetch: loop {
        let wanted = (max_rows + 1).saturating_sub(delivered);
        let batch_size = wanted.min(u64::from(FETCH_BATCH));
        let batch = tx
            .query_portal(&portal, i32::try_from(batch_size).unwrap_or(i32::MAX))
            .await
            .map_err(pg)?;
        let exhausted = (batch.len() as u64) < batch_size;
        for row in &batch {
            if delivered >= max_rows {
                truncated = true;
                break 'fetch;
            }
            let mut cells = Vec::with_capacity(types.len());
            for (i, ty) in types.iter().enumerate() {
                let raw: Raw<'_> = row
                    .try_get(i)
                    .map_err(|e| AppError::internal("reading result row", e))?;
                cells.push(decode(ty, raw.0));
            }
            delivered += 1;
            if !sink.row(cells)? {
                truncated = true;
                break 'fetch;
            }
        }
        if exhausted {
            break;
        }
    }
    Ok(Fetched {
        rows: delivered,
        truncated,
    })
}

/// Rough in-memory/serialised size of a cell, for the result budget.
fn cell_size(cell: &Cell) -> usize {
    16 + match cell {
        Cell::Text { v, .. } | Cell::Json { v, .. } => v.len(),
        Cell::Int { v }
        | Cell::Decimal { v }
        | Cell::Uuid { v }
        | Cell::Date { v }
        | Cell::Time { v } => v.len(),
        Cell::Timestamp { v } | Cell::Timestamptz { v } => v.len(),
        Cell::Unsupported { pg_type } => pg_type.len(),
        Cell::Null | Cell::Bool { .. } | Cell::Float { .. } | Cell::Bytes { .. } => 8,
    }
}

/// `SELECT * FROM "schema"."name" LIMIT n` with both identifiers safely quoted.
pub fn preview_sql(schema: &str, name: &str, row_limit: u32) -> String {
    use crate::pg::policy::quote_ident;
    format!(
        "SELECT * FROM {}.{} LIMIT {}",
        quote_ident(schema),
        quote_ident(name),
        row_limit.clamp(1, MAX_ROW_LIMIT) + 1
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_sql_quotes_identifiers_and_passes_policy() {
        let sql = preview_sql("sales", "Order \"Items\"; DROP TABLE x", 100);
        assert_eq!(
            sql,
            "SELECT * FROM \"sales\".\"Order \"\"Items\"\"; DROP TABLE x\" LIMIT 101"
        );
        assert!(check_read_only(&sql).is_ok());
    }
}
