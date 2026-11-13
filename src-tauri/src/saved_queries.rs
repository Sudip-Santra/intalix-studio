//! Saved queries: SQL plus the variable definitions that turn it into filters.
//!
//! Saved queries are not tied to a connection, so the same query can run against development,
//! staging and production copies of a database.

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::lang::variables::{VariableDef, validate_definitions};
use crate::pg::query::MAX_SQL_BYTES;

const MAX_NAME_CHARS: usize = 120;
const MAX_DESCRIPTION_CHARS: usize = 2000;
/// Keeps the list fast; far above any realistic need.
pub const MAX_SAVED_QUERIES: i64 = 2000;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedQuery {
    pub id: String,
    pub name: String,
    pub description: String,
    pub sql: String,
    pub variables: Vec<VariableDef>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedQueryDraft {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub sql: String,
    #[serde(default)]
    pub variables: Vec<VariableDef>,
}

pub fn validate_query_id(id: &str) -> AppResult<()> {
    uuid::Uuid::parse_str(id)
        .map(|_| ())
        .map_err(|_| AppError::invalid("Invalid query id."))
}

/// Checks a draft before it is stored and returns it with a trimmed name and description.
pub fn validate_draft(mut draft: SavedQueryDraft) -> AppResult<SavedQueryDraft> {
    draft.name = draft.name.trim().to_owned();
    draft.description = draft.description.trim().to_owned();
    if draft.name.is_empty() {
        return Err(AppError::invalid("Give the query a name."));
    }
    if draft.name.chars().count() > MAX_NAME_CHARS {
        return Err(AppError::invalid(format!(
            "Query names can be at most {MAX_NAME_CHARS} characters."
        )));
    }
    if draft.name.chars().any(char::is_control) {
        return Err(AppError::invalid(
            "Query names cannot contain control characters.",
        ));
    }
    if draft.description.chars().count() > MAX_DESCRIPTION_CHARS {
        return Err(AppError::invalid("The description is too long."));
    }
    if draft.sql.len() > MAX_SQL_BYTES {
        return Err(AppError::invalid(
            "This query is too large to save (over 1 MB).",
        ));
    }
    if draft.sql.contains('\0') {
        return Err(AppError::invalid("The query contains a null character."));
    }
    validate_definitions(&draft.variables)?;
    Ok(draft)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::variables::VariableType;
    use serde_json::Value;

    fn draft(name: &str, sql: &str) -> SavedQueryDraft {
        SavedQueryDraft {
            name: name.into(),
            description: String::new(),
            sql: sql.into(),
            variables: vec![VariableDef {
                name: "start".into(),
                label: "Start".into(),
                kind: VariableType::Date,
                required: true,
                default: Value::Null,
                options: None,
                help: String::new(),
            }],
        }
    }

    #[test]
    fn accepts_and_trims() {
        let d = validate_draft(draft("  Sales by region ", "SELECT {{start}}")).unwrap();
        assert_eq!(d.name, "Sales by region");
    }

    #[test]
    fn rejects_bad_drafts() {
        assert!(validate_draft(draft(" ", "SELECT 1")).is_err());
        assert!(validate_draft(draft(&"x".repeat(121), "SELECT 1")).is_err());
        assert!(validate_draft(draft("a\nb", "SELECT 1")).is_err());
        assert!(validate_draft(draft("ok", "SELECT '\0'")).is_err());
        let mut bad_var = draft("ok", "SELECT 1");
        bad_var.variables[0].name = "Bad Name".into();
        assert!(validate_draft(bad_var).is_err());
    }

    #[test]
    fn query_ids_must_be_uuids() {
        assert!(validate_query_id("4f1c2a5e-8b7d-4c3a-9e2f-1a2b3c4d5e6f").is_ok());
        assert!(validate_query_id("x").is_err());
    }
}
