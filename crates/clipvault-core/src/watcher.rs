//! Polling-based clipboard watcher.
//!
//! `arboard` and most platform clipboard APIs do not expose native
//! change events in a portable way. ClipVault falls back to polling at
//! a configurable interval and compares the hash of the latest read
//! against the previous one. The watcher never panics: every
//! clipboard error becomes a [`WatchTickOutcome`] variant so the
//! caller can react without inspecting the adapter.
//!
//! The watcher is `Clone` and shares its dedupe state (`last_hash`,
//! `interval`) across every clone. The shell relies on this so the
//! background capture loop and the manual `Tick capture` command
//! operate on the same dedupe history — when the loop discards a
//! blacklisted payload the manual tick sees the same hash and returns
//! `Unchanged` instead of re-running the `PrivacyGate` with a stale
//! source-application snapshot.
//!
//! ## Opt-in capture debug instrumentation
//!
//! When the bootstrap installs a non-disabled
//! [`crate::capture_diagnostic::CaptureDebugSinkHandle`] the watcher
//! threads a monotonic [`CorrelationId`] through every event the
//! `linux-source-app-metadata` instrumentation exposes:
//!
//! - [`AttemptSnapshot`] describing the tick origin and the cache
//!   state before the refresh;
//! - [`ClipboardSnapshot`] describing the clipboard read outcome
//!   (kind, byte length, mime type — never content);
//! - [`GateSnapshot`] emitted by [`crate::privacy::PrivacyGate`]
//!   after evaluating the resolved identifier;
//! - [`MetadataSnapshot`] emitted by
//!   [`crate::history::TextHistoryService::enrich_metadata`] after
//!   consulting the application-metadata provider;
//! - [`PersistenceSnapshot`] emitted after the SQLite write;
//! - [`OutcomeSnapshot`] emitted at the very end so the user can
//!   `grep` one terminal line per attempt.
//!
//! Every event is metadata-only: clipboard content, snippets,
//! hashes, `asset_ref` values, absolute paths, full window titles
//! and environment-variable values never reach the log stream.

use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use clipvault_platform::{
    ClipboardBackend, ClipboardBackendError, ClipboardImage, ClipboardPayload, RichTextPayload,
};
use parking_lot::Mutex;
use tracing::warn;

use crate::bootstrap::AppContext;
use crate::capture_diagnostic::{
    AttemptOrigin, AttemptSnapshot, ClipboardSnapshot, EnvironmentSnapshot, OutcomeSnapshot,
};
use crate::history::{hash_content, HistoryOutcome};
use crate::paste_suppression::SuppressionFingerprint;

/// Result of a single watcher tick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatchTickOutcome {
    /// The clipboard content changed since the previous tick; the
    /// [`HistoryOutcome`] returned by the capture pipeline is
    /// attached.
    Captured(HistoryOutcome),
    /// The clipboard is unchanged.
    Unchanged,
    /// The clipboard backend reported nothing ClipVault can capture:
    /// no usable text and no supported raster image (a file list,
    /// audio, video, an RTF-only payload, ...). No row was created and
    /// the watcher keeps polling.
    Ignored,
    /// The clipboard changed but the observed payload matches a
    /// suppression token armed by a recent paste. The watcher MUST
    /// NOT create a new row, MUST NOT refresh an existing row and
    /// MUST NOT emit `history-updated`. The variant is internal: the
    /// loop counts it but the user-facing outcome is the same as
    /// `Unchanged` (no card change). The variant is part of the
    /// public surface so tests can assert the suppression path
    /// without parsing free-form strings.
    Suppressed,
    /// The clipboard backend failed. The error message MUST NOT
    /// contain clipboard content.
    Failed { message: String },
}

/// Polling-based clipboard watcher. Cheap to clone: every clone refers
/// to the same [`CaptureWatcherInner`] via [`Arc`], which keeps the
/// dedupe state and the configured interval consistent across callers.
#[derive(Clone)]
pub struct CaptureWatcher {
    inner: Arc<CaptureWatcherInner>,
}

struct CaptureWatcherInner {
    clipboard: Arc<dyn ClipboardBackend>,
    state: Mutex<WatcherState>,
}

#[derive(Debug)]
struct WatcherState {
    last_hash: Option<String>,
    interval: Duration,
}

impl CaptureWatcher {
    /// Build a watcher that polls the given clipboard backend.
    pub fn new(clipboard: Arc<dyn ClipboardBackend>, interval: Duration) -> Self {
        Self {
            inner: Arc::new(CaptureWatcherInner {
                clipboard,
                state: Mutex::new(WatcherState {
                    last_hash: None,
                    interval,
                }),
            }),
        }
    }

