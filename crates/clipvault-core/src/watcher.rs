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

use std::sync::Arc;
use std::time::Duration;

use clipvault_platform::{ClipboardBackend, ClipboardBackendError, ClipboardPayload};
use parking_lot::Mutex;
use tracing::warn;

use crate::bootstrap::AppContext;
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
    pub fn tick(&self, context: &AppContext, source_app: Option<&str>) -> WatchTickOutcome {
        match self.inner.clipboard.read_payload() {
            Ok(Some(payload)) => {
                let new_hash = payload_fingerprint(&payload);
                let changed = {
                    let mut state = self.inner.state.lock();
                    let previous = state.last_hash.clone();
                    state.last_hash = Some(new_hash.clone());
                    previous.as_ref() != Some(&new_hash)
                };
                if !changed {
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
                    return WatchTickOutcome::Suppressed;
                }
                let history = context.history();
                let outcome = history.record_clipboard_payload(context, payload, source_app);
                WatchTickOutcome::Captured(outcome)
            }
            Ok(None) => WatchTickOutcome::Ignored,
            Err(error) => {
                warn!(kind = error.kind_str(), "capture watcher: read failed");
                WatchTickOutcome::Failed {
                    message: error_message(&error),
                }
            }
        }
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

    fn bootstrap_default() -> AppContext {
        let dir = tempdir();
        AppBootstrap::new()
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

        let first = watcher.tick(&context, None);
        assert!(matches!(
            first,
            WatchTickOutcome::Captured(HistoryOutcome::Stored { .. })
        ));

        let second = watcher.tick(&context, None);
        assert_eq!(second, WatchTickOutcome::Unchanged);
    }

    #[test]
    fn tick_reports_ignored_when_clipboard_empty() {
        let backend = Arc::new(FakeClipboardBackend::new());
        backend.push_read(Ok(None));
        let context = bootstrap_default();
        let watcher = CaptureWatcher::new(backend, Duration::from_millis(10));
        assert_eq!(watcher.tick(&context, None), WatchTickOutcome::Ignored);
    }

    #[test]
    fn tick_reports_failed_when_backend_errors() {
        let backend = Arc::new(FakeClipboardBackend::new());
        backend.push_read(Err(ClipboardBackendError::backend("backend gone")));
        let context = bootstrap_default();
        let watcher = CaptureWatcher::new(backend, Duration::from_millis(10));
        let outcome = watcher.tick(&context, None);
        assert!(matches!(outcome, WatchTickOutcome::Failed { .. }));
    }

    #[test]
    fn tick_records_new_payload_when_content_changes() {
        let backend = Arc::new(FakeClipboardBackend::new());
        backend.push_read(Ok(Some("first".into())));
        backend.push_read(Ok(Some("second".into())));
        let context = bootstrap_default();
        let watcher = CaptureWatcher::new(backend, Duration::from_millis(10));

        let first = watcher.tick(&context, None);
        let second = watcher.tick(&context, None);
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

        let loop_outcome = loop_watcher.tick(&context, None);
        let tick_outcome = tick_watcher.tick(&context, None);

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
