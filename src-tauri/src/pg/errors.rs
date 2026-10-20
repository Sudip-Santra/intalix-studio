//! Maps PostgreSQL driver errors to user-safe [`AppError`]s.
//!
//! Server error messages (e.g. `relation "x" does not exist`) are shown as-is: they describe
//! the user's own SQL and never contain credentials. Network/TLS failures get our own wording.

use std::error::Error as StdError;
use std::io;

use tokio_postgres::error::{ErrorPosition, SqlState};

use crate::error::{AppError, ErrorCategory};

/// Where the error happened; decides the wording for errors without a SQLSTATE.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Connect,
    Query,
}

pub fn map_pg_error(error: &tokio_postgres::Error, phase: Phase) -> AppError {
    if let Some(db) = error.as_db_error() {
        let position = match db.position() {
            Some(ErrorPosition::Original(p)) => Some(*p),
            _ => None,
        };
        let message = db.message().to_owned();
        let hint = db.hint().map(str::to_owned);
        let code = db.code();
        let category = if *code == SqlState::INVALID_PASSWORD
            || *code == SqlState::INVALID_AUTHORIZATION_SPECIFICATION
        {
            ErrorCategory::AuthenticationFailed
        } else if *code == SqlState::INVALID_CATALOG_NAME {
            ErrorCategory::DatabaseNotFound
        } else if *code == SqlState::INSUFFICIENT_PRIVILEGE {
            ErrorCategory::PermissionDenied
        } else if *code == SqlState::SYNTAX_ERROR {
            ErrorCategory::SyntaxError
        } else if [
            SqlState::UNDEFINED_TABLE,
            SqlState::UNDEFINED_COLUMN,
            SqlState::UNDEFINED_FUNCTION,
            SqlState::UNDEFINED_OBJECT,
            SqlState::INVALID_SCHEMA_NAME,
        ]
        .contains(code)
        {
            ErrorCategory::UndefinedObject
        } else if *code == SqlState::QUERY_CANCELED {
            if message.contains("statement timeout") {
                return AppError::new(ErrorCategory::Timeout, "The query took longer than the allowed time and was stopped.")
                    .with_hint("Narrow the query with WHERE or LIMIT, or raise the query timeout for this connection.");
            }
            return AppError::cancelled();
        } else if *code == SqlState::READ_ONLY_SQL_TRANSACTION {
            return AppError::new(ErrorCategory::QueryRejected, message)
                .with_hint("Intalix runs queries in a read-only transaction. This statement tries to change data.")
                .with_position(position);
        } else if *code == SqlState::TOO_MANY_CONNECTIONS {
            return AppError::new(ErrorCategory::ConnectionFailed, message).with_hint(
                "The server has no free connections. Try again later or lower Max connections.",
            );
        } else if [
            SqlState::ADMIN_SHUTDOWN,
            SqlState::CRASH_SHUTDOWN,
            SqlState::CANNOT_CONNECT_NOW,
        ]
        .contains(code)
        {
            ErrorCategory::ConnectionLost
        } else if phase == Phase::Connect {
            ErrorCategory::ConnectionFailed
        } else {
            ErrorCategory::QueryFailed
        };
        let mut app = AppError::new(category, message).with_position(position);
        app.hint = hint.or_else(|| default_hint(category));
        return app;
    }

    // No SQLSTATE: a network, TLS or protocol problem.
    if let Some(tls) = find_source::<rustls::Error>(error) {
        tracing::warn!(%tls, "TLS failure");
        return tls_failure(tls);
    }
    if let Some(io) = find_source::<io::Error>(error) {
        return match io.kind() {
            io::ErrorKind::TimedOut => AppError::new(
                ErrorCategory::Timeout,
                "The server did not respond in time.",
            )
            .with_hint("Check the host and port, VPN and firewall, or raise the connect timeout."),
            io::ErrorKind::ConnectionRefused => AppError::new(
                ErrorCategory::ConnectionFailed,
                "The server refused the connection.",
            )
            .with_hint("Check that PostgreSQL is running and listening on this host and port."),
            io::ErrorKind::ConnectionReset
            | io::ErrorKind::ConnectionAborted
            | io::ErrorKind::BrokenPipe => AppError::new(
                ErrorCategory::ConnectionLost,
                "The connection to the server was lost.",
            )
            .with_hint("Check your network connection and try again."),
            _ => AppError::new(
                ErrorCategory::ConnectionFailed,
                "Could not reach the server.",
            )
            .with_hint(
                "Check the host name and port, and that you are on the right network or VPN.",
            ),
        };
    }
    // The driver often wraps the real reason one level down, e.g.
    // "error performing TLS handshake" -> "server does not support TLS".
    let text = chain_text(error);
    if text.contains("server does not support TLS") {
        return AppError::new(
            ErrorCategory::TlsError,
            "The server does not accept encrypted (TLS) connections.",
        )
        .with_hint(
            "Ask your DBA to enable TLS, or choose Prefer or Disable for a trusted local network.",
        );
    }
    if text.contains("TLS handshake") {
        return AppError::new(
            ErrorCategory::TlsError,
            "The secure (TLS) connection could not be established.",
        )
        .with_hint("Check the SSL mode and certificate settings.");
    }
    if error.is_closed() {
        return AppError::new(
            ErrorCategory::ConnectionLost,
            "The connection to the server was lost.",
        )
        .with_hint("Check your network connection and try again.");
    }
    tracing::warn!(%error, ?phase, "unclassified PostgreSQL error");
    match phase {
        Phase::Connect => AppError::new(
            ErrorCategory::ConnectionFailed,
            "Could not connect to the server.",
        )
        .with_hint("Check the connection details and try again."),
        Phase::Query => AppError::new(
            ErrorCategory::QueryFailed,
            "The query could not be completed.",
        ),
    }
}

