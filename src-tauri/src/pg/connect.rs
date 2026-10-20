//! Connection configuration, connection tests, and the per-connection pool manager.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use deadpool_postgres::{Manager, ManagerConfig, Pool, RecyclingMethod, Runtime};
use secrecy::{ExposeSecret, SecretString};
use tokio::sync::RwLock;
use tokio_postgres::config::SslMode as PgSslMode;
use tokio_postgres_rustls::MakeRustlsConnect;

use crate::error::{AppError, AppResult, ErrorCategory};
use crate::model::{ConnectionTarget, ConnectionTestResult, SslMode, TestSuccess};
use crate::pg::errors::{Phase, map_pg_error};
use crate::pg::tls::client_config;

const DEFAULT_APPLICATION_NAME: &str = "Intalix Studio";
/// How long a query waits for a free pooled connection before giving up.
const POOL_WAIT: Duration = Duration::from_secs(30);

/// Builds the driver configuration.
///
/// No startup `options` are sent: connection poolers such as PgBouncer reject them. Read-only
/// behaviour is enforced per query with an explicit READ ONLY transaction instead.
pub fn pg_config(target: &ConnectionTarget, password: &SecretString) -> tokio_postgres::Config {
    let mut c = tokio_postgres::Config::new();
    // Accept "[::1]" as typed for IPv6 and pass the bare address to the driver.
    let host = target.host.trim_start_matches('[').trim_end_matches(']');
    c.host(host)
        .port(target.port)
        .dbname(&target.database)
        .user(&target.username)
        .password(password.expose_secret().as_bytes())
        .application_name(if target.application_name.is_empty() {
            DEFAULT_APPLICATION_NAME
        } else {
            &target.application_name
        })
        .connect_timeout(Duration::from_secs(u64::from(target.connect_timeout_secs)))
        .keepalives(true)
        .keepalives_idle(Duration::from_secs(60))
        .ssl_mode(match target.ssl_mode {
            SslMode::Disable => PgSslMode::Disable,
            SslMode::Prefer => PgSslMode::Prefer,
            SslMode::Require | SslMode::VerifyCa | SslMode::VerifyFull => PgSslMode::Require,
        });
    c
}

pub fn tls_connector(target: &ConnectionTarget) -> AppResult<MakeRustlsConnect> {
    Ok(MakeRustlsConnect::new(client_config(
        target.ssl_mode,
        &target.ca_cert_path,
    )?))
}

/// Opens a fresh connection (not pooled), reads the server version and TLS status, closes it.
pub async fn test_connection(
    target: &ConnectionTarget,
    password: &SecretString,
) -> ConnectionTestResult {
    match try_test(target, password).await {
        Ok(success) => ConnectionTestResult::Ok(success),
        Err(error) => ConnectionTestResult::failed(error),
    }
}

async fn try_test(target: &ConnectionTarget, password: &SecretString) -> AppResult<TestSuccess> {
    let tls = tls_connector(target)?;
    let config = pg_config(target, password);
    // The driver's connect_timeout applies per socket attempt; this bounds the whole handshake.
    let budget = Duration::from_secs(u64::from(target.connect_timeout_secs) + 5);

    let (client, connection) = tokio::time::timeout(budget, config.connect(tls))
        .await
        .map_err(|_| {
            AppError::new(
                ErrorCategory::Timeout,
                format!("No response from {}:{} in time.", target.host, target.port),
            )
            .with_hint("Check the host and port, VPN and firewall, or raise the connect timeout.")
        })?
        .map_err(|e| map_pg_error(&e, Phase::Connect))?;
    let driver = tokio::spawn(async move {
        if let Err(error) = connection.await {
            tracing::debug!(%error, "test connection closed with error");
        }
    });

    let started = Instant::now();
    let row = client
        .query_one(
            "SELECT current_setting('server_version'), \
                    (SELECT version FROM pg_stat_ssl WHERE pid = pg_backend_pid())",
            &[],
        )
        .await
        .map_err(|e| map_pg_error(&e, Phase::Query))?;
    let latency_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    let version: String = row
        .try_get(0)
        .map_err(|e| AppError::internal("reading server version", e))?;
    let tls: Option<String> = row.try_get(1).ok().flatten();

    drop(client);
    driver.abort();
    Ok(TestSuccess {
        ok: true,
        server_version: format!("PostgreSQL {version}"),
        latency_ms,
        tls: tls.map(|v| v.replace("TLSv", "TLS ")),
        simulated: false,
    })
}

