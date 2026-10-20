//! Shared state managed by Tauri.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use clipvault_core::{AppContext, PlatformAdapters, SettingsServiceError, WatchTickOutcome};
use parking_lot::Mutex;

use crate::bootstrap::AppState;

/// Thin wrapper that exposes the pieces the commands need while
/// keeping the underlying [`AppState`] immutable.
#[derive(Clone)]
pub struct SharedState {
    inner: Arc<AppState>,
    shortcut_status: Arc<Mutex<HashMap<String, String>>>,
    pending_image_import: Arc<Mutex<Option<PendingImageImport>>>,
    next_image_import_session_id: Arc<AtomicU64>,
}

struct PendingImageImport {
    session_id: u64,
    files: Vec<PendingImageImportFile>,
    skipped: usize,
}

struct PendingImageImportFile {
    id: u32,
    path: PathBuf,
    display_name: Option<String>,
}

/// Metadata safe to return to the frontend while an image import is awaiting
/// order confirmation. Source paths remain in `SharedState`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PendingImageImportFilePreview {
    pub id: u32,
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PendingImageImportPreview {
    pub session_id: u64,
    pub files: Vec<PendingImageImportFilePreview>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingImageImportError {
    MissingOrExpired,
    InvalidOrder,
}

impl SharedState {
    pub fn new(state: AppState) -> Self {
        Self {
            inner: Arc::new(state),
            shortcut_status: Arc::new(Mutex::new(HashMap::new())),
            pending_image_import: Arc::new(Mutex::new(None)),
            next_image_import_session_id: Arc::new(AtomicU64::new(1)),
        }
    }

    /// Retain paths for one pending multi-file import and return only
    /// transient basename labels and opaque per-file IDs to the webview.
    pub fn begin_image_import(
        &self,
        paths: Vec<PathBuf>,
        skipped: usize,
    ) -> PendingImageImportPreview {
        let session_id = self
            .next_image_import_session_id
            .fetch_add(1, Ordering::Relaxed);
        let files = paths
            .into_iter()
            .enumerate()
            .map(|(index, path)| PendingImageImportFile {
                id: index as u32,
                display_name: path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned()),
                path,
            })
            .collect::<Vec<_>>();
        let preview = PendingImageImportPreview {
            session_id,
            files: files
                .iter()
                .map(|file| PendingImageImportFilePreview {
                    id: file.id,
                    display_name: file.display_name.clone(),
                })
                .collect(),
        };
        *self.pending_image_import.lock() = Some(PendingImageImport {
            session_id,
            files,
            skipped,
        });
        preview
    }

    /// Validate the submitted sequence as an exact permutation, then consume
    /// the pending selection and return paths in the requested logical order.
    pub fn take_ordered_image_import(
        &self,
        session_id: u64,
        ordered_file_ids: &[u32],
    ) -> Result<(Vec<PathBuf>, usize), PendingImageImportError> {
        let mut pending = self.pending_image_import.lock();
        let Some(selection) = pending.as_ref() else {
            return Err(PendingImageImportError::MissingOrExpired);
        };
        if selection.session_id != session_id {
            return Err(PendingImageImportError::MissingOrExpired);
        }
        if ordered_file_ids.len() != selection.files.len() {
            return Err(PendingImageImportError::InvalidOrder);
        }
        let mut submitted = HashSet::with_capacity(ordered_file_ids.len());
        if ordered_file_ids.iter().any(|id| !submitted.insert(*id))
            || selection
                .files
                .iter()
                .any(|file| !submitted.contains(&file.id))
        {
            return Err(PendingImageImportError::InvalidOrder);
        }

        let selection = pending.take().expect("pending selection was validated");
        let mut ordered_paths = Vec::with_capacity(selection.files.len());
        for id in ordered_file_ids {
            let file = selection
                .files
                .iter()
                .find(|file| file.id == *id)
                .expect("exact permutation was validated");
            ordered_paths.push(file.path.clone());
        }
        Ok((ordered_paths, selection.skipped))
    }

    /// Discard pending source paths when the user cancels the order review.
    pub fn cancel_image_import(&self, session_id: u64) {
        let mut pending = self.pending_image_import.lock();
        if pending
            .as_ref()
            .is_some_and(|selection| selection.session_id == session_id)
        {
            pending.take();
        }
    }

    pub fn context(&self) -> &AppContext {
        &self.inner.context
    }

    pub fn adapters(&self) -> &PlatformAdapters {
        &self.inner.adapters
    }

    pub fn set_shortcut_status(&self, id: &str, status: &str) {
        self.shortcut_status
            .lock()
            .insert(id.to_string(), status.to_string());
    }

    pub fn shortcut_status(&self) -> HashMap<String, String> {
        self.shortcut_status.lock().clone()
    }

    pub fn app_state(&self) -> &AppState {
        &self.inner
    }

    /// Force one watcher tick. The caller-supplied `source_app` is
    /// deliberately ignored: the shell resolves the identifier from
    /// the cached active-app probe so the manual button can never
    /// attribute a capture to ClipVault (the focused window) and the
    /// `PrivacyGate` observes the same identifier the background
    /// loop would have consulted on the next tick.
    ///
    /// The helper shares the cached probe with the background
    /// capture loop (no second watcher, no second dedupe state)
    /// and refreshes the cache synchronously on non-macOS hosts so
    /// the manual tick observes the identifier the user actually
    /// has focused after switching windows. On macOS the main-queue
    /// refresher keeps the cache populated.
    pub fn tick(&self, _source_app: Option<&str>) -> WatchTickOutcome {
        if !self.inner.watcher.is_capture_enabled() {
            return WatchTickOutcome::Paused;
        }
        crate::bootstrap::refresh_active_application_cache_for_loop_tick(&self.inner.context);
        let identifier = crate::bootstrap::resolved_source_identifier(&self.inner.context);
        self.inner.watcher.tick(
            &self.inner.context,
            identifier.as_deref(),
            clipvault_core::AttemptOrigin::ManualTick,
        )
    }

    /// Return the effective local clipboard-capture state used by all
    /// watcher and manual capture entry points.
    pub fn capture_enabled(&self) -> bool {
        self.inner.watcher.is_capture_enabled()
    }

    /// Return the runtime local collection selection used by automatic
    /// capture assignment.
    pub fn active_capture_collection_id(&self) -> Option<i64> {
        self.inner.watcher.active_collection_id()
    }

    /// Update the runtime local collection selection shared by every watcher
    /// clone. The persistence layer validates eligibility when an entry is
    /// assigned.
    pub fn set_active_capture_collection_id(&self, collection_id: Option<i64>) {
        self.inner.watcher.set_active_collection_id(collection_id);
    }

    /// Persist and atomically apply a requested state, or toggle it when
    /// `requested` is `None`. The watcher gate serializes this transition
    /// with every in-flight local capture.
    pub fn update_capture_enabled(
        &self,
        requested: Option<bool>,
    ) -> Result<bool, SettingsServiceError> {
        let context = &self.inner.context;
        self.inner
            .watcher
            .update_capture_enabled(requested, |enabled| {
                context
                    .settings()
                    .set_local_clipboard_capture_enabled(context, enabled)
            })
    }

    /// Run a local capture entry point while holding the shared pause gate.
    /// Returns `None` when capture is paused.
    pub fn with_capture_enabled<T>(&self, action: impl FnOnce() -> T) -> Option<T> {
        self.inner.watcher.with_capture_enabled(action)
    }

    /// Recompute the capability matrix from the cached platform info.
    /// The result is stored in the [`AppContext`] so the next call to
    /// [`Self::context`] returns the refreshed matrix.
    pub fn refresh_capabilities(&self) {
        let refreshed = self.inner.adapters.refresh_capabilities();
        self.inner.context.set_capabilities(refreshed);
    }

    /// Ask the background capture loop to stop. Used by the cleanup
    /// hook so the thread does not race the shutdown retention pass.
    pub fn signal_capture_loop_stop(&self) {
        self.inner.cancel_capture.store(true, Ordering::SeqCst);
    }
}
