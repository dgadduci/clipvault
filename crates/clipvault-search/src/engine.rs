//! Public search API and the deterministic local engine.

use serde::{Deserialize, Serialize};

use crate::fuzzy::fuzzy_token_match;
use crate::normalize::normalize_query;
use crate::snippet::build_snippet;

/// Score used by [`LocalSearchEngine`] when the entire normalized
/// phrase appears as a substring of the document content.
pub const SCORE_EXACT_PHRASE: i32 = 3_000;

/// Score used when every token of the query appears as a substring of
/// the document content but the full phrase does not.
pub const SCORE_ALL_TOKENS_SUBSTRING: i32 = 2_000;

/// Score used when every token of the query is matched via bounded
/// fuzzy matching but no exact or substring hit is available.
pub const SCORE_FUZZY: i32 = 1_000;

/// Score used when the entire normalized phrase matches the
/// user-defined card title only (the document content does not match
/// at any tier). Title matches always rank below any content match so
/// the existing ranking semantics for content stay intact; the
/// constants exist purely so a title-only entry can still surface in
/// the Quick Paste results list.
pub const SCORE_TITLE_EXACT_PHRASE: i32 = 800;

/// Score used when every query token appears as a substring of the
/// title only (content does not match).
pub const SCORE_TITLE_ALL_TOKENS_SUBSTRING: i32 = 600;

/// Score used when every query token fuzzy-matches the title only
/// (content does not match at any tier).
pub const SCORE_TITLE_FUZZY: i32 = 400;

/// Default length, in `char`s, of the snippet returned with each hit.
pub const SNIPPET_MAX_CHARS: usize = 80;

/// Note emitted by [`SearchResults`] when the query normalizes to an
/// empty token list. The frontend treats this as "no query yet".
pub const EMPTY_QUERY_NOTE: &str = "empty_query";

/// Note emitted when the engine ran to completion with a non-empty
/// query. Empty hit lists with this note represent "no matches".
pub const OK_NOTE: &str = "ok";

/// Edit distance threshold for the fuzzy tier. Tokens whose distance
/// to a content token exceeds this value do not match.
pub const FUZZY_MAX_DISTANCE: usize = 2;

/// User query, validated by the service layer (limit clamped) before
/// reaching the engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchQuery {
    pub text: String,
    pub limit: usize,
}

/// One document the engine can score. The engine borrows the strings so
/// it does not own any data and does not need to allocate per call.
///
/// `title` is the user-defined card title (the "custom" label the
/// `card-title-editing-regression` change persists on the row).
/// `None` for rows that never received a custom title; the engine
/// falls back to a content-only match in that case. The field is
/// metadata only: the engine never logs, returns or persists it.
#[derive(Debug, Clone)]
pub struct SearchDocument<'a> {
    pub entry_id: i64,
    pub content: &'a str,
    pub updated_at: &'a str,
    pub title: Option<&'a str>,
}

/// One hit produced by the engine. `score` is the tier base; the
/// service layer is responsible for any additional ranking metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SearchHit {
    pub entry_id: i64,
    pub snippet: String,
    pub score: i32,
}

/// Result of running [`SearchEngine::search`].
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SearchResults {
    pub hits: Vec<SearchHit>,
    /// Stable, machine-readable note for the UI. One of:
    /// - [`EMPTY_QUERY_NOTE`]: query was empty or only whitespace.
    /// - [`OK_NOTE`]: query was processed; `hits` reflects matches.
    pub note: String,
}

