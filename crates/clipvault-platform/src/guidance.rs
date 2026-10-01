//! Platform-issue diagnostics and settings navigation.
//!
//! [`PlatformGuidance`] is the typed payload every adapter returns
//! when a user-triggered operation cannot proceed because of a missing
//! permission, an unsupported session, an unavailable backend or an
//! unclassified failure. The struct is deliberately serialisable so the
//! frontend can render an actionable modal without parsing free-form
//! error strings.
//!
//! The companion [`SettingsNavigator`] trait centralises every attempt
//! to open a known system settings destination. Implementations must
//! reject arbitrary URLs and never include clipboard content in their
//! outcomes or telemetry.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Categorises why a platform operation cannot proceed.
///
/// The enum is closed: new variants require a matching change in the
/// frontend modal so the user always sees a label, not a code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlatformIssueKind {
    /// The user must grant a permission to use the capability.
    PermissionRequired,
    /// The current session cannot provide the capability (for example
    /// Wayland without a paste portal).
    UnsupportedSession,
    /// The backend exists but refused to fulfil the request.
    BackendUnavailable,
    /// The cause could not be classified more precisely.
    Unknown,
}

impl PlatformIssueKind {
    pub fn as_str(self) -> &'static str {
        match self {
            PlatformIssueKind::PermissionRequired => "permission_required",
            PlatformIssueKind::UnsupportedSession => "unsupported_session",
            PlatformIssueKind::BackendUnavailable => "backend_unavailable",
            PlatformIssueKind::Unknown => "unknown",
        }
    }
}

impl fmt::Display for PlatformIssueKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Known, safe destinations the navigator knows how to open.
///
/// The enum is intentionally narrow: unknown targets are rejected by
/// the navigator and never resolved against an arbitrary URL.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlatformSettingsTarget {
    /// macOS System Settings → Privacy & Security → Accessibility.
    MacosAccessibility,
    /// Linux desktop integration entry for known environments
    /// (currently only GNOME Settings has a reliable deep link).
    LinuxDesktopIntegration,
}

impl PlatformSettingsTarget {
    pub fn as_str(self) -> &'static str {
        match self {
            PlatformSettingsTarget::MacosAccessibility => "macos_accessibility",
            PlatformSettingsTarget::LinuxDesktopIntegration => "linux_desktop_integration",
        }
    }
}

/// Structured remediation delivered to the frontend.
///
/// The struct never holds clipboard content, secrets, tokens or
/// private keys. Helpers in this module return values that already
/// pass that contract; tests assert the invariant directly on the
/// serialised payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlatformGuidance {
    /// Stable catalog identifier for the user-facing copy.
    pub message_id: PlatformGuidanceId,
    /// Stable identifier of the capability the failure belongs to
    /// (`synthetic_paste`, `global_hotkey`, ...).
    pub capability: String,
    /// Human-readable machine-friendly issue kind.
    pub kind: PlatformIssueKind,
    /// Whether the user can retry after applying the remediation.
    pub retryable: bool,
    /// Whether the navigator exposes a known safe destination to open.
    pub can_open_settings: bool,
    /// Navigator target, when `can_open_settings` is `true`.
    pub settings_target: Option<PlatformSettingsTarget>,
}

/// Catalog entry used to render remediation copy in every open UI surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlatformGuidanceId {
    MacosAccessibility,
    LinuxX11BackendUnavailable,
    LinuxWaylandUnsupported,
    LinuxUnknownSession,
    BackendUnavailable,
    Unknown,
    LinuxSettingsFallback,
    GenericSettingsFallback,
}

impl PlatformGuidance {
    /// Construct a guidance object and assert it contains no obvious
    /// payload. The constructor is preferred over struct literals so
    /// tests and runtime code share the same invariants.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        message_id: PlatformGuidanceId,
        capability: impl Into<String>,
        kind: PlatformIssueKind,
        retryable: bool,
        can_open_settings: bool,
        settings_target: Option<PlatformSettingsTarget>,
    ) -> Self {
        Self {
            message_id,
            capability: capability.into(),
            kind,
            retryable,
            can_open_settings,
            settings_target,
        }
    }

    /// Returns `true` when the guidance advertises a settings action.
    pub fn has_settings_target(&self) -> bool {
        self.can_open_settings && self.settings_target.is_some()
    }
}

