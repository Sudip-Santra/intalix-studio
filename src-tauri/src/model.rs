//! Data shapes exchanged with the UI. Field names serialise to camelCase and must match
//! `src/services/client/types.ts` exactly; change both sides together.

use secrecy::SecretString;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

/* ------------------------------------------------------------------------------------------ */
/* Connections                                                                                 */
/* ------------------------------------------------------------------------------------------ */

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Environment {
    Development,
    Staging,
    Production,
}

impl Environment {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Development => "development",
            Self::Staging => "staging",
            Self::Production => "production",
        }
    }

    pub fn parse(value: &str) -> AppResult<Self> {
        match value {
            "development" => Ok(Self::Development),
            "staging" => Ok(Self::Staging),
            "production" => Ok(Self::Production),
            other => Err(AppError::invalid(format!(
                "Unknown environment \"{other}\"."
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SslMode {
    Disable,
    Prefer,
    Require,
    VerifyCa,
    VerifyFull,
}

impl SslMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Disable => "disable",
            Self::Prefer => "prefer",
            Self::Require => "require",
            Self::VerifyCa => "verify-ca",
            Self::VerifyFull => "verify-full",
        }
    }

    pub fn parse(value: &str) -> AppResult<Self> {
        match value {
            "disable" => Ok(Self::Disable),
            "prefer" => Ok(Self::Prefer),
            "require" => Ok(Self::Require),
            "verify-ca" => Ok(Self::VerifyCa),
            "verify-full" => Ok(Self::VerifyFull),
            other => Err(AppError::invalid(format!("Unknown SSL mode \"{other}\"."))),
        }
    }
}

/// A connection as typed in the form. The password is the only secret and is held as a
/// [`SecretString`] so it is never printed by `Debug` and is wiped from memory on drop.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionDraft {
    pub name: String,
    pub environment: Environment,
    pub host: String,
    pub port: u16,
    pub database: String,
    pub username: String,
    /// Empty when editing a profile whose stored password should be kept.
    pub password: SecretString,
    pub save_password: bool,
    pub ssl_mode: SslMode,
    pub ca_cert_path: String,
    pub connect_timeout_secs: u32,
    pub query_timeout_secs: u32,
    pub max_connections: u32,
    pub application_name: String,
}

/// PostgreSQL truncates identifiers longer than 63 bytes.
const PG_IDENTIFIER_MAX: usize = 63;

impl ConnectionDraft {
    /// Server-side validation. The UI validates too, but this is the trust boundary.
    pub fn validate(&mut self) -> AppResult<()> {
        self.name = self.name.trim().to_owned();
        self.host = self.host.trim().to_owned();
        self.database = self.database.trim().to_owned();
        self.username = self.username.trim().to_owned();
        self.ca_cert_path = self.ca_cert_path.trim().to_owned();
        self.application_name = self.application_name.trim().to_owned();

        check_len("Connection name", &self.name, 1, 80)?;
        check_len("Host", &self.host, 1, 253)?;
        if !self
            .host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._-:[]".contains(c))
        {
            return Err(AppError::invalid(
                "Host can only contain letters, digits, dots, hyphens, or an IPv6 address.",
            ));
        }
        if self.port == 0 {
            return Err(AppError::invalid("Port must be between 1 and 65535."));
        }
        check_len("Database", &self.database, 1, PG_IDENTIFIER_MAX)?;
        check_len("Username", &self.username, 1, PG_IDENTIFIER_MAX)?;
        check_len(
            "Application name",
            &self.application_name,
            0,
            PG_IDENTIFIER_MAX,
        )?;
        check_len("CA certificate path", &self.ca_cert_path, 0, 1024)?;
        check_range("Connect timeout", self.connect_timeout_secs, 1, 120)?;
        check_range("Query timeout", self.query_timeout_secs, 1, 3600)?;
        check_range("Max connections", self.max_connections, 1, 20)?;
        if contains_control(&self.name) || contains_control(&self.application_name) {
            return Err(AppError::invalid(
                "Names cannot contain control characters.",
            ));
        }
        Ok(())
    }
}

fn contains_control(value: &str) -> bool {
    value.chars().any(char::is_control)
}

fn check_len(field: &str, value: &str, min: usize, max: usize) -> AppResult<()> {
    let len = value.chars().count();
    if len < min {
        return Err(AppError::invalid(format!("{field} is required.")));
    }
    if len > max {
        return Err(AppError::invalid(format!(
            "{field} must be at most {max} characters."
        )));
    }
    Ok(())
}

fn check_range(field: &str, value: u32, min: u32, max: u32) -> AppResult<()> {
    if value < min || value > max {
        return Err(AppError::invalid(format!(
            "{field} must be between {min} and {max}."
        )));
    }
    Ok(())
}

/// A saved connection. Contains no secret; `has_stored_password` only says one exists.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionProfile {
    pub id: String,
    pub name: String,
    pub environment: Environment,
    pub host: String,
    pub port: u16,
    pub database: String,
    pub username: String,
    pub ssl_mode: SslMode,
    pub ca_cert_path: String,
    pub connect_timeout_secs: u32,
    pub query_timeout_secs: u32,
    pub max_connections: u32,
    pub application_name: String,
    pub has_stored_password: bool,
    pub created_at: String,
    pub last_connected_at: Option<String>,
}

