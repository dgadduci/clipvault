//! Global hotkey trait.
//!
//! The platform layer exposes a small [`HotkeyManager`] interface so
//! the bootstrap can wire whatever native implementation the current OS
//! provides without leaking its API to the rest of the application.

use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// One concrete hotkey combination.
///
/// `id` is the application-defined identifier used by the core to map
/// an incoming event back to a high-level action (e.g. `"quick_search"`).
/// Modifiers and key are normalised by the platform adapter so the
/// shell does not have to know which keycodes mean "V" on X11 vs. macOS.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HotkeyBinding {
    pub id: String,
    pub modifiers: HotkeyModifiers,
    pub key: HotkeyKey,
}

/// Modifier bits. The combination `CMD | SHIFT` corresponds to the
/// macOS default (`Cmd + Shift + V`); `CTRL | SHIFT` corresponds to the
/// Linux default (`Ctrl + Shift + V`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HotkeyModifiers {
    pub cmd_or_ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub meta: bool,
}

impl HotkeyModifiers {
    pub const EMPTY: HotkeyModifiers = HotkeyModifiers {
        cmd_or_ctrl: false,
        shift: false,
        alt: false,
        meta: false,
    };

    pub const CMD_SHIFT: HotkeyModifiers = HotkeyModifiers {
        cmd_or_ctrl: true,
        shift: true,
        alt: false,
        meta: false,
    };

    pub const CTRL_SHIFT: HotkeyModifiers = HotkeyModifiers {
        cmd_or_ctrl: true,
        shift: true,
        alt: false,
        meta: false,
    };
}

/// Minimal hotkey key set. The MVP only needs a handful of letters; the
/// enum is open enough to grow without breaking the serialised shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HotkeyKey {
    #[serde(rename = "v")]
    V,
    #[serde(rename = "enter")]
    Enter,
    #[serde(rename = "escape")]
    Escape,
}

impl HotkeyKey {
    pub fn as_str(self) -> &'static str {
        match self {
            HotkeyKey::V => "v",
            HotkeyKey::Enter => "enter",
            HotkeyKey::Escape => "escape",
        }
    }
}

/// Default binding used by the bootstrap on macOS.
pub fn default_macos_binding(id: impl Into<String>) -> HotkeyBinding {
    HotkeyBinding {
        id: id.into(),
        modifiers: HotkeyModifiers::CMD_SHIFT,
        key: HotkeyKey::V,
    }
}

/// Default binding used by the bootstrap on Linux.
pub fn default_linux_binding(id: impl Into<String>) -> HotkeyBinding {
    HotkeyBinding {
        id: id.into(),
        modifiers: HotkeyModifiers::CTRL_SHIFT,
        key: HotkeyKey::V,
    }
}

/// Result of attempting to register a binding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HotkeyOutcome {
    /// The OS confirmed the registration.
    Registered,
    /// Another application already owns the binding; ClipVault MUST NOT
    /// terminate.
    Conflict { reason: String },
    /// The current session does not support global hotkeys (for example
    /// a Wayland compositor without `wlr-global-shortcuts`).
    Unsupported { reason: String },
    /// The backend rejected the request for an unspecified reason.
    Failed { reason: String },
}

impl HotkeyOutcome {
    pub fn is_registered(&self) -> bool {
        matches!(self, HotkeyOutcome::Registered)
    }

    pub fn kind(&self) -> &'static str {
        match self {
            HotkeyOutcome::Registered => "registered",
            HotkeyOutcome::Conflict { .. } => "conflict",
            HotkeyOutcome::Unsupported { .. } => "unsupported",
            HotkeyOutcome::Failed { .. } => "failed",
        }
    }
}

/// Typed error for [`HotkeyManager::register`].
#[derive(Debug, Error)]
pub enum HotkeyError {
    #[error("hotkey backend failed: {details}")]
    Backend { details: String },
}

impl HotkeyError {
    pub fn backend(details: impl fmt::Display) -> Self {
        HotkeyError::Backend {
            details: details.to_string(),
        }
    }
}

/// Identifies the hotkey backend currently in use. Mirrors
/// [`crate::ClipboardBackendKind`] so the frontend can render a unified
/// diagnostics section.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HotkeyBackendKind {
    GlobalHotkey,
    Unavailable,
}

impl HotkeyBackendKind {
    pub fn as_str(self) -> &'static str {
        match self {
            HotkeyBackendKind::GlobalHotkey => "global_hotkey",
            HotkeyBackendKind::Unavailable => "unavailable",
        }
    }
}

/// Platform-agnostic hotkey manager. Implementations must be safe to
/// share across threads.
pub trait HotkeyManager: Send + Sync {
    /// Register `binding`. The `on_activate` callback is invoked from
    /// the backend's worker thread; implementations are responsible for
    /// any required thread marshalling.
    ///
    /// The function returns a [`HotkeyOutcome`] describing how the OS
    /// reacted. Calling `register` again with the same binding replaces
    /// the previous callback.
    fn register(
        &self,
        binding: &HotkeyBinding,
        on_activate: Box<dyn Fn() + Send + Sync + 'static>,
    ) -> Result<HotkeyOutcome, HotkeyError>;

    /// Release every binding registered through this manager. Safe to
    /// call multiple times.
    fn unregister_all(&self) -> Result<(), HotkeyError>;

    /// Stable identifier for diagnostics.
    fn name(&self) -> &'static str;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_bindings_use_expected_modifiers() {
        let mac = default_macos_binding("quick_search");
        assert!(mac.modifiers.cmd_or_ctrl);
        assert!(mac.modifiers.shift);
        assert_eq!(mac.key, HotkeyKey::V);

        let linux = default_linux_binding("quick_search");
        assert!(linux.modifiers.cmd_or_ctrl);
        assert!(linux.modifiers.shift);
        assert_eq!(linux.key, HotkeyKey::V);

        // Same shape, different defaults are encoded through id+OS in
        // the bootstrap, not the binding itself.
        assert_eq!(mac, linux);
    }

    #[test]
    fn hotkey_outcome_kind_strings() {
        assert_eq!(HotkeyOutcome::Registered.kind(), "registered");
        assert_eq!(
            HotkeyOutcome::Conflict { reason: "x".into() }.kind(),
            "conflict"
        );
        assert_eq!(
            HotkeyOutcome::Unsupported { reason: "x".into() }.kind(),
            "unsupported"
        );
        assert_eq!(
            HotkeyOutcome::Failed { reason: "x".into() }.kind(),
            "failed"
        );
    }
}