/// One pool per saved connection, created on first use and rebuilt if the profile changes.
pub struct Pools {
    inner: RwLock<HashMap<String, PoolEntry>>,
}

struct PoolEntry {
    pool: Pool,
    target: ConnectionTarget,
    tls: MakeRustlsConnect,
}

/// What a caller needs to run and cancel queries on a connection.
#[derive(Clone)]
pub struct PoolHandle {
    pub pool: Pool,
    pub tls: MakeRustlsConnect,
    pub query_timeout_secs: u32,
}

impl Pools {
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(HashMap::new()),
        }
    }

    /// Returns the pool for `profile_id`, creating it with `password` if needed.
    pub async fn get_or_create(
        &self,
        profile_id: &str,
        target: &ConnectionTarget,
        password: impl FnOnce() -> AppResult<SecretString>,
    ) -> AppResult<PoolHandle> {
        if let Some(entry) = self.inner.read().await.get(profile_id)
            && entry.target == *target
        {
            return Ok(handle(entry));
        }

        let mut map = self.inner.write().await;
        // Another task may have created it while we waited for the lock.
        if let Some(entry) = map.get(profile_id)
            && entry.target == *target
        {
            return Ok(handle(entry));
        }
        if let Some(old) = map.remove(profile_id) {
            old.pool.close();
        }

        let secret = password()?;
        let tls = tls_connector(target)?;
        let manager = Manager::from_config(
            pg_config(target, &secret),
            tls.clone(),
            ManagerConfig {
                recycling_method: RecyclingMethod::Fast,
            },
        );
        let connect_budget = Duration::from_secs(u64::from(target.connect_timeout_secs) + 5);
        let pool = Pool::builder(manager)
            .max_size(usize::try_from(target.max_connections).unwrap_or(4))
            .runtime(Runtime::Tokio1)
            .wait_timeout(Some(POOL_WAIT))
            .create_timeout(Some(connect_budget))
            .recycle_timeout(Some(Duration::from_secs(5)))
            .build()
            .map_err(|e| AppError::internal("creating connection pool", e))?;

        let entry = PoolEntry {
            pool,
            target: target.clone(),
            tls,
        };
        let result = handle(&entry);
        map.insert(profile_id.to_owned(), entry);
        tracing::info!(profile = %profile_id, max = target.max_connections, "connection pool created");
        Ok(result)
    }

    /// Closes and forgets a connection's pool (on edit, delete or disconnect).
    pub async fn invalidate(&self, profile_id: &str) {
        if let Some(entry) = self.inner.write().await.remove(profile_id) {
            entry.pool.close();
            tracing::info!(profile = %profile_id, "connection pool closed");
        }
    }
}

fn handle(entry: &PoolEntry) -> PoolHandle {
    PoolHandle {
        pool: entry.pool.clone(),
        tls: entry.tls.clone(),
        query_timeout_secs: entry.target.query_timeout_secs,
    }
}

/// Maps pool errors (timeouts, connect failures) to user-facing errors.
pub fn map_pool_error(error: deadpool_postgres::PoolError) -> AppError {
    use deadpool_postgres::PoolError;
    match error {
        PoolError::Backend(e) => map_pg_error(&e, Phase::Connect),
        PoolError::Timeout(_) => AppError::new(
            ErrorCategory::Timeout,
            "Timed out waiting for a database connection.",
        )
        .with_hint(
            "Other queries may be using every connection. Try again, or raise Max connections.",
        ),
        PoolError::Closed => {
            AppError::new(ErrorCategory::ConnectionLost, "The connection was closed.")
                .with_hint("Reconnect and try again.")
        }
        other => AppError::internal("getting a pooled connection", other),
    }
}