/// The network-facing part of a connection, shared by drafts and saved profiles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionTarget {
    pub host: String,
    pub port: u16,
    pub database: String,
    pub username: String,
    pub ssl_mode: SslMode,
    pub ca_cert_path: String,
    pub connect_timeout_secs: u32,
    pub query_timeout_secs: u32,
    pub max_connections: u32,
    pub application_name: String,
}

impl From<&ConnectionDraft> for ConnectionTarget {
    fn from(d: &ConnectionDraft) -> Self {
        Self {
            host: d.host.clone(),
            port: d.port,
            database: d.database.clone(),
            username: d.username.clone(),
            ssl_mode: d.ssl_mode,
            ca_cert_path: d.ca_cert_path.clone(),
            connect_timeout_secs: d.connect_timeout_secs,
            query_timeout_secs: d.query_timeout_secs,
            max_connections: d.max_connections,
            application_name: d.application_name.clone(),
        }
    }
}

impl From<&ConnectionProfile> for ConnectionTarget {
    fn from(p: &ConnectionProfile) -> Self {
        Self {
            host: p.host.clone(),
            port: p.port,
            database: p.database.clone(),
            username: p.username.clone(),
            ssl_mode: p.ssl_mode,
            ca_cert_path: p.ca_cert_path.clone(),
            connect_timeout_secs: p.connect_timeout_secs,
            query_timeout_secs: p.query_timeout_secs,
            max_connections: p.max_connections,
            application_name: p.application_name.clone(),
        }
    }
}

