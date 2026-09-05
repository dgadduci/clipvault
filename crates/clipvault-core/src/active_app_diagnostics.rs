//! Metadata-only diagnostics for the active-application cache.
//!
//! The privacy-settings change keeps the blacklist gated by the cached
//! active-app probe so the background capture loop can evaluate events
//! without ever invoking `NSWorkspace` off the main thread on macOS.
//! That split is correct in the abstract, but it also means the cache
//! can be empty, stale or report ClipVault itself when the user focuses
//! the main window. The user has no way to diagnose that without
//! metadata from the backend.
//!
//! This module exposes a typed, metadata-only summary of the cache
//! state plus the outcome of the most recent synchronous refresh the
//! capture loop performed. The summary never carries clipboard
//! content, content hashes, snippets or source identifiers from past
//! captures — it only reports what the platform layer is reporting
//! right now and the lifecycle counters the shell increments around
//! every refresh attempt.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use parking_lot::RwLock;
use serde::Serialize;

use clipvault_platform::{
    ActiveAppBackendKind, ActiveAppError, ActiveApplication, CachedActiveApplication,
};

/// Outcome of the most recent refresh attempt. The shell reports this
/// value to the user so a failed refresh does not silently degrade the
/// blacklist into the "unknown source" path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActiveAppRefreshOutcome {
    /// No refresh has happened yet (the cache has never been populated).
    Pending,
    /// The last refresh succeeded. `identifier` carries the value the
    /// matcher will consult on the next capture.
    Ok,
    /// The last refresh failed. `failure_kind` distinguishes the
    /// precise failure cause so the UI can surface actionable context
    /// (e.g. "main thread did not pick up the closure" vs "probe
    /// unavailable on this platform" vs "probe returned a backend
    /// error"). `message` is sanitised so it never contains
    /// clipboard content.
    Failed {
        failure_kind: ActiveAppFailureKind,
        message: String,
    },
}

impl ActiveAppRefreshOutcome {
    pub fn kind(&self) -> &'static str {
        match self {
            ActiveAppRefreshOutcome::Pending => "pending",
            ActiveAppRefreshOutcome::Ok => "ok",
            ActiveAppRefreshOutcome::Failed { .. } => "failed",
        }
    }
}

/// Concrete reason a refresh attempt failed. The diagnostics surface
/// exposes this enum so the user can tell apart "Tauri refused to
/// schedule the closure" from "the closure ran but the platform probe
/// returned an error" without parsing free-form strings. Every variant
/// maps to a documented remediation in the manual flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActiveAppFailureKind {
    /// Tauri refused to enqueue the closure (event loop shut down or
    /// proxy channel disconnected).
    Schedule,
    /// The closure was enqueued but the main thread did not pick it up
    /// inside the synchronous timeout.
    Timeout,
    /// The platform probe cannot run on the current thread (typically
    /// because Apple requires `NSWorkspace` to be called on the main
    /// thread and the closure reached a non-main executor).
    Unavailable,
    /// The platform probe returned a backend error (X11 disconnect,
    /// NSWorkspace exception, etc.). The `message` field carries the
    /// sanitised reason.
    Backend,
}

impl ActiveAppFailureKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ActiveAppFailureKind::Schedule => "schedule",
            ActiveAppFailureKind::Timeout => "timeout",
            ActiveAppFailureKind::Unavailable => "unavailable",
            ActiveAppFailureKind::Backend => "backend",
        }
    }
}

