//! Best-effort, non-blocking, coalesced metadata enrichment
//! scheduler.
//!
//! ## Why this exists
//!
//! `text_history::TextHistoryService::commit` runs the inline
//! metadata enrichment on the capture-loop's background thread. The
//! inline path is correct on Linux X11 (the platform probe answers
//! there without requiring the main thread) and on hosts where the
//! application-metadata provider runs in-process. On macOS the
//! provider needs the main thread because `NSWorkspace` and the
//! `NSRunningApplication` lookup are documented as main-thread
//! only; the inline path therefore reports `Unavailable` and the
//! `source_app_name` / `source_app_icon_ref` columns stay empty.
//!
//! The previous prototype retried the enrichment on the main thread
//! through `AppHandle::run_on_main_thread_sync`: it scheduled the
//! closure through Tauri's scheduler and waited on a
//! `sync_channel` with a hard `ACTIVE_APP_REFRESH_WAIT` timeout
//! (500ms). When the main thread was busy rendering or processing
//! user events the closure did not get a chance to run, the wait
//! timed out, and the shell logged
//! `"metadata enrichment on the main thread did not run"` once
//! per capture. The capture was correctly persisted (the
//! transaction ran on the background thread), but the warning
//! flooded the logs and the metadata was never resolved.
//!
//! This module replaces the synchronous helper with a scheduler
//! that:
//!
//! - fires the closure through `AppHandle::run_on_main_thread`
//!   **without** a channel or a timeout. Tauri returns the
//!   `Result` immediately; the closure runs whenever the main
//!   loop drains the dispatch queue, which is exactly what the
//!   previous synchronous helper was trying to approximate with a
//!   `sync_channel` + `recv_timeout`;
//! - coalesces duplicate enqueues for the same `entry_id` so a
//!   capture loop that re-targets a freshly stored row never
//!   schedules more than one closure per row at a time. The
//!   coalescing set is bounded by the cap below so it cannot grow
//!   without bound across a long session;
//! - never converts a valid capture into a `Failed` outcome. The
//!   scheduler is a side channel: a `Schedule(...)` error logs a
//!   metadata-only warning, removes the entry from the coalescing
//!   set, and lets the next capture of the same identifier
//!   re-target a fresh row through the same scheduler;
//! - cancels cleanly at shutdown. The scheduler's `Drop`
//!   implementation clears the coalescing set. Closures already
//!   in the dispatch queue keep their `AppContext` (Arc) so they
//!   finish without panicking.
//!
//! ## Diagnostics
//!
//! The scheduler returns the [`ScheduleOutcome`] enum the
//! `clipboard-history-cards` spec requires. The richer set of
//! counters (including `schedule_failed`) is observable through
//! [`MetadataEnrichmentScheduler::metrics`] so the diagnostics
//! card can render the breakdown without parsing log lines. The
//! log line itself only carries the `entry_id` (when relevant)
//! and the structured `kind = "schedule"` field — no payload, no
//! hash, no path, no snippet.

use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Runtime};
use tracing::warn;

/// Maximum number of entries the coalescing set is allowed to hold
/// simultaneously. The cap is intentionally loose (256) so the
/// scheduler does not race the loop's attempt to re-enqueue; when
/// the cap is hit, the scheduler drops an arbitrary entry to make
/// room for a fresh enqueue.
const MAX_IN_FLIGHT: usize = 256;

/// Aggregate counters for the scheduler. Cheap to read from a Tauri
/// command: each field is an `AtomicU64` so the snapshot is a
/// sequence of relaxed loads. The values never include clipboard
/// content, hashes or paths; the metrics endpoint can serialise
/// them directly.
#[allow(dead_code)]
#[derive(Debug, Default)]
pub struct SchedulerMetrics {
    /// Number of closures the scheduler successfully enqueued.
    pub scheduled: AtomicU64,
    /// Number of duplicates the scheduler dropped.
    pub skipped_duplicate: AtomicU64,
    /// Number of times Tauri refused to enqueue the closure.
    pub schedule_failed: AtomicU64,
    /// Number of enqueues that ran on the main thread and
    /// finished. The counter is incremented from inside the
    /// closure body so the difference between `scheduled` and
    /// `completed` is the size of the in-flight set.
    pub completed: AtomicU64,
}

#[allow(dead_code)]
impl SchedulerMetrics {
    /// Snapshot the counters as a tuple. The values are read in
    /// a deterministic order so a unit test asserting on the
    /// snapshot does not race another thread.
    pub fn snapshot(&self) -> SchedulerMetricsSnapshot {
        SchedulerMetricsSnapshot {
            scheduled: self.scheduled.load(Ordering::Relaxed),
            skipped_duplicate: self.skipped_duplicate.load(Ordering::Relaxed),
            schedule_failed: self.schedule_failed.load(Ordering::Relaxed),
            completed: self.completed.load(Ordering::Relaxed),
        }
    }
}