    /// Suggested polling interval: 1.5 Hz.
    pub fn default_interval() -> Duration {
        Duration::from_millis(660)
    }

    /// Configured polling interval.
    pub fn interval(&self) -> Duration {
        self.inner.state.lock().interval
    }

    /// Force one tick right now. Returns the outcome.
    ///
    /// The tick reads a [`ClipboardPayload`], which applies the
    /// documented text-first priority: non-empty text always wins, and
    /// an image is only considered when no usable text exists. The
    /// change detection compares a cheap in-memory fingerprint so a
    /// clipboard that keeps holding the same image is not re-encoded on
    /// every poll; the *persistence* dedupe still uses the normalized
    /// PNG hash computed by the capture pipeline.
    ///
    /// The caller passes the [`AttemptOrigin`] (background loop or
    /// manual tick) so the optional capture-debug sink can surface it.
    /// When the configured sink is enabled the watcher emits a
    /// sequence of structured events keyed by a single
    /// [`CorrelationId`]; when the sink is disabled the watcher
    /// behaves exactly as before the instrumentation shipped.
    pub fn tick(
        &self,
        context: &AppContext,
        source_app: Option<&str>,
        origin: AttemptOrigin,
    ) -> WatchTickOutcome {
        let debug_handle = context.capture_debug();
        let debug_enabled = debug_handle.is_enabled();
        let correlation_id = if debug_enabled {
            Some(debug_handle.next_correlation_id())
        } else {
            None
        };
        let started_at = Instant::now();

        // attempt_start: snapshot of the watcher's interval, the cache
        // state before the refresh, and the dedupe history. We also
        // fire the one-shot environment snapshot the very first time
        // a tick runs so the user sees the platform / capabilities
        // matrix even when no capture succeeds.
        if let Some(id) = correlation_id {
            if !context.environment_snapshot_emitted() {
                debug_handle
                    .sink()
                    .environment(id, build_environment_snapshot(context));
                context.mark_environment_snapshot_emitted();
            }
            let interval_ms = {
                let s = self.inner.state.lock();
                s.interval.as_millis() as u64
            };
            let cache_populated_before = context.cached_active_application().is_some();
            let has_previous_value = {
                let s = self.inner.state.lock();
                s.last_hash.is_some()
            };
            let started_at_unix_ms = unix_millis_now();
            let attempt = AttemptSnapshot {
                origin,
                thread: thread::current()
                    .name()
                    .map(str::to_string)
                    .unwrap_or_else(|| "unknown".to_string()),
                shared_watcher_used: true,
                interval_ms,
                cache_populated_before,
                has_previous_value,
                started_at_unix_ms,
            };
            debug_handle.sink().attempt_start(id, attempt);
        }

        let read_started = Instant::now();
        let read_result = self.inner.clipboard.read_payload();
        let read_duration_ms = read_started.elapsed().as_millis() as u64;

        match read_result {
            Ok(Some(payload)) => {
                if let Some(id) = correlation_id {
                    let snapshot = build_clipboard_snapshot(
                        &payload,
                        read_duration_ms,
                        self.inner.clipboard.name(),
                        None,
                    );
                    debug_handle.sink().clipboard_read(id, snapshot);
                }
                let new_hash = payload_fingerprint(&payload);
                let changed = {
                    let mut state = self.inner.state.lock();
                    let previous = state.last_hash.clone();
                    state.last_hash = Some(new_hash.clone());
                    previous.as_ref() != Some(&new_hash)
                };
                if !changed {
                    if let Some(id) = correlation_id {
                        debug_handle.sink().outcome(
                            id,
                            build_outcome_snapshot(context, "unchanged", started_at, None),
                        );
                    }
                    return WatchTickOutcome::Unchanged;
                }
                // Suppression check happens AFTER the dedupe state is
                // updated so the next observation of the same payload
                // — whether it arrives inside or outside the
                // suppression window — is always reported as
                // `Unchanged`. The token is metadata-only: a payload
                // match is decided on canonical hashes, never on
                // text or bytes.
                let fingerprint = suppression_fingerprint_for(&payload);
                if context
                    .paste_suppression()
                    .matches_and_consume(&fingerprint)
                {
                    if let Some(id) = correlation_id {
                        debug_handle.sink().outcome(
                            id,
                            build_outcome_snapshot(context, "suppressed", started_at, None),
                        );
                    }
                    return WatchTickOutcome::Suppressed;
                }
                let history = context.history();
                let outcome = history.record_clipboard_payload_with_correlation(
                    context,
                    payload,
                    source_app,
                    correlation_id,
                );
                if let Some(id) = correlation_id {
                    debug_handle.sink().outcome(
                        id,
                        build_outcome_snapshot(context, outcome.kind(), started_at, None),
                    );
                }
                WatchTickOutcome::Captured(outcome)
            }
            Ok(None) => {
                if let Some(id) = correlation_id {
                    debug_handle.sink().outcome(
                        id,
                        build_outcome_snapshot(
                            context,
                            "ignored_empty_clipboard",
                            started_at,
                            Some("empty"),
                        ),
                    );
                }
                WatchTickOutcome::Ignored
            }
            Err(error) => {
                let kind_static: &'static str = match error.kind_str() {
                    "empty" => "empty",
                    "backend" => "backend",
                    "unavailable" => "unavailable",
                    "unsupported_format" => "unsupported_format",
                    "invalid_image" => "invalid_image",
                    _ => "unknown",
                };
                warn!(kind = error.kind_str(), "capture watcher: read failed");
                let message = error_message(&error);
                if let Some(id) = correlation_id {
                    debug_handle.sink().outcome(
                        id,
                        build_outcome_snapshot(
                            context,
                            "failed_watcher",
                            started_at,
                            Some(kind_static),
                        ),
                    );
                }
                WatchTickOutcome::Failed { message }
            }
        }
    }
}

