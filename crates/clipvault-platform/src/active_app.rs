//! Active-application detection.
//!
//! The probe is intentionally a single, narrow interface: return the
//! currently focused application when the OS can answer the question,
//! or a typed error otherwise. The frontend never receives
//! `application.pid` or window content — only the public app name and
//! identifier that the user already sees in their dock/taskbar.

use std::fmt;
use std::sync::Arc;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Lightweight, privacy-preserving description of the application the
/// user is currently interacting with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveApplication {
    /// User-visible application name (for example "Terminal" or
    /// "Firefox"). The platform adapter chooses the best available
    /// source (`NSWorkspace.localizedName` on macOS, `WM_CLASS` on X11).
    pub name: String,
    /// Stable, machine-friendly identifier (`com.apple.Terminal`,
    /// `firefox`). May be empty when the platform cannot provide one.
    #[serde(default)]
    pub identifier: String,
}

impl ActiveApplication {
    pub fn new(name: impl Into<String>, identifier: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            identifier: identifier.into(),
        }
    }
}

/// Typed error returned by [`ActiveApplicationProbe::active_application`].
#[derive(Debug, Clone, Error)]
pub enum ActiveAppError {
    /// The current session cannot answer the question (for example a
    /// Wayland compositor without an active-app protocol).
    #[error("active application detection is unavailable on this platform")]
    Unavailable,
    /// The backend failed for a non-fatal reason. The capture pipeline
    /// must keep running.
    #[error("active application detection failed: {details}")]
    Backend { details: String },
}

impl ActiveAppError {
    pub fn backend(details: impl fmt::Display) -> Self {
        ActiveAppError::Backend {
            details: details.to_string(),
        }
    }
}

/// Identifies the backend currently in use.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActiveAppBackendKind {
    MacOsWorkspace,
    /// Plain X11 session — `WAYLAND_DISPLAY` is unset and the EWMH
    /// probe connected to the X server via `$DISPLAY`.
    X11Ewmh,
    /// Wayland session where the active application is exposed by
    /// XWayland. The EWMH probe still connects through `$DISPLAY`
    /// but the host is structurally Wayland, so the diagnostics
    /// surface distinguishes this branch from the plain X11 one.
    XWaylandEwmh,
    Unavailable,
}

impl ActiveAppBackendKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ActiveAppBackendKind::MacOsWorkspace => "macos_workspace",
            ActiveAppBackendKind::X11Ewmh => "x11_ewmh",
            ActiveAppBackendKind::XWaylandEwmh => "xwayland_ewmh",
            ActiveAppBackendKind::Unavailable => "unavailable",
        }
    }
}

/// Platform-agnostic probe.
pub trait ActiveApplicationProbe: Send + Sync {
    /// Return the currently focused application, or `Ok(None)` when no
    /// application is focused.
    fn active_application(&self) -> Result<Option<ActiveApplication>, ActiveAppError>;

    /// Stable identifier for diagnostics.
    fn name(&self) -> &'static str;
}

/// Thread-safe probe wrapper that caches the most recent answer from a
/// platform probe.
///
/// macOS's `NSWorkspace` API requires the main thread; this wrapper
/// lets the capture watcher (which runs on a background thread) read
/// the last cached answer while a periodic main-thread refresh keeps
/// the cache fresh. On platforms where the inner probe is callable
/// from any thread (X11) the cache is essentially free and still
/// useful for tests.
#[derive(Clone)]
pub struct CachedActiveApplication {
    inner: Arc<dyn ActiveApplicationProbe>,
    cache: Arc<Mutex<Option<ActiveApplication>>>,
}

impl CachedActiveApplication {
    /// Build a wrapper around `inner`. The cache starts empty.
    pub fn new(inner: Arc<dyn ActiveApplicationProbe>) -> Self {
        Self {
            inner,
            cache: Arc::new(Mutex::new(None)),
        }
    }

    /// Borrow the wrapped probe (used by the bootstrap to refresh the
    /// cache from the platform thread).
    pub fn inner(&self) -> &Arc<dyn ActiveApplicationProbe> {
        &self.inner
    }

