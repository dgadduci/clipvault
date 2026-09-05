//! Bounded fuzzy matching for individual tokens.
//!
//! Two tokens "match" when they are identical, when one is a substring
//! of the other, or when their Levenshtein distance is at most
//! [`FUZZY_MAX_DISTANCE`]. Short query tokens (length ≤ 3) require an
//! exact match to keep the false-positive rate low.

use crate::engine::{FUZZY_MAX_DISTANCE, SCORE_FUZZY};

/// Maximum length of a token that still demands an exact match (no
/// fuzzy). Below this threshold the Levenshtein ratio becomes too
/// generous on very short strings.
pub const SHORT_TOKEN_LEN: usize = 3;

/// Returns true when `query_token` matches `content_token` according
/// to the fuzzy rules used by the search engine.
///
/// Identical matches, substring matches and Levenshtein ≤
/// [`FUZZY_MAX_DISTANCE`] all qualify. Short query tokens require an
/// exact match.
pub fn fuzzy_token_match(query_token: &str, content_token: &str) -> bool {
    if query_token == content_token {
        return true;
    }
    if query_token.is_empty() || content_token.is_empty() {
        return false;
    }
    if query_token.len() <= SHORT_TOKEN_LEN || content_token.len() <= SHORT_TOKEN_LEN {
        // Either side is short: stay strict to avoid spurious matches
        // (e.g. "cat" -> "cut" or "cta" -> "category").
        return false;
    }
    if query_token.contains(content_token) || content_token.contains(query_token) {
        return true;
    }
    bounded_distance(query_token, content_token, FUZZY_MAX_DISTANCE)
}

/// Levenshtein distance computed with a row of `max_distance + 1`
/// cells. Returns `None` as soon as the distance is known to exceed
/// `max_distance`, allowing short-circuit exits on long strings.
fn bounded_distance(a: &str, b: &str, max_distance: usize) -> bool {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let n = a_chars.len();
    let m = b_chars.len();

    if n.abs_diff(m) > max_distance {
        return false;
    }

    let mut prev: Vec<usize> = (0..=m).collect();
    let mut curr = vec![0usize; m + 1];

    for i in 1..=n {
        curr[0] = i;
        let mut row_min = curr[0];
        for j in 1..=m {
            let cost = if a_chars[i - 1] == b_chars[j - 1] {
                0
            } else {
                1
            };
            let val = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
            curr[j] = val;
            if val < row_min {
                row_min = val;
            }
        }
        if row_min > max_distance {
            return false;
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    prev[m] <= max_distance
}

/// Reference Levenshtein implementation (unbounded) used only in tests
/// to cross-check the bounded version.
#[cfg(test)]
fn distance(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let n = a_chars.len();
    let m = b_chars.len();
    let mut prev: Vec<usize> = (0..=m).collect();
    let mut curr = vec![0usize; m + 1];
    for i in 1..=n {
        curr[0] = i;
        for j in 1..=m {
            let cost = if a_chars[i - 1] == b_chars[j - 1] {
                0
            } else {
                1
            };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[m]
}

// Touch the SCORE_FUZZY constant so the engine module exports remain
// in sync with the fuzzy module's dependency on it.
#[allow(dead_code)]
const _FUZZY_TIER_SCORE: i32 = SCORE_FUZZY;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_matches_always_count() {
        assert!(fuzzy_token_match("hello", "hello"));
    }

    #[test]
    fn substring_matches_count_for_long_tokens() {
        assert!(fuzzy_token_match("hello", "hellos"));
        assert!(fuzzy_token_match("worlds", "world"));
    }

    #[test]
    fn short_tokens_require_exact_match() {
        assert!(!fuzzy_token_match("cat", "cut"));
        assert!(!fuzzy_token_match("cta", "category"));
        assert!(!fuzzy_token_match("hi", "history"));
    }

    #[test]
    fn bounded_distance_matches_within_threshold() {
        // distance("kitten", "sitten") is 1 (substitute k -> s).
        assert!(fuzzy_token_match("kitten", "sitten"));
        // distance("hello", "helo") is 1 (delete one 'l').
        assert!(fuzzy_token_match("hello", "helo"));
        // distance("flaw", "lawn") is 2 (substitute f->l, substitute w->n).
        assert!(fuzzy_token_match("flaw", "lawn"));
        // distance("moon", "morning") is 4, above the threshold.
        assert!(!fuzzy_token_match("moon", "morning"));
        // "colour" -> "colorado": distance 4 (delete 'u', insert 'a','d','o').
        assert!(!fuzzy_token_match("colour", "colorado"));
    }

    #[test]
    fn bounded_distance_agrees_with_unbounded_for_small_inputs() {
        let pairs = [
            ("hello", "hello"),
            ("hello", "hellp"),
            ("hello", "yellow"),
            ("flaw", "lawn"),
            ("abcdef", "azcfgh"),
        ];
        for (a, b) in pairs {
            let d = distance(a, b);
            assert!(
                bounded_distance(a, b, d),
                "distance({a},{b})={d} should be reachable"
            );
            if d > 0 {
                assert!(
                    !bounded_distance(a, b, d - 1),
                    "distance({a},{b})={d} should not be reachable with d-1"
                );
            }
        }
    }
}
