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

/// Metadata-only stage a Linux X11/XWayland probe reaches on the most
/// recent call. The diagnostics surface exposes the value verbatim so
/// the user can tell apart "no X11 window is focused" from "WM_CLASS is
/// published but not parseable" without parsing free-form log lines.
///
/// macOS and Windows probes return [`ProbeStage::NotApplicable`] (the
/// default): the NSWorkspace / `GetForegroundWindow` paths never
/// granularise the failure modes the Linux probe does, and the
/// `not_applicable` sentinel keeps the diagnostics endpoint stable for
/// every platform.
///
/// The serialised strings are stable identifiers the dashboard
/// consumes; renaming a variant is a breaking change for the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProbeStage {
    /// The probe is not relevant on this host (macOS, no-op probe,
    /// future Windows adapter before it ships granular stages).
    NotApplicable,
    /// The probe started but did not finish — value not refreshed yet.
    Started,
    /// `_NET_ACTIVE_WINDOW` was either unreadable, the value had the
    /// wrong `format`, or the server returned zero entries. Whatever
    /// the cause, the probe cannot look up `WM_CLASS` until another
    /// window becomes focused.
    ActiveWindowMissing,
    /// `_NET_ACTIVE_WINDOW` was returned and parsed correctly but the
    /// value parsed as an empty `Window` id. The probe stayed at the
    /// no-window-no-class state and returned `Ok(None)`.
    ActiveWindowEmpty,
    /// `_NET_ACTIVE_WINDOW` resolved to a valid window but reading
    /// `WM_CLASS` failed (X11 error or `AnyPropertyType` fallback did
    /// not match). The fallback to `_NET_WM_NAME` was either skipped
    /// because the reply was empty or was used as a last resort.
    WmClassMissing,
    /// `WM_CLASS` was read but the produced identifier (after parsing
    /// the `instance\0class` payload) was empty.
    IdentifierEmpty,
    /// The probe resolved an identifier.
    Identified,
    /// The platform returned a typed "unavailable" error.
    Unavailable,
    /// The platform returned a typed backend error.
    Backend,
}