/// Metadata-only snapshot of the cached probe.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ActiveAppDiagnostics {
    /// Whether the bootstrap wired a real adapter (true) or the no-op
    /// fallback (false). Mirrors the `capabilities.active_application`
    /// flag the diagnostics endpoint already exposes.
    pub available: bool,
    /// Backend name as a stable identifier (`"macos_workspace"`,
    /// `"x11_ewmh"`, `"unavailable"`). Mirrors
    /// [`ActiveAppBackendKind::as_str`].
    pub backend: &'static str,
    /// Whether the cache currently holds a non-empty identifier. A
    /// `false` value means the probe has never been refreshed (Pending)
    /// or the most recent refresh produced an empty `Some`.
    pub cache_populated: bool,
    /// The identifier the matcher will use for the next capture, when
    /// known. The string is exactly what the platform adapter
    /// returned — the matcher normalises both sides on every
    /// evaluation.
    pub identifier: Option<String>,
    /// Display label reported by the adapter (for example the
    /// application name).
    pub name: Option<String>,
    /// Outcome of the most recent refresh attempt.
    pub refresh_outcome: ActiveAppRefreshOutcome,
    /// Concrete reason the most recent refresh failed, when applicable.
    /// `None` for `pending` and `ok` outcomes. Lets the UI tell apart
    /// `failed:schedule` from `failed:timeout` from `failed:unavailable`
    /// from `failed:backend` without parsing the human-readable
    /// `message`. The values mirror [`ActiveAppFailureKind::as_str`].
    /// Skipped from the serialised JSON when no failure happened so
    /// the frontend can rely on a truthy check before rendering the
    /// failure card.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_kind: Option<&'static str>,
    /// True once `install_capture_loop` has spun up its background
    /// thread. Lets the user distinguish "the loop never started"
    /// from "the cache is empty because no application has been
    /// focused yet". The flag flips to `true` exactly once.
    pub loop_started: bool,
    /// True once the shell installed the macOS dispatch main-queue
    /// refresher and the timer is alive. On non-macOS hosts this
    /// stays `false` because the platform only ships a periodic
    /// refresher on macOS. The flag is part of the metadata-only
    /// surface so the user can tell apart "no cache updates" from
    /// "no periodic refresher at all".
    pub refresher_installed: bool,
    /// Number of times the macOS dispatch main-queue timer fired its
    /// callback. Increments once per fired callback regardless of
    /// the probe outcome so the diagnostics surface can confirm the
    /// timer itself is alive even when every refresh has failed.
    /// `0` on non-macOS hosts and on hosts where the periodic
    /// refresher could not be installed.
    pub timer_callback_count: u64,
    /// Number of refresh attempts the shell has dispatched since
    /// boot. Every attempt — successful or not — increments the
    /// counter so a stuck loop is visible in the diagnostics.
    pub refresh_attempts: u64,
    /// Successful refreshes (the inner probe answered with a
    /// non-error value, whether or not the identifier was empty).
    pub successful_refreshes: u64,
    /// Failed refreshes (the inner probe raised `ActiveAppError`,
    /// the shell raised `MainThreadSyncError`, or Tauri refused to
    /// schedule the closure). Counts every failure independently
    /// from `successful_refreshes` so a flaky adapter is easy to
    /// spot.
    pub failed_refreshes: u64,
    /// Unix-millisecond timestamp of the most recent refresh
    /// attempt, regardless of outcome. `None` until the first
    /// refresh happens.
    pub last_refresh_unix_ms: Option<i64>,
    /// Metadata-only label describing the most recent capture
    /// outcome the loop evaluated (for example
    /// `"allowed"`, `"discarded:blacklisted"`, `"unchanged"`).
    /// Never carries the clipboard content, snippet or hash.
    pub last_capture_decision: Option<String>,
}

impl ActiveAppDiagnostics {
    /// Build the "no adapter wired" snapshot. The helper centralises
    /// the defaults so the shell and the tests can compare against a
    /// single, documented baseline.
    pub fn unavailable(backend_kind: ActiveAppBackendKind) -> Self {
        Self {
            available: false,
            backend: backend_kind.as_str(),
            cache_populated: false,
            identifier: None,
            name: None,
            refresh_outcome: ActiveAppRefreshOutcome::Pending,
            failure_kind: None,
            loop_started: false,
            refresher_installed: false,
            timer_callback_count: 0,
            refresh_attempts: 0,
            successful_refreshes: 0,
            failed_refreshes: 0,
            last_refresh_unix_ms: None,
            last_capture_decision: None,
        }
    }
}