/// Result of testing a connection. Serialises to the TypeScript `ConnectionTestResult` union.
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum ConnectionTestResult {
    Ok(TestSuccess),
    Failed(TestFailure),
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestSuccess {
    pub ok: bool,
    pub server_version: String,
    pub latency_ms: u64,
    pub tls: Option<String>,
    pub simulated: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestFailure {
    pub ok: bool,
    pub category: crate::error::ErrorCategory,
    pub message: String,
    pub hint: Option<String>,
    pub simulated: bool,
}

impl ConnectionTestResult {
    pub fn failed(error: AppError) -> Self {
        Self::Failed(TestFailure {
            ok: false,
            category: error.category,
            message: error.message,
            hint: error.hint,
            simulated: false,
        })
    }

    pub fn is_ok(&self) -> bool {
        matches!(self, Self::Ok(_))
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveConnectionResult {
    pub test: ConnectionTestResult,
    pub profile: Option<ConnectionProfile>,
}

/* ------------------------------------------------------------------------------------------ */
/* Schema metadata                                                                             */
/* ------------------------------------------------------------------------------------------ */

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseInfo {
    pub name: String,
    pub server_version: String,
    pub size_bytes: Option<i64>,
    pub current_user: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaSummary {
    pub name: String,
    pub owner: String,
    pub comment: Option<String>,
    pub is_system: bool,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RelationKind {
    Table,
    PartitionedTable,
    View,
    MaterializedView,
    ForeignTable,
}

impl RelationKind {
    /// Maps `pg_class.relkind`.
    pub fn from_relkind(relkind: i8) -> Option<Self> {
        match relkind as u8 {
            b'r' => Some(Self::Table),
            b'p' => Some(Self::PartitionedTable),
            b'v' => Some(Self::View),
            b'm' => Some(Self::MaterializedView),
            b'f' => Some(Self::ForeignTable),
            _ => None,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelationSummary {
    pub schema: String,
    pub name: String,
    pub kind: RelationKind,
    pub estimated_rows: Option<i64>,
    pub size_bytes: Option<i64>,
    pub comment: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RoutineKind {
    Function,
    Procedure,
    Aggregate,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutineSummary {
    pub schema: String,
    pub name: String,
    pub kind: RoutineKind,
    pub arguments: String,
    pub returns: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SequenceSummary {
    pub schema: String,
    pub name: String,
    pub data_type: String,
    pub owned_by: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaObjects {
    pub relations: Vec<RelationSummary>,
    pub routines: Vec<RoutineSummary>,
    pub sequences: Vec<SequenceSummary>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForeignKeyRef {
    pub schema: String,
    pub table: String,
    pub column: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnInfo {
    pub position: i16,
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    pub default_value: Option<String>,
    pub is_primary_key: bool,
    pub foreign_key: Option<ForeignKeyRef>,
    pub comment: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexInfo {
    pub name: String,
    pub columns: Vec<String>,
    pub is_unique: bool,
    pub is_primary: bool,
    pub method: String,
    pub definition: String,
    pub size_bytes: Option<i64>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConstraintKind {
    PrimaryKey,
    ForeignKey,
    Unique,
    Check,
    Exclusion,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConstraintRef {
    pub schema: String,
    pub table: String,
    pub columns: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConstraintInfo {
    pub name: String,
    pub kind: ConstraintKind,
    pub columns: Vec<String>,
    pub definition: String,
    pub references: Option<ConstraintRef>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelationDetail {
    pub summary: RelationSummary,
    pub owner: String,
    pub columns: Vec<ColumnInfo>,
    pub indexes: Vec<IndexInfo>,
    pub constraints: Vec<ConstraintInfo>,
    pub ddl: String,
}

/* ------------------------------------------------------------------------------------------ */
/* Query results                                                                               */
/* ------------------------------------------------------------------------------------------ */

/// A typed cell. Exact numerics are strings so no precision is lost on the way to JavaScript.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "t", rename_all = "lowercase")]
pub enum Cell {
    Null,
    Bool {
        v: bool,
    },
    Int {
        v: String,
    },
    Float {
        v: f64,
    },
    Decimal {
        v: String,
    },
    Text {
        v: String,
        #[serde(skip_serializing_if = "is_false")]
        truncated: bool,
    },
    Date {
        v: String,
    },
    Time {
        v: String,
    },
    Timestamp {
        v: String,
    },
    Timestamptz {
        v: String,
    },
    Uuid {
        v: String,
    },
    Json {
        v: String,
        #[serde(skip_serializing_if = "is_false")]
        truncated: bool,
    },
    Bytes {
        size: usize,
    },
    Unsupported {
        #[serde(rename = "pgType")]
        pg_type: String,
    },
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultColumn {
    pub name: String,
    pub pg_type: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryResultSet {
    pub columns: Vec<ResultColumn>,
    pub rows: Vec<Vec<Cell>>,
    pub truncated: bool,
    pub row_limit: u32,
}

/// Serialises to the TypeScript `QueryOutcome` union.
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum QueryOutcome {
    Ok(QuerySuccess),
    Failed(QueryFailure),
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuerySuccess {
    pub ok: bool,
    pub result: QueryResultSet,
    pub duration_ms: u64,
    pub simulated: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryFailure {
    pub ok: bool,
    pub category: crate::error::ErrorCategory,
    pub message: String,
    pub hint: Option<String>,
    pub position: Option<u32>,
    pub duration_ms: u64,
    pub simulated: bool,
}

impl QueryOutcome {
    pub fn failed(error: AppError, duration_ms: u64) -> Self {
        Self::Failed(QueryFailure {
            ok: false,
            category: error.category,
            message: error.message,
            hint: error.hint,
            position: error.position,
            duration_ms,
            simulated: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cells_serialise_to_the_typescript_shape() {
        let json = serde_json::to_string(&vec![
            Cell::Null,
            Cell::Decimal { v: "12.50".into() },
            Cell::Text {
                v: "a".into(),
                truncated: false,
            },
            Cell::Text {
                v: "b".into(),
                truncated: true,
            },
            Cell::Unsupported {
                pg_type: "tsvector".into(),
            },
        ])
        .unwrap_or_default();
        assert_eq!(
            json,
            r#"[{"t":"null"},{"t":"decimal","v":"12.50"},{"t":"text","v":"a"},{"t":"text","v":"b","truncated":true},{"t":"unsupported","pgType":"tsvector"}]"#
        );
    }

    #[test]
    fn ssl_modes_round_trip() {
        for mode in [
            SslMode::Disable,
            SslMode::Prefer,
            SslMode::Require,
            SslMode::VerifyCa,
            SslMode::VerifyFull,
        ] {
            assert_eq!(SslMode::parse(mode.as_str()).ok(), Some(mode));
        }
        assert!(SslMode::parse("allow").is_err());
    }
}
