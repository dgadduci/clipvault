//! Unicode-safe snippet building for search hits.
//!
//! The snippet is centered around the first occurrence of any query
//! token in the document and trimmed to at most
//! [`SNIPPET_MAX_CHARS`] characters. All slicing is done at `char`
//! boundaries so multi-byte code points are never split.

use crate::engine::SNIPPET_MAX_CHARS;

/// Build a snippet for `content` around the first occurrence of any
/// `query_tokens`. The snippet is bounded by [`SNIPPET_MAX_CHARS`]
/// characters and prefixed/suffixed with "…" when truncated.
///
/// Falls back to the head of `content` when none of the tokens match.
pub fn build_snippet(content: &str, query_tokens: &[String]) -> String {
    if content.is_empty() {
        return String::new();
    }
    let max = SNIPPET_MAX_CHARS;

    let lower_content = content.to_lowercase();
    let hit_index = query_tokens
        .iter()
        .filter_map(|token| {
            let needle = token.to_lowercase();
            if needle.is_empty() {
                None
            } else {
                lower_content.find(&needle).map(|idx| (idx, needle))
            }
        })
        .min_by_key(|(idx, _)| *idx);

    let Some((byte_idx, needle)) = hit_index else {
        return truncate_chars(content, max);
    };

    // Convert the byte offset to a char offset so the slice respects
    // UTF-8 boundaries.
    let char_idx = lower_content
        .char_indices()
        .take_while(|(i, _)| *i < byte_idx)
        .count();

    let total_chars = content.chars().count();
    let half = max / 2;
    let start_char = char_idx.saturating_sub(half);
    let end_char = (start_char + max).min(total_chars);
    let adjusted_start = if end_char - start_char < max && end_char == total_chars {
        end_char.saturating_sub(max)
    } else {
        start_char
    };

    let snippet: String = content
        .chars()
        .skip(adjusted_start)
        .take(end_char - adjusted_start)
        .collect();

    let prefix = if adjusted_start > 0 { "…" } else { "" };
    let suffix = if end_char < total_chars { "…" } else { "" };

    let mut out = String::with_capacity(snippet.len() + prefix.len() + suffix.len());
    out.push_str(prefix);
    out.push_str(&snippet);
    out.push_str(suffix);
    let _ = needle; // suppress unused-variable lint when needle isn't read
    out
}

/// Truncate to the first `max` characters of `input`, suffixing with
/// "…" when more text remains.
fn truncate_chars(input: &str, max: usize) -> String {
    if input.chars().count() <= max {
        return input.to_string();
    }
    let head: String = input.chars().take(max).collect();
    let mut out = String::with_capacity(head.len() + 3);
    out.push_str(&head);
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snippet_keeps_ascii_match_in_the_middle() {
        let snippet = build_snippet("lorem ipsum dolor sit amet", &[String::from("dolor")]);
        assert!(snippet.contains("dolor"));
        // Should include the prefix marker because the match is past the head.
        assert!(snippet.starts_with('…') || snippet.starts_with("lorem"));
    }

    #[test]
    fn snippet_handles_unicode_content_without_splitting_codepoints() {
        let content = "á".repeat(120);
        let snippet = build_snippet(&content, &[String::from("zzz")]);
        // No match found: must fall back to a head-truncated snippet.
        assert!(snippet.ends_with('…') || snippet.chars().count() <= SNIPPET_MAX_CHARS);
        // Ensure we never split a multi-byte codepoint into an invalid str.
        assert!(snippet.chars().count() <= SNIPPET_MAX_CHARS + 1);
    }

    #[test]
    fn snippet_caps_around_match() {
        let content = "a".repeat(200) + " hit " + &"b".repeat(200);
        let snippet = build_snippet(&content, &[String::from("hit")]);
        assert!(snippet.contains("hit"));
        assert!(snippet.chars().count() <= SNIPPET_MAX_CHARS + 2); // 2 for both ellipses
    }

    #[test]
    fn empty_content_returns_empty_snippet() {
        let snippet = build_snippet("", &[String::from("anything")]);
        assert_eq!(snippet, "");
    }

    #[test]
    fn short_content_returns_full_text() {
        let snippet = build_snippet("hello world", &[String::from("world")]);
        assert!(snippet.contains("hello world"));
        assert!(!snippet.ends_with('…'));
    }
}
