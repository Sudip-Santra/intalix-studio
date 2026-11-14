//! What the last report preview read and laid out, kept for a few minutes.
//!
//! - Changing only the template, orientation or name redraws the preview from the kept rows
//!   instead of running the query again.
//! - When the preview already holds every row of the query, Generate reuses those rows (and
//!   the PDF layout, if the template and name are unchanged) instead of starting over.
//!
//! Only one preview is kept (at most `PREVIEW_ROWS` rows), and it expires after `FRESH_FOR`,
//! so a report is never built from rows older than that.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use serde_json::json;

use crate::error::AppResult;
use crate::model::{Cell, ResultColumn};
use crate::pg::query::{Fetched, RowSink};
use crate::report::ReportRequest;
use crate::report::pdf::Laid;

/// How long kept rows may be reused.
pub const FRESH_FOR: Duration = Duration::from_secs(5 * 60);

/// Rows as PostgreSQL returned them.
#[derive(Debug, Default)]
pub struct Recorded {
    pub columns: Vec<ResultColumn>,
    pub rows: Vec<Vec<Cell>>,
    /// Every row of the query is here (the preview limit wasn't reached).
    pub complete: bool,
}

/// Passes rows on to another sink and keeps a copy.
pub struct Recorder<'a> {
    inner: &'a mut dyn RowSink,
    pub kept: Recorded,
}

impl<'a> Recorder<'a> {
    pub fn new(inner: &'a mut dyn RowSink) -> Self {
        Self {
            inner,
            kept: Recorded::default(),
        }
    }
}

impl RowSink for Recorder<'_> {
    fn start(&mut self, columns: &[ResultColumn]) -> AppResult<()> {
        self.kept.columns = columns.to_vec();
        self.inner.start(columns)
    }

    fn row(&mut self, cells: Vec<Cell>) -> AppResult<bool> {
        self.kept.rows.push(cells.clone());
        self.inner.row(cells)
    }
}

/// Feeds kept rows to a sink as if they came from PostgreSQL.
pub fn replay(kept: &Recorded, sink: &mut dyn RowSink, max_rows: u64) -> AppResult<Fetched> {
    sink.start(&kept.columns)?;
    let mut rows = 0u64;
    for cells in &kept.rows {
        if rows >= max_rows {
            return Ok(Fetched {
                rows,
                truncated: true,
            });
        }
        rows += 1;
        if !sink.row(cells.clone())? {
            break;
        }
    }
    Ok(Fetched {
        rows,
        truncated: !kept.complete,
    })
}

/// Identifies the rows: same connection, SQL and filter values.
pub fn data_key(profile_id: &str, request: &ReportRequest) -> String {
    json!([profile_id, request.sql, request.variables, request.values]).to_string()
}

/// Identifies the PDF layout of those rows: same template (with orientation) and name.
pub fn layout_key(request: &ReportRequest) -> String {
    json!([request.title, request.template]).to_string()
}

struct Entry {
    data_key: String,
    kept: Arc<Recorded>,
    at: Instant,
    layout: Option<(String, Arc<Laid>)>,
}

impl Entry {
    fn fresh(&self, data_key: &str) -> bool {
        self.data_key == data_key && self.at.elapsed() < FRESH_FOR
    }
}

#[derive(Default)]
pub struct PreviewCache(Mutex<Option<Entry>>);

impl PreviewCache {
    fn lock(&self) -> std::sync::MutexGuard<'_, Option<Entry>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Kept rows for this data, if still fresh.
    pub fn rows(&self, data_key: &str) -> Option<Arc<Recorded>> {
        self.lock()
            .as_ref()
            .filter(|e| e.fresh(data_key))
            .map(|e| Arc::clone(&e.kept))
    }

    /// Keeps freshly read rows (replacing whatever was kept before).
    pub fn keep_rows(&self, data_key: String, kept: Recorded) {
        *self.lock() = Some(Entry {
            data_key,
            kept: Arc::new(kept),
            at: Instant::now(),
            layout: None,
        });
    }

    /// Keeps the layout drawn from the kept rows.
    pub fn keep_layout(&self, data_key: &str, layout_key: String, laid: Arc<Laid>) {
        if let Some(entry) = self.lock().as_mut().filter(|e| e.data_key == data_key) {
            entry.layout = Some((layout_key, laid));
        }
    }

    /// The kept layout for this data, template and name, if still fresh.
    pub fn layout(&self, data_key: &str, layout_key: &str) -> Option<Arc<Laid>> {
        self.lock()
            .as_ref()
            .filter(|e| e.fresh(data_key))
            .and_then(|e| e.layout.as_ref())
            .filter(|(key, _)| key == layout_key)
            .map(|(_, laid)| Arc::clone(laid))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Count(u64);
    impl RowSink for Count {
        fn start(&mut self, _: &[ResultColumn]) -> AppResult<()> {
            Ok(())
        }
        fn row(&mut self, _: Vec<Cell>) -> AppResult<bool> {
            self.0 += 1;
            Ok(true)
        }
    }

    fn kept(n: usize, complete: bool) -> Recorded {
        Recorded {
            columns: vec![ResultColumn {
                name: "n".into(),
                pg_type: "int4".into(),
            }],
            rows: (0..n)
                .map(|i| vec![Cell::Int { v: i.to_string() }])
                .collect(),
            complete,
        }
    }

    #[test]
    fn records_and_replays() {
        let mut count = Count(0);
        let mut rec = Recorder::new(&mut count);
        let source = kept(3, true);
        let fetched = replay(&source, &mut rec, 10).unwrap();
        assert_eq!((fetched.rows, fetched.truncated), (3, false));
        assert_eq!(rec.kept.rows.len(), 3);
        assert_eq!(count.0, 3);

        let mut count = Count(0);
        let fetched = replay(&kept(5, false), &mut count, 2).unwrap();
        assert_eq!((fetched.rows, fetched.truncated), (2, true));
    }

    #[test]
    fn keeps_one_preview() {
        let cache = PreviewCache::default();
        assert!(cache.rows("a").is_none());
        cache.keep_rows("a".into(), kept(2, true));
        assert_eq!(cache.rows("a").unwrap().rows.len(), 2);
        assert!(cache.rows("b").is_none());
        cache.keep_rows("b".into(), kept(1, false));
        assert!(cache.rows("a").is_none());
        assert!(cache.layout("b", "t").is_none());
    }
}
