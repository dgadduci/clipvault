//! Source-application filter model shared by the core and the
//! database layer.
//!
//! The `SourceAppFilter` enum lives in `clipvault-db` so the SQL
//! predicates the recents/search queries emit can be assembled
//! without depending on `clipvault-core`. The enum carries no
//! behaviour beyond the wire shape: the `clipvault-core` crate
//! owns the snapshot query and the option aggregation.
//!
//! The variants intentionally distinguish `All` from `Unknown`
//! because the absence of a source identifier is a meaningful
//! state the user can target. Collapsing `Unknown` into `All`
//! would hide the "Aplicación desconocida" affordance the design
//! document pins for the desktop.

use serde::{Deserialize, Serialize};

/// How the source-app facet filters entries. The enum is the
/// single source of truth shared by the recents/search filter and
/// the dedicated `clipvault_source_applications` query. It is
/// serializable so the Tauri shell can receive the same shape the
/// core uses without an extra translation layer.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SourceAppFilter {
    /// No restriction: every eligible row, regardless of source.
    #[default]
    All,
    /// Restrict to a single stable `source_app` identifier.
    Known { source_app: String },
    /// Restrict to rows whose `source_app` is `NULL` or empty.
    Unknown,
}

/// SQL predicate builder used by the recents/search queries and
/// the aggregated source-applications query. Centralising the
/// `SourceAppFilter` -> SQL translation here keeps the rest of
/// the codebase free of string-concatenated fragments and lets a
/// contributor reason about the filter in a single place.
///
/// The function returns `None` for [`SourceAppFilter::All`]: the
/// caller skips the predicate and falls back to the unfiltered
/// branch. For the remaining variants it returns the SQL fragment
/// plus the parameters the binding must supply in the same
/// order. The fragment never embeds user input — only the
/// placeholder `?` tokens the prepared statement expects.
pub(crate) fn source_app_predicate(filter: &SourceAppFilter) -> Option<SourceAppPredicate> {
    match filter {
        SourceAppFilter::All => None,
        SourceAppFilter::Known { source_app } => {
            if source_app.is_empty() {
                // An empty identifier cannot match a non-empty
                // stored value; collapse to "matches NULL or empty"
                // so the UI cannot enter a degenerate "known but
                // blank" state that silently behaves like Unknown.
                Some(SourceAppPredicate {
                    sql: "source_app IS NULL OR source_app = ''",
                    params: vec![],
                })
            } else {
                Some(SourceAppPredicate {
                    sql: "source_app = ?",
                    params: vec![SourceAppParam::Text(source_app.clone())],
                })
            }
        }
        SourceAppFilter::Unknown => Some(SourceAppPredicate {
            sql: "source_app IS NULL OR source_app = ''",
            params: vec![],
        }),
    }
}

#[derive(Debug, Clone)]
pub(crate) struct SourceAppPredicate {
    pub sql: &'static str,
    pub params: Vec<SourceAppParam>,
}

#[derive(Debug, Clone)]
pub(crate) enum SourceAppParam {
    Text(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_app_filter_default_is_all() {
        assert_eq!(SourceAppFilter::default(), SourceAppFilter::All);
    }

    #[test]
    fn source_app_predicate_all_is_none() {
        assert!(source_app_predicate(&SourceAppFilter::All).is_none());
    }

    #[test]
    fn source_app_predicate_known_emits_equality() {
        let predicate = source_app_predicate(&SourceAppFilter::Known {
            source_app: "com.example.Editor".into(),
        })
        .expect("predicate");
        assert_eq!(predicate.sql, "source_app = ?");
        assert_eq!(predicate.params.len(), 1);
    }

    #[test]
    fn source_app_predicate_unknown_emits_null_or_empty() {
        let predicate = source_app_predicate(&SourceAppFilter::Unknown).expect("predicate");
        assert_eq!(predicate.sql, "source_app IS NULL OR source_app = ''");
        assert!(predicate.params.is_empty());
    }

    #[test]
    fn source_app_predicate_known_with_empty_identifier_collapses_to_unknown_sql() {
        let predicate = source_app_predicate(&SourceAppFilter::Known {
            source_app: "".into(),
        })
        .expect("predicate");
        assert_eq!(predicate.sql, "source_app IS NULL OR source_app = ''");
        assert!(predicate.params.is_empty());
    }
}
