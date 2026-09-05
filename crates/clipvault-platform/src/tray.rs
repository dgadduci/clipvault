//! Tray / menu-bar integration.
//!
//! The platform layer defines a small trait so the shell can swap in a
//! Tauri-backed implementation (macOS menu bar item, Linux status
//! notifier item) without leaking the desktop framework to the core.

use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Menu entries exposed by the tray. The variants intentionally mirror
/// the spec's required actions; actions that depend on not-yet-implemented
/// capabilities remain as typed events and report
/// [`TrayOutcome::Unavailable`](crate::tray::TrayOutcome::Unavailable).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrayAction {
    OpenMainWindow,
    OpenQuickSearch,
    OpenFavorites,
    ClearHistory,
    OpenSettings,
    Quit,
}

impl TrayAction {
    pub fn as_str(self) -> &'static str {
        match self {
            TrayAction::OpenMainWindow => "open_main_window",
            TrayAction::OpenQuickSearch => "open_quick_search",
            TrayAction::OpenFavorites => "open_favorites",
            TrayAction::ClearHistory => "clear_history",
            TrayAction::OpenSettings => "open_settings",
            TrayAction::Quit => "quit",
        }
    }

    /// Returns `true` for actions whose underlying capability is part
    /// of this change (open main window, open quick search, quit). The
    /// other actions depend on future specs and must report
    /// `TrayOutcome::Unavailable` until those specs land.
    pub fn is_supported_in_mvp(self) -> bool {
        matches!(
            self,
            TrayAction::OpenMainWindow | TrayAction::OpenQuickSearch | TrayAction::Quit
        )
    }
}

/// Result of invoking a [`TrayAction`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayOutcome {
    /// The action was delivered to the application (e.g. the menu bar
    /// emitted the corresponding event).
    Delivered,
    /// The action was emitted but the underlying capability is not yet
    /// implemented; the frontend should surface the placeholder.
    Unavailable { action: TrayAction, reason: String },
    /// The backend rejected the invocation (e.g. the menu could not be
    /// rebuilt). The shell can retry or fall back.
    Failed { reason: String },
}

/// Typed error returned by [`TrayController::install`].
#[derive(Debug, Error)]
pub enum TrayError {
    #[error("tray backend failed: {details}")]
    Backend { details: String },
    #[error("tray capability unavailable on this platform")]
    Unavailable,
}

impl TrayError {
    pub fn backend(details: impl fmt::Display) -> Self {
        TrayError::Backend {
            details: details.to_string(),
        }
    }
}

/// Identifies the backend currently in use.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TrayBackendKind {
    Tauri,
    Unavailable,
}

impl TrayBackendKind {
    pub fn as_str(self) -> &'static str {
        match self {
            TrayBackendKind::Tauri => "tauri",
            TrayBackendKind::Unavailable => "unavailable",
        }
    }
}

/// Opaque handle that the shell can use to drive the tray at runtime
/// (rebuild the menu, dispatch an action, shut down cleanly).
pub trait TrayHandle: Send + Sync {
    /// Rebuild the menu with `entries`. The exact visual layout is up
    /// to the implementation.
    fn set_menu(&self, entries: &[TrayEntry]) -> Result<(), TrayError>;

    /// Simulate clicking the menu entry that matches `action`. Used by
    /// tests and by the bootstrap to validate the wiring.
    fn invoke(&self, action: TrayAction) -> Result<TrayOutcome, TrayError>;

    /// Release the tray icon and any associated resources. Safe to
    /// call multiple times.
    fn shutdown(&self) -> Result<(), TrayError>;
}

/// Definition of a single tray menu entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayEntry {
    pub label: String,
    pub action: TrayAction,
}

/// Platform-agnostic tray controller.
pub trait TrayController: Send + Sync {
    /// Install the tray icon. The returned handle is owned by the
    /// caller; calling `shutdown` on it is what removes the icon at
    /// application exit.
    fn install(&self) -> Result<Box<dyn TrayHandle>, TrayError>;

    /// Stable identifier for diagnostics.
    fn name(&self) -> &'static str;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_strings_are_stable() {
        assert_eq!(TrayAction::OpenMainWindow.as_str(), "open_main_window");
        assert_eq!(TrayAction::OpenQuickSearch.as_str(), "open_quick_search");
        assert_eq!(TrayAction::OpenFavorites.as_str(), "open_favorites");
        assert_eq!(TrayAction::ClearHistory.as_str(), "clear_history");
        assert_eq!(TrayAction::OpenSettings.as_str(), "open_settings");
        assert_eq!(TrayAction::Quit.as_str(), "quit");
    }

    #[test]
    fn mvp_supported_actions_are_a_subset() {
        assert!(TrayAction::OpenMainWindow.is_supported_in_mvp());
        assert!(TrayAction::OpenQuickSearch.is_supported_in_mvp());
        assert!(TrayAction::Quit.is_supported_in_mvp());
        assert!(!TrayAction::OpenFavorites.is_supported_in_mvp());
        assert!(!TrayAction::ClearHistory.is_supported_in_mvp());
        assert!(!TrayAction::OpenSettings.is_supported_in_mvp());
    }
}
