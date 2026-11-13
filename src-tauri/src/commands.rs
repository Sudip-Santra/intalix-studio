//! Tauri commands: the only entry points from the UI. Each one is a thin wrapper that
//! deserialises arguments, calls the service layer and returns a typed result.

use std::collections::BTreeMap;

use tauri::State;

use crate::error::AppResult;
use crate::lang::analyze::Analysis;
use crate::lang::complete::{Completions, SignatureHelp};
use crate::lang::variables::VariableDef;
use crate::model::{
    ConnectionDraft, ConnectionProfile, ConnectionTestResult, DatabaseInfo, QueryOutcome,
    RelationDetail, SaveConnectionResult, SchemaObjects, SchemaSummary,
};
use crate::pg::variables::CheckOutcome;
use crate::report::{PreviewOutcome, ReportOutcome, ReportRequest};
use crate::saved_queries::{SavedQuery, SavedQueryDraft};
use crate::service::{AppState, OptionsOutcome};
use crate::templates::{ReportTemplate, TemplateDraft};

#[tauri::command]
pub async fn connections_list(state: State<'_, AppState>) -> AppResult<Vec<ConnectionProfile>> {
    state.list_connections().await
}

#[tauri::command]
pub async fn connections_test(
    state: State<'_, AppState>,
    request_id: String,
    draft: ConnectionDraft,
    existing_id: Option<String>,
) -> AppResult<ConnectionTestResult> {
    state.test_connection(&request_id, draft, existing_id).await
}

#[tauri::command]
pub async fn connections_save(
    state: State<'_, AppState>,
    request_id: String,
    draft: ConnectionDraft,
    id: Option<String>,
) -> AppResult<SaveConnectionResult> {
    state.save_connection(&request_id, draft, id).await
}

#[tauri::command]
pub async fn connections_delete(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.delete_connection(&id).await
}

#[tauri::command]
pub async fn connections_disconnect(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.disconnect(&id).await
}

#[tauri::command]
pub async fn schema_database(
    state: State<'_, AppState>,
    connection_id: String,
) -> AppResult<DatabaseInfo> {
    state.database_info(&connection_id).await
}

#[tauri::command]
pub async fn schema_list(
    state: State<'_, AppState>,
    connection_id: String,
) -> AppResult<Vec<SchemaSummary>> {
    state.list_schemas(&connection_id).await
}

#[tauri::command]
pub async fn schema_objects(
    state: State<'_, AppState>,
    connection_id: String,
    schema: String,
) -> AppResult<SchemaObjects> {
    state.list_objects(&connection_id, &schema).await
}

#[tauri::command]
pub async fn schema_relation(
    state: State<'_, AppState>,
    connection_id: String,
    schema: String,
    name: String,
) -> AppResult<RelationDetail> {
    state.relation_detail(&connection_id, &schema, &name).await
}

#[tauri::command]
pub async fn schema_completion(
    state: State<'_, AppState>,
    connection_id: String,
) -> AppResult<BTreeMap<String, BTreeMap<String, Vec<String>>>> {
    state.completion_catalog(&connection_id).await
}

#[tauri::command]
pub async fn query_execute(
    state: State<'_, AppState>,
    request_id: String,
    connection_id: String,
    sql: String,
    row_limit: u32,
) -> AppResult<QueryOutcome> {
    state
        .execute_query(&request_id, &connection_id, &sql, row_limit)
        .await
}

#[tauri::command]
pub async fn query_preview(
    state: State<'_, AppState>,
    request_id: String,
    connection_id: String,
    schema: String,
    name: String,
    row_limit: u32,
) -> AppResult<QueryOutcome> {
    state
        .preview(&request_id, &connection_id, &schema, &name, row_limit)
        .await
}

#[tauri::command]
pub async fn request_cancel(state: State<'_, AppState>, request_id: String) -> AppResult<()> {
    state.cancel(&request_id).await
}

#[tauri::command]
pub async fn templates_list(state: State<'_, AppState>) -> AppResult<Vec<ReportTemplate>> {
    state.list_templates().await
}

