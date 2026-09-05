//! Query and document normalization.
//!
//! Normalization is purely lexical:
//! - Unicode lowercase via [`str::to_lowercase`].
//! - Whitespace trimmed and collapsed using [`char::is_whitespace`].
//!
//! Both the query and the document content run through the same
//! normalization so the substring and token comparisons stay
//! case-insensitive and Unicode-aware.

/// Owned normalized representation. Returned by [`normalize_query`]
/// because the engine needs the normalized form to outlive the source
/// slice while scoring documents.
#[derive(Debug, Clone)]
pub struct NormalizedOwned {
    /// Original input, lowercased and with whitespace collapsed.
    pub phrase: String,
    /// Whitespace-split tokens of `phrase`. Always empty when the
    /// input was empty or only whitespace.
    pub tokens: Vec<String>,
}

/// Normalize a query or document string. Returns an owned struct so
/// the caller can hold the result beyond the lifetime of the input.
pub fn normalize_query(input: &str) -> NormalizedOwned {
    let phrase = collapse_whitespace(&input.to_lowercase());
    let tokens = phrase
        .split_whitespace()
        .filter(|token| !token.is_empty())
        .map(|token| token.to_string())
        .collect();
    NormalizedOwned { phrase, tokens }
}

/// Collapse runs of Unicode whitespace into a single ASCII space and
/// trim leading/trailing whitespace.
fn collapse_whitespace(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut pending_space = false;
    let mut started = false;
    for c in input.chars() {
        if c.is_whitespace() {
            if started {
                pending_space = true;
            }
        } else {
            if pending_space {
                out.push(' ');
                pending_space = false;
            }
            out.push(c);
            started = true;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_lowercase_and_collapses_whitespace() {
        let n = normalize_query("  Hola   MUNDO\tCRUEL ");
        assert_eq!(n.phrase, "hola mundo cruel");
        assert_eq!(n.tokens, vec!["hola", "mundo", "cruel"]);
    }

    #[test]
    fn empty_input_yields_empty_phrase_and_tokens() {
        let n = normalize_query("   \t  ");
        assert_eq!(n.phrase, "");
        assert!(n.tokens.is_empty());
    }

    #[test]
    fn unicode_lowercase_is_preserved() {
        let n = normalize_query("Árbol Ñandú");
        assert_eq!(n.phrase, "árbol ñandú");
        assert_eq!(n.tokens, vec!["árbol", "ñandú"]);
    }
}
