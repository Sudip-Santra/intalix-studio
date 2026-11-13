/// Every command the UI may call. Each one becomes an `allow-<name>` permission that must be
/// granted explicitly in `capabilities/default.json`; anything not listed cannot be invoked.
const COMMANDS: &[&str] = &[
    "connections_list",
    "connections_test",
    "connections_save",
    "connections_delete",
    "connections_disconnect",
    "schema_database",
    "schema_list",
    "schema_objects",
    "schema_relation",
    "schema_completion",
    "query_execute",
    "query_preview",
    "request_cancel",
    "templates_list",
    "templates_save",
    "templates_delete",
    "queries_list",
    "queries_save",
    "queries_delete",
    "sql_analyze",
    "sql_complete",
    "sql_signature",
    "sql_refresh_catalog",
    "sql_check",
    "sql_run",
    "sql_variable_options",
    "sql_column_values",
    "report_generate",
    "report_open",
    "report_preview",
];

fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .unwrap_or_else(|error| {
        eprintln!("tauri build script failed: {error}");
        std::process::exit(1);
    });
}
