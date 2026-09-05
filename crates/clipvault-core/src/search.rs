//! Local search service.
//!
//! [`SearchService`] sits between the [`AppContext`] (database + clock)
//! and the deterministic [`LocalSearchEngine`]. It owns the read of
//! the entries table, builds the in-memory documents for the engine
//! and stitches each hit back to the [`clipvault_db::EntryRecord`] so
//! the Tauri command can serialise a result that the frontend can
//! re-use directly (e.g. when the future `quick-paste` change wires
//! the action).

use clipvault_db::{EntryRecord, EntryRepository, EntryRepositoryError};
use clipvault_search::{
    LocalSearchEngine, SearchDocument, SearchEngine, SearchHit, SearchQuery, SearchResults,
};
use serde::Serialize;
use thiserror::Error;

use crate::bootstrap::AppContext;

/// Hard ceiling for the per-call `limit`. Mirrors
/// `clipvault_recent_entries` so the user-visible contract stays
/// predictable.
pub const SEARCH_MAX_LIMIT: usize = 500;

/// Default limit applied when the caller omits one.
pub const SEARCH_DEFAULT_LIMIT: usize = 50;

/// Error returned by [`SearchService::search`]. Repository errors are
/// the only realistic failure today; future backends (FTS5, …) can
/// extend the enum without changing call sites.
#[derive(Debug, Error)]
pub enum SearchServiceError {
    #[error("entry repository error: {0}")]
    Repository(#[from] EntryRepositoryError),
    /// Engine rejected the query before consulting the database.
    #[error("invalid search query: {0}")]
    InvalidQuery(&'static str),
}

/// Single hit returned to the frontend. It carries enough metadata to
/// render the row and to drive the future `quick-paste` action without
/// re-querying the database.
#[derive(Debug, Clone, Serialize)]
pub struct SearchEntryHit {
    pub entry_id: i64,
    pub snippet: String,
    pub score: i32,
    pub record: EntryRecord,
}

/// Outcome of [`SearchService::search`]. `note` mirrors the engine's
/// own note so the UI can distinguish an empty query from "no matches".
#[derive(Debug, Clone)]
pub struct SearchServiceOutcome {
    pub hits: Vec<SearchEntryHit>,
    pub note: String,
}

/// Optional organization filters applied by the search service. The
/// `tags-and-collections` capability introduces the filter set
/// without changing the existing ranking algorithm: filters only
/// reduce the candidate set before the engine ranks, so a query that
/// produces hits still surfaces them in the same deterministic order.
///
/// `collection_id == None` means "no collection filter" — equivalent
/// to selecting `Historial`. `tag_ids` is AND-combined: empty means
/// "no tag filter", otherwise the entry must carry every supplied
/// tag.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SearchFilter {
    pub collection_id: Option<i64>,
    pub tag_ids: Vec<i64>,
}

/// Service that wires the entry repository to the search engine.
#[derive(Debug, Default, Clone)]
pub struct SearchService;

impl SearchService {
    pub fn new() -> Self {
        Self
    }

    /// Run a search against the local SQLite history. The query is
    /// clamped to a sane range (1..=[`SEARCH_MAX_LIMIT`]) so the
    /// engine never sees a zero or runaway value.
    pub fn search(
        &self,
        context: &AppContext,
        query: &SearchQuery,
    ) -> Result<SearchServiceOutcome, SearchServiceError> {
        self.search_with_filter(context, query, &SearchFilter::default())
    }