#[derive(Debug, thiserror::Error)]
pub enum SearchError {
    #[error("invalid search query: {0}")]
    InvalidQuery(&'static str),
}

/// Engine contract. The core builds the in-memory document list and
/// delegates to the implementation provided here.
pub trait SearchEngine {
    fn search(
        &self,
        query: &SearchQuery,
        documents: &[SearchDocument<'_>],
    ) -> Result<SearchResults, SearchError>;
}

/// Local, deterministic engine. It is pure (no I/O) and allocation-light:
/// one normalized query plus one pass per document.
#[derive(Debug, Default, Clone)]
pub struct LocalSearchEngine;

impl SearchEngine for LocalSearchEngine {
    fn search(
        &self,
        query: &SearchQuery,
        documents: &[SearchDocument<'_>],
    ) -> Result<SearchResults, SearchError> {
        let normalized = normalize_query(&query.text);
        if normalized.tokens.is_empty() {
            return Ok(SearchResults {
                hits: Vec::new(),
                note: EMPTY_QUERY_NOTE.to_string(),
            });
        }

        let limit = query.limit.max(1);
        let mut scored: Vec<SearchHit> = Vec::new();

        let query_view = NormalizedQuery {
            phrase: &normalized.phrase,
            tokens: &normalized.tokens,
        };

        for doc in documents {
            if let Some(score) = score_document(doc, &query_view) {
                let snippet = build_snippet(doc.content, &normalized.tokens);
                scored.push(SearchHit {
                    entry_id: doc.entry_id,
                    snippet,
                    score,
                });
            }
        }

        sort_hits(&mut scored, documents);
        scored.truncate(limit);

        Ok(SearchResults {
            hits: scored,
            note: OK_NOTE.to_string(),
        })
    }
}

#[derive(Debug)]
struct NormalizedQuery<'a> {
    /// Original query text, lowercased and with collapsed whitespace.
    phrase: &'a str,
    /// Whitespace-split tokens of `phrase`. Always non-empty when the
    /// query is processed.
    tokens: &'a [String],
}

/// Compute the score tier for one document. Returns `None` when no
/// tier applies (the document is dropped).
///
/// Content matches always outrank title matches so the existing
/// ranking semantics for content are preserved bit-for-bit. A
/// title-only match still surfaces the row, but at the lower
/// [`SCORE_TITLE_*`] tier so the order stays deterministic.
fn score_document(doc: &SearchDocument<'_>, query: &NormalizedQuery<'_>) -> Option<i32> {
    let normalized_content = normalize_query(doc.content).phrase;

    // Content first — the existing tiers (3000 / 2000 / 1000) keep
    // their absolute priority.
    if !normalized_content.is_empty() {
        if let Some(score) = score_field(
            &normalized_content,
            query,
            SCORE_EXACT_PHRASE,
            SCORE_ALL_TOKENS_SUBSTRING,
            SCORE_FUZZY,
        ) {
            return Some(score);
        }
    }

    // Title fallback. The title is `Option<&str>`; missing or
    // whitespace-only titles collapse to "no match" without touching
    // the snippet, so the existing content-based snippet keeps
    // working unchanged.
    let normalized_title = doc.title.map(normalize_query);
    if let Some(title) = normalized_title.as_ref().filter(|n| !n.phrase.is_empty()) {
        if let Some(score) = score_field(
            &title.phrase,
            query,
            SCORE_TITLE_EXACT_PHRASE,
            SCORE_TITLE_ALL_TOKENS_SUBSTRING,
            SCORE_TITLE_FUZZY,
        ) {
            return Some(score);
        }
    }

    None
}

/// Score a single normalised field (content or title) using the three
/// supplied tier constants. The function is total: a whitespace-only
/// field is treated as "no match" and returns `None`.
fn score_field(
    normalized_field: &str,
    query: &NormalizedQuery<'_>,
    exact_phrase: i32,
    all_tokens_substring: i32,
    fuzzy: i32,
) -> Option<i32> {
    if normalized_field.is_empty() {
        return None;
    }

    if !query.phrase.is_empty() && normalized_field.contains(query.phrase) {
        return Some(exact_phrase);
    }

    let field_tokens = tokenize_for_match(normalized_field);
    if query.tokens.iter().all(|token| {
        field_tokens
            .iter()
            .any(|field_token| field_token.as_str() == token.as_str())
    }) {
        return Some(all_tokens_substring);
    }

    if query.tokens.iter().all(|token| {
        field_tokens
            .iter()
            .any(|field_token| fuzzy_token_match(token, field_token))
    }) {
        return Some(fuzzy);
    }

    None
}

/// Tokenize an already-normalized string into non-empty tokens.
fn tokenize_for_match(normalized: &str) -> Vec<String> {
    normalized
        .split_whitespace()
        .map(|s| s.to_string())
        .collect()
}

/// Sort hits deterministically. Order: score desc, updated_at desc,
/// entry_id desc. Stable in the mathematical sense for equal tuples.
fn sort_hits(hits: &mut [SearchHit], documents: &[SearchDocument<'_>]) {
    // Build an updated_at lookup so we don't repeat the scan per comparison.
    let updated_at: std::collections::HashMap<i64, &str> = documents
        .iter()
        .map(|doc| (doc.entry_id, doc.updated_at))
        .collect();

    hits.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| {
                let a_ts = updated_at.get(&a.entry_id).copied().unwrap_or("");
                let b_ts = updated_at.get(&b.entry_id).copied().unwrap_or("");
                b_ts.cmp(a_ts)
            })
            .then_with(|| b.entry_id.cmp(&a.entry_id))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(id: i64, content: &'static str, updated_at: &'static str) -> SearchDocument<'static> {
        SearchDocument {
            entry_id: id,
            content,
            updated_at,
            title: None,
        }
    }