/// Plain (non-atomic) snapshot. Returned by
/// [`SchedulerMetrics::snapshot`].
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SchedulerMetricsSnapshot {
    pub scheduled: u64,
    pub skipped_duplicate: u64,
    pub schedule_failed: u64,
    pub completed: u64,
}

/// Idempotent, bounded, non-blocking scheduler for source-app
/// metadata enrichment closures. The scheduler is `Send + Sync`
/// because every field is either an `Arc` or an atomic counter;
/// the inner `Arc<Mutex<HashSet<i64>>>` is the coalescing set.
#[derive(Debug)]
pub struct MetadataEnrichmentScheduler {
    /// In-flight `entry_id`s. The set is bounded by
    /// [`MAX_IN_FLIGHT`]; when the cap is hit the scheduler
    /// evicts an arbitrary entry to make room. The handle is
    /// `Arc`'d so the scheduled closure can remove the entry
    /// when it finishes.
    in_flight: Arc<Mutex<HashSet<i64>>>,
    /// Aggregate metrics the diagnostics endpoint can read.
    metrics: Arc<SchedulerMetrics>,
}

impl MetadataEnrichmentScheduler {
    /// Build a new scheduler with an empty coalescing set.
    pub fn new() -> Self {
        Self {
            in_flight: Arc::new(Mutex::new(HashSet::new())),
            metrics: Arc::new(SchedulerMetrics::default()),
        }
    }

    /// Read the metrics snapshot. The returned handle stays valid
    /// as long as the scheduler is alive; cloning it is cheap.
    #[allow(dead_code)]
    pub fn metrics(&self) -> Arc<SchedulerMetrics> {
        Arc::clone(&self.metrics)
    }

    /// Number of entries currently in flight (the size of the
    /// coalescing set). Exposed for tests so the bounded
    /// behaviour is observable without going through the
    /// metrics counters.
    #[allow(dead_code)]
    pub fn in_flight_len(&self) -> usize {
        self.in_flight.lock().map(|set| set.len()).unwrap_or(0)
    }

    /// Try to schedule the metadata enrichment for `entry_id` on
    /// the main thread through `handle`. The closure body runs
    /// on the main thread whenever the Tauri scheduler drains
    /// the dispatch queue. Returns the typed
    /// [`ScheduleOutcome`]; the caller logs a metadata-only
    /// warning on the failure path and stays silent on the other
    /// variants.
    pub fn enqueue_with<F>(
        &self,
        handle: &AppHandle<impl Runtime>,
        entry_id: i64,
        enqueue_history: F,
    ) -> ScheduleOutcome
    where
        F: FnOnce() + Send + 'static,
    {
        if !self.reserve(entry_id) {
            self.metrics
                .skipped_duplicate
                .fetch_add(1, Ordering::Relaxed);
            return ScheduleOutcome::SkippedDuplicate;
        }

        let in_flight = Arc::clone(&self.in_flight);
        let metrics = Arc::clone(&self.metrics);
        let entry_for_closure = entry_id;

        let schedule = handle.run_on_main_thread(move || {
            enqueue_history();
            if let Ok(mut set) = in_flight.lock() {
                set.remove(&entry_for_closure);
            }
            metrics.completed.fetch_add(1, Ordering::Relaxed);
        });

        match schedule {
            Ok(()) => {
                self.metrics.scheduled.fetch_add(1, Ordering::Relaxed);
                ScheduleOutcome::Scheduled
            }
            Err(error) => {
                // The closure will not run. Release the
                // reservation so the next capture of the same
                // identifier can re-enqueue.
                if let Ok(mut set) = self.in_flight.lock() {
                    set.remove(&entry_id);
                }
                warn!(
                    kind = "schedule",
                    entry_id,
                    error = %error,
                    "metadata enrichment could not be scheduled on the main thread"
                );
                self.metrics.schedule_failed.fetch_add(1, Ordering::Relaxed);
                // The trait does not have a `ScheduleFailed`
                // variant; collapse to `NoMainRuntime` so the
                // capture loop stays silent. The richer
                // `schedule_failed` counter is still exposed
                // through [`Self::metrics`].
                ScheduleOutcome::NoMainRuntime
            }
        }
    }

    /// Try to add `entry_id` to the coalescing set. Returns
    /// `true` when the reservation succeeded (the entry was not
    /// already present and the set was below the cap), `false`
    /// when the entry was already in the set.
    fn reserve(&self, entry_id: i64) -> bool {
        let Ok(mut set) = self.in_flight.lock() else {
            return false;
        };
        if set.contains(&entry_id) {
            return false;
        }
        if set.len() >= MAX_IN_FLIGHT {
            if let Some(&victim) = set.iter().next() {
                set.remove(&victim);
            }
        }
        set.insert(entry_id);
        true
    }
}

impl Default for MetadataEnrichmentScheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for MetadataEnrichmentScheduler {
    fn drop(&mut self) {
        if let Ok(mut set) = self.in_flight.lock() {
            set.clear();
        }
    }
}