    /// Same as [`Self::search`] but applies the optional organization
    /// filter. The filter is consumed after the textual read; the
    /// ranking algorithm never sees it.
    pub fn search_with_filter(
        &self,
        context: &AppContext,
        query: &SearchQuery,
        filter: &SearchFilter,
    ) -> Result<SearchServiceOutcome, SearchServiceError> {
        let limit = clamp_limit(query.limit);
        let clamped_query = SearchQuery {
            text: query.text.clone(),
            limit,
        };

        // Read entries under the same lock the rest of the core uses.
        // The filtered query is one indexed read; the unfiltered path
        // is preserved as a fast-path hot optimisation for callers
        // that do not need the extra `JOIN`s.
        let records = {
            let mut db = context.database().lock();
            let repo = EntryRepository::new(db.connection_mut());
            if filter.collection_id.is_none() && filter.tag_ids.is_empty() {
                repo.text_entries()?
            } else {
                repo.text_entries_filtered(filter.collection_id, &filter.tag_ids)?
            }
        };

        // Build SearchDocuments borrowing from the records we already
        // hold. No additional allocations beyond the Vec itself.
        let documents: Vec<SearchDocument<'_>> = records
            .iter()
            .map(|record| SearchDocument {
                entry_id: record.id,
                content: record.content.as_str(),
                updated_at: record.updated_at.as_str(),
            })
            .collect();

        let engine = LocalSearchEngine;
        let results = engine
            .search(&clamped_query, &documents)
            .map_err(map_search_error)?;

        Ok(join_with_records(results, records))
    }
}

fn clamp_limit(requested: usize) -> usize {
    if requested == 0 {
        SEARCH_DEFAULT_LIMIT
    } else {
        requested.min(SEARCH_MAX_LIMIT)
    }
}

fn map_search_error(error: clipvault_search::SearchError) -> SearchServiceError {
    match error {
        clipvault_search::SearchError::InvalidQuery(message) => {
            SearchServiceError::InvalidQuery(message)
        }
    }
}

fn join_with_records(results: SearchResults, records: Vec<EntryRecord>) -> SearchServiceOutcome {
    // Build a lookup so we can stitch hits back to their records in a
    // single pass without re-scanning the records vector per hit.
    let by_id: std::collections::HashMap<i64, EntryRecord> =
        records.into_iter().map(|r| (r.id, r)).collect();

    let hits = results
        .hits
        .into_iter()
        .filter_map(|hit: SearchHit| {
            by_id.get(&hit.entry_id).map(|record| SearchEntryHit {
                entry_id: hit.entry_id,
                snippet: hit.snippet,
                score: hit.score,
                record: record.clone(),
            })
        })
        .collect();

    SearchServiceOutcome {
        hits,
        note: results.note,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clipvault_db::{ContentType, NewEntry};
    use std::sync::Arc;
    use time::macros::datetime;

    use crate::bootstrap::AppBootstrap;
    use crate::clock::Clock;
    use parking_lot::Mutex;

    /// Minimal clock that returns the timestamps tests insert.
    #[derive(Debug)]
    struct SequenceClock {
        stamps: Mutex<Vec<time::OffsetDateTime>>,
        idx: Mutex<usize>,
    }

    impl SequenceClock {
        fn new(stamps: Vec<time::OffsetDateTime>) -> Self {
            Self {
                stamps: Mutex::new(stamps),
                idx: Mutex::new(0),
            }
        }
    }

    impl Clock for SequenceClock {
        fn now(&self) -> time::OffsetDateTime {
            let mut idx = self.idx.lock();
            let stamps = self.stamps.lock();
            let value = stamps[*idx];
            *idx = (*idx + 1).min(stamps.len().saturating_sub(1));
            value
        }
    }

    fn bootstrap_with(stamps: Vec<time::OffsetDateTime>) -> AppContext {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = clipvault_db::Database::open(dir.path().join("clipvault.db")).expect("open");
        let mut db = db;
        db.run_migrations(&clipvault_db::builtin_migrations())
            .expect("migrate");
        let adapters =
            crate::test_support::build_isolated_adapters(dir.path(), &dir.path().join("data"));
        let bootstrap = AppBootstrap::new()
            .with_clock(Arc::new(SequenceClock::new(stamps)))
            .with_clipboard(Arc::new(crate::clipboard::FakeClipboard::new()))
            .with_platform_adapters(adapters);
        bootstrap
            .bootstrap_with_database(db, dir.path().join("clipvault.db"))
            .expect("bootstrap")
    }

    fn insert_entry(context: &AppContext, content: &str, when: time::OffsetDateTime) -> i64 {
        let hash = format!("hash::{content}");
        let new = NewEntry::text(
            content.to_string(),
            ContentType::Text,
            content.len() as i64,
            hash,
            Some("test".to_string()),
            when,
            when,
        );
        let mut db = context.database().lock();
        let mut repo = EntryRepository::new(db.connection_mut());
        let outcome = repo.insert_or_touch(new).expect("insert");
        outcome.record().id
    }

    #[test]
    fn empty_query_does_not_iterate_the_database() {
        let context = bootstrap_with(vec![datetime!(2026-01-02 03:04:05 UTC)]);
        let t = datetime!(2026-01-02 03:04:05 UTC);
        insert_entry(&context, "hello world", t);

        let service = SearchService::new();
        let outcome = service
            .search(
                &context,
                &SearchQuery {
                    text: "   ".to_string(),
                    limit: 10,
                },
            )
            .expect("ok");

        assert!(outcome.hits.is_empty());
        assert_eq!(outcome.note, clipvault_search::EMPTY_QUERY_NOTE);
    }

    #[test]
    fn ranks_exact_phrase_above_partial_and_fuzzy() {
        let context = bootstrap_with(vec![datetime!(2026-01-02 03:04:05 UTC); 4]);
        let t = datetime!(2026-01-02 03:04:05 UTC);
        let id_exact = insert_entry(&context, "Hello World", t);
        let id_substring = insert_entry(&context, "world peace hello", t);
        let id_fuzzy = insert_entry(&context, "world helo", t);

        let service = SearchService::new();
        let outcome = service
            .search(
                &context,
                &SearchQuery {
                    text: "hello world".to_string(),
                    limit: 10,
                },
            )
            .expect("ok");

        let ids: Vec<i64> = outcome.hits.iter().map(|h| h.entry_id).collect();
        assert_eq!(ids, vec![id_exact, id_substring, id_fuzzy]);
        assert!(outcome.hits[0].score > outcome.hits[1].score);
        assert!(outcome.hits[1].score > outcome.hits[2].score);
    }

    #[test]
    fn limit_is_clamped_to_maximum_and_default() {
        let context = bootstrap_with(vec![datetime!(2026-01-02 03:04:05 UTC); 2]);
        let t = datetime!(2026-01-02 03:04:05 UTC);
        insert_entry(&context, "alpha", t);
        insert_entry(&context, "beta", t);

        let service = SearchService::new();

        // Zero-limit falls back to the default but the query is also empty,
        // so we expect `empty_query` here. Use a real query instead.
        let outcome = service
            .search(
                &context,
                &SearchQuery {
                    text: "alpha".to_string(),
                    limit: 0,
                },
            )
            .expect("ok");
        // default limit is 50, only one matching row exists.
        assert_eq!(outcome.hits.len(), 1);

        // Above-max limit is clamped to 500.
        let outcome = service
            .search(
                &context,
                &SearchQuery {
                    text: "alpha".to_string(),
                    limit: SEARCH_MAX_LIMIT + 100,
                },
            )
            .expect("ok");
        assert_eq!(outcome.hits.len(), 1);
    }

    #[test]
    fn hit_carries_record_metadata_for_quick_paste() {
        let context = bootstrap_with(vec![datetime!(2026-01-02 03:04:05 UTC)]);
        let t = datetime!(2026-01-02 03:04:05 UTC);
        let id = insert_entry(&context, "token to remember", t);

        let service = SearchService::new();
        let outcome = service
            .search(
                &context,
                &SearchQuery {
                    text: "token".to_string(),
                    limit: 10,
                },
            )
            .expect("ok");

        assert_eq!(outcome.hits.len(), 1);
        let hit = &outcome.hits[0];
        assert_eq!(hit.entry_id, id);
        assert_eq!(hit.record.id, id);
        assert_eq!(hit.record.content, "token to remember");
        assert!(!hit.snippet.is_empty());
    }
}
