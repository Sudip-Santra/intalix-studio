//! Database passwords.
//!
//! - "Remember password" → Windows Credential Manager, with Local persistence so the secret
//!   stays on this machine and does not roam with a domain profile.
//! - Otherwise → kept in memory for this session only, wiped when the app exits.
//!
//! Passwords never touch SQLite, logs, error messages or anything sent back to the UI.

use std::collections::HashMap;
use std::sync::Mutex;

use secrecy::{ExposeSecret, SecretString};

use crate::error::{AppError, AppResult, ErrorCategory};

const SERVICE: &str = "Intalix Studio";

pub struct Secrets {
    session: Mutex<HashMap<String, SecretString>>,
}

fn account(profile_id: &str) -> String {
    format!("connection:{profile_id}")
}

fn credential_error(action: &str, error: impl std::fmt::Display) -> AppError {
    tracing::error!(%action, %error, "credential store failure");
    AppError::new(
        ErrorCategory::CredentialError,
        format!("Windows Credential Manager could not {action} the password."),
    )
    .with_hint("Check that Credential Manager is available for your Windows account.")
}

/// Registers the Windows credential store as the default keyring backend.
pub fn init_credential_store() -> AppResult<()> {
    #[cfg(windows)]
    {
        let store =
            windows_native_keyring_store::Store::new().map_err(|e| credential_error("open", e))?;
        keyring_core::set_default_store(store);
    }
    Ok(())
}

fn entry(profile_id: &str) -> AppResult<keyring_core::Entry> {
    let modifiers = HashMap::from([("persistence", "Local")]);
    keyring_core::Entry::new_with_modifiers(SERVICE, &account(profile_id), &modifiers)
        .map_err(|e| credential_error("access", e))
}

impl Secrets {
    pub fn new() -> Self {
        Self {
            session: Mutex::new(HashMap::new()),
        }
    }

    fn session(&self) -> std::sync::MutexGuard<'_, HashMap<String, SecretString>> {
        self.session
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Stores a password: persistently when `remember`, otherwise for this session only.
    /// Switching `remember` off deletes any persisted copy.
    pub fn store(
        &self,
        profile_id: &str,
        password: &SecretString,
        remember: bool,
    ) -> AppResult<()> {
        if remember {
            entry(profile_id)?
                .set_password(password.expose_secret())
                .map_err(|e| credential_error("save", e))?;
            self.session().remove(profile_id);
        } else {
            self.delete_persisted(profile_id)?;
            self.session()
                .insert(profile_id.to_owned(), password.clone());
        }
        Ok(())
    }

    /// The password for a profile, from this session or Credential Manager.
    pub fn get(&self, profile_id: &str) -> AppResult<Option<SecretString>> {
        if let Some(secret) = self.session().get(profile_id) {
            return Ok(Some(secret.clone()));
        }
        match entry(profile_id)?.get_password() {
            Ok(password) => Ok(Some(SecretString::from(password))),
            Err(keyring_core::Error::NoEntry) => Ok(None),
            Err(e) => Err(credential_error("read", e)),
        }
    }

    pub fn require(&self, profile_id: &str) -> AppResult<SecretString> {
        self.get(profile_id)?.ok_or_else(|| {
            AppError::new(
                ErrorCategory::AuthenticationFailed,
                "No password is saved for this connection.",
            )
            .with_hint("Open the connection, enter the password and connect again.")
        })
    }

    pub fn delete_persisted(&self, profile_id: &str) -> AppResult<()> {
        match entry(profile_id)?.delete_credential() {
            Ok(()) | Err(keyring_core::Error::NoEntry) => Ok(()),
            Err(e) => Err(credential_error("delete", e)),
        }
    }

    pub fn forget(&self, profile_id: &str) -> AppResult<()> {
        self.session().remove(profile_id);
        self.delete_persisted(profile_id)
    }
}