/// Shared handle on the diagnostics state. The bootstrap installs one
/// instance on `AppContext`; the capture loop records every refresh
/// outcome here and the frontend reads it through
/// [`AppContext::active_app_diagnostics`].
#[derive(Clone)]
pub struct ActiveAppDiagnosticsState {
    inner: Arc<RwLock<ActiveAppDiagnosticsStateInner>>,
}

struct ActiveAppDiagnosticsStateInner {
    /// Whether the active-app adapter was wired in at all.
    available: bool,
    /// Adapter name (stable string).
    backend: &'static str,
    /// Cached probe the diagnostics report mirrors.
    probe: Option<CachedActiveApplication>,
    /// Outcome of the most recent refresh the shell recorded. The
    /// snapshot keeps this value so the frontend always observes the
    /// last refresh outcome, even after a long pause.
    last_outcome: ActiveAppRefreshOutcome,
    /// True once `install_capture_loop` flipped the flag. Distinct
    /// from "the cache is populated" so the user can tell "loop is
    /// alive but cache is empty" from "loop never started".
    loop_started: bool,
    /// True once the shell installed the macOS dispatch main-queue
    /// refresher. Distinct from `loop_started` so the dashboard can
    /// distinguish "background loop is alive but no periodic
    /// refresher" from "periodic refresher is alive but loop never
    /// started". Sticky once flipped.
    refresher_installed: bool,
    /// Total timer callbacks the periodic refresher fired since
    /// boot. Monotonic.
    timer_callback_count: u64,
    /// Counters maintained by the shell. Monotonic.
    refresh_attempts: u64,
    successful_refreshes: u64,
    failed_refreshes: u64,
    /// Unix-millisecond timestamp of the most recent refresh
    /// attempt; `None` until the first attempt.
    last_refresh_unix_ms: Option<i64>,
    /// Metadata-only label of the most recent capture outcome the
    /// loop evaluated. The shell sets it via
    /// [`ActiveAppDiagnosticsState::record_capture_decision`].
    last_capture_decision: Option<String>,
}

impl ActiveAppDiagnosticsState {
    /// Build the state from the platform adapters. When the
    /// capabilities report `active_application = false` the state
    /// reports [`ActiveAppBackendKind::Unavailable`] and never
    /// refreshes.
    pub fn new(
        available: bool,
        backend: ActiveAppBackendKind,
        probe: Option<CachedActiveApplication>,
    ) -> Self {
        Self {
            inner: Arc::new(RwLock::new(ActiveAppDiagnosticsStateInner {
                available,
                backend: backend.as_str(),
                probe,
                last_outcome: ActiveAppRefreshOutcome::Pending,
                loop_started: false,
                refresher_installed: false,
                timer_callback_count: 0,
                refresh_attempts: 0,
                successful_refreshes: 0,
                failed_refreshes: 0,
                last_refresh_unix_ms: None,
                last_capture_decision: None,
            })),
        }
    }

    /// Mark the capture loop as started. The flag is sticky: once
    /// flipped to `true` it never goes back to `false`, so the user
    /// can distinguish a loop that never spawned from a loop that is
    /// alive but has not seen an active application yet.
    pub fn mark_loop_started(&self) {
        let mut guard = self.inner.write();
        guard.loop_started = true;
    }

    /// Mark the macOS main-queue periodic refresher as installed.
    /// The flag is sticky: once flipped to `true` it never goes back
    /// to `false`, so the user can distinguish a host where the
    /// periodic refresher was never installed (non-macOS, native
    /// dependency missing, libdispatch unavailable) from a host where
    /// the timer is alive.
    pub fn mark_refresher_installed(&self) {
        let mut guard = self.inner.write();
        guard.refresher_installed = true;
    }

    /// Increment the timer-callback counter by one. The shell calls
    /// this from inside the periodic-refresh callback so every fired
    /// timer event is observable independently from the probe
    /// outcome (`Ok` / `Unavailable` / `Backend`). On hosts without a
    /// periodic refresher the counter stays at `0`.
    pub fn record_timer_callback(&self) {
        let mut guard = self.inner.write();
        guard.timer_callback_count = guard.timer_callback_count.saturating_add(1);
    }