/// Build a [`ClipboardSnapshot`] from a freshly-read
/// [`ClipboardPayload`]. The helper never inspects text or pixel
/// bytes; it surfaces kind, length, mime and provenance flags
/// only. The optional `error_kind` argument records a typed backend
/// failure so the debug sink can correlate the read with the
/// terminal `outcome` event.
fn build_clipboard_snapshot(
    payload: &ClipboardPayload,
    duration_ms: u64,
    backend_name: &str,
    error_kind: Option<&'static str>,
) -> ClipboardSnapshot {
    let arboard_fallback_used = matches!(backend_name, "arboard");
    let macos_native_used = matches!(backend_name, "macos_pasteboard" | "composite");
    let linux_native_used = false;
    match payload {
        ClipboardPayload::Text(text) => ClipboardSnapshot {
            plain_text_available: !text.is_empty(),
            plain_text_bytes: text.len(),
            image_available: false,
            html_available: false,
            html_bytes: 0,
            rtf_available: false,
            rtf_bytes: 0,
            mime_type: Some("text/plain"),
            content_type: "text",
            image_width: None,
            image_height: None,
            image_mime: None,
            image_bytes: None,
            has_original_png: false,
            arboard_fallback_used,
            macos_native_used,
            linux_native_used,
            duration_ms,
            error_kind,
        },
        ClipboardPayload::Image(image) => build_image_snapshot(
            image,
            duration_ms,
            arboard_fallback_used,
            macos_native_used,
            linux_native_used,
            error_kind,
        ),
        ClipboardPayload::RichText(rich) => build_rich_snapshot(
            rich,
            duration_ms,
            arboard_fallback_used,
            macos_native_used,
            linux_native_used,
            error_kind,
        ),
    }
}

fn build_image_snapshot(
    image: &ClipboardImage,
    duration_ms: u64,
    arboard_fallback_used: bool,
    macos_native_used: bool,
    linux_native_used: bool,
    error_kind: Option<&'static str>,
) -> ClipboardSnapshot {
    let has_original_png = image.has_original_png();
    let bytes = image.byte_len();
    ClipboardSnapshot {
        plain_text_available: false,
        plain_text_bytes: 0,
        image_available: true,
        html_available: false,
        html_bytes: 0,
        rtf_available: false,
        rtf_bytes: 0,
        mime_type: Some("image/png"),
        content_type: "image",
        image_width: Some(image.width()),
        image_height: Some(image.height()),
        image_mime: Some("image/png"),
        image_bytes: Some(bytes as u64),
        has_original_png,
        arboard_fallback_used,
        macos_native_used,
        linux_native_used,
        duration_ms,
        error_kind,
    }
}

