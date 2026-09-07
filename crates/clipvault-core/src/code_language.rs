//! Canonical programming-language allowlist for the
//! `code-language-detection` capability.
//!
//! The core layer stores only one identifier per entry; every value the
//! detector, the bridge or the persistence service emits MUST resolve
//! to one of the constants in [`CODE_LANGUAGES`]. Aliases the
//! frontend detector / the highlight.js grammar registry share are
//! normalised through [`normalise_code_language`]; unknown values
//! produce [`CodeLanguageError::Unknown`] so the persistence layer can
//! reject them with a typed error rather than silently mapping to a
//! neighbouring variant.

use std::fmt;

/// Canonical programming-language identifiers persisted by
/// `clipvault_core`. The list is the authoritative source of truth: the
/// frontend detector, the persistence service and the SQL schema index
/// derive their behaviour from this slice.
pub const CODE_LANGUAGES: &[&str] = &[
    "javascript",
    "typescript",
    "java",
    "c",
    "cpp",
    "csharp",
    "python",
    "rust",
    "go",
    "kotlin",
    "swift",
    "php",
    "ruby",
    "bash",
    "shell",
];

/// Human-readable label every UI surface renders next to the code icon.
/// The list MUST stay aligned with [`CODE_LANGUAGES`] — the
/// `canonical_label` helper indexes into it positionally.
pub const CODE_LANGUAGE_LABELS: &[&str] = &[
    "JavaScript",
    "TypeScript",
    "Java",
    "C",
    "C++",
    "C#",
    "Python",
    "Rust",
    "Go",
    "Kotlin",
    "Swift",
    "PHP",
    "Ruby",
    "Bash",
    "Shell",
];

/// Aliases the frontend detector and the highlight.js grammar registry
/// use. Each entry maps an alias to its canonical identifier; aliases
/// MUST NOT persist as-is.
const CODE_LANGUAGE_ALIASES: &[(&str, &str)] = &[
    ("js", "javascript"),
    ("jsx", "javascript"),
    ("ts", "typescript"),
    ("tsx", "typescript"),
    ("py", "python"),
    ("rs", "rust"),
    ("c++", "cpp"),
    ("cs", "csharp"),
    ("golang", "go"),
    ("kt", "kotlin"),
    ("kts", "kotlin"),
    ("sh", "shell"),
    ("zsh", "shell"),
    ("rb", "ruby"),
    ("objc", "cpp"),
    ("objectivec", "cpp"),
];

/// Typed error returned by the canonical-language helpers. Persisting a
/// non-canonical value surfaces this error so callers can decide how to
/// react without panicking or silently mapping to a neighbouring
/// variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodeLanguageError {
    /// The supplied language is not part of the canonical allowlist.
    Unknown(String),
}

impl fmt::Display for CodeLanguageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CodeLanguageError::Unknown(value) => {
                write!(f, "unknown code language: {value}")
            }
        }
    }
}

impl std::error::Error for CodeLanguageError {}

/// Stable discriminator the Tauri command maps to a stable
/// `CommandError.kind`. Pinning the literal guarantees the frontend
/// can branch on it without parsing free-form messages.
impl CodeLanguageError {
    pub fn kind_str(&self) -> &'static str {
        match self {
            CodeLanguageError::Unknown(_) => "unknown_code_language",
        }
    }
}

/// True when `value` is one of the canonical identifiers the
/// allowlist accepts. Empty input is intentionally rejected — the
/// detector returns `null` for an empty payload, never an empty
/// string.
pub fn is_canonical_code_language(value: &str) -> bool {
    CODE_LANGUAGES.contains(&value)
}

/// Map a raw language identifier (alias or canonical) to its
/// canonical form. Returns [`CodeLanguageError::Unknown`] for values
/// outside the allowlist.
pub fn normalise_code_language(raw: &str) -> Result<&'static str, CodeLanguageError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(CodeLanguageError::Unknown(trimmed.to_string()));
    }
    let lower = trimmed.to_ascii_lowercase();
    if let Some((_, canonical)) = CODE_LANGUAGE_ALIASES
        .iter()
        .find(|(alias, _)| *alias == lower.as_str())
    {
        return Ok(*canonical);
    }
    if let Some(canonical) = CODE_LANGUAGES.iter().find(|known| **known == lower) {
        return Ok(*canonical);
    }
    Err(CodeLanguageError::Unknown(trimmed.to_string()))
}