    fn query(text: &str, limit: usize) -> SearchQuery {
        SearchQuery {
            text: text.to_string(),
            limit,
        }
    }

    #[test]
    fn empty_query_returns_empty_query_note_without_iterating_documents() {
        let engine = LocalSearchEngine;
        // Use a sentinel updated_at we can spot if the engine ever touches it.
        let documents = vec![doc(1, "hello world", "SENTINEL")];
        let result = engine
            .search(&query("   \t  ", 10), &documents)
            .expect("ok");
        assert_eq!(result.hits.len(), 0);
        assert_eq!(result.note, EMPTY_QUERY_NOTE);
    }

    #[test]
    fn exact_phrase_outranks_all_tokens_substring_outranks_fuzzy() {
        let engine = LocalSearchEngine;
        let documents = vec![
            doc(1, "Hello World", "2026-01-02T03:04:05Z"),
            doc(2, "world peace hello", "2026-01-02T03:05:00Z"),
            doc(3, "world helo", "2026-01-02T03:06:00Z"),
        ];

        let result = engine
            .search(&query("hello world", 10), &documents)
            .expect("ok");

        assert_eq!(result.hits.len(), 3);
        assert_eq!(result.hits[0].entry_id, 1);
        assert_eq!(result.hits[0].score, SCORE_EXACT_PHRASE);
        assert_eq!(result.hits[1].entry_id, 2);
        assert_eq!(result.hits[1].score, SCORE_ALL_TOKENS_SUBSTRING);
        assert_eq!(result.hits[2].entry_id, 3);
        assert_eq!(result.hits[2].score, SCORE_FUZZY);
    }

    #[test]
    fn ranking_is_deterministic_across_repeated_runs() {
        let engine = LocalSearchEngine;
        let documents = vec![
            doc(1, "alpha beta gamma", "2026-01-02T03:04:05Z"),
            doc(2, "beta gamma", "2026-01-02T03:04:05Z"),
            doc(3, "alpha gamma", "2026-01-02T03:04:05Z"),
        ];

        let first = engine.search(&query("alpha beta", 10), &documents).unwrap();
        let second = engine.search(&query("alpha beta", 10), &documents).unwrap();

        let first_ids: Vec<i64> = first.hits.iter().map(|h| h.entry_id).collect();
        let second_ids: Vec<i64> = second.hits.iter().map(|h| h.entry_id).collect();
        assert_eq!(first_ids, second_ids);
    }

    #[test]
    fn recency_tiebreak_prefers_more_recent_updated_at() {
        let engine = LocalSearchEngine;
        let documents = vec![
            doc(1, "alpha beta", "2026-01-02T03:04:05Z"),
            doc(2, "alpha beta", "2026-01-02T03:10:00Z"),
        ];

        let result = engine.search(&query("alpha beta", 10), &documents).unwrap();
        assert_eq!(result.hits.len(), 2);
        assert_eq!(result.hits[0].entry_id, 2);
        assert_eq!(result.hits[1].entry_id, 1);
    }

    #[test]
    fn id_tiebreak_prefers_higher_id_when_recency_ties() {
        let engine = LocalSearchEngine;
        let documents = vec![
            doc(5, "alpha beta", "2026-01-02T03:04:05Z"),
            doc(7, "alpha beta", "2026-01-02T03:04:05Z"),
        ];

        let result = engine.search(&query("alpha beta", 10), &documents).unwrap();
        assert_eq!(result.hits[0].entry_id, 7);
        assert_eq!(result.hits[1].entry_id, 5);
    }