fn build_rich_snapshot(
    rich: &RichTextPayload,
    duration_ms: u64,
    arboard_fallback_used: bool,
    macos_native_used: bool,
    linux_native_used: bool,
    error_kind: Option<&'static str>,
) -> ClipboardSnapshot {
    let html = rich.html();
    let rtf = rich.rtf();
    ClipboardSnapshot {
        plain_text_available: true,
        plain_text_bytes: rich.plain_text().len(),
        image_available: false,
        html_available: html.is_some(),
        html_bytes: html.map(|value| value.len()).unwrap_or(0),
        rtf_available: rtf.is_some(),
        rtf_bytes: rtf.map(|bytes| bytes.len()).unwrap_or(0),
        mime_type: Some(if html.is_some() {
            "text/html"
        } else if rtf.is_some() {
            "text/rtf"
        } else {
            "text/plain"
        }),
        content_type: "rich_text",
        image_width: None,
        image_height: None,
        image_mime: None,
        image_bytes: None,
        has_original_png: false,
        arboard_fallback_used,
        macos_native_used,
        linux_native_used,
        duration_ms,
        error_kind,
    }
}

fn build_outcome_snapshot(
    context: &AppContext,
    outcome_label: &'static str,
    started_at: Instant,
    error_kind: Option<&'static str>,
) -> OutcomeSnapshot {
    let snapshot = context.active_app_diagnostics();
    let stage_label = snapshot.last_probe_stage.unwrap_or("not_applicable");
    OutcomeSnapshot {
        content_type: outcome_content_type_for(outcome_label),
        source_identifier_present: context
            .cached_active_application()
            .as_ref()
            .is_some_and(|app| !app.identifier.is_empty()),
        active_app_backend: snapshot.backend,
        probe_stage: Some(stage_label),
        cache_populated: snapshot.cache_populated,
        gate_decision: outcome_label,
        metadata_match: outcome_label.starts_with("allowed:"),
        icon_persisted: false,
        row_persisted: matches!(outcome_label, "stored" | "duplicate"),
        duration_ms: started_at.elapsed().as_millis() as u64,
        error_kind,
    }
}

/// Map the watcher's outcome label back to a content-type slot the
/// `OutcomeSnapshot` exposes. The mapping is conservative on purpose
/// — the watcher is the canonical owner of the per-payload kind,
/// the outcome label does not always carry it.
fn outcome_content_type_for(outcome_label: &str) -> &'static str {
    match outcome_label {
        "failed_watcher" | "ignored_empty_clipboard" => "unknown",
        // The other labels surface a stored / duplicate / suppressed
        // capture; the watcher already logged the full content type
        // on the `clipboard_read` event so the outcome line can stay
        // generic.
        _ => "captured",
    }
}

/// Build the environment snapshot the sink emits once at startup.
/// Mirrors the metadata the existing bootstrap log line produces —
/// never `HOME`, `XDG_RUNTIME_DIR`, `DISPLAY` / `WAYLAND_DISPLAY`
/// values, only their presence flags.
fn build_environment_snapshot(context: &AppContext) -> EnvironmentSnapshot {
    let platform = context.platform();
    let capabilities = context.capabilities();
    let target_arch = std::env::consts::ARCH;
    let display_env_present = std::env::var_os("DISPLAY").is_some();
    let wayland_display_env_present = std::env::var_os("WAYLAND_DISPLAY").is_some();
    let xdg_session_type = detect_xdg_session_type();
    let desktop_environment = detect_desktop_environment();
    EnvironmentSnapshot {
        clipvault_version: context.version(),
        target_arch,
        os: os_family_label(platform.os_family),
        display_server: display_server_label(platform.display_server),
        xdg_session_type,
        display_env_present,
        wayland_display_env_present,
        desktop_environment,
        clipboard_backend: context.platform_adapters().clipboard().name(),
        active_app_backend: context.platform_adapters().active_app().name(),
        capabilities_active_application: capabilities.active_application,
        capabilities_synthetic_paste: capabilities.synthetic_paste,
    }
}

fn detect_xdg_session_type() -> Option<&'static str> {
    match std::env::var("XDG_SESSION_TYPE").as_deref() {
        Ok("wayland") => Some("wayland"),
        Ok("x11") => Some("x11"),
        Ok("tty") => Some("tty"),
        Ok("mir") => Some("mir"),
        Ok("unspecified") => Some("unspecified"),
        Ok(_) => None,
        Err(_) => None,
    }
}

fn detect_desktop_environment() -> Option<&'static str> {
    if std::env::var_os("GNOME_DESKTOP_SESSION_ID").is_some() {
        return Some("gnome");
    }
    if std::env::var_os("KDE_FULL_SESSION").is_some() {
        return Some("kde");
    }
    if std::env::var_os("XDG_CURRENT_DESKTOP").is_some() {
        return Some("xdg_current_desktop");
    }
    None
}

