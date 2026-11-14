//! Application services: the logic behind each command. Commands stay thin wrappers.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use secrecy::{ExposeSecret, SecretString};
use serde::Serialize;
use serde_json::Value;

use crate::error::{AppError, AppResult, ErrorCategory};
use crate::jobs::{Jobs, validate_request_id};
use crate::lang::analyze::{Analysis, VariableMode, analyze};
use crate::lang::catalog::{Catalog, Catalogs};
use crate::lang::columns::compared_column;
use crate::lang::complete::{Completions, SignatureHelp, complete, signature};
use crate::lang::lexer::{Kind, lex};
use crate::lang::variables::{
    MAX_LOOKUP_SQL_BYTES, OptionItem, VariableDef, resolve_values, validate_definitions,
};
use crate::model::{
    Cell, ConnectionDraft, ConnectionProfile, ConnectionTarget, ConnectionTestResult, DatabaseInfo,
    QueryFailure, QueryOutcome, RelationDetail, SaveConnectionResult, SchemaObjects, SchemaSummary,
};
use crate::pg::connect::{PoolHandle, Pools, map_pool_error, test_connection};
use crate::pg::variables::CheckOutcome;
use crate::pg::{introspect, query};
use crate::report::{
    self, PreviewOutcome, PreviewSuccess, ReportOutcome, ReportRequest, ReportSuccess,
};
use crate::saved_queries::{
    MAX_SAVED_QUERIES, SavedQuery, SavedQueryDraft, validate_draft as validate_query_draft,
    validate_query_id,
};
use crate::secrets::Secrets;
use crate::storage::Store;
use crate::templates::{
    MAX_TEMPLATES, ReportTemplate, TemplateDraft, validate_draft, validate_template_id,
};

pub struct AppState {
    pub store: Store,
    pub secrets: Secrets,
    pub pools: Pools,
    pub jobs: Jobs,
    /// Database names for SQL hints, per connection.
    pub catalogs: Catalogs,
    /// Report files written in this session; only these can be opened from the UI.
    pub generated: std::sync::Mutex<std::collections::HashSet<std::path::PathBuf>>,
    /// Rows and layout of the last report preview.
    pub previews: report::cache::PreviewCache,
}