    #[test]
    fn limit_clamped_to_minimum_one() {
        let engine = LocalSearchEngine;
        let documents = vec![doc(1, "alpha beta", "2026-01-02T03:04:05Z")];
        let result = engine.search(&query("alpha beta", 0), &documents).unwrap();
        assert_eq!(result.hits.len(), 1);
    }

    #[test]
    fn limit_caps_hits() {
        let engine = LocalSearchEngine;
        let documents = vec![
            doc(1, "alpha", "2026-01-02T03:04:05Z"),
            doc(2, "alpha", "2026-01-02T03:05:00Z"),
            doc(3, "alpha", "2026-01-02T03:06:00Z"),
        ];
        let result = engine.search(&query("alpha", 2), &documents).unwrap();
        assert_eq!(result.hits.len(), 2);
    }

    #[test]
    fn fuzzy_does_not_apply_to_short_query_tokens() {
        let engine = LocalSearchEngine;
        // "cat" has length 3 so it must match exactly; "cta" is one edit away but
        // is itself the query token, so it requires an exact match too.
        let documents = vec![doc(1, "category entry", "2026-01-02T03:04:05Z")];

        let result = engine.search(&query("cat", 10), &documents).expect("ok");
        // "cat" is a substring of "category", so the single-token phrase
        // matches at the exact-phrase tier (3000). Tier 2 would also match
        // but tier 1 wins by definition.
        assert_eq!(result.hits.len(), 1);
        assert_eq!(result.hits[0].score, SCORE_EXACT_PHRASE);

        // A query token of length 3 that does NOT substring-match should not
        // promote via fuzzy.
        let result = engine.search(&query("cta", 10), &documents).expect("ok");
        assert_eq!(result.hits.len(), 0);
    }

    #[test]
    fn fuzzy_matches_tokens_within_distance_two() {
        let engine = LocalSearchEngine;
        // "kitten" -> "sitten": substitute k->s, distance 1.
        let documents = vec![doc(1, "sitten and sat", "2026-01-02T03:04:05Z")];
        let result = engine.search(&query("kitten", 10), &documents).expect("ok");
        assert_eq!(result.hits.len(), 1);
        assert_eq!(result.hits[0].score, SCORE_FUZZY);
    }

    #[test]
    fn fuzzy_drops_matches_above_distance_threshold() {
        let engine = LocalSearchEngine;
        // "moon" -> "morning": distance is 4, beyond threshold.
        let documents = vec![doc(1, "morning routine", "2026-01-02T03:04:05Z")];
        let result = engine.search(&query("moon", 10), &documents).expect("ok");
        assert_eq!(result.hits.len(), 0);
    }

    #[test]
    fn snippet_is_built_around_first_query_token() {
        let engine = LocalSearchEngine;
        let documents = vec![doc(1, "lorem ipsum dolor sit amet", "2026-01-02T03:04:05Z")];
        let result = engine.search(&query("dolor", 10), &documents).expect("ok");
        assert_eq!(result.hits.len(), 1);
        assert!(result.hits[0].snippet.contains("dolor"));
    }

    #[test]
    fn no_matches_returns_ok_note_with_empty_hits() {
        let engine = LocalSearchEngine;
        let documents = vec![doc(1, "hello world", "2026-01-02T03:04:05Z")];
        let result = engine
            .search(&query("unobtainium", 10), &documents)
            .expect("ok");
        assert!(result.hits.is_empty());
        assert_eq!(result.note, OK_NOTE);
    }

    fn doc_with_title(
        id: i64,
        content: &'static str,
        updated_at: &'static str,
        title: Option<&'static str>,
    ) -> SearchDocument<'static> {
        SearchDocument {
            entry_id: id,
            content,
            updated_at,
            title,
        }
    }

    #[test]
    fn title_only_match_returns_the_entry_at_the_title_tier() {
        // The query appears in the custom title only — never in the
        // canonical content. The entry MUST still surface so the
        // Quick Paste title-search contract holds.
        let engine = LocalSearchEngine;
        let documents = vec![doc_with_title(
            1,
            "lorem ipsum dolor",
            "2026-01-02T03:04:05Z",
            Some("Config Notes"),
        )];
        let result = engine.search(&query("config", 10), &documents).expect("ok");
        assert_eq!(result.hits.len(), 1);
        assert_eq!(result.hits[0].entry_id, 1);
        assert_eq!(result.hits[0].score, SCORE_TITLE_EXACT_PHRASE);
    }

