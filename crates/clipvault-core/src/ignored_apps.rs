//! Domain types and errors for the application-picker flow.
//!
//! The picker flow has three layers:
//!
//! 1. The [`clipvault_platform::ApplicationPicker`] trait surfaces a
//!    [`clipvault_platform::SelectedApplication`] with the bundle
//!    identifier, display name and an optional icon representation
//!    extracted from a real macOS bundle.
//! 2. The [`IgnoredAppsService`] in this module normalises the
//!    identifier, persists the row idempotently and keeps the
//!    [`PrivacyGate`](crate::privacy::PrivacyGate) snapshot in sync.
//! 3. The shell renders [`IgnoredAppEntry`] so the frontend only ever
//!    sees presentation metadata — never clipboard content, hashes or
//!    snippets.
//!
//! ## Normalization & idempotency
//!
//! Identifiers are normalised through [`normalize_identifier`]
//! (trim + ASCII lowercase) so the matcher compares two snapshots
//! without false negatives. The repository upserts by normalised
//! identifier: a second selection of the same application updates
//! the existing row's display name and icon reference instead of
//! creating a duplicate, and it never mutates a different row's
//! identifier.
//!
//! ## Compatibility with legacy rows
//!
//! Pre-picker rows only carry `id` and `created_at`. The repository
//! reads those rows as [`IgnoredAppEntry`] with `display_name` and
//! `icon_ref` set to `None`. The frontend renders them with a safe
//! fallback (the identifier, where allowed) so the user never sees
//! an empty row after migrating to a build that ships this change.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::settings::MAX_IDENTIFIER_LENGTH;

/// Stable metadata-only record the frontend renders for every
/// blacklisted application.
///
/// The shape is intentionally limited to the fields the UI can show:
/// `id`, `display_name` and `icon_ref`. Clipboard content, content
/// hashes, snippets and capture diagnostics MUST never leak into this
/// DTO.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct IgnoredAppEntry {
    /// Normalised application identifier used by the matcher.
    pub id: String,
    /// User-visible application name. `None` for legacy rows that
    /// predate the picker feature — the frontend must render a
    /// fallback in that case.
    pub display_name: Option<String>,
    /// Opaque, locally-controlled reference for the application icon
    /// (typically a PNG stored under `~/.clipvault/assets/`). `None`
    /// when the icon could not be extracted or the row is a legacy
    /// entry without metadata.
    pub icon_ref: Option<String>,
    /// Insertion timestamp (RFC 3339, UTC). Surfaced for transparency
    /// only — it carries no sensitive content.
    pub created_at: String,
}

impl IgnoredAppEntry {
    /// Convenience constructor for tests and the picker service. The
    /// `created_at` value is the caller-supplied timestamp.
    pub fn new(
        id: impl Into<String>,
        display_name: Option<String>,
        icon_ref: Option<String>,
        created_at: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            display_name,
            icon_ref,
            created_at: created_at.into(),
        }
    }
}

/// Outcome returned by [`IgnoredAppsService::pick_and_add`].
///
/// `Cancelled` is **not** an error: the picker was closed without a
/// selection and the blacklist stays untouched. `Failed` carries the
/// typed [`IgnoredAppError`] so the shell can render the matching
/// localised copy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PickAndAddOutcome {
    /// The application was added (new row inserted).
    Added(IgnoredAppEntry),
    /// The application was already present; metadata was refreshed
    /// without creating a duplicate.
    Updated(IgnoredAppEntry),
    /// The picker was cancelled. Blacklist is unchanged.
    Cancelled,
}

/// Typed errors for the application-picker flow.
///
/// Each variant carries a stable string returned to the frontend
/// through the Tauri command surface; the shell maps them to the
/// localised copy without inspecting the raw message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IgnoredAppError {
    /// The user cancelled the picker. Not surfaced to the UI as a
    /// mutation error; the blacklist is unchanged.
    Cancelled,
    /// The selection was not a usable `.app` bundle or its metadata
    /// could not be parsed (missing CFBundleIdentifier, malformed
    /// Info.plist, ...).
    InvalidSelection { reason: String },
    /// The selected bundle did not advertise an identifier. A
    /// metadata-less bundle is rejected: PrivacyGate matches by id,
    /// so accepting one would let the rule silently never fire.
    MissingIdentifier,
    /// The picker backend cannot run on the current host (no
    /// `NSOpenPanel` access, missing native bridge, ...).
    BackendUnavailable { reason: String },
    /// The current session cannot map the selection to a stable
    /// identifier used by the active-app adapter. Returned by the
    /// Linux stub until a safe `.desktop ↔ WM_CLASS/app_id` mapping
    /// is wired in.
    UnsupportedSession { reason: String },
    /// Persistence failure (SQLite error, repository failure, ...).
    Persistence { reason: String },
}

impl IgnoredAppError {
    /// Stable snake_case identifier the frontend consumes. The shell
    /// never inspects the free-form `reason` to make routing
    /// decisions; it only renders it after picking the matching copy.
    pub fn kind_str(&self) -> &'static str {
        match self {
            IgnoredAppError::Cancelled => "cancelled",
            IgnoredAppError::InvalidSelection { .. } => "invalid_selection",
            IgnoredAppError::MissingIdentifier => "missing_identifier",
            IgnoredAppError::BackendUnavailable { .. } => "backend_unavailable",
            IgnoredAppError::UnsupportedSession { .. } => "unsupported_session",
            IgnoredAppError::Persistence { .. } => "persistence_error",
        }
    }
}

