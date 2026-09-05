//! Periodic refresh of the active-application probe on the macOS
//! main queue.
//!
//! Apple's contract for `NSWorkspace` requires the call to land on the
//! Cocoa main thread. Tauri 2.x's `AppHandle::run_on_main_thread`
//! scheduler goes through the `tao` event-loop proxy: it sends the
//! closure over a `crossbeam_channel`, signals a `CFRunLoopSource`,
//! and asks `CFRunLoopGetMain()` to wake up. In practice this path can
//! stall when the closure waits on the main thread for the same proxy
//! to drain, leaving the cached probe empty and every privacy
//! evaluation falling into the "unknown source" branch.
//!
//! This module provides an alternative dispatcher that does not depend
//! on `tao`'s proxy: a `dispatch2::DispatchSource` of type timer,
//! attached to `DispatchQueue::main()`. The timer fires on the actual
//! NSApp main thread without round-tripping through the user-event
//! queue, so the inner probe sees `MainThreadMarker::new() == Some`
//! and `NSWorkspace::sharedWorkspace()` returns a real
//! `frontmostApplication`.
//!
//! The refresher is intentionally conservative:
//!
//! * It owns a `dispatch2::DispatchRetained<DispatchSource>` that the
//!   caller stores for the lifetime of the loop. Dropping the handle
//!   cancels the timer; no background threads outlive the application.
//! * The handler only touches the wrapped probe and the supplied
//!   diagnostics state; it never logs the clipboard content or the
//!   bundle identifier.
//! * The interval defaults to one second. Tighter intervals would
//!   produce unnecessary wake-ups; looser intervals would let a
//!   blacklisted app stay focused for a full tick before the gate
//!   blocks its capture.

use std::sync::Arc;
use std::time::Duration;

use block2::RcBlock;
use dispatch2::{DispatchObject, DispatchQueue, DispatchSource, DispatchTime};

use crate::active_app::CachedActiveApplication;

/// Default polling interval. The timer fires this often on the main
/// queue and refreshes the cached probe. One second matches the
/// capture-loop cadence (`max(default, 750ms)`) without overwhelming
/// the main thread.
pub const DEFAULT_REFRESH_INTERVAL: Duration = Duration::from_secs(1);

/// Outcome of a single main-queue refresh. The variants mirror the
/// user-facing diagnostics taxonomy so the shell can keep the
/// platform and core layers decoupled. The shell converts the
/// outcome to [`crate::active_app_diagnostics::ActiveAppFailureKind`]
/// for the UI.
pub mod outcome {
    use crate::active_app::{ActiveAppError, ActiveApplication};

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum ActiveAppRefreshOutcome {
        /// The probe answered; the cache now holds `value`.
        Ok(Option<ActiveApplication>),
        /// The probe could not run on this session (typically because
        /// `MainThreadMarker::new()` returned `None`).
        Unavailable,
        /// The probe returned a backend error.
        Backend { details: String },
    }

    /// Convenience constructor that maps a raw
    /// `Result<Option<ActiveApplication>, ActiveAppError>` to the
    /// typed outcome.
    pub fn from_probe(
        result: Result<Option<ActiveApplication>, ActiveAppError>,
    ) -> ActiveAppRefreshOutcome {
        match result {
            Ok(value) => ActiveAppRefreshOutcome::Ok(value),
            Err(ActiveAppError::Unavailable) => ActiveAppRefreshOutcome::Unavailable,
            Err(ActiveAppError::Backend { details }) => {
                ActiveAppRefreshOutcome::Backend { details }
            }
        }
    }
}

/// Owning handle for the periodic main-queue refresher. Dropping the
/// handle cancels the underlying `DispatchSource`; the closure stored
/// inside the timer is dropped alongside it, so no background thread
/// outlives the application.
pub struct MainQueueActiveAppRefresher {
    _source: dispatch2::DispatchRetained<DispatchSource>,
}

/// Error variants returned by [`MainQueueActiveAppRefresher::install`].
/// The shell treats every variant other than `Ok` as "stay on the
/// synchronous helper" so the user can still press **Refrescar
/// diagnóstico** and observe the failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MainQueueRefresherError {
    /// `dispatch_source_create` returned a null pointer. Unusual but
    /// possible if libdispatch is unavailable on the host.
    SourceCreateFailed,
}

impl std::fmt::Display for MainQueueRefresherError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MainQueueRefresherError::SourceCreateFailed => {
                write!(f, "dispatch_source_create returned null on the main queue")
            }
        }
    }
}

impl std::error::Error for MainQueueRefresherError {}

impl MainQueueActiveAppRefresher {
    /// Install a periodic timer on the macOS main queue that calls
    /// the inner probe behind `cache` and stores the answer on the
    /// same cache the background watcher reads.
    ///
    /// `on_outcome` is invoked on the main thread for every fired
    /// event with a typed outcome so the shell can surface the result
    /// through the diagnostics endpoint. The closure must not block:
    /// it runs on the main queue and any delay would stall the UI.
    pub fn install(
        cache: CachedActiveApplication,
        on_outcome: Arc<dyn Fn(outcome::ActiveAppRefreshOutcome) + Send + Sync>,
        interval: Duration,
    ) -> Result<Self, MainQueueRefresherError> {
        install_inner(cache, on_outcome, interval)
    }
}