    /// Record a successful refresh. Stores the fresh
    /// [`ActiveApplication`] returned by the inner probe on the
    /// underlying cache, then mirrors it into the diagnostics state.
    pub fn record_refresh(&self, fresh: Option<ActiveApplication>) -> ActiveAppDiagnostics {
        let snapshot = {
            let mut guard = self.inner.write();
            if let Some(probe) = guard.probe.as_ref() {
                probe.refresh_with(fresh.clone());
            }
            guard.refresh_attempts = guard.refresh_attempts.saturating_add(1);
            guard.successful_refreshes = guard.successful_refreshes.saturating_add(1);
            guard.last_refresh_unix_ms = Some(unix_millis_now());
            let diag = ActiveAppDiagnostics {
                available: guard.available,
                backend: guard.backend,
                cache_populated: matches!(fresh, Some(ref app) if !app.identifier.is_empty()),
                identifier: fresh.as_ref().and_then(|app| {
                    if app.identifier.is_empty() {
                        None
                    } else {
                        Some(app.identifier.clone())
                    }
                }),
                name: fresh.as_ref().and_then(|app| {
                    if app.name.is_empty() {
                        None
                    } else {
                        Some(app.name.clone())
                    }
                }),
                refresh_outcome: ActiveAppRefreshOutcome::Ok,
                failure_kind: None,
                loop_started: guard.loop_started,
                refresher_installed: guard.refresher_installed,
                timer_callback_count: guard.timer_callback_count,
                refresh_attempts: guard.refresh_attempts,
                successful_refreshes: guard.successful_refreshes,
                failed_refreshes: guard.failed_refreshes,
                last_refresh_unix_ms: guard.last_refresh_unix_ms,
                last_capture_decision: guard.last_capture_decision.clone(),
            };
            guard.last_outcome = diag.refresh_outcome.clone();
            diag
        };
        snapshot
    }

    /// Record a refresh failure. The cache and the diagnostics state
    /// stay at their previous value so the next capture can still
    /// match (or fail open under the "unknown source" contract).
    pub fn record_failure(&self, error: &ActiveAppError) -> ActiveAppDiagnostics {
        let kind = failure_kind_for(error);
        let message = sanitize_error(error);
        self.record_failure_with_kind(kind, message)
    }

    /// Record a refresh failure with an explicit
    /// [`ActiveAppFailureKind`]. The shell uses this overload to
    /// surface `MainThreadSyncError::Schedule` and `Timeout` outcomes
    /// that originate from Tauri's `run_on_main_thread` plumbing and
    /// never reach the platform probe.
    pub fn record_failure_with_kind(
        &self,
        kind: ActiveAppFailureKind,
        message: String,
    ) -> ActiveAppDiagnostics {
        let mut guard = self.inner.write();
        let cached = guard.probe.as_ref().and_then(|probe| probe.cached());
        let (cache_populated, identifier, name) = match cached {
            Some(app) if !app.identifier.is_empty() => (true, Some(app.identifier), Some(app.name)),
            _ => (false, None, None),
        };
        guard.refresh_attempts = guard.refresh_attempts.saturating_add(1);
        guard.failed_refreshes = guard.failed_refreshes.saturating_add(1);
        guard.last_refresh_unix_ms = Some(unix_millis_now());
        let diag = ActiveAppDiagnostics {
            available: guard.available,
            backend: guard.backend,
            cache_populated,
            identifier,
            name,
            refresh_outcome: ActiveAppRefreshOutcome::Failed {
                failure_kind: kind,
                message,
            },
            failure_kind: Some(kind.as_str()),
            loop_started: guard.loop_started,
            refresher_installed: guard.refresher_installed,
            timer_callback_count: guard.timer_callback_count,
            refresh_attempts: guard.refresh_attempts,
            successful_refreshes: guard.successful_refreshes,
            failed_refreshes: guard.failed_refreshes,
            last_refresh_unix_ms: guard.last_refresh_unix_ms,
            last_capture_decision: guard.last_capture_decision.clone(),
        };
        guard.last_outcome = diag.refresh_outcome.clone();
        diag
    }

