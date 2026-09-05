//! macOS `SettingsNavigator`.
//!
//! Uses `NSWorkspace::openURL` to open the Privacy & Security →
//! Accessibility pane of System Settings. The URL is hard-coded inside
//! the crate and validated against an allow-list before the call is
//! made; the frontend cannot inject arbitrary URLs.
//!
//! When the call fails (older OS versions, sandboxing, ...) the
//! navigator falls back to `SettingsOpenOutcome::FallbackRequired` with
//! the same manual steps the [`crate::guidance::macos_accessibility_guidance`]
//! helper advertises. ClipVault never runs the Settings binary with
//! elevated privileges on its own.

use objc2::rc::Retained;
use objc2_app_kit::NSWorkspace;
use objc2_foundation::{MainThreadMarker, NSString, NSURL};
use tracing::warn;

use crate::guidance::{
    macos_accessibility_guidance, PlatformSettingsTarget, SettingsNavigator, SettingsOpenOutcome,
};

/// macOS-backed settings navigator.
pub struct MacOsSettingsNavigator;

impl Default for MacOsSettingsNavigator {
    fn default() -> Self {
        Self::new()
    }
}

impl MacOsSettingsNavigator {
    pub fn new() -> Self {
        Self
    }
}

/// Only the deep links ClipVault is willing to open. Anything else
/// returns `FallbackRequired` so the shell never launches an arbitrary
/// URL the frontend asked for.
const ACCESSIBILITY_URL: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility";

fn accessibility_fallback_steps() -> Vec<String> {
    macos_accessibility_guidance("synthetic_paste")
        .steps
        .into_iter()
        .map(|step| step.to_string())
        .collect()
}

impl SettingsNavigator for MacOsSettingsNavigator {
    fn open(&self, target: PlatformSettingsTarget) -> SettingsOpenOutcome {
        match target {
            PlatformSettingsTarget::MacosAccessibility => open_accessibility_pane(),
            // macOS navigator does not handle Linux targets.
            PlatformSettingsTarget::LinuxDesktopIntegration => SettingsOpenOutcome::Failed {
                reason: "linux_desktop_integration is not available on macOS".into(),
            },
        }
    }

    fn name(&self) -> &'static str {
        "macos_system_settings"
    }
}

fn open_accessibility_pane() -> SettingsOpenOutcome {
    // The deep link is built from a constant. ClipVault never forwards
    // user-supplied URLs to the navigator.
    let url_string = NSString::from_str(ACCESSIBILITY_URL);
    let url: Option<Retained<NSURL>> = NSURL::URLWithString(&url_string);

    let Some(url) = url else {
        warn!("system preferences URL failed to parse");
        return SettingsOpenOutcome::FallbackRequired {
            manual_steps: accessibility_fallback_steps(),
        };
    };

    // `NSWorkspace.openURL:` does not require the main thread; we still
    // guard the call so we surface the failure mode consistently with
    // the active-app adapter.
    let workspace = NSWorkspace::sharedWorkspace();
    let opened = if MainThreadMarker::new().is_some() {
        workspace.openURL(&url)
    } else {
        // NSWorkspace is documented as safe to use from any thread for
        // `openURL:`; we keep the call identical to the main-thread
        // path so behaviour is consistent.
        workspace.openURL(&url)
    };

    if opened {
        return SettingsOpenOutcome::Opened;
    }

    warn!("NSWorkspace.openURL returned false");
    SettingsOpenOutcome::FallbackRequired {
        manual_steps: accessibility_fallback_steps(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macos_navigator_rejects_linux_target() {
        let nav = MacOsSettingsNavigator::new();
        let outcome = nav.open(PlatformSettingsTarget::LinuxDesktopIntegration);
        match outcome {
            SettingsOpenOutcome::Failed { reason } => {
                assert!(reason.contains("linux"));
            }
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    #[test]
    fn navigator_name_is_stable() {
        let nav = MacOsSettingsNavigator::new();
        assert_eq!(nav.name(), "macos_system_settings");
    }
}