    /// Replace the cached value. Returns the previous value so the
    /// caller can decide whether to log a transition.
    pub fn refresh_with(&self, value: Option<ActiveApplication>) -> Option<ActiveApplication> {
        let mut guard = self.cache.lock();
        std::mem::replace(&mut *guard, value)
    }

    /// Ask the inner probe and update the cache with the answer.
    /// Returns the same value the cache stores after the call.
    pub fn refresh(&self) -> Result<Option<ActiveApplication>, ActiveAppError> {
        let answer = self.inner.active_application()?;
        self.refresh_with(answer.clone());
        Ok(answer)
    }

    /// Snapshot of the cached value without touching the inner probe.
    pub fn cached(&self) -> Option<ActiveApplication> {
        self.cache.lock().clone()
    }
}

impl ActiveApplicationProbe for CachedActiveApplication {
    fn active_application(&self) -> Result<Option<ActiveApplication>, ActiveAppError> {
        // The capture pipeline runs on a background thread where the
        // macOS probe is not safe to call. We always serve the cached
        // value populated by the platform thread; the watcher can
        // still match an empty cache (no entry recorded) against the
        // blacklist as the PrivacyGate treats "unknown source" as
        // "allow".
        Ok(self.cached())
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct ScriptedProbe {
        answers: Mutex<Vec<Result<Option<ActiveApplication>, ActiveAppError>>>,
    }

    impl ScriptedProbe {
        fn new(answers: Vec<Result<Option<ActiveApplication>, ActiveAppError>>) -> Self {
            Self {
                answers: Mutex::new(answers),
            }
        }
    }

    impl ActiveApplicationProbe for ScriptedProbe {
        fn active_application(&self) -> Result<Option<ActiveApplication>, ActiveAppError> {
            self.answers.lock().pop().unwrap_or(Ok(None))
        }
        fn name(&self) -> &'static str {
            "scripted"
        }
    }

    #[test]
    fn active_application_serialises_with_empty_identifier() {
        let app = ActiveApplication::new("Terminal", "");
        let json = serde_json::to_string(&app).unwrap();
        assert!(json.contains("\"identifier\":\"\""));
    }

    #[test]
    fn backend_kind_strings_are_stable() {
        assert_eq!(
            ActiveAppBackendKind::MacOsWorkspace.as_str(),
            "macos_workspace"
        );
        assert_eq!(ActiveAppBackendKind::X11Ewmh.as_str(), "x11_ewmh");
        assert_eq!(ActiveAppBackendKind::XWaylandEwmh.as_str(), "xwayland_ewmh");
        assert_eq!(ActiveAppBackendKind::Unavailable.as_str(), "unavailable");
    }

    #[test]
    fn cached_probe_serves_empty_until_refreshed() {
        let inner = Arc::new(ScriptedProbe::new(vec![Ok(Some(ActiveApplication::new(
            "Firefox", "firefox",
        )))]));
        let cached = CachedActiveApplication::new(inner.clone());
        assert!(cached.active_application().unwrap().is_none());
        cached.refresh().expect("refresh");
        assert_eq!(
            cached.active_application().unwrap(),
            Some(ActiveApplication::new("Firefox", "firefox"))
        );
    }

    #[test]
    fn cached_probe_serves_last_refresh_after_inner_exhausted() {
        let inner = Arc::new(ScriptedProbe::new(vec![Ok(Some(ActiveApplication::new(
            "Firefox", "firefox",
        )))]));
        let cached = CachedActiveApplication::new(inner.clone());
        cached.refresh().expect("first refresh");
        // The inner probe is exhausted; subsequent calls must keep
        // returning the cached value.
        assert_eq!(
            cached.active_application().unwrap(),
            Some(ActiveApplication::new("Firefox", "firefox"))
        );
    }

    #[test]
    fn refresh_with_replaces_and_returns_previous() {
        let inner = Arc::new(ScriptedProbe::new(vec![]));
        let cached = CachedActiveApplication::new(inner);
        cached.refresh_with(Some(ActiveApplication::new("Firefox", "firefox")));
        let previous = cached.refresh_with(Some(ActiveApplication::new("Safari", "safari")));
        assert_eq!(previous.unwrap().identifier, "firefox");
        assert_eq!(cached.cached().unwrap().identifier, "safari");
    }
}
