//! Local search service.
//!
//! [`SearchService`] sits between the [`AppContext`] (database + clock)
//! and the deterministic [`LocalSearchEngine`]. It owns the read of
//! the entries table, builds the in-memory documents for the engine
//! and stitches each hit back to the [`clipvault_db::EntryRecord`] so
//! the Tauri command can serialise a result that the frontend can
//! re-use directly (e.g. when the future `quick-paste` change wires
//! the action).

use clipvault_db::{EntryRecord, EntryRepository, EntryRepositoryError, SourceAppFilter};
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
/// tag. `source_app` is the `source-app-filter` capability's third
/// facet: `All` is the absence of a restriction, `Known` pins the
/// candidate set to a single stable identifier, and `Unknown` keeps
/// the rows whose `source_app` is `NULL` or empty. All three facets
/// are AND-combined and applied before the ranking engine runs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SearchFilter {
    pub collection_id: Option<i64>,
    pub tag_ids: Vec<i64>,
    pub source_app: SourceAppFilter,
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
    /// filter. The filter is consumed after the read; the ranking
    /// algorithm never sees it.
    ///
    /// Candidates are pulled through [`EntryRepository::entries_filtered`]
    /// so every supported entry type is eligible. Textual rows keep
    /// their canonical content as a searchable field; non-textual rows
    /// (images today, any future binary payload) contribute an empty
    /// content field so the engine never inspects asset bytes, asset
    /// references or any other binary metadata. Every row still
    /// carries its persisted custom `title`, which is the only
    /// searchable field for a non-textual capture.
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
        // `entries_filtered` already enforces the deterministic
        // `created_at DESC, id DESC` order shared with the rail and
        // accepts the same `collection_id` / `tag_ids` / `source_app`
        // filter triple the rest of the rail uses, so the search
        // candidate set lines up byte-for-byte with what the user sees
        // when no query is active.
        let records = {
            let mut db = context.database().lock();
            let repo = EntryRepository::new(db.connection_mut());
            repo.entries_filtered(filter.collection_id, &filter.tag_ids, &filter.source_app)?
        };

        // Build SearchDocuments borrowing from the records we already
        // hold. No additional allocations beyond the Vec itself. The
        // `title` field carries the user-defined card title so the
        // local search can match a custom label without inspecting
        // clipboard content. `content` is only borrowed for textual
        // rows so the engine never reads asset bytes, asset
        // references or other binary metadata for an image row; the
        // persisted `IMAGE_CONTENT_SENTINEL` would already be empty,
        // but filtering explicitly here keeps the invariant local
        // and survives a future sentinel change.
        let documents: Vec<SearchDocument<'_>> = records
            .iter()
            .map(|record| {
                let searchable_content: &str = if record.content_type.is_textual() {
                    record.content.as_str()
                } else {
                    ""
                };
                SearchDocument {
                    entry_id: record.id,
                    content: searchable_content,
                    updated_at: record.updated_at.as_str(),
                    title: record.title.as_deref(),
                }
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
    use clipvault_db::{
        ContentType, NewEntry, OrganizationRepository, SourceAppFilter, IMAGE_CONTENT_SENTINEL,
        IMAGE_MIME_PNG,
    };
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

    fn insert_text_with_source(
        context: &AppContext,
        content: &str,
        source_app: Option<&str>,
        when: time::OffsetDateTime,
    ) -> i64 {
        let hash = format!("hash::{content}");
        let new = NewEntry::text(
            content.to_string(),
            ContentType::Text,
            content.len() as i64,
            hash,
            source_app.map(|s| s.to_string()),
            when,
            when,
        );
        let mut db = context.database().lock();
        let mut repo = EntryRepository::new(db.connection_mut());
        let outcome = repo.insert_or_touch(new).expect("insert");
        outcome.record().id
    }

    fn insert_image_entry(
        context: &AppContext,
        hash: &str,
        width: u32,
        height: u32,
        source_app: Option<&str>,
        when: time::OffsetDateTime,
    ) -> i64 {
        let new = NewEntry {
            content: IMAGE_CONTENT_SENTINEL.to_string(),
            content_type: ContentType::Image,
            content_size: 1_024,
            content_hash: hash.to_string(),
            source_app: source_app.map(|s| s.to_string()),
            created_at: when,
            last_seen_at: when,
            asset_ref: Some(format!("clipboard/{hash}.png")),
            mime_type: Some(IMAGE_MIME_PNG.to_string()),
            payload_width: Some(width),
            payload_height: Some(height),
            rich_text_hash: None,
            rich_html_ref: None,
            rich_rtf_ref: None,
            rich_preview_ref: None,
            rich_html_size: None,
            rich_rtf_size: None,
            code_language: None,
        };
        let mut db = context.database().lock();
        let mut repo = EntryRepository::new(db.connection_mut());
        let outcome = repo.insert_or_touch(new).expect("insert");
        outcome.record().id
    }

    fn set_title(context: &AppContext, id: i64, title: Option<&str>) {
        let now = datetime!(2026-01-02 03:04:05 UTC);
        let mut db = context.database().lock();
        let mut repo = EntryRepository::new(db.connection_mut());
        repo.set_title(id, title, now).expect("set_title");
    }

    fn create_user_collection(context: &AppContext, name: &str) -> i64 {
        let now = datetime!(2026-01-02 03:04:05 UTC);
        let mut db = context.database().lock();
        let mut repo = OrganizationRepository::new(db.connection_mut());
        let collection = repo.create_user_collection(name, now).expect("collection");
        collection.id
    }

    fn assign_to_collection(context: &AppContext, entry_id: i64, collection_id: i64) {
        let now = datetime!(2026-01-02 03:04:05 UTC);
        let mut db = context.database().lock();
        let mut repo = OrganizationRepository::new(db.connection_mut());
        repo.replace_entry_collections(entry_id, &[collection_id], now)
            .expect("assign");
    }

    fn upsert_and_assign_tag(context: &AppContext, entry_id: i64, name: &str) -> i64 {
        let now = datetime!(2026-01-02 03:04:05 UTC);
        let mut db = context.database().lock();
        let mut repo = OrganizationRepository::new(db.connection_mut());
        repo.upsert_and_assign_tag(entry_id, name, now)
            .expect("tag")
            .id
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

    // -----------------------------------------------------------------
    // `search-title-matching` regression suite.
    //
    // Every test below demonstrates one of the title-only or
    // mixed-field contracts from `openspec/changes/search-title-matching`.
    // They are written against the SearchService so a future
    // contributor can break either the service wiring or the engine
    // scoring without hiding behind the other layer.
    // -----------------------------------------------------------------

    fn service() -> SearchService {
        SearchService::new()
    }

    fn query(text: &str) -> SearchQuery {
        SearchQuery {
            text: text.to_string(),
            limit: 50,
        }
    }

    #[test]
    fn title_only_query_returns_text_entry_with_full_record() {
        let context = bootstrap_with(vec![datetime!(2026-01-02 03:04:05 UTC)]);
        let t = datetime!(2026-01-02 03:04:05 UTC);
        let id = insert_entry(&context, "lorem ipsum dolor", t);
        set_title(&context, id, Some("Proyecto Alfa"));

        let outcome = service().search(&context, &query("proyecto")).expect("ok");

        assert_eq!(outcome.hits.len(), 1);
        let hit = &outcome.hits[0];
        assert_eq!(hit.entry_id, id);
        assert_eq!(hit.record.title.as_deref(), Some("Proyecto Alfa"));
        assert_eq!(hit.record.content, "lorem ipsum dolor");
        assert_eq!(
            hit.score,
            clipvault_search::SCORE_TITLE_EXACT_PHRASE,
            "title-only hit must surface at the title tier",
        );
    }

    #[test]
    fn title_only_query_returns_image_entry_with_full_image_metadata() {
        let context = bootstrap_with(vec![datetime!(2026-01-02 03:04:05 UTC)]);
        let t = datetime!(2026-01-02 03:04:05 UTC);
        let hash = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let id = insert_image_entry(&context, hash, 8, 4, Some("com.example.Preview"), t);
        set_title(&context, id, Some("Mockup Final"));

        let outcome = service().search(&context, &query("mockup")).expect("ok");

        assert_eq!(outcome.hits.len(), 1);
        let hit = &outcome.hits[0];
        assert_eq!(hit.entry_id, id);
        assert_eq!(hit.record.content_type, ContentType::Image);
        assert_eq!(
            hit.record.asset_ref.as_deref(),
            Some("clipboard/0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef.png")
        );
        assert_eq!(hit.record.mime_type.as_deref(), Some("image/png"));
        assert_eq!(hit.record.payload_width, Some(8));
        assert_eq!(hit.record.payload_height, Some(4));
        assert_eq!(hit.record.title.as_deref(), Some("Mockup Final"));
        assert_eq!(hit.score, clipvault_search::SCORE_TITLE_EXACT_PHRASE);
    }

    #[test]
    fn title_only_image_hit_carries_an_empty_snippet() {
        let context = bootstrap_with(vec![datetime!(2026-01-02 03:04:05 UTC)]);
        let t = datetime!(2026-01-02 03:04:05 UTC);
        let id = insert_image_entry(
            &context,
            "abcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcd",
            2,
            2,
            None,
            t,
        );
        set_title(&context, id, Some("Screenshot"));

        let outcome = service()
            .search(&context, &query("screenshot"))
            .expect("ok");

        assert_eq!(outcome.hits.len(), 1);
        let hit = &outcome.hits[0];
        assert_eq!(hit.entry_id, id);
        assert!(
            hit.snippet.is_empty(),
            "image rows have no textual content; snippet must stay empty",
        );
    }

    #[test]
    fn whitespace_only_title_does_not_match() {
        let context = bootstrap_with(vec![datetime!(2026-01-02 03:04:05 UTC)]);
        let t = datetime!(2026-01-02 03:04:05 UTC);
        let id = insert_entry(&context, "lorem ipsum", t);
        set_title(&context, id, Some("   \t  "));

        let outcome = service().search(&context, &query("config")).expect("ok");

        assert!(outcome.hits.is_empty());
    }

    #[test]
    fn missing_title_falls_back_to_content_search() {
        let context = bootstrap_with(vec![datetime!(2026-01-02 03:04:05 UTC)]);
        let t = datetime!(2026-01-02 03:04:05 UTC);
        let id = insert_entry(&context, "configuración inicial", t);
        // No title set; default `None`.
        let outcome = service().search(&context, &query("config")).expect("ok");
        assert_eq!(outcome.hits.len(), 1);
        assert_eq!(outcome.hits[0].entry_id, id);
        assert_eq!(outcome.hits[0].record.title, None);
    }

    #[test]
    fn content_match_outranks_title_only_match() {
        let context = bootstrap_with(vec![datetime!(2026-01-02 03:04:05 UTC); 2]);
        let t = datetime!(2026-01-02 03:04:05 UTC);
        let title_only = insert_entry(&context, "lorem ipsum", t);
        set_title(&context, title_only, Some("Config Notes"));
        let content_match = insert_entry(&context, "alpha config beta", t);

        let outcome = service().search(&context, &query("config")).expect("ok");

        let ids: Vec<i64> = outcome.hits.iter().map(|h| h.entry_id).collect();
        assert_eq!(ids, vec![content_match, title_only]);
        assert_eq!(outcome.hits[0].score, clipvault_search::SCORE_EXACT_PHRASE);
        assert_eq!(
            outcome.hits[1].score,
            clipvault_search::SCORE_TITLE_EXACT_PHRASE
        );
    }

    #[test]
    fn mixed_ranking_is_deterministic_with_recency_and_id_tiebreak() {
        let context = bootstrap_with(vec![
            datetime!(2026-01-02 03:04:05 UTC),
            datetime!(2026-01-02 03:04:06 UTC),
            datetime!(2026-01-02 03:04:07 UTC),
        ]);
        // Distinct content keeps the dedupe key unique so every
        // insertion creates a fresh row. Insertion order keeps the
        // ids monotonic; later insertion -> higher id -> wins the
        // deterministic id tiebreak.
        let t_old = datetime!(2026-01-02 03:04:05 UTC);
        let t_mid = datetime!(2026-01-02 03:04:06 UTC);
        let t_new = datetime!(2026-01-02 03:04:07 UTC);
        let old_id = insert_entry(&context, "lorem ipsum dolor", t_old);
        let _ = insert_entry(&context, "alpha config beta", t_mid);
        let new_id = insert_entry(&context, "sit amet consectetur", t_new);
        set_title(&context, old_id, Some("Config Notes"));
        set_title(&context, new_id, Some("Config Backup"));

        let first = service().search(&context, &query("config")).expect("ok");
        let second = service().search(&context, &query("config")).expect("ok");

        let first_ids: Vec<i64> = first.hits.iter().map(|h| h.entry_id).collect();
        let second_ids: Vec<i64> = second.hits.iter().map(|h| h.entry_id).collect();
        assert_eq!(first_ids, second_ids);
        // The mid entry is the only content match and ranks first.
        // The two title-only hits share the same tier; the more
        // recent id wins.
        assert_eq!(first_ids, vec![2, new_id, old_id]);
        assert_eq!(first.hits[0].score, clipvault_search::SCORE_EXACT_PHRASE);
        assert_eq!(
            first.hits[1].score,
            clipvault_search::SCORE_TITLE_EXACT_PHRASE
        );
        assert_eq!(
            first.hits[2].score,
            clipvault_search::SCORE_TITLE_EXACT_PHRASE
        );
    }

    #[test]
    fn collection_scope_excludes_title_only_image_outside_collection() {
        let context = bootstrap_with(vec![datetime!(2026-01-02 03:04:05 UTC); 2]);
        let t = datetime!(2026-01-02 03:04:05 UTC);
        let work = create_user_collection(&context, "Trabajo");
        let personal = create_user_collection(&context, "Personal");
        let in_work = insert_entry(&context, "lorem ipsum", t);
        set_title(&context, in_work, Some("Config Trabajo"));
        assign_to_collection(&context, in_work, work);
        let _in_personal = insert_image_entry(
            &context,
            "11111111111111111111111111111111111111111111111111111111111111ab",
            4,
            2,
            None,
            t,
        );
        // The image-only title is intentionally not in `Trabajo`.
        let outside = insert_image_entry(
            &context,
            "22222222222222222222222222222222222222222222222222222222222222ab",
            4,
            2,
            None,
            t,
        );
        set_title(&context, outside, Some("Config Trabajo"));
        assign_to_collection(&context, outside, personal);

        let outcome = service()
            .search_with_filter(
                &context,
                &query("config"),
                &SearchFilter {
                    collection_id: Some(work),
                    ..SearchFilter::default()
                },
            )
            .expect("ok");

        let ids: Vec<i64> = outcome.hits.iter().map(|h| h.entry_id).collect();
        assert_eq!(ids, vec![in_work]);
        // The image with the same title lives outside the active
        // collection and must not surface.
        assert!(!ids.contains(&outside));
    }

    #[test]
    fn tag_scope_excludes_title_only_image_outside_tag() {
        let context = bootstrap_with(vec![datetime!(2026-01-02 03:04:05 UTC); 2]);
        let t = datetime!(2026-01-02 03:04:05 UTC);
        let tagged = insert_entry(&context, "lorem ipsum", t);
        set_title(&context, tagged, Some("Config Tagged"));
        let tag_id = upsert_and_assign_tag(&context, tagged, "código");
        let other = insert_image_entry(
            &context,
            "33333333333333333333333333333333333333333333333333333333333333ab",
            2,
            2,
            None,
            t,
        );
        set_title(&context, other, Some("Config Tagged"));

        let outcome = service()
            .search_with_filter(
                &context,
                &query("config"),
                &SearchFilter {
                    tag_ids: vec![tag_id],
                    ..SearchFilter::default()
                },
            )
            .expect("ok");

        let ids: Vec<i64> = outcome.hits.iter().map(|h| h.entry_id).collect();
        assert_eq!(ids, vec![tagged]);
        assert!(!ids.contains(&other));
    }

    #[test]
    fn source_app_scope_excludes_title_only_image_outside_source() {
        let context = bootstrap_with(vec![datetime!(2026-01-02 03:04:05 UTC); 2]);
        let t = datetime!(2026-01-02 03:04:05 UTC);
        let in_source =
            insert_text_with_source(&context, "lorem ipsum", Some("com.example.Editor"), t);
        set_title(&context, in_source, Some("Config Editor"));
        let _outside = insert_image_entry(
            &context,
            "44444444444444444444444444444444444444444444444444444444444444ab",
            4,
            4,
            Some("com.example.Other"),
            t,
        );
        let other_with_title = insert_image_entry(
            &context,
            "55555555555555555555555555555555555555555555555555555555555555ab",
            4,
            4,
            Some("com.example.Other"),
            t,
        );
        set_title(&context, other_with_title, Some("Config Editor"));

        let outcome = service()
            .search_with_filter(
                &context,
                &query("config"),
                &SearchFilter {
                    source_app: SourceAppFilter::Known {
                        source_app: "com.example.Editor".to_string(),
                    },
                    ..SearchFilter::default()
                },
            )
            .expect("ok");

        let ids: Vec<i64> = outcome.hits.iter().map(|h| h.entry_id).collect();
        assert_eq!(ids, vec![in_source]);
        assert!(!ids.contains(&other_with_title));
    }

    #[test]
    fn no_matches_returns_empty_hits_with_ok_note() {
        let context = bootstrap_with(vec![datetime!(2026-01-02 03:04:05 UTC)]);
        let t = datetime!(2026-01-02 03:04:05 UTC);
        let id = insert_entry(&context, "hello world", t);
        set_title(&context, id, Some("Custom"));

        let outcome = service()
            .search(&context, &query("unobtainium"))
            .expect("ok");

        assert!(outcome.hits.is_empty());
        assert_eq!(outcome.note, clipvault_search::OK_NOTE);
    }

    #[test]
    fn empty_query_does_not_iterate_documents() {
        // Reused from the pre-title contract: the engine short-circuits
        // empty queries without touching the candidate set. The new
        // image-inclusive path must keep the same short-circuit.
        let context = bootstrap_with(vec![datetime!(2026-01-02 03:04:05 UTC)]);
        let t = datetime!(2026-01-02 03:04:05 UTC);
        let _image = insert_image_entry(
            &context,
            "66666666666666666666666666666666666666666666666666666666666666ab",
            2,
            2,
            None,
            t,
        );

        let outcome = service().search(&context, &query("   ")).expect("ok");

        assert!(outcome.hits.is_empty());
        assert_eq!(outcome.note, clipvault_search::EMPTY_QUERY_NOTE);
    }

    #[test]
    fn search_outcome_does_not_mutate_records() {
        // Read every persisted field once, run a title-only query, and
        // read every persisted field again. The second snapshot must
        // match the first byte-for-byte, including the title assigned
        // by the test. This guards against accidental writes through
        // the search path.
        let context = bootstrap_with(vec![datetime!(2026-01-02 03:04:05 UTC); 2]);
        let t = datetime!(2026-01-02 03:04:05 UTC);
        let text_id = insert_entry(&context, "lorem ipsum", t);
        set_title(&context, text_id, Some("Proyecto Alfa"));
        let image_id = insert_image_entry(
            &context,
            "77777777777777777777777777777777777777777777777777777777777777ab",
            4,
            4,
            Some("com.example.Preview"),
            t,
        );
        set_title(&context, image_id, Some("Mockup"));

        let snapshot_before = snapshot_records(&context);

        let outcome = service().search(&context, &query("proyecto")).expect("ok");
        assert_eq!(outcome.hits.len(), 1);
        let outcome2 = service().search(&context, &query("mockup")).expect("ok");
        assert_eq!(outcome2.hits.len(), 1);

        let snapshot_after = snapshot_records(&context);
        assert_eq!(snapshot_before, snapshot_after);
    }

    fn snapshot_records(context: &AppContext) -> Vec<clipvault_db::EntryRecord> {
        let mut db = context.database().lock();
        let repo = EntryRepository::new(db.connection_mut());
        repo.entries_filtered(None, &[], &SourceAppFilter::All)
            .expect("snapshot")
    }

    #[test]
    fn title_phrase_tokens_and_fuzzy_tiers_all_surface_at_their_tier() {
        // Two-word query separates the three title tiers cleanly:
        // - `exact` matches when the full phrase is a substring of the
        //   title;
        // - `tokens` matches when every query token appears as a
        //   whole token of the title but the phrase is not contiguous;
        // - `fuzzy` matches when every query token fuzzy-matches a
        //   title token (here a single substitution).
        let context = bootstrap_with(vec![datetime!(2026-01-02 03:04:05 UTC); 3]);
        let t = datetime!(2026-01-02 03:04:05 UTC);
        let exact = insert_entry(&context, "lorem ipsum dolor", t);
        set_title(&context, exact, Some("Config Notes Backup"));
        let tokens = insert_entry(&context, "sit amet consectetur", t);
        set_title(&context, tokens, Some("Backup of Config and Notes"));
        let fuzzy = insert_entry(&context, "alpha beta gamma", t);
        set_title(&context, fuzzy, Some("Konfig Notes"));

        let outcome = service()
            .search(&context, &query("config notes"))
            .expect("ok");

        let tier_for = |hit: &SearchEntryHit| match hit.score {
            s if s == clipvault_search::SCORE_TITLE_EXACT_PHRASE => "exact",
            s if s == clipvault_search::SCORE_TITLE_ALL_TOKENS_SUBSTRING => "tokens",
            s if s == clipvault_search::SCORE_TITLE_FUZZY => "fuzzy",
            other => panic!("unexpected tier score: {other}"),
        };
        let mut by_id: std::collections::HashMap<i64, &str> = std::collections::HashMap::new();
        for hit in &outcome.hits {
            by_id.insert(hit.entry_id, tier_for(hit));
        }
        assert_eq!(by_id.get(&exact).copied(), Some("exact"));
        assert_eq!(by_id.get(&tokens).copied(), Some("tokens"));
        assert_eq!(by_id.get(&fuzzy).copied(), Some("fuzzy"));
    }
}