/// Choices for an enum variable, from its lookup query.
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum OptionsOutcome {
    Ok(OptionsSuccess),
    Failed(QueryFailure),
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OptionsSuccess {
    pub ok: bool,
    pub options: Vec<OptionItem>,
    /// True when the query returned more rows than are offered.
    pub truncated: bool,
    /// For a Multi-select filled from a column: `schema.table.column`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<String>,
}

/// Most choices a lookup query can offer.
const MAX_LOOKUP_OPTIONS: u32 = 1000;
/// How long hints wait for the catalog before answering without it.
const CATALOG_WAIT: Duration = Duration::from_secs(4);

/// Profile ids are UUIDs we generated; anything else is rejected before touching storage.
fn validate_profile_id(id: &str) -> AppResult<()> {
    uuid::Uuid::parse_str(id)
        .map(|_| ())
        .map_err(|_| AppError::invalid("Invalid connection id."))
}

fn validate_identifier(label: &str, value: &str) -> AppResult<()> {
    if value.is_empty() || value.len() > 256 {
        return Err(AppError::invalid(format!("Invalid {label} name.")));
    }
    Ok(())
}

impl AppState {
    /* ---------------------------------- connections ---------------------------------- */

    pub async fn list_connections(&self) -> AppResult<Vec<ConnectionProfile>> {
        self.store.list_profiles().await
    }

    /// The password to use: the one typed in the form, or the saved one when editing.
    fn resolve_password(
        &self,
        draft: &ConnectionDraft,
        existing_id: Option<&str>,
    ) -> AppResult<Option<SecretString>> {
        if !draft.password.expose_secret().is_empty() {
            return Ok(Some(draft.password.clone()));
        }
        match existing_id {
            Some(id) => self.secrets.get(id),
            None => Ok(None),
        }
    }

    /// Runs a connection test as a cancellable task.
    async fn run_test(
        &self,
        request_id: &str,
        target: ConnectionTarget,
        password: SecretString,
    ) -> AppResult<ConnectionTestResult> {
        self.jobs.register(request_id);
        let task = tokio::spawn(async move { test_connection(&target, &password).await });
        if !self.jobs.running_task(request_id, task.abort_handle()) {
            self.jobs.finish(request_id);
            return Err(AppError::cancelled());
        }
        let result = task.await;
        self.jobs.finish(request_id);
        match result {
            Ok(outcome) => Ok(outcome),
            Err(e) if e.is_cancelled() => Err(AppError::cancelled()),
            Err(e) => Err(AppError::internal("connection test task", e)),
        }
    }

    pub async fn test_connection(
        &self,
        request_id: &str,
        mut draft: ConnectionDraft,
        existing_id: Option<String>,
    ) -> AppResult<ConnectionTestResult> {
        validate_request_id(request_id)?;
        if let Some(id) = &existing_id {
            validate_profile_id(id)?;
        }
        if let Err(error) = draft.validate() {
            return Ok(ConnectionTestResult::failed(error));
        }
        let Some(password) = self.resolve_password(&draft, existing_id.as_deref())? else {
            return Ok(ConnectionTestResult::failed(
                AppError::new(
                    ErrorCategory::AuthenticationFailed,
                    "Enter the password for this user.",
                )
                .with_hint("The password is needed to connect."),
            ));
        };
        self.run_test(request_id, ConnectionTarget::from(&draft), password)
            .await
    }

    /// Tests the connection and saves it only if the test succeeds.
    pub async fn save_connection(
        &self,
        request_id: &str,
        mut draft: ConnectionDraft,
        id: Option<String>,
    ) -> AppResult<SaveConnectionResult> {
        validate_request_id(request_id)?;
        if let Some(id) = &id {
            validate_profile_id(id)?;
            // Editing must target an existing profile.
            self.store.get_profile(id).await?;
        }
        if let Err(error) = draft.validate() {
            return Ok(SaveConnectionResult {
                test: ConnectionTestResult::failed(error),
                profile: None,
            });
        }
        let Some(password) = self.resolve_password(&draft, id.as_deref())? else {
            return Ok(SaveConnectionResult {
                test: ConnectionTestResult::failed(AppError::new(
                    ErrorCategory::AuthenticationFailed,
                    "Enter the password for this user.",
                )),
                profile: None,
            });
        };

        let test = self
            .run_test(request_id, ConnectionTarget::from(&draft), password.clone())
            .await?;
        if !test.is_ok() {
            return Ok(SaveConnectionResult {
                test,
                profile: None,
            });
        }

        let id = id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        self.secrets.store(&id, &password, draft.save_password)?;
        let profile = self
            .store
            .upsert_profile(&id, &draft, draft.save_password)
            .await?;
        // Settings may have changed; the next use builds a fresh pool.
        self.pools.invalidate(&id).await;
        self.catalogs.invalidate(&id);
        tracing::info!(profile = %id, env = draft.environment.as_str(), "connection saved");
        Ok(SaveConnectionResult {
            test,
            profile: Some(profile),
        })
    }

    pub async fn delete_connection(&self, id: &str) -> AppResult<()> {
        validate_profile_id(id)?;
        self.pools.invalidate(id).await;
        self.catalogs.invalidate(id);
        self.secrets.forget(id)?;
        self.store.delete_profile(id).await?;
        tracing::info!(profile = %id, "connection deleted");
        Ok(())
    }

    pub async fn disconnect(&self, id: &str) -> AppResult<()> {
        validate_profile_id(id)?;
        self.pools.invalidate(id).await;
        self.catalogs.invalidate(id);
        Ok(())
    }

    /* ------------------------------------ templates ------------------------------------ */

    pub async fn list_templates(&self) -> AppResult<Vec<ReportTemplate>> {
        self.store.list_templates().await
    }

    /// Creates a template (no `id`) or replaces an existing one.
    pub async fn save_template(
        &self,
        draft: TemplateDraft,
        id: Option<String>,
    ) -> AppResult<ReportTemplate> {
        let (name, definition) = validate_draft(&draft)?;
        let id = match id {
            Some(id) => {
                validate_template_id(&id)?;
                // Updating must not silently recreate a template that was deleted.
                self.store.get_template(&id).await?;
                id
            }
            None => {
                if self.store.count_templates().await? >= MAX_TEMPLATES {
                    return Err(AppError::invalid(format!(
                        "You can keep up to {MAX_TEMPLATES} templates. Delete some to make room."
                    )));
                }
                uuid::Uuid::new_v4().to_string()
            }
        };
        let saved = self.store.upsert_template(&id, &name, &definition).await?;
        tracing::info!(template = %id, "template saved");
        Ok(saved)
    }

    pub async fn delete_template(&self, id: &str) -> AppResult<()> {
        validate_template_id(id)?;
        self.store.delete_template(id).await?;
        tracing::info!(template = %id, "template deleted");
        Ok(())
    }

    /* ---------------------------------- saved queries ---------------------------------- */

    pub async fn list_saved_queries(&self) -> AppResult<Vec<SavedQuery>> {
        self.store.list_saved_queries().await
    }

    /// Creates a saved query (no `id`) or replaces an existing one.
    pub async fn save_query(
        &self,
        draft: SavedQueryDraft,
        id: Option<String>,
    ) -> AppResult<SavedQuery> {
        let draft = validate_query_draft(draft)?;
        let id = match id {
            Some(id) => {
                validate_query_id(&id)?;
                // Updating must not silently recreate a query that was deleted.
                self.store.get_saved_query(&id).await?;
                id
            }
            None => {
                if self.store.count_saved_queries().await? >= MAX_SAVED_QUERIES {
                    return Err(AppError::invalid(format!(
                        "You can keep up to {MAX_SAVED_QUERIES} saved queries. Delete some to make room."
                    )));
                }
                uuid::Uuid::new_v4().to_string()
            }
        };
        let saved = self.store.upsert_saved_query(&id, &draft).await?;
        tracing::info!(query = %id, "saved query stored");
        Ok(saved)
    }

    pub async fn delete_saved_query(&self, id: &str) -> AppResult<()> {
        validate_query_id(id)?;
        self.store.delete_saved_query(id).await?;
        tracing::info!(query = %id, "saved query deleted");
        Ok(())
    }

    /* ------------------------------ SQL language service ------------------------------ */

    /// Instant offline checks. `variables` is `None` where variables are not available.
    pub fn sql_analyze(
        &self,
        sql: &str,
        variables: Option<Vec<VariableDef>>,
    ) -> AppResult<Analysis> {
        check_sql_size(sql)?;
        let mode = match &variables {
            Some(defs) => VariableMode::Defined(defs),
            None => VariableMode::NotAvailable,
        };
        Ok(analyze(sql, mode))
    }

    /// The cached catalog, or an empty one if it can't be loaded quickly. Hints must never
    /// wait long or fail because the catalog is unavailable.
    async fn hint_catalog(&self, profile_id: &str) -> AppResult<Arc<Catalog>> {
        validate_profile_id(profile_id)?;
        let load = self.catalogs.get_or_load(profile_id, || async {
            let mut client = self.client(profile_id).await?;
            introspect::language_catalog(&mut client).await
        });
        Ok(match tokio::time::timeout(CATALOG_WAIT, load).await {
            Ok(Ok(catalog)) => catalog,
            Ok(Err(error)) => {
                tracing::warn!(category = ?error.category, "catalog for hints unavailable");
                Arc::new(Catalog::default())
            }
            Err(_) => Arc::new(Catalog::default()),
        })
    }

    /// Forgets the cached catalog (e.g. after tables were added) and loads it again.
    pub async fn sql_refresh_catalog(&self, profile_id: &str) -> AppResult<()> {
        validate_profile_id(profile_id)?;
        self.catalogs.invalidate(profile_id);
        self.hint_catalog(profile_id).await.map(|_| ())
    }

    pub async fn sql_complete(
        &self,
        profile_id: &str,
        sql: &str,
        cursor: usize,
        variables: Option<Vec<VariableDef>>,
    ) -> AppResult<Completions> {
        check_sql_size(sql)?;
        let catalog = self.hint_catalog(profile_id).await?;
        Ok(complete(sql, cursor, variables.as_deref(), &catalog))
    }

    pub async fn sql_signature(
        &self,
        profile_id: &str,
        sql: &str,
        cursor: usize,
    ) -> AppResult<Option<SignatureHelp>> {
        check_sql_size(sql)?;
        let catalog = self.hint_catalog(profile_id).await?;
        Ok(signature(sql, cursor, &catalog))
    }

    /// Runs `work` as a registered, cancellable request on the profile's pool. Pool and
    /// profile errors become `on_error(error)`, so the UI shows them like query errors.
    async fn pooled_request<T>(
        &self,
        request_id: &str,
        profile_id: &str,
        work: impl AsyncFnOnce(&PoolHandle) -> AppResult<T>,
        on_error: impl FnOnce(AppError) -> T,
    ) -> AppResult<T> {
        validate_request_id(request_id)?;
        self.jobs.register(request_id);
        let result = async {
            let handle = self.pool(profile_id).await?;
            work(&handle).await
        }
        .await;
        self.jobs.finish(request_id);
        match result {
            Err(error) if error.category != ErrorCategory::Cancelled => Ok(on_error(error)),
            other => other,
        }
    }

    /// Parses and analyses the query on the server without running it.
    pub async fn sql_check(
        &self,
        request_id: &str,
        profile_id: &str,
        sql: &str,
        variables: Vec<VariableDef>,
    ) -> AppResult<CheckOutcome> {
        let failed = |e: AppError| CheckOutcome::Failed(QueryFailure::from_error(e, 0));
        if let Err(error) = validate_definitions(&variables) {
            return Ok(failed(error));
        }
        self.pooled_request(
            request_id,
            profile_id,
            async |handle| {
                crate::pg::variables::check(handle, &self.jobs, request_id, sql, &variables).await
            },
            failed,
        )
        .await
    }

    /// Runs a SQL Editor query with its filter values.
    pub async fn sql_run(
        &self,
        request_id: &str,
        profile_id: &str,
        sql: &str,
        variables: Vec<VariableDef>,
        values: BTreeMap<String, Value>,
        row_limit: u32,
    ) -> AppResult<QueryOutcome> {
        let bound = match validate_definitions(&variables)
            .and_then(|()| resolve_values(&variables, &values))
        {
            Ok(bound) => bound,
            Err(error) => return Ok(QueryOutcome::failed(error, 0)),
        };
        self.pooled_request(
            request_id,
            profile_id,
            async |handle| {
                crate::pg::variables::run(
                    handle, &self.jobs, request_id, sql, &variables, &bound, row_limit,
                )
                .await
            },
            |e| QueryOutcome::failed(e, 0),
        )
        .await
    }

    /// Distinct values of the column a Multi-select variable is compared with in `sql`,
    /// e.g. `status` in `WHERE status IN ({{status}})`.
    pub async fn column_values(
        &self,
        request_id: &str,
        profile_id: &str,
        sql: &str,
        variable: &str,
    ) -> AppResult<OptionsOutcome> {
        check_sql_size(sql)?;
        let catalog = self.hint_catalog(profile_id).await?;
        let Some(column) = compared_column(&lex(sql), &catalog, &variable.to_lowercase()) else {
            return Ok(OptionsOutcome::Failed(QueryFailure::from_error(
                AppError::invalid(format!(
                    "Couldn't tell which column {{{{{variable}}}}} filters."
                ))
                .with_hint(format!(
                    "Compare it with a table column, e.g. status IN ({{{{{variable}}}}})."
                )),
                0,
            )));
        };
        let values_sql = column.distinct_values_sql(MAX_LOOKUP_OPTIONS);
        let mut outcome = self
            .variable_options(request_id, profile_id, &values_sql)
            .await?;
        if let OptionsOutcome::Ok(success) = &mut outcome {
            success.column = Some(column.label());
        }
        Ok(outcome)
    }

    /// Runs an enum variable's lookup query. Column 1 is the value, column 2 (optional) the
    /// label shown in the filter.
    pub async fn variable_options(
        &self,
        request_id: &str,
        profile_id: &str,
        sql: &str,
    ) -> AppResult<OptionsOutcome> {
        let failed = |e: AppError| OptionsOutcome::Failed(QueryFailure::from_error(e, 0));
        if sql.len() > MAX_LOOKUP_SQL_BYTES {
            return Ok(failed(AppError::invalid("The lookup query is too long.")));
        }
        let lexed = lex(sql);
        if let Some(t) = lexed.tokens.iter().find(|t| t.kind == Kind::Variable) {
            return Ok(failed(
                AppError::new(
                    ErrorCategory::QueryRejected,
                    "Lookup queries can't use variables.",
                )
                .with_position(u32::try_from(t.start + 1).ok()),
            ));
        }
        let outcome = self
            .execute_query(request_id, profile_id, sql, MAX_LOOKUP_OPTIONS)
            .await?;
        Ok(match outcome {
            QueryOutcome::Failed(f) => OptionsOutcome::Failed(f),
            QueryOutcome::Ok(success) => {
                let mut seen = std::collections::BTreeSet::new();
                let options = success
                    .result
                    .rows
                    .iter()
                    .filter_map(|row| {
                        let value = cell_text(row.first()?)?;
                        let label = row.get(1).and_then(cell_text).unwrap_or_default();
                        seen.insert(value.clone())
                            .then_some(OptionItem { value, label })
                    })
                    .collect();
                OptionsOutcome::Ok(OptionsSuccess {
                    ok: true,
                    options,
                    truncated: success.result.truncated,
                    column: None,
                })
            }
        })
    }

    /* ------------------------------------- reports ------------------------------------- */

    /// Runs a SQL Editor query in full and writes it as PDF and/or Excel with a template.
    /// Asks where to save first; closing that dialog cancels.
    pub async fn generate_report(
        &self,
        app: &tauri::AppHandle,
        request_id: &str,
        profile_id: &str,
        request: ReportRequest,
    ) -> AppResult<ReportOutcome> {
        use tauri_plugin_dialog::DialogExt;

        validate_request_id(request_id)?;
        validate_profile_id(profile_id)?;
        check_sql_size(&request.sql)?;
        let started = std::time::Instant::now();
        let failed = |e: AppError| {
            Ok(ReportOutcome::Failed(QueryFailure::from_error(
                e,
                report::elapsed(started),
            )))
        };
        if let Err(error) = validate_definitions(&request.variables) {
            return failed(error);
        }
        let bound = match resolve_values(&request.variables, &request.values) {
            Ok(bound) => bound,
            Err(error) => return failed(error),
        };
        let mut prepared = match report::prepare(&request) {
            Ok(p) => p,
            Err(error) => return failed(error),
        };
        // A fresh preview that read every row can stand in for running the query again.
        let data_key = report::cache::data_key(profile_id, &request);
        let kept = self.previews.rows(&data_key).filter(|k| k.complete);
        let kept_layout = kept.as_ref().and_then(|_| {
            self.previews
                .layout(&data_key, &report::cache::layout_key(&request))
        });

        // Where to save. With both formats the Excel file goes next to the PDF.
        let ext = if request.pdf { "pdf" } else { "xlsx" };
        let dialog = app
            .dialog()
            .file()
            .set_title("Save report")
            .set_file_name(format!("{}.{ext}", report::file_stem(&prepared.title)))
            .add_filter(
                if request.pdf {
                    "PDF report"
                } else {
                    "Excel workbook"
                },
                &[ext],
            );
        let chosen = tokio::task::spawn_blocking(move || dialog.blocking_save_file())
            .await
            .map_err(|e| AppError::internal("save dialog", e))?;
        let Some(chosen) = chosen else {
            return Err(AppError::cancelled());
        };
        let chosen = chosen
            .into_path()
            .map_err(|e| AppError::internal("save dialog path", e))?;
        let (pdf_path, xlsx_path) = report::targets(&chosen, request.pdf, request.xlsx);

        // Stream the rows into the report (or replay the preview's).
        let fetched = if let Some(kept) = &kept {
            let sink = &mut prepared.sink;
            let max_rows = prepared.max_rows;
            match tokio::task::block_in_place(|| report::cache::replay(kept, sink, max_rows)) {
                Ok(fetched) => fetched,
                Err(error) => return failed(error),
            }
        } else {
            self.jobs.register(request_id);
            let streamed = async {
                let handle = self.pool(profile_id).await?;
                crate::pg::variables::stream(
                    &handle,
                    &self.jobs,
                    request_id,
                    &request.sql,
                    &request.variables,
                    &bound,
                    prepared.max_rows,
                    &mut prepared.sink,
                )
                .await
            }
            .await;
            self.jobs.finish(request_id);
            match streamed {
                Err(error) if error.category == ErrorCategory::Cancelled => return Err(error),
                Err(error) | Ok(Err(error)) => return failed(error),
                Ok(Ok(fetched)) => fetched,
            }
        };

        // Write the files. Layout and compression are CPU work, kept off the async threads.
        let mut files = Vec::new();
        let mut warnings = Vec::new();
        let describe = |e: &AppError| match &e.hint {
            Some(hint) => format!("{} {hint}", e.message),
            None => e.message.clone(),
        };
        let xlsx = prepared.sink.take_xlsx();
        let data = &prepared.sink.data;
        if let (Some(path), Some(x)) = (&xlsx_path, xlsx) {
            match tokio::task::block_in_place(|| report::write_xlsx(x, data, path)) {
                Ok(()) => files.push(report::generated("xlsx", path, None)),
                Err(e) => warnings.push(format!("Excel was not created. {}", describe(&e))),
            }
            if fetched.truncated {
                warnings.push(format!(
                    "The query returned more than {} rows; the Excel file has the first {}.",
                    crate::report::EXCEL_MAX_ROWS,
                    crate::report::EXCEL_MAX_ROWS
                ));
            }
        }
        if let Some(path) = &pdf_path {
            if prepared.sink.pdf_overflow {
                warnings.push(format!(
                    "The PDF was not created: the report has more than {} rows. Use Excel for large reports, or narrow the filters.",
                    crate::report::PDF_MAX_ROWS
                ));
            } else {
                let template = &prepared.template;
                let written = tokio::task::block_in_place(|| match &kept_layout {
                    Some(laid) => report::save_pdf(laid, path),
                    None => report::write_pdf(data, template, path),
                });
                match written {
                    Ok(written) => {
                        files.push(report::generated("pdf", path, Some(written.pages)));
                        if written.shortened {
                            warnings.push(report::SHORTENED_NOTE.to_owned());
                        }
                    }
                    Err(e) => warnings.push(format!("The PDF was not created. {}", describe(&e))),
                }
            }
        }
        if files.is_empty() {
            return failed(AppError::invalid(warnings.join(" ")));
        }

        let mut generated = self
            .generated
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for f in &files {
            generated.insert(std::path::PathBuf::from(&f.path));
        }
        drop(generated);
        tracing::info!(files = files.len(), rows = fetched.rows, "report generated");
        Ok(ReportOutcome::Ok(ReportSuccess {
            ok: true,
            files,
            rows: fetched.rows,
            warnings,
            from_preview: kept.is_some(),
            duration_ms: report::elapsed(started),
        }))
    }

    /// Runs the first rows of a SQL Editor query and draws the first PDF pages.
    pub async fn preview_report(
        &self,
        request_id: &str,
        profile_id: &str,
        request: ReportRequest,
    ) -> AppResult<PreviewOutcome> {
        validate_request_id(request_id)?;
        validate_profile_id(profile_id)?;
        check_sql_size(&request.sql)?;
        let started = std::time::Instant::now();
        let failed = |e: AppError| {
            Ok(PreviewOutcome::Failed(QueryFailure::from_error(
                e,
                report::elapsed(started),
            )))
        };
        if let Err(error) = validate_definitions(&request.variables) {
            return failed(error);
        }
        let bound = match resolve_values(&request.variables, &request.values) {
            Ok(bound) => bound,
            Err(error) => return failed(error),
        };
        let mut prepared = match report::prepare_preview(&request) {
            Ok(p) => p,
            Err(error) => return failed(error),
        };
        let data_key = report::cache::data_key(profile_id, &request);

        // Rows: kept from the last preview when only the look changed, otherwise read now.
        let fetched = if let Some(kept) = self.previews.rows(&data_key) {
            match report::cache::replay(&kept, &mut prepared.sink, prepared.max_rows) {
                Ok(fetched) => fetched,
                Err(error) => return failed(error),
            }
        } else {
            self.jobs.register(request_id);
            let mut recorder = report::cache::Recorder::new(&mut prepared.sink);
            let streamed = async {
                let handle = self.pool(profile_id).await?;
                crate::pg::variables::stream(
                    &handle,
                    &self.jobs,
                    request_id,
                    &request.sql,
                    &request.variables,
                    &bound,
                    prepared.max_rows,
                    &mut recorder,
                )
                .await
            }
            .await;
            self.jobs.finish(request_id);
            let fetched = match streamed {
                Err(error) if error.category == ErrorCategory::Cancelled => return Err(error),
                Err(error) | Ok(Err(error)) => return failed(error),
                Ok(Ok(fetched)) => fetched,
            };
            let mut kept = recorder.kept;
            kept.complete = !fetched.truncated;
            self.previews.keep_rows(data_key.clone(), kept);
            fetched
        };
        let read_ms = report::elapsed(started);

        let data = &prepared.sink.data;
        let template = &prepared.template;
        let laid = match tokio::task::block_in_place(|| report::pdf::layout(data, template)) {
            Ok(laid) => std::sync::Arc::new(laid),
            Err(error) => return failed(error),
        };
        let laid_ms = report::elapsed(started);
        let drawn = tokio::task::block_in_place(|| report::preview_pages(&laid));
        tracing::info!(
            rows = fetched.rows,
            read_ms,
            layout_ms = laid_ms - read_ms,
            draw_ms = report::elapsed(started) - laid_ms,
            "report preview"
        );
        self.previews
            .keep_layout(&data_key, report::cache::layout_key(&request), laid);
        match drawn {
            Ok(drawn) => Ok(PreviewOutcome::Ok(PreviewSuccess {
                ok: true,
                pages: drawn.images,
                page_count: drawn.page_count,
                rows: fetched.rows,
                more_rows: fetched.truncated,
                shortened: drawn.shortened,
                crowded: drawn.crowded,
                duration_ms: report::elapsed(started),
            })),
            Err(error) => failed(error),
        }
    }

    /// Opens a report written in this session, or shows it in File Explorer.
    pub async fn open_report(&self, path: &str, reveal: bool) -> AppResult<()> {
        let path = std::path::PathBuf::from(path);
        let known = self
            .generated
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .contains(&path);
        if !known {
            return Err(AppError::invalid(
                "Only reports created here can be opened.",
            ));
        }
        if !path.exists() {
            return Err(AppError::not_found("The report file was moved or deleted."));
        }
        tokio::task::spawn_blocking(move || {
            if reveal {
                tauri_plugin_opener::reveal_item_in_dir(&path)
            } else {
                tauri_plugin_opener::open_path(&path, None::<&str>)
            }
        })
        .await
        .map_err(|e| AppError::internal("opening the report", e))?
        .map_err(|e| AppError::invalid("The report could not be opened.").with_hint(e.to_string()))
    }

    /* --------------------------------- pooled access --------------------------------- */

    async fn pool(&self, profile_id: &str) -> AppResult<PoolHandle> {
        validate_profile_id(profile_id)?;
        let profile = self.store.get_profile(profile_id).await?;
        let target = ConnectionTarget::from(&profile);
        self.pools
            .get_or_create(profile_id, &target, || self.secrets.require(profile_id))
            .await
    }

    async fn client(&self, profile_id: &str) -> AppResult<deadpool_postgres::Object> {
        self.pool(profile_id)
            .await?
            .pool
            .get()
            .await
            .map_err(map_pool_error)
    }

    /* ------------------------------------- schema ------------------------------------- */

    pub async fn database_info(&self, profile_id: &str) -> AppResult<DatabaseInfo> {
        let mut client = self.client(profile_id).await?;
        let info = introspect::database_info(&mut client).await?;
        // Opening the workspace counts as using the connection.
        if let Err(error) = self.store.touch_connected(profile_id).await {
            tracing::warn!(?error, "could not record last use");
        }
        Ok(info)
    }

    pub async fn list_schemas(&self, profile_id: &str) -> AppResult<Vec<SchemaSummary>> {
        let mut client = self.client(profile_id).await?;
        introspect::list_schemas(&mut client).await
    }

    pub async fn list_objects(&self, profile_id: &str, schema: &str) -> AppResult<SchemaObjects> {
        validate_identifier("schema", schema)?;
        let mut client = self.client(profile_id).await?;
        introspect::list_objects(&mut client, schema).await
    }

    pub async fn relation_detail(
        &self,
        profile_id: &str,
        schema: &str,
        name: &str,
    ) -> AppResult<RelationDetail> {
        validate_identifier("schema", schema)?;
        validate_identifier("table", name)?;
        let mut client = self.client(profile_id).await?;
        introspect::relation_detail(&mut client, schema, name).await
    }

    pub async fn completion_catalog(
        &self,
        profile_id: &str,
    ) -> AppResult<BTreeMap<String, BTreeMap<String, Vec<String>>>> {
        let mut client = self.client(profile_id).await?;
        introspect::completion_catalog(&mut client).await
    }

    /* ------------------------------------- queries ------------------------------------ */

    pub async fn execute_query(
        &self,
        request_id: &str,
        profile_id: &str,
        sql: &str,
        row_limit: u32,
    ) -> AppResult<QueryOutcome> {
        validate_request_id(request_id)?;
        self.jobs.register(request_id);
        let result = async {
            let handle = self.pool(profile_id).await?;
            query::execute(
                &handle,
                &self.jobs,
                query::QueryRequest {
                    request_id,
                    sql,
                    row_limit,
                },
            )
            .await
        }
        .await;
        self.jobs.finish(request_id);
        match result {
            // Pool/profile errors (e.g. missing password) are shown like query errors.
            Err(error) if error.category != ErrorCategory::Cancelled => {
                Ok(QueryOutcome::failed(error, 0))
            }
            other => other,
        }
    }

    pub async fn preview(
        &self,
        request_id: &str,
        profile_id: &str,
        schema: &str,
        name: &str,
        row_limit: u32,
    ) -> AppResult<QueryOutcome> {
        validate_identifier("schema", schema)?;
        validate_identifier("table", name)?;
        let sql = query::preview_sql(schema, name, row_limit);
        let mut outcome = self
            .execute_query(request_id, profile_id, &sql, row_limit.saturating_add(1))
            .await?;
        // The preview fetched one extra row to learn whether more exist; report the real limit.
        if let QueryOutcome::Ok(success) = &mut outcome {
            let limit = row_limit.clamp(1, query::MAX_ROW_LIMIT) as usize;
            if success.result.rows.len() > limit {
                success.result.rows.truncate(limit);
                success.result.truncated = true;
            }
            success.result.row_limit = row_limit;
        }
        Ok(outcome)
    }

    pub async fn cancel(&self, request_id: &str) -> AppResult<()> {
        validate_request_id(request_id)?;
        self.jobs.cancel(request_id).await;
        Ok(())
    }
}

fn check_sql_size(sql: &str) -> AppResult<()> {
    if sql.len() > query::MAX_SQL_BYTES {
        return Err(AppError::invalid("This query is too large (over 1 MB)."));
    }
    Ok(())
}

/// A cell as filter-choice text; `None` for NULL and values that can't be a choice.
fn cell_text(cell: &Cell) -> Option<String> {
    match cell {
        Cell::Null | Cell::Bytes { .. } | Cell::Unsupported { .. } => None,
        Cell::Text {
            truncated: true, ..
        }
        | Cell::Json {
            truncated: true, ..
        } => None,
        Cell::Bool { v } => Some(v.to_string()),
        Cell::Float { v } => Some(v.to_string()),
        Cell::Int { v }
        | Cell::Decimal { v }
        | Cell::Text { v, .. }
        | Cell::Json { v, .. }
        | Cell::Date { v }
        | Cell::Time { v }
        | Cell::Timestamp { v }
        | Cell::Timestamptz { v }
        | Cell::Uuid { v } => Some(v.clone()),
    }
    .filter(|v| !v.is_empty() && v.chars().count() <= 500)
}
