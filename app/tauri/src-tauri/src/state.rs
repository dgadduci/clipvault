//! Shared state managed by Tauri.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use clipvault_core::{AppContext, PlatformAdapters, WatchTickOutcome};

use crate::bootstrap::AppState;

/// Thin wrapper that exposes the pieces the commands need while
/// keeping the underlying [`AppState`] immutable.
#[derive(Clone)]
pub struct SharedState {
    inner: Arc<AppState>,
}

impl SharedState {
    pub fn new(state: AppState) -> Self {
        Self {
            inner: Arc::new(state),
        }
    }

    pub fn context(&self) -> &AppContext {
        &self.inner.context
    }

    pub fn adapters(&self) -> &PlatformAdapters {
        &self.inner.adapters
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
        crate::bootstrap::refresh_active_application_cache_for_loop_tick(&self.inner.context);
        let identifier = crate::bootstrap::resolved_source_identifier(&self.inner.context);
        self.inner.watcher.tick(
            &self.inner.context,
            identifier.as_deref(),
            clipvault_core::AttemptOrigin::ManualTick,
        )
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
