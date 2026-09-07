//! Local search engine over the clipboard history.
//!
//! The engine is intentionally self-contained: it operates on
//! in-memory [`SearchDocument`] values that the core builds from
//! the SQLite repository. It does **not** depend on Tauri, Svelte,
//! SQLite or the OS clipboard.
//!
//! ## Ranking
//!
//! Hits are ranked by discrete quality tiers (exact phrase, all-tokens
//! substring, fuzzy) and then by `updated_at` descending and `id`
//! descending. See the per-module docs in [`engine::LocalSearchEngine`]
//! for the exact formula and the deterministic ordering guarantees.

mod engine;
mod fuzzy;
mod normalize;
mod snippet;

pub use engine::{
    LocalSearchEngine, SearchDocument, SearchEngine, SearchError, SearchHit, SearchQuery,
    SearchResults, EMPTY_QUERY_NOTE, FUZZY_MAX_DISTANCE, OK_NOTE, SCORE_ALL_TOKENS_SUBSTRING,
    SCORE_EXACT_PHRASE, SCORE_FUZZY, SCORE_TITLE_ALL_TOKENS_SUBSTRING, SCORE_TITLE_EXACT_PHRASE,
    SCORE_TITLE_FUZZY, SNIPPET_MAX_CHARS,
};
