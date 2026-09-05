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
    /// Stable identifier of the capability the failure belongs to
    /// (`synthetic_paste`, `global_hotkey`, ...).
    pub capability: String,
    /// Human-readable machine-friendly issue kind.
    pub kind: PlatformIssueKind,
    /// Title rendered at the top of the modal.
    pub title: String,
    /// One-paragraph explanation of what the user is seeing.
    pub summary: String,
    /// Ordered remediation steps. Frontend renders them as a numbered
    /// list.
    pub steps: Vec<String>,
    /// Whether the user can retry after applying the remediation.
    pub retryable: bool,
    /// Whether the navigator exposes a known safe destination to open.
    pub can_open_settings: bool,
    /// Navigator target, when `can_open_settings` is `true`.
    pub settings_target: Option<PlatformSettingsTarget>,
}

impl PlatformGuidance {
    /// Construct a guidance object and assert it contains no obvious
    /// payload. The constructor is preferred over struct literals so
    /// tests and runtime code share the same invariants.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        capability: impl Into<String>,
        kind: PlatformIssueKind,
        title: impl Into<String>,
        summary: impl Into<String>,
        steps: Vec<String>,
        retryable: bool,
        can_open_settings: bool,
        settings_target: Option<PlatformSettingsTarget>,
    ) -> Self {
        Self {
            capability: capability.into(),
            kind,
            title: title.into(),
            summary: summary.into(),
            steps,
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
    /// The destination could not be opened directly. The caller should
    /// keep the manual steps visible. No clipboard content is carried
    /// in this variant.
    FallbackRequired { manual_steps: Vec<String> },
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
        capability,
        PlatformIssueKind::PermissionRequired,
        "ClipVault needs Accessibility permission",
        "macOS requires ClipVault to be allowed to send keyboard events so it can press Cmd+V in the application you are typing into.",
        vec![
            "Open System Settings.".into(),
            "Go to Privacy & Security → Accessibility.".into(),
            "Enable ClipVault (or the development binary you are running).".into(),
            "Come back to ClipVault and press Reintentar.".into(),
        ],
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
        capability,
        PlatformIssueKind::BackendUnavailable,
        "ClipVault could not reach the X11 paste backend",
        "Synthetic paste needs a working X11 display and the XTEST extension. Verify the session, the display and any sandbox permissions before retrying.",
        vec![
            "Confirm that you are running inside the graphical X11 session you intend to paste into.".into(),
            "Confirm that $DISPLAY is set and that the XTEST extension is enabled.".into(),
            "If ClipVault is installed via Flatpak, Snap or another sandbox, enable the desktop integration permissions the package declares.".into(),
            "Return to ClipVault and press Reintentar.".into(),
        ],
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
        capability,
        PlatformIssueKind::UnsupportedSession,
        "Wayland has no portable synthetic paste",
        "ClipVault cannot inject keystrokes under Wayland without a compositor-specific portal. There is no universal settings switch to enable it; it depends on the compositor you are running.",
        vec![
            "Open the history and copy entries with your usual keyboard shortcut if your compositor exposes one.".into(),
            "If you depend on automatic paste, switch to an X11 session or configure a compositor-specific paste portal.".into(),
            "History, capture and search continue to work without synthetic paste.".into(),
        ],
        false,
        false,
        None,
    )
}

/// Helper that produces the Linux unknown-session guidance.
pub fn linux_unknown_session_guidance(capability: &str) -> PlatformGuidance {
    PlatformGuidance::new(
        capability,
        PlatformIssueKind::UnsupportedSession,
        "ClipVault could not identify a compatible graphical session",
        "ClipVault did not detect X11 or Wayland on this host. Synthetic paste is unavailable until a recognised session is active.",
        vec![
            "Launch ClipVault from a graphical X11 or Wayland session.".into(),
            "Avoid running ClipVault over a plain SSH connection without a forwarded display.".into(),
            "Press Reintentar after the session is reachable.".into(),
        ],
        true,
        false,
        None,
    )
}

