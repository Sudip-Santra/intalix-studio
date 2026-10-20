//! Local application data in SQLite (`intalix.db` in the app data folder).
//!
//! Schema changes are versioned migrations in `src-tauri/migrations`, applied at startup.
//! WAL journaling keeps the file consistent if the app is killed mid-write.

use std::path::Path;
use std::str::FromStr;

use sqlx::Row;
use sqlx::sqlite::{
    SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions, SqliteSynchronous,
};

use crate::error::{AppError, AppResult};
use crate::model::{ConnectionDraft, ConnectionProfile, Environment, SslMode};

#[derive(Clone)]
pub struct Store {
    pool: SqlitePool,
}

pub fn now_iso() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}

impl Store {
    pub async fn open(path: &Path) -> AppResult<Self> {
        let url = format!("sqlite://{}", path.display());
        let options = SqliteConnectOptions::from_str(&url)
            .map_err(|e| AppError::internal("opening local database", e))?
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .foreign_keys(true)
            .busy_timeout(std::time::Duration::from_secs(5));
        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(options)
            .await?;
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .map_err(|e| AppError::internal("upgrading local database", e))?;
        Ok(Self { pool })
    }

    #[cfg(test)]
    pub async fn in_memory() -> AppResult<Self> {
        let options = SqliteConnectOptions::from_str("sqlite::memory:")
            .map_err(|e| AppError::internal("opening test database", e))?;
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await?;
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .map_err(|e| AppError::internal("migrating test database", e))?;
        Ok(Self { pool })
    }

    pub async fn list_profiles(&self) -> AppResult<Vec<ConnectionProfile>> {
        let rows = sqlx::query(
            "SELECT * FROM connection_profiles ORDER BY last_connected_at IS NULL, last_connected_at DESC, created_at DESC",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_profile).collect()
    }

    pub async fn get_profile(&self, id: &str) -> AppResult<ConnectionProfile> {
        let row = sqlx::query("SELECT * FROM connection_profiles WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| AppError::not_found("This connection no longer exists."))?;
        row_to_profile(&row)
    }

    /// Inserts or updates a profile and marks it as just connected.
    pub async fn upsert_profile(
        &self,
        id: &str,
        d: &ConnectionDraft,
        has_stored_password: bool,
    ) -> AppResult<ConnectionProfile> {
        let now = now_iso();
        sqlx::query(
            "INSERT INTO connection_profiles (id, name, environment, host, port, database_name, username, ssl_mode, \
                 ca_cert_path, connect_timeout_secs, query_timeout_secs, max_connections, application_name, \
                 has_stored_password, created_at, updated_at, last_connected_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT (id) DO UPDATE SET name = excluded.name, environment = excluded.environment, \
                 host = excluded.host, port = excluded.port, database_name = excluded.database_name, \
                 username = excluded.username, ssl_mode = excluded.ssl_mode, ca_cert_path = excluded.ca_cert_path, \
                 connect_timeout_secs = excluded.connect_timeout_secs, query_timeout_secs = excluded.query_timeout_secs, \
                 max_connections = excluded.max_connections, application_name = excluded.application_name, \
                 has_stored_password = excluded.has_stored_password, updated_at = excluded.updated_at, \
                 last_connected_at = excluded.last_connected_at",
        )
        .bind(id)
        .bind(&d.name)
        .bind(d.environment.as_str())
        .bind(&d.host)
        .bind(i64::from(d.port))
        .bind(&d.database)
        .bind(&d.username)
        .bind(d.ssl_mode.as_str())
        .bind(&d.ca_cert_path)
        .bind(i64::from(d.connect_timeout_secs))
        .bind(i64::from(d.query_timeout_secs))
        .bind(i64::from(d.max_connections))
        .bind(&d.application_name)
        .bind(has_stored_password)
        .bind(&now)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await?;
        self.get_profile(id).await
    }

    pub async fn touch_connected(&self, id: &str) -> AppResult<()> {
        sqlx::query("UPDATE connection_profiles SET last_connected_at = ? WHERE id = ?")
            .bind(now_iso())
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn delete_profile(&self, id: &str) -> AppResult<()> {
        sqlx::query("DELETE FROM connection_profiles WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

fn row_to_profile(r: &sqlx::sqlite::SqliteRow) -> AppResult<ConnectionProfile> {
    let num = |col: &str| -> AppResult<u32> {
        let v: i64 = r.try_get(col)?;
        u32::try_from(v)
            .map_err(|_| AppError::internal("reading profile", format!("{col} out of range")))
    };
    Ok(ConnectionProfile {
        id: r.try_get("id")?,
        name: r.try_get("name")?,
        environment: Environment::parse(&r.try_get::<String, _>("environment")?)?,
        host: r.try_get("host")?,
        port: u16::try_from(num("port")?)
            .map_err(|_| AppError::internal("reading profile", "port out of range"))?,
        database: r.try_get("database_name")?,
        username: r.try_get("username")?,
        ssl_mode: SslMode::parse(&r.try_get::<String, _>("ssl_mode")?)?,
        ca_cert_path: r.try_get("ca_cert_path")?,
        connect_timeout_secs: num("connect_timeout_secs")?,
        query_timeout_secs: num("query_timeout_secs")?,
        max_connections: num("max_connections")?,
        application_name: r.try_get("application_name")?,
        has_stored_password: r.try_get("has_stored_password")?,
        created_at: r.try_get("created_at")?,
        last_connected_at: r.try_get("last_connected_at")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use secrecy::SecretString;

    fn draft(name: &str) -> ConnectionDraft {
        ConnectionDraft {
            name: name.into(),
            environment: Environment::Production,
            host: "db.example.com".into(),
            port: 5432,
            database: "sales".into(),
            username: "reader".into(),
            password: SecretString::from("not-stored"),
            save_password: true,
            ssl_mode: SslMode::VerifyFull,
            ca_cert_path: String::new(),
            connect_timeout_secs: 10,
            query_timeout_secs: 120,
            max_connections: 4,
            application_name: "Intalix Studio".into(),
        }
    }

    #[tokio::test]
    async fn profiles_round_trip_without_secrets() {
        let store = Store::in_memory().await.unwrap();
        let saved = store
            .upsert_profile("a", &draft("Sales"), true)
            .await
            .unwrap();
        assert_eq!(saved.name, "Sales");
        assert_eq!(saved.environment, Environment::Production);
        assert!(saved.has_stored_password);

        let updated = store
            .upsert_profile("a", &draft("Sales EU"), false)
            .await
            .unwrap();
        assert_eq!(updated.name, "Sales EU");
        assert_eq!(updated.created_at, saved.created_at);
        assert_eq!(store.list_profiles().await.unwrap().len(), 1);

        // Nothing in the stored row resembles the password.
        let dump: Vec<String> = sqlx::query_scalar(
            "SELECT group_concat(name || host || username) FROM connection_profiles",
        )
        .fetch_all(&store.pool)
        .await
        .unwrap();
        assert!(!dump.concat().contains("not-stored"));

        store.delete_profile("a").await.unwrap();
        assert!(store.get_profile("a").await.is_err());
    }
}