#[tauri::command]
pub async fn templates_save(
    state: State<'_, AppState>,
    draft: TemplateDraft,
    id: Option<String>,
) -> AppResult<ReportTemplate> {
    state.save_template(draft, id).await
}

#[tauri::command]
pub async fn templates_delete(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.delete_template(&id).await
}

#[tauri::command]
pub async fn queries_list(state: State<'_, AppState>) -> AppResult<Vec<SavedQuery>> {
    state.list_saved_queries().await
}

#[tauri::command]
pub async fn queries_save(
    state: State<'_, AppState>,
    draft: SavedQueryDraft,
    id: Option<String>,
) -> AppResult<SavedQuery> {
    state.save_query(draft, id).await
}

#[tauri::command]
pub async fn queries_delete(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.delete_saved_query(&id).await
}

/// Async so a large document is never analysed on the UI thread.
#[tauri::command]
pub async fn sql_analyze(
    state: State<'_, AppState>,
    sql: String,
    variables: Option<Vec<VariableDef>>,
) -> AppResult<Analysis> {
    state.sql_analyze(&sql, variables)
}

#[tauri::command]
pub async fn sql_complete(
    state: State<'_, AppState>,
    connection_id: String,
    sql: String,
    cursor: usize,
    variables: Option<Vec<VariableDef>>,
) -> AppResult<Completions> {
    state
        .sql_complete(&connection_id, &sql, cursor, variables)
        .await
}

#[tauri::command]
pub async fn sql_signature(
    state: State<'_, AppState>,
    connection_id: String,
    sql: String,
    cursor: usize,
) -> AppResult<Option<SignatureHelp>> {
    state.sql_signature(&connection_id, &sql, cursor).await
}

#[tauri::command]
pub async fn sql_refresh_catalog(
    state: State<'_, AppState>,
    connection_id: String,
) -> AppResult<()> {
    state.sql_refresh_catalog(&connection_id).await
}

#[tauri::command]
pub async fn sql_check(
    state: State<'_, AppState>,
    request_id: String,
    connection_id: String,
    sql: String,
    variables: Vec<VariableDef>,
) -> AppResult<CheckOutcome> {
    state
        .sql_check(&request_id, &connection_id, &sql, variables)
        .await
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn sql_run(
    state: State<'_, AppState>,
    request_id: String,
    connection_id: String,
    sql: String,
    variables: Vec<VariableDef>,
    values: BTreeMap<String, serde_json::Value>,
    row_limit: u32,
) -> AppResult<QueryOutcome> {
    state
        .sql_run(
            &request_id,
            &connection_id,
            &sql,
            variables,
            values,
            row_limit,
        )
        .await
}

#[tauri::command]
pub async fn sql_variable_options(
    state: State<'_, AppState>,
    request_id: String,
    connection_id: String,
    sql: String,
) -> AppResult<OptionsOutcome> {
    state
        .variable_options(&request_id, &connection_id, &sql)
        .await
}

#[tauri::command]
pub async fn sql_column_values(
    state: State<'_, AppState>,
    request_id: String,
    connection_id: String,
    sql: String,
    variable: String,
) -> AppResult<OptionsOutcome> {
    state
        .column_values(&request_id, &connection_id, &sql, &variable)
        .await
}

#[tauri::command]
pub async fn report_generate(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    request_id: String,
    connection_id: String,
    request: ReportRequest,
) -> AppResult<ReportOutcome> {
    state
        .generate_report(&app, &request_id, &connection_id, request)
        .await
}

#[tauri::command]
pub async fn report_preview(
    state: State<'_, AppState>,
    request_id: String,
    connection_id: String,
    request: ReportRequest,
) -> AppResult<PreviewOutcome> {
    state
        .preview_report(&request_id, &connection_id, request)
        .await
}

#[tauri::command]
pub async fn report_open(state: State<'_, AppState>, path: String, reveal: bool) -> AppResult<()> {
    state.open_report(&path, reveal).await
}
