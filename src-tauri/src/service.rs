//! Application services: the logic behind each command. Commands stay thin wrappers.

use std::collections::BTreeMap;

use secrecy::{ExposeSecret, SecretString};

use crate::error::{AppError, AppResult, ErrorCategory};
use crate::jobs::{Jobs, validate_request_id};
use crate::model::{
    ConnectionDraft, ConnectionProfile, ConnectionTarget, ConnectionTestResult, DatabaseInfo,
    QueryOutcome, RelationDetail, SaveConnectionResult, SchemaObjects, SchemaSummary,
};
use crate::pg::connect::{PoolHandle, Pools, map_pool_error, test_connection};
use crate::pg::{introspect, query};
use crate::secrets::Secrets;
use crate::storage::Store;

pub struct AppState {
    pub store: Store,
    pub secrets: Secrets,
    pub pools: Pools,
    pub jobs: Jobs,
}

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
        tracing::info!(profile = %id, env = draft.environment.as_str(), "connection saved");
        Ok(SaveConnectionResult {
            test,
            profile: Some(profile),
        })
    }

    pub async fn delete_connection(&self, id: &str) -> AppResult<()> {
        validate_profile_id(id)?;
        self.pools.invalidate(id).await;
        self.secrets.forget(id)?;
        self.store.delete_profile(id).await?;
        tracing::info!(profile = %id, "connection deleted");
        Ok(())
    }

    pub async fn disconnect(&self, id: &str) -> AppResult<()> {
        validate_profile_id(id)?;
        self.pools.invalidate(id).await;
        Ok(())
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