    /// Record the outcome category of the most recent capture tick.
    /// The label MUST be metadata-only (no clipboard content, no
    /// snippet, no hash). The shell uses this to make the "the loop
    /// decided to discard your last copy" case visible in the
    /// diagnostics card without leaking the discarded value.
    pub fn record_capture_decision(&self, decision: &str) {
        let trimmed = decision.trim();
        if trimmed.is_empty() {
            return;
        }
        // Cap the label defensively; the diagnostics endpoint is
        // metadata-only and never carries user data.
        let capped = if trimmed.len() > 80 {
            &trimmed[..80]
        } else {
            trimmed
        };
        let mut guard = self.inner.write();
        guard.last_capture_decision = Some(capped.to_string());
    }

    /// Snapshot the diagnostics state without performing a refresh.
    pub fn snapshot(&self) -> ActiveAppDiagnostics {
        let guard = self.inner.read();
        if !guard.available {
            // Preserve the lifecycle counters (loop_started /
            // refresh_attempts / last_refresh_unix_ms /
            // last_capture_decision) even when the adapter is not
            // wired, so the user can still observe the loop's state.
            return ActiveAppDiagnostics {
                available: false,
                backend: guard.backend,
                cache_populated: false,
                identifier: None,
                name: None,
                refresh_outcome: guard.last_outcome.clone(),
                failure_kind: failure_kind_str(&guard.last_outcome),
                loop_started: guard.loop_started,
                refresher_installed: guard.refresher_installed,
                timer_callback_count: guard.timer_callback_count,
                refresh_attempts: guard.refresh_attempts,
                successful_refreshes: guard.successful_refreshes,
                failed_refreshes: guard.failed_refreshes,
                last_refresh_unix_ms: guard.last_refresh_unix_ms,
                last_capture_decision: guard.last_capture_decision.clone(),
            };
        }
        let cached = guard.probe.as_ref().and_then(|probe| probe.cached());
        let (cache_populated, identifier, name) = match cached {
            Some(app) if !app.identifier.is_empty() => (true, Some(app.identifier), Some(app.name)),
            _ => (false, None, None),
        };
        ActiveAppDiagnostics {
            available: guard.available,
            backend: guard.backend,
            cache_populated,
            identifier,
            name,
            refresh_outcome: guard.last_outcome.clone(),
            failure_kind: failure_kind_str(&guard.last_outcome),
            loop_started: guard.loop_started,
            refresher_installed: guard.refresher_installed,
            timer_callback_count: guard.timer_callback_count,
            refresh_attempts: guard.refresh_attempts,
            successful_refreshes: guard.successful_refreshes,
            failed_refreshes: guard.failed_refreshes,
            last_refresh_unix_ms: guard.last_refresh_unix_ms,
            last_capture_decision: guard.last_capture_decision.clone(),
        }
    }
}

/// Reduce an [`ActiveAppError`] into a short, sanitised string. The
/// platform adapter already strips sensitive payload, but the
/// diagnostics endpoint is metadata-only and never carries clipboard
/// content; this helper ensures the schema is also free of source-app
/// identifiers (which the matcher stores separately).
fn sanitize_error(error: &ActiveAppError) -> String {
    match error {
        ActiveAppError::Unavailable => "active-app probe unavailable on this session".to_string(),
        ActiveAppError::Backend { details } => {
            // The platform layer guarantees `details` never contains
            // clipboard content; we cap the length defensively so a
            // pathological adapter cannot flood the diagnostics
            // surface.
            let trimmed = details.trim();
            if trimmed.len() > 200 {
                format!("{}…", &trimmed[..200])
            } else {
                trimmed.to_string()
            }
        }
    }
}

/// Map an [`ActiveAppError`] to the structured
/// [`ActiveAppFailureKind`] the diagnostics endpoint exposes. Keeping
/// the mapping in one place ensures the UI and the tests agree on the
/// taxonomy.
fn failure_kind_for(error: &ActiveAppError) -> ActiveAppFailureKind {
    match error {
        ActiveAppError::Unavailable => ActiveAppFailureKind::Unavailable,
        ActiveAppError::Backend { .. } => ActiveAppFailureKind::Backend,
    }
}