/// Outcome of an attempt to open a system settings destination.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SettingsOpenOutcome {
    /// The destination was opened (deep link, NSWorkspace launch,
    /// portal call, ...). No further action is required.
    Opened,
    /// The destination could not be opened directly. The caller renders
    /// the matching packaged catalog entry. No clipboard content is carried.
    FallbackRequired { message_id: PlatformGuidanceId },
    /// The navigator refused or the OS rejected the request. The
    /// `reason` field is non-sensitive.
    Failed { reason: String },
}

impl SettingsOpenOutcome {
    pub fn kind(&self) -> &'static str {
        match self {
            SettingsOpenOutcome::Opened => "opened",
            SettingsOpenOutcome::FallbackRequired { .. } => "fallback_required",
            SettingsOpenOutcome::Failed { .. } => "failed",
        }
    }
}

/// Trait every settings navigator implements.
///
/// The trait is deliberately narrow: callers hand a known
/// [`PlatformSettingsTarget`] and receive a typed outcome. The
/// implementation is responsible for refusing unknown targets without
/// invoking any external URL.
pub trait SettingsNavigator: Send + Sync {
    /// Try to open `target` and return a typed outcome.
    ///
    /// The function must be total: it never panics and never logs
    /// clipboard content. It must reject any unknown target locally so
    /// the frontend cannot trick the shell into launching arbitrary
    /// URLs.
    fn open(&self, target: PlatformSettingsTarget) -> SettingsOpenOutcome;

    /// Stable identifier for diagnostics.
    fn name(&self) -> &'static str;
}

/// Helper that produces the canonical macOS Accessibility guidance.
pub fn macos_accessibility_guidance(capability: &str) -> PlatformGuidance {
    PlatformGuidance::new(
        PlatformGuidanceId::MacosAccessibility,
        capability,
        PlatformIssueKind::PermissionRequired,
        true,
        true,
        Some(PlatformSettingsTarget::MacosAccessibility),
    )
}

/// Helper that produces the canonical Linux X11 backend-unavailable
/// guidance. We do not promise a settings panel: the navigator decides
/// at runtime whether a known target exists.
pub fn linux_x11_backend_unavailable_guidance(capability: &str) -> PlatformGuidance {
    PlatformGuidance::new(
        PlatformGuidanceId::LinuxX11BackendUnavailable,
        capability,
        PlatformIssueKind::BackendUnavailable,
        true,
        false,
        None,
    )
}

/// Helper that produces the canonical Linux Wayland unsupported-session
/// guidance. Wayland has no portable synthetic-paste API and there is
/// no universal "open settings" destination.
pub fn linux_wayland_unsupported_guidance(capability: &str) -> PlatformGuidance {
    PlatformGuidance::new(
        PlatformGuidanceId::LinuxWaylandUnsupported,
        capability,
        PlatformIssueKind::UnsupportedSession,
        false,
        false,
        None,
    )
}

/// Helper that produces the Linux unknown-session guidance.
pub fn linux_unknown_session_guidance(capability: &str) -> PlatformGuidance {
    PlatformGuidance::new(
        PlatformGuidanceId::LinuxUnknownSession,
        capability,
        PlatformIssueKind::UnsupportedSession,
        true,
        false,
        None,
    )
}

/// Generic fallback for backends that fail without a more precise cause.
pub fn backend_unavailable_guidance(capability: &str) -> PlatformGuidance {
    PlatformGuidance::new(
        PlatformGuidanceId::BackendUnavailable,
        capability,
        PlatformIssueKind::BackendUnavailable,
        true,
        false,
        None,
    )
}