/// Human-readable label for a canonical language. Unknown values fall
/// back to the trimmed raw string so the UI never renders an empty
/// badge.
pub fn canonical_label(language: &str) -> &str {
    if let Ok(canonical) = normalise_code_language(language) {
        if let Some((_, label)) = CODE_LANGUAGES
            .iter()
            .zip(CODE_LANGUAGE_LABELS.iter())
            .find(|(known, _)| **known == canonical)
        {
            return label;
        }
    }
    // Unknown values: keep the trimmed raw string visible. The frontend
    // is expected to have validated the language before calling this
    // helper, so reaching the fallback should be rare.
    let trimmed = language.trim();
    if trimmed.is_empty() {
        "Code"
    } else {
        trimmed
    }
}

/// Whether `language` is a canonical, normalised value the persistence
/// layer accepts without further transformation.
pub fn is_canonical_normalised(language: &str) -> bool {
    normalise_code_language(language)
        .map(|canonical| canonical == language)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_allowlist_lists_every_supported_language() {
        // The list is the documented surface every detector and the
        // SQL index rely on. Adding a new language MUST land here
        // and in the frontend allowlist at the same time.
        assert_eq!(
            CODE_LANGUAGES,
            &[
                "javascript",
                "typescript",
                "java",
                "c",
                "cpp",
                "csharp",
                "python",
                "rust",
                "go",
                "kotlin",
                "swift",
                "php",
                "ruby",
                "bash",
                "shell",
            ],
        );
    }

    #[test]
    fn canonical_label_list_matches_canonical_allowlist() {
        assert_eq!(CODE_LANGUAGE_LABELS.len(), CODE_LANGUAGES.len());
    }

    #[test]
    fn normalise_returns_canonical_for_every_alias() {
        let cases = [
            ("js", "javascript"),
            ("JS", "javascript"),
            ("jsx", "javascript"),
            ("ts", "typescript"),
            ("TS", "typescript"),
            ("tsx", "typescript"),
            ("py", "python"),
            ("PY", "python"),
            ("rs", "rust"),
            ("RS", "rust"),
            ("c++", "cpp"),
            ("C++", "cpp"),
            ("cs", "csharp"),
            ("CS", "csharp"),
            ("golang", "go"),
            ("GOLANG", "go"),
            ("kt", "kotlin"),
            ("kts", "kotlin"),
            ("rb", "ruby"),
            ("sh", "shell"),
            ("zsh", "shell"),
        ];
        for (input, expected) in cases {
            assert_eq!(
                normalise_code_language(input).expect("alias"),
                expected,
                "input = {input}"
            );
        }
    }

    #[test]
    fn normalise_passes_through_canonical_values() {
        for known in CODE_LANGUAGES {
            assert_eq!(
                normalise_code_language(known).expect("canonical"),
                *known,
                "input = {known}"
            );
        }
    }

    #[test]
    fn normalise_rejects_unknown_values() {
        let cases = ["", "   ", "perl", "ruby3", "kotlinx", "cxx"];
        for raw in cases {
            assert!(
                normalise_code_language(raw).is_err(),
                "raw = {raw:?} should reject"
            );
        }
    }

    #[test]
    fn normalise_rejects_unknown_with_payload_in_error() {
        let err = normalise_code_language("perl").unwrap_err();
        let kind = err.kind_str();
        match &err {
            CodeLanguageError::Unknown(value) => assert_eq!(value, "perl"),
        }
        assert_eq!(kind, "unknown_code_language");
    }

    #[test]
    fn is_canonical_matches_allowlist_only() {
        assert!(is_canonical_code_language("python"));
        assert!(is_canonical_code_language("javascript"));
        assert!(!is_canonical_code_language("py"));
        assert!(!is_canonical_code_language("Perl"));
        assert!(!is_canonical_code_language(""));
    }

    #[test]
    fn is_canonical_normalised_requires_exact_match() {
        assert!(is_canonical_normalised("python"));
        assert!(is_canonical_normalised("rust"));
        assert!(!is_canonical_normalised("py"));
        assert!(!is_canonical_normalised("rust "));
    }

    #[test]
    fn canonical_label_returns_human_label_for_canonical_input() {
        assert_eq!(canonical_label("python"), "Python");
        assert_eq!(canonical_label("javascript"), "JavaScript");
        assert_eq!(canonical_label("cpp"), "C++");
        assert_eq!(canonical_label("csharp"), "C#");
    }

    #[test]
    fn canonical_label_normalises_aliases() {
        assert_eq!(canonical_label("py"), "Python");
        assert_eq!(canonical_label("c++"), "C++");
    }

    #[test]
    fn canonical_label_falls_back_to_trimmed_input() {
        // Unknown values must still surface something visible so the
        // UI never renders an empty badge.
        assert_eq!(canonical_label("perl"), "perl");
        assert_eq!(canonical_label(" Perl "), "Perl");
    }

    #[test]
    fn canonical_label_falls_back_when_input_is_empty() {
        assert_eq!(canonical_label(""), "Code");
        assert_eq!(canonical_label("   "), "Code");
    }
}
