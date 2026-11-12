//! Report templates: user-designed presentation rules (page, header, table, footer, formats).
//!
//! The UI owns the detailed template schema (`src/features/templates/model.ts`) and normalises
//! every definition it loads, so the backend treats `definition` as a versioned JSON document
//! and enforces only what keeps storage safe: a known schema version, an object shape, a size
//! cap, and a logo that can only be an embedded raster image.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{AppError, AppResult};

/// The template schema version this build understands.
pub const SCHEMA_VERSION: u64 = 1;
/// Upper bound for one stored definition, logo included.
const MAX_DEFINITION_BYTES: usize = 2 * 1024 * 1024;
/// Upper bound for the embedded logo (a data URL, so about 4/3 of the image size).
const MAX_LOGO_BYTES: usize = 1536 * 1024;
const MAX_NAME_CHARS: usize = 80;
/// Keeps the list fast and the database small; far above any realistic need.
pub const MAX_TEMPLATES: i64 = 500;

/// Logos must be embedded raster images. SVG is excluded because it can carry scripts and
/// external references.
const LOGO_PREFIXES: &[&str] = &[
    "data:image/png;base64,",
    "data:image/jpeg;base64,",
    "data:image/webp;base64,",
];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportTemplate {
    pub id: String,
    pub name: String,
    pub definition: Value,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateDraft {
    pub name: String,
    pub definition: Value,
}

pub fn validate_template_id(id: &str) -> AppResult<()> {
    uuid::Uuid::parse_str(id)
        .map(|_| ())
        .map_err(|_| AppError::invalid("Invalid template id."))
}

/// Checks a draft and returns the trimmed name and the serialised definition to store.
pub fn validate_draft(draft: &TemplateDraft) -> AppResult<(String, String)> {
    let name = draft.name.trim();
    if name.is_empty() {
        return Err(AppError::invalid("Give the template a name."));
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err(AppError::invalid(format!(
            "Template names can be at most {MAX_NAME_CHARS} characters."
        )));
    }
    if name.chars().any(char::is_control) {
        return Err(AppError::invalid(
            "Template names cannot contain control characters.",
        ));
    }

    let Some(object) = draft.definition.as_object() else {
        return Err(AppError::invalid("The template definition is not valid."));
    };
    if object.get("schemaVersion").and_then(Value::as_u64) != Some(SCHEMA_VERSION) {
        return Err(AppError::invalid(
            "This template was made by a different version of Intalix Studio.",
        ));
    }

    if let Some(logo) = object.get("header").and_then(|h| h.get("logo")) {
        match logo {
            Value::Null => {}
            Value::String(s) if s.is_empty() => {}
            Value::String(s) => {
                if !LOGO_PREFIXES.iter().any(|p| s.starts_with(p)) {
                    return Err(AppError::invalid(
                        "The logo must be a PNG, JPEG or WebP image.",
                    ));
                }
                if s.len() > MAX_LOGO_BYTES {
                    return Err(AppError::invalid("The logo is too large.")
                        .with_hint("Use an image under 1 MB."));
                }
            }
            _ => return Err(AppError::invalid("The template logo is not valid.")),
        }
    }

    let serialized = serde_json::to_string(&draft.definition)
        .map_err(|e| AppError::internal("saving template", e))?;
    if serialized.len() > MAX_DEFINITION_BYTES {
        return Err(AppError::invalid("The template is too large to save.")
            .with_hint("Use a smaller logo image."));
    }
    Ok((name.to_owned(), serialized))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn draft(name: &str, definition: Value) -> TemplateDraft {
        TemplateDraft {
            name: name.into(),
            definition,
        }
    }

    #[test]
    fn accepts_a_normal_template() {
        let (name, json) =
            validate_draft(&draft("  Monthly Sales  ", json!({"schemaVersion": 1}))).unwrap();
        assert_eq!(name, "Monthly Sales");
        assert_eq!(json, r#"{"schemaVersion":1}"#);
    }

    #[test]
    fn rejects_bad_names() {
        let def = json!({"schemaVersion": 1});
        assert!(validate_draft(&draft("   ", def.clone())).is_err());
        assert!(validate_draft(&draft(&"x".repeat(81), def.clone())).is_err());
        assert!(validate_draft(&draft("a\u{0}b", def)).is_err());
    }

    #[test]
    fn rejects_unknown_versions_and_shapes() {
        assert!(validate_draft(&draft("T", json!({"schemaVersion": 2}))).is_err());
        assert!(validate_draft(&draft("T", json!({}))).is_err());
        assert!(validate_draft(&draft("T", json!([1, 2]))).is_err());
        assert!(validate_draft(&draft("T", json!("text"))).is_err());
    }

    #[test]
    fn logo_must_be_an_embedded_raster_image() {
        let with_logo = |logo: Value| json!({"schemaVersion": 1, "header": {"logo": logo}});
        assert!(
            validate_draft(&draft("T", with_logo(json!("data:image/png;base64,AAAA")))).is_ok()
        );
        assert!(validate_draft(&draft("T", with_logo(Value::Null))).is_ok());
        assert!(validate_draft(&draft("T", with_logo(json!("")))).is_ok());
        assert!(
            validate_draft(&draft(
                "T",
                with_logo(json!("data:image/svg+xml;base64,AAAA"))
            ))
            .is_err()
        );
        assert!(
            validate_draft(&draft("T", with_logo(json!("https://example.com/a.png")))).is_err()
        );
        assert!(validate_draft(&draft("T", with_logo(json!("file:///C:/a.png")))).is_err());
        assert!(validate_draft(&draft("T", with_logo(json!(42)))).is_err());
        let huge = format!("data:image/png;base64,{}", "A".repeat(MAX_LOGO_BYTES));
        assert!(validate_draft(&draft("T", with_logo(json!(huge)))).is_err());
    }

    #[test]
    fn rejects_oversized_definitions() {
        let big = json!({"schemaVersion": 1, "footer": {"text": "x".repeat(MAX_DEFINITION_BYTES)}});
        assert!(validate_draft(&draft("T", big)).is_err());
    }

    #[test]
    fn template_ids_must_be_uuids() {
        assert!(validate_template_id("4f1c2a5e-8b7d-4c3a-9e2f-1a2b3c4d5e6f").is_ok());
        assert!(validate_template_id("../etc").is_err());
    }
}
