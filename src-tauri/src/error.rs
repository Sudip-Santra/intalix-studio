//! Typed, user-safe errors.
//!
//! Every error that crosses the Tauri boundary is an [`AppError`]: a stable category the UI can
//! switch on, a message that is safe to show, and an optional hint. Messages never contain
//! passwords, connection strings or file contents. Detailed causes go to the log instead.

use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCategory {
    // Connecting
    ConnectionFailed,
    AuthenticationFailed,
    DatabaseNotFound,
    TlsError,
    Timeout,
    // Querying
    QueryRejected,
    SyntaxError,
    UndefinedObject,
    PermissionDenied,
    /// A server error with no more specific category (e.g. division by zero).
    QueryFailed,
    ConnectionLost,
    Cancelled,
    // Application
    NotFound,
    InvalidInput,
    CredentialError,
    StorageError,
    Internal,
}

#[derive(Debug, Clone, Serialize, thiserror::Error)]
#[serde(rename_all = "camelCase")]
#[error("{message}")]
pub struct AppError {
    pub category: ErrorCategory,
    pub message: String,
    pub hint: Option<String>,
    /// 1-based character offset into the SQL, when PostgreSQL reports one.
    pub position: Option<u32>,
}

impl AppError {
    pub fn new(category: ErrorCategory, message: impl Into<String>) -> Self {
        Self {
            category,
            message: message.into(),
            hint: None,
            position: None,
        }
    }

    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    pub fn with_position(mut self, position: Option<u32>) -> Self {
        self.position = position;
        self
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new(ErrorCategory::InvalidInput, message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(ErrorCategory::NotFound, message)
    }

    pub fn cancelled() -> Self {
        Self::new(ErrorCategory::Cancelled, "The operation was cancelled.")
    }

    /// For failures that are bugs or environment problems. The detail is logged, not shown.
    pub fn internal(context: &str, detail: impl std::fmt::Display) -> Self {
        tracing::error!(%context, %detail, "internal error");
        Self::new(
            ErrorCategory::Internal,
            format!("Something went wrong ({context})."),
        )
        .with_hint("Try again. If it keeps happening, check the application log.")
    }
}

pub type AppResult<T> = Result<T, AppError>;

impl From<sqlx::Error> for AppError {
    fn from(error: sqlx::Error) -> Self {
        tracing::error!(%error, "local storage error");
        AppError::new(
            ErrorCategory::StorageError,
            "Intalix could not read or write its local data.",
        )
        .with_hint("Check that the disk is not full and that you can write to your profile folder.")
    }
}
