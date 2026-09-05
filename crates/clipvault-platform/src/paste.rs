//! Synthetic paste trait.
//!
//! [`PasteController`] is the platform-agnostic boundary for "trigger
//! the OS-level paste action into the previously focused application".
//! Implementations translate the call into the right native API:
//! `CGEventCreateKeyboardEvent` on macOS, `XTestFakeKeyEvent` on X11,
//! or `CapabilityUnavailable` on Wayland.

use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::guidance::PlatformGuidance;
use crate::Capability;

/// Typed error returned by [`PasteController::paste`].
#[derive(Debug, Clone, Error)]
pub enum PasteError {
    /// The current session does not support synthetic paste.
    #[error("synthetic paste is unavailable on this platform")]
    Unavailable,
    /// The backend refused for an unspecified reason (e.g. the target
    /// application rejected the keystroke).
    #[error("synthetic paste failed: {details}")]
    Backend { details: String },
    /// The platform layer could not run the operation because the user
    /// must grant a permission (e.g. Accessibility on macOS). The
    /// `guidance` payload is safe to forward to the UI: it contains
    /// remediation steps but never clipboard content.
    #[error("synthetic paste requires a platform permission")]
    PermissionRequired { guidance: PlatformGuidance },
    /// The session does not support the capability for structural
    /// reasons (e.g. Wayland without a paste portal). The `guidance`
    /// payload explains why this is not a missing macOS permission.
    #[error("synthetic paste is not supported in this session")]
    UnsupportedSession { guidance: PlatformGuidance },
}

impl PasteError {
    pub fn backend(details: impl fmt::Display) -> Self {
        PasteError::Backend {
            details: details.to_string(),
        }
    }

    pub fn capability() -> Self {
        PasteError::Unavailable
    }

    pub fn capability_label() -> Capability {
        Capability::SyntheticPaste
    }

    /// Build a [`PasteError::PermissionRequired`] carrying the typed
    /// guidance the frontend will render in the modal.
    pub fn permission_required(guidance: PlatformGuidance) -> Self {
        PasteError::PermissionRequired { guidance }
    }

    /// Build a [`PasteError::UnsupportedSession`] carrying the typed
    /// guidance the frontend will render in the modal.
    pub fn unsupported_session(guidance: PlatformGuidance) -> Self {
        PasteError::UnsupportedSession { guidance }
    }

    /// Returns the guidance payload when the error variant carries one.
    /// `Backend` and `Unavailable` return `None` to preserve the
    /// existing serialised shape.
    pub fn guidance(&self) -> Option<&PlatformGuidance> {
        match self {
            PasteError::PermissionRequired { guidance }
            | PasteError::UnsupportedSession { guidance } => Some(guidance),
            PasteError::Unavailable | PasteError::Backend { .. } => None,
        }
    }
}

/// Identifies the backend currently in use.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PasteBackendKind {
    MacOsCoreGraphics,
    X11Test,
    Unavailable,
}

impl PasteBackendKind {
    pub fn as_str(self) -> &'static str {
        match self {
            PasteBackendKind::MacOsCoreGraphics => "macos_cgevent",
            PasteBackendKind::X11Test => "x11_xtest",
            PasteBackendKind::Unavailable => "unavailable",
        }
    }
}

/// Platform-agnostic paste trigger.
pub trait PasteController: Send + Sync {
    /// Trigger the OS-level paste action.
    ///
    /// Implementations must not log the clipboard payload, must not
    /// panic on failure and must return a typed error so the caller can
    /// decide whether to keep the application running.
    fn paste(&self) -> Result<(), PasteError>;

    /// Stable identifier for diagnostics.
    fn name(&self) -> &'static str;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::guidance::{
        backend_unavailable_guidance, linux_wayland_unsupported_guidance,
        macos_accessibility_guidance, PlatformIssueKind,
    };

    #[test]
    fn paste_error_unavailable_is_distinct_from_backend() {
        let capability = PasteError::capability();
        let backend = PasteError::backend("target app rejected keystroke");
        assert!(matches!(capability, PasteError::Unavailable));
        assert!(matches!(backend, PasteError::Backend { .. }));
        assert!(capability.guidance().is_none());
        assert!(backend.guidance().is_none());
    }

    #[test]
    fn permission_required_carries_guidance_and_kind_label() {
        let guidance = macos_accessibility_guidance("synthetic_paste");
        let error = PasteError::permission_required(guidance.clone());
        match &error {
            PasteError::PermissionRequired { guidance: g } => {
                assert_eq!(g.kind, PlatformIssueKind::PermissionRequired);
                assert_eq!(g.capability, "synthetic_paste");
            }
            other => panic!("expected PermissionRequired, got {other:?}"),
        }
        assert_eq!(error.guidance(), Some(&guidance));
    }

    #[test]
    fn unsupported_session_carries_guidance() {
        let guidance = linux_wayland_unsupported_guidance("synthetic_paste");
        let error = PasteError::unsupported_session(guidance.clone());
        match &error {
            PasteError::UnsupportedSession { guidance: g } => {
                assert_eq!(g.kind, PlatformIssueKind::UnsupportedSession);
            }
            other => panic!("expected UnsupportedSession, got {other:?}"),
        }
        assert_eq!(error.guidance(), Some(&guidance));
    }

    #[test]
    fn backend_variant_does_not_carry_guidance() {
        let error = PasteError::backend("boom");
        assert!(error.guidance().is_none());
    }

    #[test]
    fn guidance_payload_never_references_clipboard_content() {
        let guidance = backend_unavailable_guidance("synthetic_paste");
        let json = serde_json::to_string(&guidance).unwrap();
        assert!(!json.contains("password"));
        assert!(!json.contains("token"));
        assert!(!json.contains("clipboard"));
    }

    #[test]
    fn backend_kind_strings_are_stable() {
        assert_eq!(
            PasteBackendKind::MacOsCoreGraphics.as_str(),
            "macos_cgevent"
        );
        assert_eq!(PasteBackendKind::X11Test.as_str(), "x11_xtest");
        assert_eq!(PasteBackendKind::Unavailable.as_str(), "unavailable");
    }
}