fn tls_failure(error: &rustls::Error) -> AppError {
    use rustls::{CertificateError, Error};
    let (message, hint) = match error {
        Error::InvalidCertificate(CertificateError::UnknownIssuer) => (
            "The server's certificate was issued by an authority this computer doesn't trust.",
            "Provide the CA certificate under Encryption, or ask IT to install your company CA.",
        ),
        Error::InvalidCertificate(
            CertificateError::NotValidForName | CertificateError::NotValidForNameContext { .. },
        ) => (
            "The server's certificate does not match the host name.",
            "Connect using the host name on the certificate, or use Verify CA if you trust this server.",
        ),
        Error::InvalidCertificate(
            CertificateError::Expired | CertificateError::ExpiredContext { .. },
        ) => (
            "The server's certificate has expired.",
            "Ask your DBA to renew the server certificate.",
        ),
        Error::InvalidCertificate(_) => (
            "The server's certificate could not be verified.",
            "Check the CA certificate configured for this connection.",
        ),
        _ => (
            "The secure (TLS) connection could not be established.",
            "Check the SSL mode and certificate settings.",
        ),
    };
    AppError::new(ErrorCategory::TlsError, message).with_hint(hint)
}

fn default_hint(category: ErrorCategory) -> Option<String> {
    match category {
        ErrorCategory::AuthenticationFailed => Some("Check the username and password.".into()),
        ErrorCategory::DatabaseNotFound => {
            Some("Check the database name. Names are case-sensitive when quoted.".into())
        }
        ErrorCategory::PermissionDenied => Some(
            "The database user doesn't have access to this. Ask your DBA to grant SELECT.".into(),
        ),
        _ => None,
    }
}

/// The messages of an error and all its sources, joined, for text-based classification.
fn chain_text(error: &(dyn StdError + 'static)) -> String {
    let mut parts = Vec::new();
    let mut current: Option<&(dyn StdError + 'static)> = Some(error);
    while let Some(e) = current {
        parts.push(e.to_string());
        current = e.source();
    }
    parts.join(": ")
}

/// Walks the error's `source()` chain looking for a specific error type.
fn find_source<'a, T: StdError + 'static>(error: &'a (dyn StdError + 'static)) -> Option<&'a T> {
    let mut current: Option<&(dyn StdError + 'static)> = Some(error);
    while let Some(e) = current {
        if let Some(found) = e.downcast_ref::<T>() {
            return Some(found);
        }
        current = e.source();
    }
    None
}
