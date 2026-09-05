//! Linux stub for the application picker.
//!
//! The picker surface requires a safe mapping between the application
//! the user selects and the identifier the active-app adapter feeds
//! the matcher. On X11 the candidate is a `WM_CLASS` resolved through
//! EWMH; on Wayland the canonical id is `wl_app_id` but not every
//! compositor exposes it.
//!
//! Implementing a `.desktop ↔ WM_CLASS/app_id` bridge that always
//! returns a stable identifier requires either:
//! - a Wayland portal that round-trips through the user (not
//!   available in the MVP session),
//! - or an X11 EWMH probe that resolves every installed
//!   `.desktop` file to its `StartupWMClass` (and falls back to a
//!   generated id otherwise — but a generated id would let the user
//!   blacklist a bundle that the active-app probe will never report,
//!   which is the exact failure mode the spec calls out).
//!
//! Until one of those mappings is wired in, the picker adapter
//! returns [`ApplicationPickerError::UnsupportedSession`] so the
//! shell can surface the limitation in the UI without inventing an
//! identifier. The blacklist remains usable on Linux through the
//! manual `Add identifier` flow.

use crate::app_picker::{ApplicationPicker, ApplicationPickerError, SelectedApplication};

/// Linux stub. Always reports the picker as unsupported until a safe
/// `.desktop ↔ WM_CLASS/app_id` mapping is implemented.
pub struct LinuxApplicationPicker;

impl LinuxApplicationPicker {
    pub fn new() -> Self {
        Self
    }
}

impl Default for LinuxApplicationPicker {
    fn default() -> Self {
        Self::new()
    }
}

impl ApplicationPicker for LinuxApplicationPicker {
    fn pick(&self) -> Result<SelectedApplication, ApplicationPickerError> {
        Err(ApplicationPickerError::UnsupportedSession {
            reason: "no safe .desktop ↔ WM_CLASS/app_id mapping is available yet".into(),
        })
    }

    fn name(&self) -> &'static str {
        "linux_app_picker"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linux_picker_reports_unsupported_session() {
        // The MVP Linux path returns the typed unsupported error so
        // the shell can surface the limitation in the UI. Renaming
        // the variant is a breaking change for the frontend.
        let picker = LinuxApplicationPicker::new();
        let err = picker.pick().expect_err("must reject");
        assert_eq!(err.kind_str(), "unsupported_session");
        assert_eq!(picker.name(), "linux_app_picker");
    }
}