fn unix_millis_now() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

fn os_family_label(family: clipvault_platform::OsFamily) -> &'static str {
    use clipvault_platform::OsFamily;
    match family {
        OsFamily::Macos => "macos",
        OsFamily::Linux => "linux",
        OsFamily::Windows => "windows",
        OsFamily::Other => "other",
    }
}

fn display_server_label(server: clipvault_platform::DisplayServer) -> &'static str {
    use clipvault_platform::DisplayServer;
    match server {
        DisplayServer::X11 => "x11",
        DisplayServer::Wayland => "wayland",
        DisplayServer::Unknown => "unknown",
    }
}

/// Cheap, deterministic change-detection fingerprint for a payload.
///
/// Text keeps [`hash_content`] so the dedupe history of an existing
/// installation is preserved exactly. Images and rich-text payloads
/// use a separate hash domain so a text payload and an image payload
/// can never collide, and the dimensions participate so a resize of
/// otherwise identical pixels still counts as a change. Rich text
/// mixes the canonical `plain_text` plus the available rich
/// representations so a payload whose `plain_text` is unchanged but
/// whose styles changed still registers as a new clipboard event.
///
/// This value is **not** persisted: it never reaches SQLite and never
/// appears in an event. The persisted dedupe key for an image is the
/// SHA-256 of its normalized PNG and for a rich-text payload the
/// canonical `rich_text_hash` the capture pipeline computes.
fn payload_fingerprint(payload: &ClipboardPayload) -> String {
    match payload {
        ClipboardPayload::Text(text) => hash_content(text),
        ClipboardPayload::Image(image) => {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};

            let mut hasher = DefaultHasher::new();
            "clipvault-image:v1".hash(&mut hasher);
            image.width().hash(&mut hasher);
            image.height().hash(&mut hasher);
            image.rgba().hash(&mut hasher);
            format!("{:016x}", hasher.finish())
        }
        ClipboardPayload::RichText(rich) => {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};

            let mut hasher = DefaultHasher::new();
            "clipvault-rich-text:v1".hash(&mut hasher);
            rich.plain_text().hash(&mut hasher);
            rich.html().hash(&mut hasher);
            // Cheap, deterministic, non-cryptographic; the canonical
            // rich-text hash lives in the persisted dedupe column.
            let rtf_len = rich.rtf().map(|bytes| bytes.len()).unwrap_or(0);
            rtf_len.hash(&mut hasher);
            format!("{:016x}", hasher.finish())
        }
    }
}

/// Build the suppression fingerprint from an observed payload.
///
/// Mirrors [`SuppressionFingerprint::from_payload`]; the image leg
/// is already filled by the constructor when the payload carries
/// RGBA bytes, so the watcher only needs to forward the result.
fn suppression_fingerprint_for(payload: &ClipboardPayload) -> SuppressionFingerprint {
    SuppressionFingerprint::from_payload(payload)
}

