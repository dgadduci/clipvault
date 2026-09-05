//! Source-application query and option aggregation for the
//! `source-app-filter` capability.
//!
//! The frontend combobox renders the metadata the
//! [`SourceApplicationsQuery`] produces. The query is the single
//! entry point the Tauri shell uses to populate the combobox for a
//! given collection scope; it is metadata-only by construction
//! and never carries clipboard content, hashes, asset references
//! or paths.
//!
//! The wire-level [`SourceAppFilter`](clipvault_db::SourceAppFilter)
//! enum lives in [`clipvault_db`] because the SQL predicate
//! builder the recents and search queries consume must stay in
//! the database layer to avoid a circular dependency. This module
//! re-exports the enum from the core so callers can use one
//! canonical name.

use clipvault_db::{EntryRepository, EntryRepositoryError};
use serde::Serialize;
use thiserror::Error;

use crate::bootstrap::AppContext;

/// Single metadata-only entry the combobox renders. `display_name`
/// is the user-visible label the frontend surfaces; `source_app`
/// is the stable internal identifier used by every equality
/// predicate. `icon_ref` is the relative reference inside the local
/// `application-icons/` namespace resolved by the existing icon
/// bridge; `fallback` indicates the metadata provider could not
/// resolve a name/icon for this identifier (or that the row is
/// the synthetic unknown branch) so the combobox should render the
/// generic glyph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SourceApplicationOption {
    /// Stable wire identifier; the frontend MUST treat it as
    /// opaque and never render it as visible text.
    pub source_app: Option<String>,
    /// User-visible display name the combobox renders.
    pub display_name: String,
    /// Optional relative reference under `application-icons/`.
    pub icon_ref: Option<String>,
    /// Whether the combobox must render the generic glyph because
    /// no persisted icon was found.
    pub fallback: bool,
}

/// Result of [`SourceApplicationsQuery::load`]. `options` always
/// contains `Todas` first (synthesised by the query so the
/// frontend cannot drift from the backend); the remaining entries
/// are the distinct known applications represented in the active
/// scope, followed by `Aplicación desconocida` when at least one
/// eligible row has no `source_app`. Order is deterministic:
///
/// 1. `Todas`
/// 2. `Aplicación desconocida` (only if at least one row lacks a
///    `source_app`)
/// 3. known applications, ordered case-insensitive by
///    `display_name`, with the stable `source_app` as the
///    deterministic tiebreaker
///
/// `scope` mirrors the parameters the query consumed so the
/// frontend can reconcile the options it just received with the
/// state the user actually sees.
#[derive(Debug, Clone, Serialize)]
pub struct SourceApplicationsSnapshot {
    pub options: Vec<SourceApplicationOption>,
    pub scope: SourceApplicationsScope,
}

/// Metadata describing the scope a [`SourceApplicationsSnapshot`]
/// was computed against. Returned alongside `options` so the
/// frontend can detect a stale response (a collection switch
/// that landed while the query was in flight, for example) and
/// drop it without polluting the combobox.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SourceApplicationsScope {
    pub collection_id: Option<i64>,
    pub tag_ids: Vec<i64>,
}

impl SourceApplicationsScope {
    pub fn new(collection_id: Option<i64>, tag_ids: &[i64]) -> Self {
        Self {
            collection_id,
            tag_ids: tag_ids.to_vec(),
        }
    }
}

/// Typed error returned by [`SourceApplicationsQuery::load`]. The
/// repository error is the only realistic failure today; future
/// backends (FTS5, …) can extend the enum without changing call
/// sites.
#[derive(Debug, Error)]
pub enum SourceApplicationsError {
    #[error("entry repository error: {0}")]
    Repository(#[from] EntryRepositoryError),
}

/// Query the distinct source applications represented in the
/// active scope. The struct is stateless: it only borrows the
/// [`AppContext`] for the lifetime of the call. The query is
/// deliberately not restricted by the rail limit so the combobox
/// can never silently drop an option the user expects to see.
#[derive(Debug, Default, Clone)]
pub struct SourceApplicationsQuery;

impl SourceApplicationsQuery {
    pub fn new() -> Self {
        Self
    }

    /// Load the combobox options for `scope`. The `Todas` option
    /// is always prepended so the frontend never has to special-case
    /// the "no restriction" branch. The `Aplicación desconocida`
    /// option is appended only when the scope actually contains
    /// rows with an empty `source_app`.
    pub fn load(
        &self,
        context: &AppContext,
        scope: &SourceApplicationsScope,
    ) -> Result<SourceApplicationsSnapshot, SourceApplicationsError> {
        let mut options = Vec::new();
        options.push(SourceApplicationOption {
            source_app: None,
            display_name: "Todas".to_string(),
            icon_ref: None,
            fallback: true,
        });

        let aggregated = {
            let mut db = context.database().lock();
            let repo = EntryRepository::new(db.connection_mut());
            repo.aggregated_source_apps(scope.collection_id, &scope.tag_ids)?
        };

        if aggregated.has_unknown {
            options.push(SourceApplicationOption {
                source_app: None,
                display_name: "Aplicación desconocida".to_string(),
                icon_ref: None,
                fallback: true,
            });
        }

        let mut known = aggregated.known;
        // Deterministic order: case-insensitive display_name, then
        // the stable identifier as the internal tiebreaker. We
        // intentionally use `to_lowercase` on UTF-8 strings rather
        // than a locale-aware collation so the order is reproducible
        // across machines and tests.
        known.sort_by(|a, b| {
            let by_name = a
                .display_name
                .to_lowercase()
                .cmp(&b.display_name.to_lowercase());
            if by_name.is_eq() {
                a.source_app.cmp(&b.source_app)
            } else {
                by_name
            }
        });

        for entry in known {
            let fallback = entry.icon_ref.is_none();
            options.push(SourceApplicationOption {
                source_app: Some(entry.source_app),
                display_name: entry.display_name,
                icon_ref: entry.icon_ref,
                fallback,
            });
        }

        Ok(SourceApplicationsSnapshot {
            options,
            scope: scope.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_application_option_serializes_metadata_only() {
        let value = SourceApplicationOption {
            source_app: Some("com.example.Editor".to_string()),
            display_name: "Editor".to_string(),
            icon_ref: Some("application-icons/com_example_Editor.png".to_string()),
            fallback: false,
        };
        let json = serde_json::to_value(&value).expect("serialize");
        assert_eq!(json["source_app"], "com.example.Editor");
        assert_eq!(json["display_name"], "Editor");
        assert_eq!(json["icon_ref"], "application-icons/com_example_Editor.png");
        assert_eq!(json["fallback"], false);
    }

    #[test]
    fn scope_equality_is_field_wise() {
        let a = SourceApplicationsScope::new(Some(2), &[1, 2]);
        let b = SourceApplicationsScope::new(Some(2), &[1, 2]);
        assert_eq!(a, b);
        let c = SourceApplicationsScope::new(Some(3), &[1, 2]);
        assert_ne!(a, c);
    }
}
