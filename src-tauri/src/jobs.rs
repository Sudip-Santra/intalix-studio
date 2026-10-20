//! Registry of cancellable in-flight requests, keyed by a request id chosen by the UI.
//!
//! A request is registered before it waits for anything (pool, network), so a cancel that
//! arrives early is remembered and honoured as soon as the work reaches a cancellable point.

use std::collections::HashMap;
use std::sync::Mutex;

use tokio::task::AbortHandle;
use tokio_postgres::CancelToken;
use tokio_postgres_rustls::MakeRustlsConnect;

use crate::error::{AppError, AppResult};

enum State {
    /// Registered, not yet cancellable on the server. `cancelled` records an early cancel.
    Pending { cancelled: bool },
    /// Running on the server; cancel by sending PostgreSQL's cancel request.
    Query {
        token: CancelToken,
        tls: MakeRustlsConnect,
    },
    /// Local async work (e.g. a connection test); cancel by aborting the task.
    Task { abort: AbortHandle },
}

#[derive(Default)]
pub struct Jobs {
    inner: Mutex<HashMap<String, State>>,
}

/// Request ids come from the UI; keep them short and boring.
pub fn validate_request_id(id: &str) -> AppResult<()> {
    if id.is_empty()
        || id.len() > 64
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(AppError::invalid("Invalid request id."));
    }
    Ok(())
}

impl Jobs {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, State>> {
        // A poisoned lock only means another thread panicked mid-update; the map is still usable.
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub fn register(&self, id: &str) {
        self.lock()
            .insert(id.to_owned(), State::Pending { cancelled: false });
    }

    pub fn is_cancelled(&self, id: &str) -> bool {
        matches!(
            self.lock().get(id),
            Some(State::Pending { cancelled: true })
        )
    }

    /// Marks the request as running on the server. Returns false if it was already cancelled.
    pub fn running_query(&self, id: &str, token: CancelToken, tls: MakeRustlsConnect) -> bool {
        let mut map = self.lock();
        if matches!(map.get(id), Some(State::Pending { cancelled: true })) {
            return false;
        }
        map.insert(id.to_owned(), State::Query { token, tls });
        true
    }

    pub fn running_task(&self, id: &str, abort: AbortHandle) -> bool {
        let mut map = self.lock();
        if matches!(map.get(id), Some(State::Pending { cancelled: true })) {
            abort.abort();
            return false;
        }
        map.insert(id.to_owned(), State::Task { abort });
        true
    }

    pub fn finish(&self, id: &str) {
        self.lock().remove(id);
    }

    /// Cancels a request. Unknown ids are ignored (the request may have just finished).
    pub async fn cancel(&self, id: &str) {
        let action = {
            let mut map = self.lock();
            match map.get_mut(id) {
                Some(State::Pending { cancelled }) => {
                    *cancelled = true;
                    None
                }
                Some(State::Task { abort }) => {
                    abort.abort();
                    None
                }
                Some(State::Query { token, tls }) => Some((token.clone(), tls.clone())),
                None => None,
            }
        };
        if let Some((token, tls)) = action
            && let Err(error) = token.cancel_query(tls).await
        {
            tracing::warn!(%error, "sending cancel request failed");
        }
    }
}