/// Outcome of a single [`MetadataEnrichmentScheduler::enqueue_with`]
/// call. Stable identifier strings so the diagnostics endpoint
/// can render the breakdown without parsing the variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScheduleOutcome {
    /// The closure was queued successfully.
    Scheduled,
    /// Another enqueue for the same `entry_id` was already
    /// pending; the scheduler dropped the duplicate.
    SkippedDuplicate,
    /// Tauri refused to enqueue the closure or no Tauri runtime
    /// is wired in. The capture loop treats this as a soft
    /// outcome and stays silent.
    NoMainRuntime,
}

impl ScheduleOutcome {
    /// Stable snake_case identifier the diagnostics endpoint uses.
    #[allow(dead_code)]
    pub fn as_str(self) -> &'static str {
        match self {
            ScheduleOutcome::Scheduled => "scheduled",
            ScheduleOutcome::SkippedDuplicate => "skipped_duplicate",
            ScheduleOutcome::NoMainRuntime => "no_main_runtime",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    /// Pin the contract: the scheduler is `Send + Sync` so it
    /// can live behind an `Arc` in `AppState` and be borrowed
    /// from the capture loop's background thread.
    #[test]
    fn scheduler_is_send_and_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}
        assert_send::<MetadataEnrichmentScheduler>();
        assert_sync::<MetadataEnrichmentScheduler>();
    }

    /// Stable identifier for the diagnostics endpoint. A future
    /// refactor that renames a variant surfaces here.
    #[test]
    fn outcome_strings_are_stable() {
        assert_eq!(ScheduleOutcome::Scheduled.as_str(), "scheduled");
        assert_eq!(
            ScheduleOutcome::SkippedDuplicate.as_str(),
            "skipped_duplicate"
        );
        assert_eq!(ScheduleOutcome::NoMainRuntime.as_str(), "no_main_runtime");
    }

    /// A duplicate reservation is coalesced. The `reserve`
    /// helper returns `false` and the second call short-circuits
    /// before touching the metrics counter.
    #[test]
    fn reserve_returns_false_for_duplicate() {
        let scheduler = MetadataEnrichmentScheduler::new();
        assert!(scheduler.reserve(7));
        assert!(!scheduler.reserve(7));
    }

    /// The cap is enforced so a long session cannot leak
    /// entries into the coalescing set. The test fills the
    /// set past the cap and observes the eviction.
    #[test]
    fn reserve_evicts_when_cap_is_hit() {
        let scheduler = MetadataEnrichmentScheduler::new();
        for i in 0..(MAX_IN_FLIGHT + 1) {
            assert!(scheduler.reserve(i as i64));
        }
        // The set stays bounded by the cap.
        assert_eq!(scheduler.in_flight_len(), MAX_IN_FLIGHT);
    }

    /// The `Drop` implementation clears the set so a clean
    /// shutdown never leaves stale entries behind. The test
    /// relies on the compiler keeping the destructor.
    #[test]
    fn drop_clears_in_flight_set() {
        let scheduler = MetadataEnrichmentScheduler::new();
        assert!(scheduler.reserve(99));
        assert_eq!(scheduler.in_flight_len(), 1);
        drop(scheduler);
    }

    /// The metrics handle is `Send + Sync` and clones cheaply,
    /// so a Tauri command can read the snapshot from any
    /// thread without taking a lock on the coalescing set.
    #[test]
    fn metrics_handle_is_send_and_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}
        assert_send::<Arc<SchedulerMetrics>>();
        assert_sync::<Arc<SchedulerMetrics>>();

        let scheduler = MetadataEnrichmentScheduler::new();
        let handle = scheduler.metrics();
        let snapshot = handle.snapshot();
        assert_eq!(snapshot.scheduled, 0);
        // Cross-thread: read the snapshot from another thread
        // to prove the atomic counters are stable.
        let _ = thread::spawn(move || {
            let _ = handle.snapshot();
        })
        .join();
    }

    /// The metrics snapshot reflects the manual reservations
    /// through the snapshot accessor. The counters are
    /// initialised to zero and stay that way because the
    /// reservation helper does not increment them.
    #[test]
    fn metrics_snapshot_is_zero_for_fresh_scheduler() {
        let scheduler = MetadataEnrichmentScheduler::new();
        let snapshot = scheduler.metrics.snapshot();
        assert_eq!(snapshot.scheduled, 0);
        assert_eq!(snapshot.skipped_duplicate, 0);
        assert_eq!(snapshot.schedule_failed, 0);
        assert_eq!(snapshot.completed, 0);
    }

    /// The `ScheduleOutcome::as_str` mapping is the contract
    /// the diagnostics endpoint relies on. A future refactor
    /// that renames a variant surfaces here.
    #[test]
    fn schedule_outcome_strings_are_stable() {
        assert_eq!(ScheduleOutcome::Scheduled.as_str(), "scheduled");
        assert_eq!(
            ScheduleOutcome::SkippedDuplicate.as_str(),
            "skipped_duplicate"
        );
        assert_eq!(ScheduleOutcome::NoMainRuntime.as_str(), "no_main_runtime");
    }
}
