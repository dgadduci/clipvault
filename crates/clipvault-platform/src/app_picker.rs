//! Platform abstraction for selecting an installed application and
//! extracting its presentation metadata.
//!
//! The picker is intentionally narrow: open a native dialog, return a
//! [`SelectedApplication`] with the bundle identifier, display name
//! and an optional icon reference, or surface a typed
//! [`ApplicationPickerError`]. No platform or filesystem logic leaks
//! into the core: the rest of the application only sees the result
//! type.
//!
//! The macOS implementation lives in `runtime::macos_app_picker` and
//! is feature-gated by `macos-native`. The Linux stub returns
//! [`ApplicationPickerError::UnsupportedSession`] until a safe
//! `.desktop ↔ WM_CLASS/app_id` mapping exists; the previous
//! prototype fell into the same trap the spec calls out (Wayland
//! sessions receiving a fabricated identifier), so the stub is the
//! canonical "we know this is unsupported" surface.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Lightweight, privacy-preserving description of the application
/// the user picked through the platform picker.
///
/// The path the picker resolved is intentionally **not** stored; it
/// stays inside the adapter for the lifetime of the call so the core
/// never persists a free-form filesystem reference. The icon, when
/// available, is referenced through `icon_ref` — an opaque,
/// platform-controlled identifier the renderer can resolve to a PNG
/// without trusting an arbitrary user-supplied path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectedApplication {
    /// Normalised bundle identifier used by PrivacyGate. Empty
    /// strings MUST be rejected by the picker adapter before this
    /// type is constructed.
    pub identifier: String,
    /// User-visible application name (e.g. "Terminal").
    pub display_name: String,
    /// Opaque, locally-controlled reference for the application icon.
    /// `None` when extraction failed or the platform does not expose
    /// an icon at all.
    pub icon_ref: Option<String>,
}

/// Typed errors returned by [`ApplicationPicker::pick`].
///
/// Each variant carries a stable `kind_str()` the shell uses to
/// route the matching localised copy. The free-form `reason` is
/// sanitised by the adapter and never contains clipboard content,
/// hashes or snippets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ApplicationPickerError {
    /// The user dismissed the picker without selecting anything.
    /// The blacklist MUST stay unchanged when this error is
    /// surfaced.
    Cancelled,
    /// The selection was not a usable `.app` bundle or its metadata
    /// could not be parsed.
    InvalidSelection { reason: String },
    /// The selected bundle did not advertise a usable identifier.
    MissingIdentifier,
    /// The picker backend cannot run on the current host (no
    /// `NSOpenPanel` access, missing native bridge, ...).
    BackendUnavailable { reason: String },
    /// The current session cannot map the selection to a stable
    /// identifier used by the active-app adapter. Returned on Linux
    /// until a safe `.desktop ↔ WM_CLASS/app_id` mapping is wired in.
    UnsupportedSession { reason: String },
}

impl ApplicationPickerError {
    /// Stable snake_case identifier the shell uses to route the
    /// matching localised copy. Frontend-facing — renaming a variant
    /// is a breaking change for the settings panel.
    pub fn kind_str(&self) -> &'static str {
        match self {
            ApplicationPickerError::Cancelled => "cancelled",
            ApplicationPickerError::InvalidSelection { .. } => "invalid_selection",
            ApplicationPickerError::MissingIdentifier => "missing_identifier",
            ApplicationPickerError::BackendUnavailable { .. } => "backend_unavailable",
            ApplicationPickerError::UnsupportedSession { .. } => "unsupported_session",
        }
    }
}

impl fmt::Display for ApplicationPickerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ApplicationPickerError::Cancelled => f.write_str("selection cancelled"),
            ApplicationPickerError::InvalidSelection { reason } => {
                write!(f, "invalid selection: {reason}")
            }
            ApplicationPickerError::MissingIdentifier => {
                f.write_str("selected bundle does not advertise a usable identifier")
            }
            ApplicationPickerError::BackendUnavailable { reason } => {
                write!(f, "picker backend unavailable: {reason}")
            }
            ApplicationPickerError::UnsupportedSession { reason } => {
                write!(f, "picker session is not supported here: {reason}")
            }
        }
    }
}

impl std::error::Error for ApplicationPickerError {}

/// Platform abstraction for the application picker.
///
/// Implementations MUST:
/// - return [`ApplicationPickerError::Cancelled`] (not a success
///   variant) when the user closes the dialog without selecting;
/// - never execute, install, open or modify the selected bundle;
/// - never invoke shell, `osascript`, `open`, `xdg-open` or any
///   external command;
/// - never log clipboard content, hashes or snippets;
/// - return a non-empty `identifier` on success or one of the typed
///   errors above.
pub trait ApplicationPicker: Send + Sync {
    /// Open the native picker and return the selected application
    /// metadata. Implementations block until the user either
    /// confirms a selection or dismisses the dialog.
    fn pick(&self) -> Result<SelectedApplication, ApplicationPickerError>;

    /// Stable identifier for diagnostics.
    fn name(&self) -> &'static str;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_application_serialises_with_metadata() {
        let app = SelectedApplication {
            identifier: "com.apple.terminal".to_string(),
            display_name: "Terminal".to_string(),
            icon_ref: Some("icons/com.apple.terminal.png".to_string()),
        };
        let json = serde_json::to_string(&app).expect("serialise");
        assert!(json.contains("\"identifier\":\"com.apple.terminal\""));
        assert!(json.contains("\"display_name\":\"Terminal\""));
        assert!(json.contains("\"icon_ref\":\"icons/com.apple.terminal.png\""));
    }

    #[test]
    fn selected_application_serialises_without_icon() {
        let app = SelectedApplication {
            identifier: "com.example.app".to_string(),
            display_name: "Example".to_string(),
            icon_ref: None,
        };
        let json = serde_json::to_string(&app).expect("serialise");
        assert!(json.contains("\"icon_ref\":null"));
    }

    #[test]
    fn application_picker_error_kind_str_is_stable() {
        assert_eq!(ApplicationPickerError::Cancelled.kind_str(), "cancelled");
        assert_eq!(
            ApplicationPickerError::InvalidSelection { reason: "x".into() }.kind_str(),
            "invalid_selection"
        );
        assert_eq!(
            ApplicationPickerError::MissingIdentifier.kind_str(),
            "missing_identifier"
        );
        assert_eq!(
            ApplicationPickerError::BackendUnavailable { reason: "x".into() }.kind_str(),
            "backend_unavailable"
        );
        assert_eq!(
            ApplicationPickerError::UnsupportedSession {
                reason: "wayland".into()
            }
            .kind_str(),
            "unsupported_session"
        );
    }

    #[test]
    fn application_picker_error_display_describes_each_variant() {
        // The display string MUST NOT include clipboard content,
        // hashes or snippets — only the stable variant label and the
        // sanitised reason.
        assert_eq!(
            ApplicationPickerError::Cancelled.to_string(),
            "selection cancelled"
        );
        assert_eq!(
            ApplicationPickerError::InvalidSelection {
                reason: "not a bundle".into()
            }
            .to_string(),
            "invalid selection: not a bundle"
        );
        assert_eq!(
            ApplicationPickerError::MissingIdentifier.to_string(),
            "selected bundle does not advertise a usable identifier"
        );
        assert_eq!(
            ApplicationPickerError::BackendUnavailable { reason: "x".into() }.to_string(),
            "picker backend unavailable: x"
        );
        assert_eq!(
            ApplicationPickerError::UnsupportedSession {
                reason: "wayland".into()
            }
            .to_string(),
            "picker session is not supported here: wayland"
        );
    }
}
