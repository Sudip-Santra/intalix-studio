//! Tauri commands: the only entry points from the UI. Each one is a thin wrapper that
//! deserialises arguments, calls the service layer and returns a typed result.

use std::collections::BTreeMap;

use tauri::State;

use crate::error::AppResult;
use crate::model::{
    ConnectionDraft, ConnectionProfile, ConnectionTestResult, DatabaseInfo, QueryOutcome,
    RelationDetail, SaveConnectionResult, SchemaObjects, SchemaSummary,
};
use crate::service::AppState;

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