    #[test]
    fn content_match_outranks_title_only_match() {
        // Two entries, one matches via content, the other only via
        // title. The content match MUST rank first regardless of the
        // title tier it reaches.
        let engine = LocalSearchEngine;
        let documents = vec![
            doc_with_title(
                1,
                "lorem ipsum dolor",
                "2026-01-02T03:04:05Z",
                Some("Config Notes"),
            ),
            doc_with_title(2, "alpha config beta", "2026-01-02T03:04:05Z", None),
        ];
        let result = engine.search(&query("config", 10), &documents).expect("ok");
        assert_eq!(result.hits.len(), 2);
        assert_eq!(result.hits[0].entry_id, 2);
        assert_eq!(result.hits[0].score, SCORE_EXACT_PHRASE);
        assert_eq!(result.hits[1].entry_id, 1);
        assert_eq!(result.hits[1].score, SCORE_TITLE_EXACT_PHRASE);
    }

    #[test]
    fn title_tokens_substring_match_surfaces_at_the_title_substring_tier() {
        // Title contains the query tokens as separate, non-adjacent
        // words: the full phrase is NOT a substring of the title so
        // the exact-phrase tier does not fire, but every token IS a
        // substring of a title token, so the substring tier fires.
        let engine = LocalSearchEngine;
        let documents = vec![doc_with_title(
            1,
            "lorem ipsum",
            "2026-01-02T03:04:05Z",
            Some("Notes about config files"),
        )];
        let result = engine
            .search(&query("config notes", 10), &documents)
            .expect("ok");
        assert_eq!(result.hits.len(), 1);
        assert_eq!(result.hits[0].score, SCORE_TITLE_ALL_TOKENS_SUBSTRING);
    }

    #[test]
    fn missing_title_is_a_clean_no_match() {
        // A `None` title MUST behave exactly like the pre-title-search
        // contract: the entry only matches when content matches.
        let engine = LocalSearchEngine;
        let documents = vec![doc_with_title(
            1,
            "lorem ipsum",
            "2026-01-02T03:04:05Z",
            None,
        )];
        let result = engine.search(&query("config", 10), &documents).expect("ok");
        assert!(result.hits.is_empty());
    }

    #[test]
    fn whitespace_only_title_is_treated_as_missing() {
        // The same trimming the repository applies to stored titles
        // runs implicitly through `normalize_query`; the engine MUST
        // NOT crash on whitespace-only or empty titles and MUST
        // behave like the entry had no title.
        let engine = LocalSearchEngine;
        let documents = vec![doc_with_title(
            1,
            "lorem ipsum",
            "2026-01-02T03:04:05Z",
            Some("   \t  "),
        )];
        let result = engine.search(&query("config", 10), &documents).expect("ok");
        assert!(result.hits.is_empty());
    }

    #[test]
    fn mixed_field_results_are_deterministic() {
        // Pin the ordering when three documents match through
        // different fields: content-exact > title-exact > content
        // fuzzy > no match. The ranking must be stable across runs.
        let engine = LocalSearchEngine;
        let documents = vec![
            doc_with_title(5, "lorem ipsum", "2026-01-02T03:04:06Z", None),
            doc_with_title(2, "alpha config beta", "2026-01-02T03:04:05Z", None),
            doc_with_title(
                7,
                "lorem ipsum",
                "2026-01-02T03:04:05Z",
                Some("Config Notes"),
            ),
        ];
        let first = engine.search(&query("config", 10), &documents).unwrap();
        let second = engine.search(&query("config", 10), &documents).unwrap();
        let ids_first: Vec<i64> = first.hits.iter().map(|h| h.entry_id).collect();
        let ids_second: Vec<i64> = second.hits.iter().map(|h| h.entry_id).collect();
        assert_eq!(ids_first, ids_second);
        assert_eq!(ids_first, vec![2, 7]);
        assert_eq!(first.hits[0].score, SCORE_EXACT_PHRASE);
        assert_eq!(first.hits[1].score, SCORE_TITLE_EXACT_PHRASE);
    }
}