impl ProbeStage {
    pub fn as_str(self) -> &'static str {
        match self {
            ProbeStage::NotApplicable => "not_applicable",
            ProbeStage::Started => "started",
            ProbeStage::ActiveWindowMissing => "active_window_missing",
            ProbeStage::ActiveWindowEmpty => "active_window_empty",
            ProbeStage::WmClassMissing => "wm_class_missing",
            ProbeStage::IdentifierEmpty => "identifier_empty",
            ProbeStage::Identified => "identified",
            ProbeStage::Unavailable => "unavailable",
            ProbeStage::Backend => "backend",
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

    /// Snapshot of the granular probe stage reached on the most
    /// recent call. The default `ProbeStage::NotApplicable` mirrors
    /// the macOS / no-op probes that do not granularise their
    /// outcome; the Linux X11/XWayland probe overrides the helper so
    /// the diagnostics endpoint can distinguish "X11 probe ran but
    /// did not see a window" from "WM_CLASS was undeclared on the
    /// focused window". Returning the same stage as
    /// `Err(ActiveAppError::Backend { .. })` would defeat the
    /// purpose — probes that surface a backend error must also
    /// surface the stage explicitly via [`Self::last_probe_stage`].
    fn last_probe_stage(&self) -> ProbeStage {
        ProbeStage::NotApplicable
    }
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

    /// Granular probe stage the wrapped probe reached on its most
    /// recent call. The default is `NotApplicable`; the Linux X11 /
    /// XWayland probe overrides the underlying probe so the value
    /// reflects the granular `_NET_ACTIVE_WINDOW` → `WM_CLASS`
    /// stages documented by the design. Returns the snapshot value
    /// without re-invoking the inner probe.
    pub fn last_probe_stage(&self) -> ProbeStage {
        self.inner.last_probe_stage()
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

    fn last_probe_stage(&self) -> ProbeStage {
        self.inner.last_probe_stage()
    }
}

/// Decode the bytes of an X11 `GetPropertyReply` for
/// `_NET_ACTIVE_WINDOW`. Returns the parsed 32-bit window id when the
/// reply's `format` is `32` and the value carries at least four
/// bytes; `None` for every other shape.
///
/// The helper lives in the always-compiled module instead of the
/// `linux-x11`-gated runtime module so it can be exercised by unit
/// tests on hosts where the X11 probe is never compiled in (notably
/// the macOS dev host). The original implementation read
/// `reply.value.first()` and converted the resulting `u8` to a
/// `Window`, which discarded the upper three bytes and corrupted the
/// window id — the regression that motivated the patch the
/// `linux-source-app-metadata` change ships after the original
/// `with_kind` → `connect_to_kind` and capture-loop cache refresh
/// fixes.
pub fn parse_active_window_id(format: u8, value: &[u8]) -> Option<u32> {
    if format != 32 {
        return None;
    }
    let bytes: [u8; 4] = value.get(..4)?.try_into().ok()?;
    Some(u32::from_ne_bytes(bytes))
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
    fn probe_stage_strings_are_stable() {
        assert_eq!(ProbeStage::NotApplicable.as_str(), "not_applicable");
        assert_eq!(ProbeStage::Started.as_str(), "started");
        assert_eq!(
            ProbeStage::ActiveWindowMissing.as_str(),
            "active_window_missing"
        );
        assert_eq!(
            ProbeStage::ActiveWindowEmpty.as_str(),
            "active_window_empty"
        );
        assert_eq!(ProbeStage::WmClassMissing.as_str(), "wm_class_missing");
        assert_eq!(ProbeStage::IdentifierEmpty.as_str(), "identifier_empty");
        assert_eq!(ProbeStage::Identified.as_str(), "identified");
        assert_eq!(ProbeStage::Unavailable.as_str(), "unavailable");
        assert_eq!(ProbeStage::Backend.as_str(), "backend");
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

    /// Reusable scripted probe that exposes a programmatic
    /// `last_probe_stage` so the tests below can confirm the
    /// `CachedActiveApplication` wrapper surfaces the inner probe's
    /// granular stage instead of always returning `NotApplicable`.
    struct ScriptedStageProbe {
        stage: Mutex<ProbeStage>,
    }

    impl ScriptedStageProbe {
        fn new(stage: ProbeStage) -> Self {
            Self {
                stage: Mutex::new(stage),
            }
        }
        fn set_stage(&self, stage: ProbeStage) {
            *self.stage.lock() = stage;
        }
    }

    impl ActiveApplicationProbe for ScriptedStageProbe {
        fn active_application(&self) -> Result<Option<ActiveApplication>, ActiveAppError> {
            Ok(None)
        }
        fn name(&self) -> &'static str {
            "scripted_stage"
        }
        fn last_probe_stage(&self) -> ProbeStage {
            *self.stage.lock()
        }
    }

    #[test]
    fn cached_probe_surfaces_inner_probe_stage() {
        // The diagnostics surface relies on the wrapper forwarding
        // `last_probe_stage` so it can report the granular Linux
        // stage instead of the unspecified macOS fallback. The
        // default probe returns `NotApplicable`; the X11 probe
        // overrides it. Pin the contract here so a future refactor
        // that drops the forwarding breaks the test instead of the
        // production diagnostics card.
        let inner = Arc::new(ScriptedStageProbe::new(ProbeStage::Identified));
        let cached = CachedActiveApplication::new(inner.clone());
        assert_eq!(cached.last_probe_stage(), ProbeStage::Identified);
        inner.set_stage(ProbeStage::ActiveWindowMissing);
        assert_eq!(cached.last_probe_stage(), ProbeStage::ActiveWindowMissing);
        assert_eq!(
            <CachedActiveApplication as ActiveApplicationProbe>::last_probe_stage(&cached),
            ProbeStage::ActiveWindowMissing,
            "trait method dispatch must reach the inner probe"
        );
    }
}

/// Tests for the always-compiled `parse_active_window_id` helper.
/// Runs on every target so the regression the `linux-source-app-metadata`
/// patch fixes is pinned independently of the X11-gated probe module.
#[cfg(test)]
mod window_id_tests {
    use super::parse_active_window_id;

    #[test]
    fn decodes_format_32_window_id_with_full_byte_width() {
        // The regression the patch fixes: a real `_NET_ACTIVE_WINDOW`
        // reply returns `format == 32` with four bytes that encode
        // the X11 window id in native endianness. The legacy code
        // read only the first byte (the LSB) and converted it to a
        // `Window`, discarding 24 bits. The dev.warp.Warp window id
        // we confirmed with `xprop -root _NET_ACTIVE_WINDOW` is
        // something like `0x01aabbcc`; reading only the first byte
        // would resolve to `0xcc`, an entirely different window.
        // The helper pins the four-byte, native-endian decoding.
        let bytes: [u8; 4] = [0xcc, 0xbb, 0xaa, 0x01];
        let id = parse_active_window_id(32, &bytes).expect("non-empty");
        assert_eq!(id, u32::from_ne_bytes(bytes));
        // On little-endian hosts (every Linux desktop the
        // `linux-source-app-metadata` change targets) the value is
        // exactly `0x01aabbcc`; on a hypothetical big-endian host
        // the byte order would be reversed and the helper would
        // round-trip through the same native-endian conversion
        // because the X protocol uses the server's endianness.
        let expected = u32::from_ne_bytes([0xcc, 0xbb, 0xaa, 0x01]);
        assert_eq!(id, expected);
    }

    #[test]
    fn rejects_non_format_32_reply() {
        // A reply that does not match the documented `WINDOW` type
        // must be refused without panicking, exactly like the
        // `value32()` iterator. The `format == 8` shape the X server
        // sends for `STRING` / `UTF8_STRING` properties is the
        // canonical example the helper must reject.
        assert!(parse_active_window_id(8, b"abcd").is_none());
        assert!(parse_active_window_id(16, &[0, 1, 0, 0]).is_none());
        assert!(parse_active_window_id(0, &[0, 0, 0, 0]).is_none());
    }

    #[test]
    fn rejects_short_value() {
        // The X server may return a partial reply when the value
        // fits inside the long_offset / long_length we requested. The
        // helper must refuse anything shorter than four bytes rather
        // than reading uninitialised memory through a bogus cast.
        assert!(parse_active_window_id(32, &[]).is_none());
        assert!(parse_active_window_id(32, &[0xaa]).is_none());
        assert!(parse_active_window_id(32, &[0xaa, 0xbb, 0xcc]).is_none());
    }

    #[test]
    fn first_byte_only_is_not_what_get_property_returns() {
        // Compile-time pin that the helper does not silently do what
        // the legacy implementation did. The very first byte of
        // `value` is the LSB of the window id on little-endian
        // systems; for a window id `0x01aabbcc` that byte is
        // `0xcc`. Reading only the first byte would round-trip to a
        // `Window::from(u8)` of `0xcc`, never to the real id. The
        // helper MUST return the full 32-bit value, not just the
        // first byte.
        let bytes: [u8; 4] = [0xcc, 0xbb, 0xaa, 0x01];
        let only_first_byte = bytes[0] as u32;
        let parsed = parse_active_window_id(32, &bytes).expect("non-empty");
        assert_ne!(
            parsed, only_first_byte,
            "parse_active_window_id must read all four bytes, not just the first"
        );
        assert_eq!(parsed, u32::from_ne_bytes(bytes));
    }
}