fn install_inner(
    cache: CachedActiveApplication,
    on_outcome: Arc<dyn Fn(outcome::ActiveAppRefreshOutcome) + Send + Sync>,
    interval: Duration,
) -> Result<MainQueueActiveAppRefresher, MainQueueRefresherError> {
    // `dispatch_source_create(DISPATCH_SOURCE_TYPE_TIMER, 0, 0, main_queue)`
    // allocates a timer source bound to the main queue. The raw
    // pointer to `_dispatch_source_type_timer` is what libdispatch
    // expects.
    let raw_type: dispatch2::dispatch_source_type_t = unsafe {
        &dispatch2::_dispatch_source_type_timer as *const _ as dispatch2::dispatch_source_type_t
    };
    let queue = DispatchQueue::main();
    let source = unsafe { DispatchSource::new(raw_type, 0, 0, Some(queue)) };

    // Configure the timer. The saturating cast keeps the call safe
    // for any interval above ~584 years (well above any sane value).
    let interval_ns = u64::try_from(interval.as_nanos()).unwrap_or(u64::MAX);
    let leeway_ns = interval_ns / 4;
    source.set_timer(DispatchTime::NOW, interval_ns, leeway_ns);

    // The handler block owns the cache and the callback. We capture
    // both by move so the source keeps them alive. `RcBlock::new`
    // allocates on the heap and `set_event_handler_with_block`
    // borrows it; the dispatch source retains its own reference.
    let cache_for_block = cache.clone();
    let on_outcome_for_block = Arc::clone(&on_outcome);
    let handler = RcBlock::new(move || {
        // The closure executes on the macOS main thread (the
        // dispatch main queue is bound to it). The probe sees
        // `MainThreadMarker::new() == Some` here, so
        // `NSWorkspace::sharedWorkspace()` returns a real
        // application. The outcome is then stored on the cache so
        // the background watcher can read it on its next tick.
        let outcome = outcome::from_probe(cache_for_block.refresh());
        let new_cached = match &outcome {
            outcome::ActiveAppRefreshOutcome::Ok(value) => value.clone(),
            _ => None,
        };
        cache_for_block.refresh_with(new_cached);
        on_outcome_for_block(outcome);
    });

    // Attach the block to the source. The handler is retained by
    // libdispatch until the source is deallocated. `RcBlock::as_ptr`
    // returns `*mut Block<F>` which matches the `dispatch_block_t`
    // alias `*mut DynBlock<dyn Fn()>`.
    unsafe {
        source.set_event_handler_with_block(RcBlock::as_ptr(&handler));
    }

    // Activate the source. From this point on libdispatch will fire
    // the handler on the main queue at the configured interval.
    source.activate();

    Ok(MainQueueActiveAppRefresher { _source: source })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::active_app::{ActiveAppError, ActiveApplication, CachedActiveApplication};
    use parking_lot::Mutex;
    use std::sync::Arc;

    struct ScriptedProbe(Result<Option<ActiveApplication>, ActiveAppError>);
    impl crate::ActiveApplicationProbe for ScriptedProbe {
        fn active_application(&self) -> Result<Option<ActiveApplication>, ActiveAppError> {
            self.0.clone()
        }
        fn name(&self) -> &'static str {
            "scripted"
        }
    }

    #[test]
    fn outcome_from_probe_classifies_failures() {
        let outcome = outcome::from_probe(Err(ActiveAppError::Unavailable));
        assert!(matches!(
            outcome,
            outcome::ActiveAppRefreshOutcome::Unavailable
        ));
        let outcome = outcome::from_probe(Err(ActiveAppError::Backend {
            details: "x".into(),
        }));
        assert!(matches!(
            outcome,
            outcome::ActiveAppRefreshOutcome::Backend { .. }
        ));
        let outcome = outcome::from_probe(Ok(Some(ActiveApplication::new(
            "TextEdit",
            "com.apple.TextEdit",
        ))));
        assert!(matches!(outcome, outcome::ActiveAppRefreshOutcome::Ok(_)));
    }

    #[test]
    fn cached_probe_round_trip_through_outcome() {
        let inner: Arc<dyn crate::ActiveApplicationProbe> = Arc::new(ScriptedProbe(Ok(Some(
            ActiveApplication::new("TextEdit", "com.apple.TextEdit"),
        ))));
        let cache = CachedActiveApplication::new(inner);
        let outcome = outcome::from_probe(cache.refresh());
        let new_cached = match &outcome {
            outcome::ActiveAppRefreshOutcome::Ok(value) => value.clone(),
            _ => None,
        };
        cache.refresh_with(new_cached);
        assert_eq!(
            cache.cached().unwrap().identifier,
            "com.apple.TextEdit".to_string()
        );
    }

    #[test]
    fn refresher_drops_cleanly_without_installing() {
        // `install` requires libdispatch, which is unavailable in
        // unit tests. We only assert the wrapper is `Send` so the
        // type can be stored across thread boundaries.
        fn assert_send<T: Send>() {}
        assert_send::<MainQueueActiveAppRefresher>();
    }

    #[test]
    fn diagnostics_callback_contract() {
        // The trait-bound callback must be `Send + Sync` and accept
        // the typed outcome. The on-dispatch wiring is exercised in
        // production only.
        let captured: Arc<Mutex<Vec<outcome::ActiveAppRefreshOutcome>>> =
            Arc::new(Mutex::new(Vec::new()));
        let captured_for_cb = Arc::clone(&captured);
        let cb: Arc<dyn Fn(outcome::ActiveAppRefreshOutcome) + Send + Sync> =
            Arc::new(move |outcome| captured_for_cb.lock().push(outcome));
        cb(outcome::ActiveAppRefreshOutcome::Unavailable);
        cb(outcome::ActiveAppRefreshOutcome::Ok(Some(
            ActiveApplication::new("App", "com.example"),
        )));
        let observed = captured.lock();
        assert_eq!(observed.len(), 2);
        assert!(matches!(
            observed[0],
            outcome::ActiveAppRefreshOutcome::Unavailable
        ));
    }
}
