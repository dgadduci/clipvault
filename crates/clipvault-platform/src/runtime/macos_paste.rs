//! macOS `CGEventCreateKeyboardEvent`-backed paste controller.

use std::time::Duration;

use objc2_core_graphics::{CGEvent, CGEventFlags, CGEventSource, CGEventTapLocation, CGKeyCode};
use tracing::warn;

use crate::guidance::macos_accessibility_guidance;
use crate::paste::{PasteController, PasteError};

/// Paste controller that simulates `Cmd + V` through the HID event tap.
///
/// The implementation:
/// 1. Runs `CGPreflightPostEventAccess` and refuses the call when
///    macOS has not granted Accessibility to the process. The refusal
///    is reported as [`PasteError::PermissionRequired`] together with
///    the canonical [`crate::guidance::macos_accessibility_guidance`]
///    payload so the UI can render an actionable modal.
/// 2. Creates a `keyDown` event for the `V` key (`kVK_ANSI_V = 0x09`).
/// 3. Sets the `kCGEventFlagMaskCommand` flag so the target app
///    receives `Cmd+V`.
/// 4. Posts it to the HID event tap.
/// 5. Mirrors the sequence with `keyUp` after a brief delay.
///
/// The preflight runs on every [`Self::paste`] call so the controller
/// can recover after the user grants Accessibility without having to
/// rebuild it. Production callers wire this controller in regardless
/// of the initial capability detection result.
pub struct MacOsPasteController {
    preflight: fn() -> bool,
}

impl Default for MacOsPasteController {
    fn default() -> Self {
        Self::new()
    }
}

impl MacOsPasteController {
    pub fn new() -> Self {
        Self {
            preflight: preflight_post_event_access,
        }
    }

    /// Test-only constructor that injects a deterministic preflight
    /// stub. Production code uses [`Self::new`] which calls the real
    /// `CGPreflightPostEventAccess`.
    #[cfg(test)]
    pub fn with_preflight_override(preflight: fn() -> bool) -> Self {
        Self { preflight }
    }

    /// Returns `true` when the process is allowed to publish keyboard
    /// events. Exposed for tests that need to validate the preflight
    /// contract without going through `paste`.
    pub fn preflight_post_event_access() -> bool {
        preflight_post_event_access()
    }
}

/// Real macOS preflight. Always uses the native API; the runtime
/// detection wraps it.
fn preflight_post_event_access() -> bool {
    #[cfg(all(target_os = "macos", feature = "macos-native"))]
    {
        objc2_core_graphics::CGPreflightPostEventAccess()
    }
    #[cfg(not(all(target_os = "macos", feature = "macos-native")))]
    {
        false
    }
}

const K_VK_ANSI_V: CGKeyCode = 0x09;

impl PasteController for MacOsPasteController {
    fn paste(&self) -> Result<(), PasteError> {
        if !(self.preflight)() {
            warn!("CGPreflightPostEventAccess denied; reporting PermissionRequired");
            return Err(PasteError::permission_required(
                macos_accessibility_guidance("synthetic_paste"),
            ));
        }

        // Down + Up; we do both in the same call so the caller does not
        // wait for the OS to consume the keystroke.
        let source: Option<&CGEventSource> = None;
        let down = CGEvent::new_keyboard_event(source, K_VK_ANSI_V, true)
            .ok_or_else(|| PasteError::backend("CGEventCreateKeyboardEvent(down) failed"))?;
        CGEvent::set_flags(Some(&down), CGEventFlags::MaskCommand);
        CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&down));

        // Tiny delay so the OS treats the events as distinct.
        std::thread::sleep(Duration::from_millis(20));

        let up = CGEvent::new_keyboard_event(source, K_VK_ANSI_V, false)
            .ok_or_else(|| PasteError::backend("CGEventCreateKeyboardEvent(up) failed"))?;
        CGEvent::set_flags(Some(&up), CGEventFlags::MaskCommand);
        CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&up));

        Ok(())
    }

    fn name(&self) -> &'static str {
        "macos_cgevent"
    }
}

#[allow(dead_code)]
fn _unused_paste_error() -> PasteError {
    warn!("placeholder to keep tracing available");
    PasteError::backend("unused")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paste::PasteError;

    #[test]
    fn preflight_returns_bool_without_panicking() {
        // The function delegates to a system API; we only assert that
        // it returns a bool (i.e. does not panic on this host).
        let _: bool = MacOsPasteController::preflight_post_event_access();
    }

    #[test]
    fn controller_name_is_stable() {
        let controller = MacOsPasteController::new();
        assert_eq!(controller.name(), "macos_cgevent");
    }

    #[test]
    fn paste_with_preflight_denied_returns_permission_required_with_guidance() {
        fn denied() -> bool {
            false
        }
        let controller = MacOsPasteController::with_preflight_override(denied);
        let error = controller.paste().expect_err("denied preflight must fail");
        match error {
            PasteError::PermissionRequired { guidance } => {
                assert_eq!(guidance.capability, "synthetic_paste");
            }
            other => panic!("expected PermissionRequired, got {other:?}"),
        }
    }

    #[test]
    fn paste_with_preflight_granted_attempts_keyboard_event() {
        fn granted() -> bool {
            true
        }
        let controller = MacOsPasteController::with_preflight_override(granted);
        // We cannot observe a real HID event post on CI, but we can
        // guarantee the controller does not return the permission
        // variant when the preflight is granted. The exact outcome
        // (success or backend failure) depends on the host hardware.
        let outcome = controller.paste();
        if let Err(PasteError::PermissionRequired { .. }) = outcome {
            panic!("granted preflight must not return PermissionRequired");
        }
    }
}
