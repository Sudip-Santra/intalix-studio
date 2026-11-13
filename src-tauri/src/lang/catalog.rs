//! Database names the language service suggests: relations with their columns and types,
//! the user's own functions, and the search path. Loaded once per connection and cached;
//! the cache is dropped when the connection is edited, disconnected or refreshed.

use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, Mutex};

use tokio::sync::OnceCell;

use crate::error::AppResult;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelKind {
    Table,
    View,
    MaterializedView,
    ForeignTable,
}

impl RelKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Table => "table",
            Self::View => "view",
            Self::MaterializedView => "materialized view",
            Self::ForeignTable => "foreign table",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Column {
    pub name: String,
    pub data_type: String,
}

#[derive(Debug, Clone)]
pub struct Relation {
    pub schema: String,
    pub name: String,
    pub kind: RelKind,
    pub columns: Vec<Column>,
}

#[derive(Debug, Clone)]
pub struct Routine {
    pub schema: String,
    pub name: String,
    /// As PostgreSQL prints it, e.g. `p_from date, p_to date`.
    pub arguments: String,
    pub result: String,
    pub aggregate: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Catalog {
    /// Schemas searched for unqualified names, in order (`current_schemas(false)`).
    pub search_path: Vec<String>,
    pub schemas: Vec<String>,
    pub relations: Vec<Relation>,
    pub routines: Vec<Routine>,
}

impl Catalog {
    pub fn in_search_path(&self, schema: &str) -> bool {
        self.search_path.iter().any(|s| s == schema)
    }

    /// Resolves `[schema.]name` the way PostgreSQL would: an unqualified name is looked up
    /// along the search path, then (as a convenience for hints) in any schema.
    pub fn find_relation(&self, schema: Option<&str>, name: &str) -> Option<&Relation> {
        if let Some(schema) = schema {
            return self
                .relations
                .iter()
                .find(|r| r.schema == schema && r.name == name);
        }
        self.search_path
            .iter()
            .find_map(|s| {
                self.relations
                    .iter()
                    .find(|r| &r.schema == s && r.name == name)
            })
            .or_else(|| self.relations.iter().find(|r| r.name == name))
    }

    pub fn routines_named<'a>(
        &'a self,
        schema: Option<&'a str>,
        name: &'a str,
    ) -> impl Iterator<Item = &'a Routine> + 'a {
        self.routines
            .iter()
            .filter(move |r| r.name == name && schema.is_none_or(|s| r.schema == s))
    }
}

/// Per-connection catalog cache. Concurrent requests for the same connection share one load.
#[derive(Default)]
pub struct Catalogs {
    inner: Mutex<HashMap<String, Arc<OnceCell<Arc<Catalog>>>>>,
}

impl Catalogs {
    fn cell(&self, profile_id: &str) -> Arc<OnceCell<Arc<Catalog>>> {
        let mut map = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        map.entry(profile_id.to_owned()).or_default().clone()
    }

    /// The cached catalog, loading it with `load` the first time. A failed load is not
    /// cached, so the next request tries again.
    pub async fn get_or_load<F, Fut>(&self, profile_id: &str, load: F) -> AppResult<Arc<Catalog>>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = AppResult<Catalog>>,
    {
        self.cell(profile_id)
            .get_or_try_init(|| async { load().await.map(Arc::new) })
            .await
            .cloned()
    }

    pub fn invalidate(&self, profile_id: &str) {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(profile_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rel(schema: &str, name: &str) -> Relation {
        Relation {
            schema: schema.into(),
            name: name.into(),
            kind: RelKind::Table,
            columns: vec![],
        }
    }

    #[test]
    fn unqualified_names_follow_the_search_path() {
        let catalog = Catalog {
            search_path: vec!["sales".into(), "public".into()],
            schemas: vec![],
            relations: vec![
                rel("public", "orders"),
                rel("sales", "orders"),
                rel("hr", "staff"),
            ],
            routines: vec![],
        };
        assert_eq!(
            catalog.find_relation(None, "orders").unwrap().schema,
            "sales"
        );
        assert_eq!(
            catalog
                .find_relation(Some("public"), "orders")
                .unwrap()
                .schema,
            "public"
        );
        // Outside the search path, still found as a hint.
        assert_eq!(catalog.find_relation(None, "staff").unwrap().schema, "hr");
        assert!(catalog.find_relation(Some("hr"), "orders").is_none());
    }

    #[tokio::test]
    async fn failed_loads_are_retried_and_successes_cached() {
        let catalogs = Catalogs::default();
        let failed = catalogs
            .get_or_load("p", || async {
                Err(crate::error::AppError::invalid("down"))
            })
            .await;
        assert!(failed.is_err());
        let loaded = catalogs
            .get_or_load("p", || async { Ok(Catalog::default()) })
            .await;
        assert!(loaded.is_ok());
        // Cached: this loader would fail if it ran.
        let again = catalogs
            .get_or_load("p", || async {
                Err(crate::error::AppError::invalid("should not run"))
            })
            .await;
        assert!(again.is_ok());
        catalogs.invalidate("p");
        assert!(
            catalogs
                .get_or_load("p", || async {
                    Err(crate::error::AppError::invalid("reloaded"))
                })
                .await
                .is_err()
        );
    }
}