/// Resolve the `failure_kind` field for a snapshot that mirrors the
/// last recorded outcome. Returns `None` for `Pending` and `Ok`.
fn failure_kind_str(outcome: &ActiveAppRefreshOutcome) -> Option<&'static str> {
    match outcome {
        ActiveAppRefreshOutcome::Failed { failure_kind, .. } => Some(failure_kind.as_str()),
        ActiveAppRefreshOutcome::Pending | ActiveAppRefreshOutcome::Ok => None,
    }
}

/// Unix epoch in milliseconds. `None` if the system clock is
/// unavailable (extremely rare; tests can ignore it because the
/// counter still increments). The value is metadata-only; the
/// diagnostics card uses it to confirm the loop is alive without
/// carrying any clipboard payload.
fn unix_millis_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clipvault_platform::{ActiveAppError, ActiveApplication, CachedActiveApplication};
    use std::sync::Arc;

    fn script_probe_with(value: &'static str) -> Arc<CachedActiveApplication> {
        struct Scripted(&'static str);
        impl clipvault_platform::ActiveApplicationProbe for Scripted {
            fn active_application(&self) -> Result<Option<ActiveApplication>, ActiveAppError> {
                Ok(Some(ActiveApplication::new("App", self.0)))
            }
            fn name(&self) -> &'static str {
                "scripted"
            }
        }
        Arc::new(CachedActiveApplication::new(Arc::new(Scripted(value))))
    }

    #[test]
    fn snapshot_reports_pending_until_first_refresh() {
        let probe = script_probe_with("firefox");
        let state = ActiveAppDiagnosticsState::new(
            true,
            ActiveAppBackendKind::MacOsWorkspace,
            Some((*probe).clone()),
        );
        let diag = state.snapshot();
        assert!(diag.available);
        assert_eq!(diag.backend, "macos_workspace");
        assert!(!diag.cache_populated);
        assert_eq!(diag.refresh_outcome, ActiveAppRefreshOutcome::Pending);
        assert!(
            diag.failure_kind.is_none(),
            "Pending outcome must not surface a failure_kind"
        );
        assert!(
            !diag.loop_started,
            "loop_started is false until the shell flips it"
        );
        assert_eq!(diag.refresh_attempts, 0);
        assert_eq!(diag.successful_refreshes, 0);
        assert_eq!(diag.failed_refreshes, 0);
        assert!(diag.last_refresh_unix_ms.is_none());
        assert!(diag.last_capture_decision.is_none());
    }

    #[test]
    fn record_refresh_populates_cache_and_outcome() {
        let probe = script_probe_with("1password");
        let state = ActiveAppDiagnosticsState::new(
            true,
            ActiveAppBackendKind::MacOsWorkspace,
            Some((*probe).clone()),
        );
        let diag = state.record_refresh(Some(ActiveApplication::new("1Password", "1password")));
        assert!(diag.cache_populated);
        assert_eq!(diag.identifier.as_deref(), Some("1password"));
        assert_eq!(diag.name.as_deref(), Some("1Password"));
        assert_eq!(diag.refresh_outcome, ActiveAppRefreshOutcome::Ok);
        assert!(
            diag.failure_kind.is_none(),
            "Ok outcome must not surface a failure_kind"
        );
        assert_eq!(diag.refresh_attempts, 1);
        assert_eq!(diag.successful_refreshes, 1);
        assert_eq!(diag.failed_refreshes, 0);
        assert!(diag.last_refresh_unix_ms.is_some());
    }

    #[test]
    fn record_failure_keeps_previous_cache_value() {
        let probe = script_probe_with("safari");
        let state = ActiveAppDiagnosticsState::new(
            true,
            ActiveAppBackendKind::X11Ewmh,
            Some((*probe).clone()),
        );
        state.record_refresh(Some(ActiveApplication::new("Safari", "safari")));
        let diag = state.record_failure(&ActiveAppError::Backend {
            details: "x11 disconnected".to_string(),
        });
        assert!(diag.cache_populated, "previous value must remain visible");
        assert_eq!(diag.identifier.as_deref(), Some("safari"));
        match &diag.refresh_outcome {
            ActiveAppRefreshOutcome::Failed {
                failure_kind,
                message,
            } => {
                assert_eq!(*failure_kind, ActiveAppFailureKind::Backend);
                assert!(message.contains("x11 disconnected"));
            }
            other => panic!("expected Failed, got {other:?}"),
        }
        assert_eq!(diag.failure_kind, Some("backend"));
        assert_eq!(diag.refresh_attempts, 2);
        assert_eq!(diag.successful_refreshes, 1);
        assert_eq!(diag.failed_refreshes, 1);
        assert!(diag.last_refresh_unix_ms.is_some());
    }

    #[test]
    fn unavailable_state_never_reports_a_cache() {
        let state = ActiveAppDiagnosticsState::new(false, ActiveAppBackendKind::Unavailable, None);
        let diag = state.snapshot();
        assert!(!diag.available);
        assert_eq!(diag.backend, "unavailable");
        assert!(!diag.cache_populated);
        assert!(diag.identifier.is_none());
        assert_eq!(diag.refresh_outcome, ActiveAppRefreshOutcome::Pending);
        assert!(diag.failure_kind.is_none());
    }

    #[test]
    fn failure_kind_distinguishes_unavailable_from_backend() {
        // The four canonical failure causes each map to a stable
        // `failure_kind` string the UI uses to render the cause without
        // parsing free-form text. The diagnostics snapshot must keep
        // this taxonomy even when the underlying error is `Backend`.
        let probe = script_probe_with("1password");
        let state = ActiveAppDiagnosticsState::new(
            true,
            ActiveAppBackendKind::MacOsWorkspace,
            Some((*probe).clone()),
        );
        let diag = state.record_failure(&ActiveAppError::Unavailable);
        assert_eq!(diag.failure_kind, Some("unavailable"));
        let diag = state.record_failure_with_kind(
            ActiveAppFailureKind::Schedule,
            "main thread did not accept the closure".to_string(),
        );
        assert_eq!(diag.failure_kind, Some("schedule"));
        let diag = state.record_failure_with_kind(
            ActiveAppFailureKind::Timeout,
            "main thread did not execute the closure in 500ms".to_string(),
        );
        assert_eq!(diag.failure_kind, Some("timeout"));
        assert_eq!(diag.refresh_attempts, 3);
        assert_eq!(diag.failed_refreshes, 3);
        assert_eq!(diag.successful_refreshes, 0);
    }

    #[test]
    fn mark_loop_started_is_sticky() {
        let probe = script_probe_with("safari");
        let state = ActiveAppDiagnosticsState::new(
            true,
            ActiveAppBackendKind::MacOsWorkspace,
            Some((*probe).clone()),
        );
        assert!(!state.snapshot().loop_started);
        state.mark_loop_started();
        assert!(state.snapshot().loop_started);
        // Snapshotting again does not clear the flag.
        let _ = state.snapshot();
        assert!(state.snapshot().loop_started);
    }

    #[test]
    fn record_capture_decision_is_metadata_only() {
        let probe = script_probe_with("safari");
        let state = ActiveAppDiagnosticsState::new(
            true,
            ActiveAppBackendKind::MacOsWorkspace,
            Some((*probe).clone()),
        );
        state.record_capture_decision("discarded:blacklisted");
        let snap = state.snapshot();
        assert_eq!(
            snap.last_capture_decision.as_deref(),
            Some("discarded:blacklisted")
        );

        // Empty / whitespace-only inputs are ignored so the field
        // never ends up populated with a meaningless empty string.
        state.record_capture_decision("   ");
        assert_eq!(
            snap.last_capture_decision.as_deref(),
            Some("discarded:blacklisted")
        );
    }

    #[test]
    fn record_capture_decision_caps_oversized_labels() {
        let probe = script_probe_with("safari");
        let state = ActiveAppDiagnosticsState::new(
            true,
            ActiveAppBackendKind::MacOsWorkspace,
            Some((*probe).clone()),
        );
        state.record_capture_decision(&"a".repeat(200));
        let snap = state.snapshot();
        let stored = snap.last_capture_decision.expect("decision stored");
        assert!(
            stored.len() <= 80,
            "the label must be capped, got {} chars",
            stored.len()
        );
    }

    #[test]
    fn refresh_attempts_increment_on_every_outcome() {
        let probe = script_probe_with("safari");
        let state = ActiveAppDiagnosticsState::new(
            true,
            ActiveAppBackendKind::MacOsWorkspace,
            Some((*probe).clone()),
        );
        state.record_refresh(Some(ActiveApplication::new("Safari", "safari")));
        state.record_failure(&ActiveAppError::Unavailable);
        state.record_refresh(Some(ActiveApplication::new("Safari", "safari")));
        let snap = state.snapshot();
        assert_eq!(snap.refresh_attempts, 3);
        assert_eq!(snap.successful_refreshes, 2);
        assert_eq!(snap.failed_refreshes, 1);
    }

    #[test]
    fn mark_refresher_installed_is_sticky() {
        let probe = script_probe_with("safari");
        let state = ActiveAppDiagnosticsState::new(
            true,
            ActiveAppBackendKind::MacOsWorkspace,
            Some((*probe).clone()),
        );
        assert!(!state.snapshot().refresher_installed);
        state.mark_refresher_installed();
        assert!(state.snapshot().refresher_installed);
        // Subsequent snapshots do not clear the flag.
        state.record_refresh(Some(ActiveApplication::new("Safari", "safari")));
        assert!(state.snapshot().refresher_installed);
    }

    #[test]
    fn timer_callback_count_increments_independently_of_outcome() {
        let probe = script_probe_with("safari");
        let state = ActiveAppDiagnosticsState::new(
            true,
            ActiveAppBackendKind::MacOsWorkspace,
            Some((*probe).clone()),
        );
        // Five timer callbacks fire even when only one of them
        // succeeded: the counter reflects activity, not probe
        // success.
        state.record_timer_callback();
        state.record_timer_callback();
        state.record_refresh(Some(ActiveApplication::new("Safari", "safari")));
        state.record_timer_callback();
        state.record_failure(&ActiveAppError::Backend {
            details: "transient".to_string(),
        });
        state.record_timer_callback();
        state.record_timer_callback();
        let snap = state.snapshot();
        assert_eq!(snap.timer_callback_count, 5);
        assert_eq!(snap.successful_refreshes, 1);
        assert_eq!(snap.failed_refreshes, 1);
        assert_eq!(snap.refresh_attempts, 2);
    }

    #[test]
    fn timer_callback_count_starts_at_zero_when_refresher_never_installed() {
        let probe = script_probe_with("safari");
        let state = ActiveAppDiagnosticsState::new(
            true,
            ActiveAppBackendKind::MacOsWorkspace,
            Some((*probe).clone()),
        );
        let snap = state.snapshot();
        assert!(!snap.refresher_installed);
        assert_eq!(snap.timer_callback_count, 0);
    }

    #[test]
    fn unavailable_state_reports_no_refresher_installed() {
        let state = ActiveAppDiagnosticsState::new(false, ActiveAppBackendKind::Unavailable, None);
        let diag = state.snapshot();
        assert!(!diag.refresher_installed);
        assert_eq!(diag.timer_callback_count, 0);
    }

    #[test]
    fn refresh_outcome_serialisation_includes_lifecycle_fields() {
        let probe = script_probe_with("safari");
        let state = ActiveAppDiagnosticsState::new(
            true,
            ActiveAppBackendKind::MacOsWorkspace,
            Some((*probe).clone()),
        );
        state.mark_loop_started();
        state.mark_refresher_installed();
        state.record_timer_callback();
        state.record_refresh(Some(ActiveApplication::new("Safari", "safari")));
        let json = serde_json::to_string(&state.snapshot()).unwrap();
        // The new lifecycle fields must surface in the JSON payload
        // so the frontend can render the dashboard without extra
        // round-trips.
        assert!(json.contains("\"refresher_installed\":true"));
        assert!(json.contains("\"timer_callback_count\":1"));
        assert!(json.contains("\"loop_started\":true"));
    }
}
