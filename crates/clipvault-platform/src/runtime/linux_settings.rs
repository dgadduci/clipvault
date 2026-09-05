//! Linux `SettingsNavigator`.
//!
//! Linux does not have a single cross-distro settings panel, so the
//! navigator is deliberately conservative: it inspects
//! `XDG_CURRENT_DESKTOP` and only attempts to launch a known-good
//! binary via a direct path lookup (no `$PATH` search). When no known
//! desktop environment is detected, the navigator returns
//! [`SettingsOpenOutcome::FallbackRequired`] so the modal keeps the
//! manual steps visible and the user retains control.
//!
//! The navigator never accepts URLs or commands from the frontend.

use std::process::Command;

use tracing::warn;

use crate::guidance::{
    linux_unknown_session_guidance, linux_x11_backend_unavailable_guidance, PlatformSettingsTarget,
    SettingsNavigator, SettingsOpenOutcome,
};

/// Linux-backed settings navigator.
pub struct LinuxSettingsNavigator;

impl Default for LinuxSettingsNavigator {
    fn default() -> Self {
        Self::new()
    }
}

impl LinuxSettingsNavigator {
    pub fn new() -> Self {
        Self
    }
}

const PATH_GNOME_CONTROL_CENTER: &str = "/usr/bin/gnome-control-center";
const PATH_KDE_SYSTEM_SETTINGS: &str = "/usr/bin/systemsettings5";

impl SettingsNavigator for LinuxSettingsNavigator {
    fn open(&self, target: PlatformSettingsTarget) -> SettingsOpenOutcome {
        match target {
            PlatformSettingsTarget::LinuxDesktopIntegration => open_linux_settings(),
            // Linux navigator does not handle macOS targets.
            PlatformSettingsTarget::MacosAccessibility => SettingsOpenOutcome::Failed {
                reason: "macos_accessibility is not available on Linux".into(),
            },
        }
    }

    fn name(&self) -> &'static str {
        "linux_desktop_settings"
    }
}

fn open_linux_settings() -> SettingsOpenOutcome {
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
    let desktop_lower = desktop.to_lowercase();

    let command_path = if desktop_lower.split(':').any(|token| token == "gnome") {
        PATH_GNOME_CONTROL_CENTER
    } else if desktop_lower
        .split(':')
        .any(|token| token == "kde" || token == "plasma")
    {
        PATH_KDE_SYSTEM_SETTINGS
    } else {
        warn!(desktop = %desktop, "no known Linux desktop target; returning fallback");
        return SettingsOpenOutcome::FallbackRequired {
            manual_steps: fallback_steps(),
        };
    };

    if !std::path::Path::new(command_path).exists() {
        warn!(path = %command_path, "desktop settings binary not present");
        return SettingsOpenOutcome::FallbackRequired {
            manual_steps: fallback_steps(),
        };
    }

    match Command::new(command_path).spawn() {
        Ok(_child) => SettingsOpenOutcome::Opened,
        Err(error) => {
            warn!(error = %error, path = %command_path, "failed to launch desktop settings");
            SettingsOpenOutcome::FallbackRequired {
                manual_steps: fallback_steps(),
            }
        }
    }
}

fn fallback_steps() -> Vec<String> {
    let mut steps = linux_x11_backend_unavailable_guidance("synthetic_paste").steps;
    steps.extend(
        linux_unknown_session_guidance("synthetic_paste")
            .steps
            .into_iter()
            .map(|s| s.to_string()),
    );
    steps
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linux_navigator_rejects_macos_target() {
        let nav = LinuxSettingsNavigator::new();
        let outcome = nav.open(PlatformSettingsTarget::MacosAccessibility);
        match outcome {
            SettingsOpenOutcome::Failed { reason } => {
                assert!(reason.contains("macos"));
            }
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    #[test]
    fn unknown_desktop_returns_fallback_steps() {
        // Force the navigator down the unknown-desktop branch by
        // overriding the env var before each call (the function reads
        // it locally).
        std::env::set_var("XDG_CURRENT_DESKTOP", "unknown-desktop");
        let nav = LinuxSettingsNavigator::new();
        let outcome = nav.open(PlatformSettingsTarget::LinuxDesktopIntegration);
        match outcome {
            SettingsOpenOutcome::FallbackRequired { manual_steps } => {
                assert!(!manual_steps.is_empty());
                // Must not reference macOS or Accessibility.
                let joined = manual_steps.join(" ").to_lowercase();
                assert!(!joined.contains("macos"));
                assert!(!joined.contains("accessibility"));
            }
            other => panic!("expected FallbackRequired, got {other:?}"),
        }
        std::env::remove_var("XDG_CURRENT_DESKTOP");
    }

    #[test]
    fn navigator_name_is_stable() {
        let nav = LinuxSettingsNavigator::new();
        assert_eq!(nav.name(), "linux_desktop_settings");
    }

    #[test]
    fn gnome_desktop_with_missing_binary_returns_fallback() {
        // Sandbox CI hosts do not ship GNOME control center; we
        // exercise the "known target but binary missing" branch.
        std::env::set_var("XDG_CURRENT_DESKTOP", "GNOME");
        let nav = LinuxSettingsNavigator::new();
        let outcome = nav.open(PlatformSettingsTarget::LinuxDesktopIntegration);
        // Either Opened (developer machine with GNOME installed) or
        // FallbackRequired (CI without GNOME) is acceptable, but it
        // must never be Failed because the navigator only attempts a
        // known safe target.
        match outcome {
            SettingsOpenOutcome::Opened | SettingsOpenOutcome::FallbackRequired { .. } => {}
            SettingsOpenOutcome::Failed { .. } => {
                panic!("Linux navigator must never report Failed for GNOME")
            }
        }
        std::env::remove_var("XDG_CURRENT_DESKTOP");
    }
}