fn error_message(error: &ClipboardBackendError) -> String {
    match error {
        ClipboardBackendError::Empty => "clipboard returned no text".to_string(),
        ClipboardBackendError::Backend { details } => {
            format!("clipboard backend failed: {details}")
        }
        ClipboardBackendError::Unavailable { capability } => {
            format!("clipboard capability unavailable: {}", capability)
        }
        ClipboardBackendError::UnsupportedFormat => {
            "clipboard holds an unsupported representation".to_string()
        }
        // `ImageValidationError`'s Display carries dimensions and byte
        // counts only — never pixels.
        ClipboardBackendError::InvalidImage(reason) => {
            format!("clipboard image is invalid: {reason}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bootstrap::AppBootstrap;
    use crate::fakes::FakeClipboardBackend;

    fn tempdir() -> tempfile::TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    /// Build an [`AppContext`] whose asset stores live inside a
    /// `tempfile::TempDir`. Required because the watcher funnels
    /// captured payloads through the asset collector when an image
    /// lands; without the isolated harness the collector would
    /// sweep `~/.clipvault/assets/clipboard/*.png` (the regression
    /// caught by the 11:20:44 audit).
    fn bootstrap_default() -> AppContext {
        let dir = tempdir();
        let adapters =
            crate::test_support::build_isolated_adapters(dir.path(), &dir.path().join("data"));
        AppBootstrap::new()
            .with_platform_adapters(adapters)
            .bootstrap_at(dir.path().join("clipvault.db"))
            .expect("bootstrap")
    }

    #[test]
    fn tick_emits_unchanged_for_same_payload() {
        let backend = Arc::new(FakeClipboardBackend::new());
        backend.push_read(Ok(Some("hello".into())));
        backend.push_read(Ok(Some("hello".into())));
        let context = bootstrap_default();
        let watcher = CaptureWatcher::new(backend, Duration::from_millis(10));

        let first = watcher.tick(&context, None, AttemptOrigin::BackgroundLoop);
        assert!(matches!(
            first,
            WatchTickOutcome::Captured(HistoryOutcome::Stored { .. })
        ));

        let second = watcher.tick(&context, None, AttemptOrigin::BackgroundLoop);
        assert_eq!(second, WatchTickOutcome::Unchanged);
    }

    #[test]
    fn tick_reports_ignored_when_clipboard_empty() {
        let backend = Arc::new(FakeClipboardBackend::new());
        backend.push_read(Ok(None));
        let context = bootstrap_default();
        let watcher = CaptureWatcher::new(backend, Duration::from_millis(10));
        assert_eq!(
            watcher.tick(&context, None, AttemptOrigin::ManualTick),
            WatchTickOutcome::Ignored
        );
    }

    #[test]
    fn tick_reports_failed_when_backend_errors() {
        let backend = Arc::new(FakeClipboardBackend::new());
        backend.push_read(Err(ClipboardBackendError::backend("backend gone")));
        let context = bootstrap_default();
        let watcher = CaptureWatcher::new(backend, Duration::from_millis(10));
        let outcome = watcher.tick(&context, None, AttemptOrigin::BackgroundLoop);
        assert!(matches!(outcome, WatchTickOutcome::Failed { .. }));
    }

    #[test]
    fn tick_records_new_payload_when_content_changes() {
        let backend = Arc::new(FakeClipboardBackend::new());
        backend.push_read(Ok(Some("first".into())));
        backend.push_read(Ok(Some("second".into())));
        let context = bootstrap_default();
        let watcher = CaptureWatcher::new(backend, Duration::from_millis(10));

        let first = watcher.tick(&context, None, AttemptOrigin::BackgroundLoop);
        let second = watcher.tick(&context, None, AttemptOrigin::BackgroundLoop);
        assert!(matches!(
            first,
            WatchTickOutcome::Captured(HistoryOutcome::Stored { .. })
        ));
        assert!(matches!(
            second,
            WatchTickOutcome::Captured(HistoryOutcome::Stored { .. })
        ));
    }

    #[test]
    fn clones_share_the_dedupe_state() {
        // Bug regression: the background capture loop and the manual
        // `Tick capture` command both hold a handle to the watcher.
        // Both handles MUST report the same dedupe outcome for the
        // same clipboard payload so a discarded event cannot be
        // re-evaluated (and possibly persisted) by the second caller.
        let backend = Arc::new(FakeClipboardBackend::new());
        // Two clipboard reads: the first write is recorded by the
        // "loop-side" watcher, the second read attempts to re-record
        // the same content from the "command-side" watcher. Both
        // handles share the same `Arc<CaptureWatcherInner>` so the
        // second call sees the previous hash and returns Unchanged.
        backend.push_read(Ok(Some("duplicate payload".into())));
        backend.push_read(Ok(Some("duplicate payload".into())));
        let context = bootstrap_default();
        let loop_watcher = CaptureWatcher::new(backend.clone(), Duration::from_millis(10));
        let tick_watcher = loop_watcher.clone();

        let loop_outcome = loop_watcher.tick(&context, None, AttemptOrigin::BackgroundLoop);
        let tick_outcome = tick_watcher.tick(&context, None, AttemptOrigin::ManualTick);

        assert!(matches!(
            loop_outcome,
            WatchTickOutcome::Captured(HistoryOutcome::Stored { .. })
        ));
        assert_eq!(
            tick_outcome,
            WatchTickOutcome::Unchanged,
            "second caller must observe the loop's last_hash"
        );
    }

    #[test]
    fn clones_share_the_configured_interval() {
        // The shell relies on the captured interval staying constant
        // across the cloned watchers so the loop honours the cadence
        // exposed by `AppState::watcher.interval()` regardless of
        // which handle reads it.
        let backend = Arc::new(FakeClipboardBackend::new());
        let configured = Duration::from_millis(1_250);
        let watcher = CaptureWatcher::new(backend, configured);
        let clone = watcher.clone();
        assert_eq!(watcher.interval(), configured);
        assert_eq!(clone.interval(), configured);
    }
}