impl fmt::Display for IgnoredAppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IgnoredAppError::Cancelled => f.write_str("selection cancelled"),
            IgnoredAppError::InvalidSelection { reason } => {
                write!(f, "invalid selection: {reason}")
            }
            IgnoredAppError::MissingIdentifier => {
                f.write_str("selected bundle does not advertise a usable identifier")
            }
            IgnoredAppError::BackendUnavailable { reason } => {
                write!(f, "picker backend unavailable: {reason}")
            }
            IgnoredAppError::UnsupportedSession { reason } => {
                write!(f, "picker session is not supported here: {reason}")
            }
            IgnoredAppError::Persistence { reason } => {
                write!(f, "failed to persist ignored application: {reason}")
            }
        }
    }
}

impl std::error::Error for IgnoredAppError {}

/// Normalise an application identifier for storage and matching.
///
/// Rules (mirrored from [`crate::privacy::normalize`]):
/// - trim surrounding whitespace,
/// - lowercase ASCII letters,
/// - drop identifiers that end up empty or exceed
///   [`MAX_IDENTIFIER_LENGTH`].
///
/// The same rule is applied to the identifier extracted from a
/// macOS bundle and to any identifier that already lives in the
/// database, so legacy rows are rewritten consistently on the next
/// upsert and the matcher never sees two casings of the same id.
pub fn normalize_identifier(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let lowered = trimmed.to_ascii_lowercase();
    if lowered.len() > MAX_IDENTIFIER_LENGTH {
        return String::new();
    }
    lowered
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignored_app_entry_round_trips_through_serde() {
        let entry = IgnoredAppEntry::new(
            "com.apple.terminal",
            Some("Terminal".to_string()),
            Some("icons/com.apple.terminal.png".to_string()),
            "2026-01-02T03:04:05Z",
        );
        let json = serde_json::to_string(&entry).expect("serialise");
        assert!(json.contains("\"id\":\"com.apple.terminal\""));
        assert!(json.contains("\"display_name\":\"Terminal\""));
        assert!(json.contains("\"icon_ref\":\"icons/com.apple.terminal.png\""));
        assert!(json.contains("\"created_at\":\"2026-01-02T03:04:05Z\""));
        let parsed: IgnoredAppEntry = serde_json::from_str(&json).expect("parse");
        assert_eq!(parsed, entry);
    }

    #[test]
    fn ignored_app_entry_serialises_legacy_rows_without_metadata() {
        // Rows inserted before the picker change only carry `id` and
        // `created_at`. The frontend reads `display_name` and
        // `icon_ref` as `None` and renders a safe fallback.
        let entry = IgnoredAppEntry::new("firefox", None, None, "2026-01-02T03:04:05Z");
        let json = serde_json::to_string(&entry).expect("serialise");
        assert!(json.contains("\"display_name\":null"));
        assert!(json.contains("\"icon_ref\":null"));
    }

    #[test]
    fn ignored_app_error_kind_str_is_stable() {
        // The frontend consumes the `kind_str` to pick the matching
        // localised copy. Renaming a variant is a breaking change for
        // the settings panel — pin the strings here.
        assert_eq!(IgnoredAppError::Cancelled.kind_str(), "cancelled");
        assert_eq!(
            IgnoredAppError::InvalidSelection { reason: "x".into() }.kind_str(),
            "invalid_selection"
        );
        assert_eq!(
            IgnoredAppError::MissingIdentifier.kind_str(),
            "missing_identifier"
        );
        assert_eq!(
            IgnoredAppError::BackendUnavailable { reason: "x".into() }.kind_str(),
            "backend_unavailable"
        );
        assert_eq!(
            IgnoredAppError::UnsupportedSession { reason: "x".into() }.kind_str(),
            "unsupported_session"
        );
        assert_eq!(
            IgnoredAppError::Persistence { reason: "x".into() }.kind_str(),
            "persistence_error"
        );
    }

    #[test]
    fn ignored_app_error_display_describes_each_variant() {
        // The `Display` impl is what the shell logs. It MUST NOT
        // include clipboard content, hashes or snippets — only the
        // stable variant label and the sanitised reason.
        assert_eq!(
            IgnoredAppError::Cancelled.to_string(),
            "selection cancelled"
        );
        assert_eq!(
            IgnoredAppError::InvalidSelection {
                reason: "not a bundle".into()
            }
            .to_string(),
            "invalid selection: not a bundle"
        );
        assert_eq!(
            IgnoredAppError::MissingIdentifier.to_string(),
            "selected bundle does not advertise a usable identifier"
        );
        assert_eq!(
            IgnoredAppError::BackendUnavailable { reason: "x".into() }.to_string(),
            "picker backend unavailable: x"
        );
        assert_eq!(
            IgnoredAppError::UnsupportedSession {
                reason: "wayland".into()
            }
            .to_string(),
            "picker session is not supported here: wayland"
        );
        assert_eq!(
            IgnoredAppError::Persistence {
                reason: "db".into()
            }
            .to_string(),
            "failed to persist ignored application: db"
        );
    }

    #[test]
    fn normalize_identifier_trims_and_lowercases() {
        assert_eq!(
            normalize_identifier("  Com.Apple.Terminal  "),
            "com.apple.terminal"
        );
    }

    #[test]
    fn normalize_identifier_returns_empty_for_blank_input() {
        assert_eq!(normalize_identifier("   "), "");
        assert_eq!(normalize_identifier(""), "");
    }

    #[test]
    fn normalize_identifier_drops_oversized_input() {
        let huge = "a".repeat(MAX_IDENTIFIER_LENGTH + 1);
        assert_eq!(normalize_identifier(&huge), "");
    }

    #[test]
    fn normalize_identifier_is_idempotent() {
        let once = normalize_identifier("  Com.Apple.Terminal  ");
        let twice = normalize_identifier(&once);
        assert_eq!(once, twice);
    }
}