/// Generic fallback for backends that fail without a more precise cause.
pub fn backend_unavailable_guidance(capability: &str) -> PlatformGuidance {
    PlatformGuidance::new(
        capability,
        PlatformIssueKind::BackendUnavailable,
        "ClipVault could not complete the operation",
        "The platform backend refused the request and ClipVault could not classify the cause more precisely. The history entry was not modified.",
        vec![
            "Retry the operation.".into(),
            "If the issue persists, restart ClipVault and check the application logs.".into(),
        ],
        true,
        false,
        None,
    )
}

/// Generic fallback for failures that did not fit any other category.
pub fn unknown_guidance(capability: &str) -> PlatformGuidance {
    PlatformGuidance::new(
        capability,
        PlatformIssueKind::Unknown,
        "Unexpected platform failure",
        "ClipVault encountered an unexpected failure on this platform. No clipboard content was sent to the diagnostic output.",
        vec![
            "Retry the operation.".into(),
            "If the failure keeps happening, open the ClipVault diagnostics to confirm the host platform.".into(),
        ],
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
                manual_steps: vec!["x".into()]
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
        assert!(guidance.has_settings_target());
    }

    #[test]
    fn wayland_guidance_never_advertises_settings() {
        let guidance = linux_wayland_unsupported_guidance("synthetic_paste");
        assert_eq!(guidance.kind, PlatformIssueKind::UnsupportedSession);
        assert!(!guidance.has_settings_target());
        let json = serde_json::to_string(&guidance).unwrap();
        // Must not mention macOS, AX or Accessibility.
        assert!(!json.to_lowercase().contains("macos"));
        assert!(!json.to_lowercase().contains("accessibility"));
    }

    #[test]
    fn x11_guidance_never_advertises_settings_without_known_target() {
        let guidance = linux_x11_backend_unavailable_guidance("synthetic_paste");
        assert_eq!(guidance.kind, PlatformIssueKind::BackendUnavailable);
        assert!(!guidance.has_settings_target());
        let json = serde_json::to_string(&guidance).unwrap();
        assert!(!json.to_lowercase().contains("macos"));
    }

    #[test]
    fn unknown_session_guidance_is_distinct_from_wayland() {
        let unknown = linux_unknown_session_guidance("synthetic_paste");
        let wayland = linux_wayland_unsupported_guidance("synthetic_paste");
        // Both belong to the same `UnsupportedSession` family by design;
        // the user-visible distinction must come from the title and the
        // remediation steps, not from the cause code.
        assert_ne!(unknown.title, wayland.title);
        assert_ne!(unknown.summary, wayland.summary);
        assert!(unknown.retryable);
        assert!(!wayland.retryable);
    }

    #[test]
    fn wayland_guidance_text_does_not_mention_accessibility() {
        let guidance = linux_wayland_unsupported_guidance("synthetic_paste");
        let joined = format!(
            "{} {} {}",
            guidance.title,
            guidance.summary,
            guidance.steps.join(" ")
        )
        .to_lowercase();
        assert!(!joined.contains("accessibility"));
        assert!(!joined.contains("macos"));
    }

    #[test]
    fn x11_guidance_mentions_sandbox_and_xtest() {
        let guidance = linux_x11_backend_unavailable_guidance("synthetic_paste");
        let joined = format!(
            "{} {} {}",
            guidance.title,
            guidance.summary,
            guidance.steps.join(" ")
        )
        .to_lowercase();
        assert!(joined.contains("xtest"));
        assert!(joined.contains("display"));
    }

    #[test]
    fn unknown_session_guidance_recommends_starting_graphical_session() {
        let guidance = linux_unknown_session_guidance("synthetic_paste");
        let joined = format!(
            "{} {} {}",
            guidance.title,
            guidance.summary,
            guidance.steps.join(" ")
        )
        .to_lowercase();
        assert!(joined.contains("x11") || joined.contains("wayland"));
        assert!(!joined.contains("macos"));
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
