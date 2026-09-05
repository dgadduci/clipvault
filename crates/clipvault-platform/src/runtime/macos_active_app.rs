//! macOS `NSWorkspace`-backed active-application probe.

use objc2::rc::Retained;
use objc2_app_kit::NSRunningApplication;
use objc2_app_kit::NSWorkspace;
use objc2_foundation::{MainThreadMarker, NSString};

use crate::active_app::{ActiveAppError, ActiveApplication, ActiveApplicationProbe};

/// Probe backed by `NSWorkspace::frontmostApplication`. Reads the
/// `localizedName` and `bundleIdentifier` of the running application
/// the OS reports as currently focused.
///
/// `active_application` must be called from the main thread; Apple
/// requires it for `NSWorkspace`. Off the main thread we return
/// [`ActiveAppError::Unavailable`] instead of panicking.
pub struct MacOsActiveApplication;

impl Default for MacOsActiveApplication {
    fn default() -> Self {
        Self::new()
    }
}

impl MacOsActiveApplication {
    pub fn new() -> Self {
        Self
    }
}

impl ActiveApplicationProbe for MacOsActiveApplication {
    fn active_application(&self) -> Result<Option<ActiveApplication>, ActiveAppError> {
        let Some(_mtm) = MainThreadMarker::new() else {
            return Err(ActiveAppError::Unavailable);
        };
        let workspace = NSWorkspace::sharedWorkspace();
        let app: Option<Retained<NSRunningApplication>> = workspace.frontmostApplication();
        let Some(app) = app else {
            return Ok(None);
        };

        let name = app
            .localizedName()
            .map(|s| s.to_string())
            .unwrap_or_default();
        let identifier = app
            .bundleIdentifier()
            .map(|s: Retained<NSString>| s.to_string())
            .unwrap_or_default();

        if name.is_empty() {
            return Ok(None);
        }

        Ok(Some(ActiveApplication::new(name, identifier)))
    }

    fn name(&self) -> &'static str {
        "macos_workspace"
    }
}

#[cfg(test)]
mod tests {
    use objc2_foundation::MainThreadMarker;

    #[test]
    fn main_thread_marker_is_none_on_a_background_thread() {
        // `cargo test` runs unit tests on the test harness threads,
        // which are not the macOS main thread. We exercise the
        // production branch by confirming `MainThreadMarker::new()`
        // correctly returns `None` from a worker thread — this is
        // the branch that keeps the capture loop off the main
        // thread.
        let handle = std::thread::Builder::new()
            .spawn(|| MainThreadMarker::new().is_none())
            .expect("spawn");
        assert!(
            handle.join().expect("join"),
            "MainThreadMarker::new() must return None off the main thread"
        );
    }
}
