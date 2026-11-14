// Tests may unwrap and panic freely; production code may not (see Cargo.toml lints).
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

mod commands;
mod error;
mod jobs;
mod lang;
mod model;
mod pg;
mod report;
mod saved_queries;
mod secrets;
mod service;
mod storage;
mod templates;
mod window;

use tauri::Manager;

use crate::jobs::Jobs;
use crate::pg::connect::Pools;
use crate::secrets::Secrets;
use crate::service::AppState;
use crate::storage::Store;

fn init_logging() {
    use tracing_subscriber::EnvFilter;
    let filter = EnvFilter::try_from_env("INTALIX_LOG").unwrap_or_else(|_| EnvFilter::new("info"));
    // Logs carry ids and error categories only; secrets and result rows are never logged.
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .try_init();
}

/// Builds and runs the Tauri application. Wiring only; logic lives in `service`.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_logging();

    let result = tauri::Builder::default()
        // Native Save dialog for reports (used from Rust only; no dialog commands are exposed).
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            secrets::init_credential_store()?;
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let store = tauri::async_runtime::block_on(Store::open(&data_dir.join("intalix.db")))?;
            app.manage(AppState {
                store,
                secrets: Secrets::new(),
                pools: Pools::new(),
                jobs: Jobs::default(),
                catalogs: Default::default(),
                generated: Default::default(),
                previews: Default::default(),
            });
            // Report fonts load in the background, ready for the first preview.
            std::thread::spawn(report::pdf::warm_up);
            if let Some(main) = app.get_webview_window("main") {
                window::fit_and_show(&main);
            }
            tracing::info!("Intalix Studio started");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::connections_list,
            commands::connections_test,
            commands::connections_save,
            commands::connections_delete,
            commands::connections_disconnect,
            commands::schema_database,
            commands::schema_list,
            commands::schema_objects,
            commands::schema_relation,
            commands::schema_completion,
            commands::query_execute,
            commands::query_preview,
            commands::request_cancel,
            commands::templates_list,
            commands::templates_save,
            commands::templates_delete,
            commands::queries_list,
            commands::queries_save,
            commands::queries_delete,
            commands::sql_analyze,
            commands::sql_complete,
            commands::sql_signature,
            commands::sql_refresh_catalog,
            commands::sql_check,
            commands::sql_run,
            commands::sql_variable_options,
            commands::sql_column_values,
            commands::report_generate,
            commands::report_open,
            commands::report_preview,
        ])
        .run(tauri::generate_context!());

    if let Err(error) = result {
        // Nothing can recover from a failed startup; report it and exit with a failure code.
        eprintln!("Intalix Studio failed to start: {error}");
        std::process::exit(1);
    }
}