/// Generic fallback for failures that did not fit any other category.
pub fn unknown_guidance(capability: &str) -> PlatformGuidance {
    PlatformGuidance::new(
        PlatformGuidanceId::Unknown,
        capability,
        PlatformIssueKind::Unknown,
        true,
        false,
        None,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issue_kind_strings_are_stable() {
        assert_eq!(
            PlatformIssueKind::PermissionRequired.as_str(),
            "permission_required"
        );
        assert_eq!(
            PlatformIssueKind::UnsupportedSession.as_str(),
            "unsupported_session"
        );
        assert_eq!(
            PlatformIssueKind::BackendUnavailable.as_str(),
            "backend_unavailable"
        );
        assert_eq!(PlatformIssueKind::Unknown.as_str(), "unknown");
    }

    #[test]
    fn settings_target_strings_are_stable() {
        assert_eq!(
            PlatformSettingsTarget::MacosAccessibility.as_str(),
            "macos_accessibility"
        );
        assert_eq!(
            PlatformSettingsTarget::LinuxDesktopIntegration.as_str(),
            "linux_desktop_integration"
        );
    }

    #[test]
    fn settings_open_outcome_kind_strings_are_stable() {
        assert_eq!(SettingsOpenOutcome::Opened.kind(), "opened");
        assert_eq!(
            SettingsOpenOutcome::FallbackRequired {
                message_id: PlatformGuidanceId::MacosAccessibility,
            }
            .kind(),
            "fallback_required"
        );
        assert_eq!(
            SettingsOpenOutcome::Failed { reason: "x".into() }.kind(),
            "failed"
        );
    }

    #[test]
    fn guidance_serialises_without_clipboard_payload() {
        let guidance = macos_accessibility_guidance("synthetic_paste");
        let json = serde_json::to_string(&guidance).unwrap();
        assert!(!json.contains("clipboard"));
        assert!(!json.contains("password"));
        assert!(!json.contains("token"));
        assert!(json.contains("\"capability\":\"synthetic_paste\""));
        assert!(json.contains("\"message_id\":\"macos_accessibility\""));
        assert!(guidance.has_settings_target());
    }

    #[test]
    fn wayland_guidance_never_advertises_settings() {
        let guidance = linux_wayland_unsupported_guidance("synthetic_paste");
        assert_eq!(guidance.kind, PlatformIssueKind::UnsupportedSession);
        assert!(!guidance.has_settings_target());
        assert_eq!(
            guidance.message_id,
            PlatformGuidanceId::LinuxWaylandUnsupported
        );
    }

    #[test]
    fn x11_guidance_never_advertises_settings_without_known_target() {
        let guidance = linux_x11_backend_unavailable_guidance("synthetic_paste");
        assert_eq!(guidance.kind, PlatformIssueKind::BackendUnavailable);
        assert!(!guidance.has_settings_target());
        assert_eq!(
            guidance.message_id,
            PlatformGuidanceId::LinuxX11BackendUnavailable
        );
    }

    #[test]
    fn unknown_session_guidance_is_distinct_from_wayland() {
        let unknown = linux_unknown_session_guidance("synthetic_paste");
        let wayland = linux_wayland_unsupported_guidance("synthetic_paste");
        // Both share the unsupported-session cause but have distinct catalog
        // identifiers so the UI can render their platform-specific guidance.
        assert_ne!(unknown.message_id, wayland.message_id);
        assert!(unknown.retryable);
        assert!(!wayland.retryable);
    }

    #[test]
    fn wayland_guidance_has_platform_specific_catalog_identifier() {
        let guidance = linux_wayland_unsupported_guidance("synthetic_paste");
        assert_eq!(
            guidance.message_id,
            PlatformGuidanceId::LinuxWaylandUnsupported
        );
    }

    #[test]
    fn x11_guidance_has_platform_specific_catalog_identifier() {
        let guidance = linux_x11_backend_unavailable_guidance("synthetic_paste");
        assert_eq!(
            guidance.message_id,
            PlatformGuidanceId::LinuxX11BackendUnavailable
        );
    }

    #[test]
    fn unknown_session_guidance_has_platform_specific_catalog_identifier() {
        let guidance = linux_unknown_session_guidance("synthetic_paste");
        assert_eq!(guidance.message_id, PlatformGuidanceId::LinuxUnknownSession);
    }

    #[test]
    fn has_settings_target_requires_target_field() {
        let mut guidance = macos_accessibility_guidance("synthetic_paste");
        guidance.settings_target = None;
        assert!(!guidance.has_settings_target());
        guidance.settings_target = Some(PlatformSettingsTarget::MacosAccessibility);
        assert!(guidance.has_settings_target());
    }
}
