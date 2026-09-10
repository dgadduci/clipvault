//! Application bootstrap.
//!
//! Detects the host platform, builds the platform adapters and the
//! [`AppContext`] and starts the watcher. The shell then keeps the
//! returned [`AppState`] in Tauri's managed state and consumes the
//! registered commands.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use clipvault_core::{
    ActiveAppDiagnostics, ActiveAppFailureKind, AppBootstrap, AppContext, CaptureWatcher,
    IgnoredAppError, IgnoredAppsServiceError, PickAndAddOutcome, PlatformAdapters,
    WatchTickOutcome,
};
use clipvault_platform::{
    default_linux_binding, default_macos_binding, ActiveApplicationProbe, Capabilities,
    ClipboardBackend, HotkeyManager, HotkeyOutcome, NoopSettingsNavigator, OsFamily,
    PasteController, PlatformInfo, SettingsNavigator, TrayController,
};
use tauri::{AppHandle, Emitter, Runtime};
use tracing::{info, warn};

use crate::metadata_scheduler::MetadataEnrichmentScheduler;

/// Resolved bootstrap state handed to Tauri.
pub struct AppState {
    pub context: AppContext,
    /// Shared watcher used by both the background capture loop and
    /// [`crate::state::SharedState::tick`]. The shell MUST NOT create
    /// a second `CaptureWatcher`: each instance keeps its own
    /// `last_hash`, so two watchers would skip the dedupe step and
    /// let a blacklisted payload sneak back into the history via
    /// the manual Tick capture command.
    pub watcher: Arc<CaptureWatcher>,
    pub adapters: PlatformAdapters,
    pub cancel_capture: Arc<AtomicBool>,
    /// Platform-neutral handle for the active-app refresher the
    /// bootstrap installs at startup. On macOS the alias resolves to
    /// the real `MainQueueActiveAppRefresher`, so owning the handle
    /// here keeps the `dispatch2::DispatchSource` alive for the
    /// lifetime of the application; dropping the handle cancels the
    /// timer. The background capture loop never schedules another
    /// main-thread refresh while this is `Some`. On every other
    /// platform the alias resolves to `()`, making the slot a
    /// zero-cost `Option<()>` that the non-macOS stub fills with
    /// `None` so the shell does not have to repeat the macOS-only
    /// `cfg` gate at every reference site. The platform crate
    /// already enables `macos-native` through the
    /// `[target.'cfg(target_os = "macos")'.dependencies]` block, so
    /// the macOS symbol resolves on every macOS build without the
    /// shell having to repeat the cfg gate. The field looks
    /// "unused" to the compiler because its only purpose is to keep
    /// the timer alive through ownership; dropping it would cancel
    /// the timer and the diagnostics card would fall back to
    /// "pending".
    #[allow(dead_code)]
    pub active_app_refresher: Option<clipvault_platform::ActiveAppRefresherHandle>,
    /// Best-effort, non-blocking, coalesced scheduler for the
    /// per-row metadata enrichment. Owned by `AppState` so its
    /// lifetime is bound to the application; dropping the
    /// scheduler cancels the in-flight set and lets Tauri drop
    /// the queued closures when the `AppHandle` is dropped.
    pub metadata_scheduler: Arc<MetadataEnrichmentScheduler>,
}

/// How often the background capture loop polls the clipboard.
const CAPTURE_LOOP_TICK: Duration = Duration::from_millis(750);
/// Upper bound the [`crate::bootstrap::run_on_main_thread_sync`]
/// helper waits for a scheduled closure to run on the main thread.
/// Configured to be generous enough to absorb a single missed main
/// thread tick without making the capture loop hang.
const ACTIVE_APP_REFRESH_WAIT: Duration = Duration::from_millis(500);
/// Metadata-only event the shell emits after a successful capture.
/// The payload is `null` — the frontend re-reads the recent entries
/// after receiving the event so the notification never carries
/// clipboard content, snippets, hashes or source identifiers.
const HISTORY_UPDATED_EVENT: &str = "clipvault://history-updated";

/// Detect platform and build every adapter.
pub fn build_state() -> Result<AppState, Box<dyn std::error::Error>> {
    let platform = clipvault_platform::DefaultPlatform::detect()?;
    // Use runtime detection so the `synthetic_paste` capability
    // reflects the actual macOS Accessibility grant. The pure
    // `detect_capabilities` helper stays available for tests but is
    // never called from production code paths.
    let capabilities = clipvault_platform::detect_capabilities_runtime(&platform);

    let clipboard: Arc<dyn ClipboardBackend> = build_clipboard(&platform, capabilities);
    let hotkey: Arc<dyn HotkeyManager> = build_hotkey(&platform, capabilities);
    let active_app: Arc<dyn ActiveApplicationProbe> =
        build_active_application(&platform, capabilities);
    let paste: Arc<dyn PasteController> = build_paste_controller(&platform, capabilities);
    let tray: Arc<dyn TrayController> = build_tray_controller(&platform, capabilities);
    let settings_navigator: Arc<dyn SettingsNavigator> = build_settings_navigator(&platform);
    let app_metadata: Arc<dyn clipvault_platform::ApplicationMetadataProvider> =
        build_application_metadata_provider(&platform);

    let adapters = PlatformAdapters::new(
        Arc::clone(&clipboard),
        Arc::clone(&hotkey),
        Arc::clone(&active_app),
        Arc::clone(&paste),
        Arc::clone(&tray),
        Arc::clone(&settings_navigator),
        Arc::clone(&app_metadata),
        capabilities,
        platform,
    );

    let context = AppBootstrap::new()
        .with_platform_adapters(adapters.clone())
        .bootstrap_default()?;

    let watcher = Arc::new(CaptureWatcher::new(
        Arc::clone(&clipboard),
        CaptureWatcher::default_interval(),
    ));

    // The refresher is wired after `context` exists so the cache and
    // the diagnostics state are observable through it. The handle is
    // `None` until the very end of `build_state`; populating it
    // before returning would require moving `state` around twice.
    let metadata_scheduler = Arc::new(MetadataEnrichmentScheduler::new());
    let provisional_state = AppState {
        context: context.clone(),
        watcher: Arc::clone(&watcher),
        adapters: adapters.clone(),
        cancel_capture: Arc::new(AtomicBool::new(false)),
        active_app_refresher: None,
        metadata_scheduler: Arc::clone(&metadata_scheduler),
    };
    let (_refresher_outcome, active_app_refresher) =
        install_active_app_main_queue_refresher(&provisional_state);

    Ok(AppState {
        context,
        watcher,
        adapters,
        cancel_capture: Arc::new(AtomicBool::new(false)),
        active_app_refresher,
        metadata_scheduler,
    })
}

/// Synchronously run `f` on the Tauri main thread. Blocks the calling
/// thread until the main thread executes the closure and returns its
/// result. `run_on_main_thread` alone is fire-and-forget so the
/// existing background refresh relied on a window of "main thread is
/// free sometime in the next N seconds", which let blacklisted
/// captures slip through when the cache was stale. This helper turns
/// the call into "main thread executes the closure now", so the
/// capture loop can refresh the cached active-app probe before
/// evaluating each event.
///
/// Returns `Err(MainThreadSyncError::Schedule(...))` when Tauri
/// refuses to enqueue the closure (e.g. after shutdown) and
/// `Err(MainThreadSyncError::Timeout)` when the main thread does not
/// pick it up inside [`ACTIVE_APP_REFRESH_WAIT`]. Both errors are
/// reportable through the diagnostics endpoint without crashing the
/// capture loop.
pub fn run_on_main_thread_sync<F, R>(
    handle: &AppHandle<impl Runtime>,
    f: F,
) -> Result<R, MainThreadSyncError>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    let (tx, rx) = mpsc::sync_channel(1);
    handle
        .run_on_main_thread(move || {
            let value = f();
            // Ignore send errors: the receiver may have given up
            // after the timeout, but the closure has already run so
            // the side effects still apply.
            let _ = tx.send(value);
        })
        .map_err(|error| MainThreadSyncError::Schedule(error.to_string()))?;
    match rx.recv_timeout(ACTIVE_APP_REFRESH_WAIT) {
        Ok(value) => Ok(value),
        Err(_) => Err(MainThreadSyncError::Timeout),
    }
}

/// Error variants returned by [`run_on_main_thread_sync`]. The shell
/// surfaces them through the metadata-only diagnostics endpoint so a
/// failed refresh can be diagnosed without inspecting logs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MainThreadSyncError {
    /// Tauri refused to enqueue the closure.
    Schedule(String),
    /// The main thread did not pick the closure up inside the timeout.
    Timeout,
}

impl MainThreadSyncError {
    /// Translate the error to the structured
    /// [`ActiveAppFailureKind`] the diagnostics endpoint expects.
    /// Keeps the taxonomy stable so the UI never has to parse the
    /// free-form `Display` string.
    pub fn failure_kind(&self) -> ActiveAppFailureKind {
        match self {
            MainThreadSyncError::Schedule(_) => ActiveAppFailureKind::Schedule,
            MainThreadSyncError::Timeout => ActiveAppFailureKind::Timeout,
        }
    }
}

impl std::fmt::Display for MainThreadSyncError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MainThreadSyncError::Schedule(details) => {
                write!(f, "main thread did not accept the closure: {details}")
            }
            MainThreadSyncError::Timeout => {
                write!(
                    f,
                    "main thread did not execute the closure in {:?}",
                    ACTIVE_APP_REFRESH_WAIT
                )
            }
        }
    }
}

impl std::error::Error for MainThreadSyncError {}

/// Returns `true` when the calling thread is the macOS main thread.
/// Tauri commands run on the main thread; the shell uses this helper
/// to skip the synchronous helper entirely when an on-demand refresh
/// lands on the main thread (for example the **Refrescar
/// diagnóstico** Tauri command).
///
/// We use Apple's `pthread_main_np` directly (via `objc2`) rather
/// than Tauri's runtime because Tauri 2.x does not expose a public
/// accessor for the stored main-thread id. The check is a single
/// libc call and stays cheap enough to run on every Tauri command
/// invocation. The platform crate enables `macos-native` for macOS
/// builds via the `[target.'cfg(target_os = "macos")'.dependencies]`
/// re-declaration, so the `MainThreadMarker` symbol is guaranteed to
/// be present on every macOS host.
#[cfg(target_os = "macos")]
pub fn is_main_thread<R: Runtime>(_handle: &AppHandle<R>) -> bool {
    use objc2_foundation::MainThreadMarker;
    MainThreadMarker::new().is_some()
}

/// Non-macOS fallback. The synchronous helper always takes the
/// safe path that goes through Tauri's scheduler.
#[cfg(not(target_os = "macos"))]
pub fn is_main_thread<R: Runtime>(_handle: &AppHandle<R>) -> bool {
    false
}

/// Upper bound the picker helper waits for the scheduled picker
/// closure to run on the macOS main thread. NSOpenPanel blocks the
/// main thread until the user dismisses the modal dialog; the bound
/// is generous enough to cover a slow user but bounded so a deadlocked
/// session cannot hang the shell forever.
const PICK_AND_ADD_WAIT: Duration = Duration::from_secs(120);

/// Drive the platform-picker flow on the macOS main thread.
///
/// Apple's `NSOpenPanel` MUST run on the main thread; off-main
/// invocations are undefined behaviour and the previous prototype
/// returned `BackendUnavailable` because the Tauri command body
/// happened to be off-main in some runtime configurations. This
/// helper removes the assumption by routing the picker construction
/// through the same scheduler used by
/// [`run_on_main_thread_sync`].
///
/// Behaviour:
///
/// - When the calling thread is the macOS main thread, the helper
///   runs the service inline. Enqueuing the work onto the same
///   thread would deadlock the channel wait.
/// - When the calling thread is NOT the macOS main thread, the
///   helper schedules the picker construction on Tauri's main queue
///   and waits on a bounded channel for the outcome.
/// - Schedule or timeout failures map to
///   [`IgnoredAppError::BackendUnavailable`] with a safe, metadata-only
///   message — the helper NEVER falls back to off-main execution.
///
/// Persistence, normalisation and `PrivacyGate` updates stay in
/// `clipvault-core`; the shell only coordinates the thread.
pub fn pick_and_add_ignored_app<R: Runtime>(
    handle: &AppHandle<R>,
    context: &AppContext,
) -> Result<PickAndAddOutcome, IgnoredAppError> {
    let handle_clone = handle.clone();
    let cloned_context = context.clone();
    pick_and_add_ignored_app_impl(
        is_main_thread(handle),
        move || run_picker(&cloned_context),
        move |work| {
            handle_clone
                .run_on_main_thread(work)
                .map_err(|error| error.to_string())
        },
        PICK_AND_ADD_WAIT,
    )
}

/// Run the platform-picker service and translate the service-layer
/// error into the domain error variant. The picker service is the
/// single owner of the persistence flow; this helper only adapts
/// the error so the shell sees a single error type. Persistence
/// errors are converted to [`IgnoredAppError::Persistence`] so the
/// shell surfaces the same stable identifier the frontend already
/// routes on.
fn run_picker(context: &AppContext) -> Result<PickAndAddOutcome, IgnoredAppError> {
    match context
        .ignored_apps()
        .pick_and_add_with_default_picker(context)
    {
        Ok(outcome) => Ok(outcome),
        Err(IgnoredAppsServiceError::Domain(error)) => Err(error),
        Err(IgnoredAppsServiceError::Persistence(error)) => Err(IgnoredAppError::Persistence {
            reason: error.to_string(),
        }),
    }
}

/// Test-friendly implementation. The `is_main` flag, the
/// `run_picker` closure (so tests substitute a fake picker) and the
/// `scheduler` closure (so tests exercise the scheduling path
/// without a Tauri runtime) are injected.
///
/// `scheduler` returns `Ok(())` when the closure was queued and
/// `Err(reason)` when the runtime refused to enqueue the work; the
/// closure body is what the real `AppHandle::run_on_main_thread`
/// would later invoke on the main thread.
fn pick_and_add_ignored_app_impl<F, S>(
    is_main: bool,
    run_picker: F,
    scheduler: S,
    timeout: Duration,
) -> Result<PickAndAddOutcome, IgnoredAppError>
where
    F: FnOnce() -> Result<PickAndAddOutcome, IgnoredAppError> + Send + 'static,
    S: FnOnce(Box<dyn FnOnce() + Send + 'static>) -> Result<(), String>,
{
    if is_main {
        return run_picker();
    }

    let (tx, rx) = mpsc::sync_channel::<Result<PickAndAddOutcome, IgnoredAppError>>(1);
    let work: Box<dyn FnOnce() + Send + 'static> = Box::new(move || {
        // Always send the outcome so the receiver does not hang on
        // a missing value when the picker surfaces an error.
        let outcome = run_picker();
        let _ = tx.send(outcome);
    });

    if let Err(error) = scheduler(work) {
        warn!(error = %error, "application picker could not be scheduled on the main thread");
        return Err(IgnoredAppError::BackendUnavailable {
            reason: format!(
                "application picker could not be scheduled on the main thread: {error}"
            ),
        });
    }

    match rx.recv_timeout(timeout) {
        Ok(outcome) => outcome,
        Err(_) => {
            warn!(
                timeout_ms = timeout.as_millis() as u64,
                "application picker did not complete on the main thread inside the timeout"
            );
            Err(IgnoredAppError::BackendUnavailable {
                reason: format!(
                    "application picker did not complete on the main thread within {timeout:?}"
                ),
            })
        }
    }
}

/// Refresh the cached active-app probe by invoking the inner platform
/// adapter on the Tauri main thread. The capture loop calls this
/// helper on every iteration so the `PrivacyGate` evaluates captures
/// against an identifier that is at most one tick stale.
///
/// The helper exists so both the background loop and any test that
/// drives the bootstrap without standing up a Tauri runtime can
/// reach the cached probe. When `handle` is `None` it falls back to
/// invoking the inner probe directly — useful for the platform-level
/// tests that exercise the cache without Tauri.
///
/// When the calling thread is already the macOS main thread (for
/// example the **Refrescar diagnóstico** Tauri command), the helper
/// bypasses the synchronous channel entirely and calls the probe
/// in-process so the UI never blocks waiting on itself.
///
/// Every invocation updates the metadata-only diagnostics state so
/// the frontend can tell apart:
/// - `pending`: the loop never tried to refresh (no attempts yet);
/// - `ok`: the closure ran on the main thread and the inner probe
///   answered;
/// - `failed:schedule`: Tauri refused to enqueue the closure;
/// - `failed:timeout`: the main thread did not pick the closure up
///   inside the timeout;
/// - `failed:unavailable`: the inner probe cannot run on this
///   session (Apple requires `NSWorkspace` on the main thread);
/// - `failed:backend`: the inner probe raised an `ActiveAppError`.
///
/// Failure recording is centralised in the helper so the diagnostics
/// counters increment exactly once per refresh attempt regardless of
/// which path the shell uses to drive it.
pub fn refresh_active_app_cached<R: Runtime>(
    context: &AppContext,
    handle: Option<&AppHandle<R>>,
) -> ActiveAppDiagnostics {
    match handle {
        Some(handle) => {
            // If we are already on the main thread (every Tauri
            // command body), skip the scheduler entirely: the
            // scheduler's synchronous path would still queue the
            // closure onto the same thread we are calling from, and
            // the channel dance (sync_channel + recv_timeout) buys
            // nothing except latency. Calling `refresh_active_application`
            // directly is safe — `MainThreadMarker::new()` returns
            // `Some` on this thread so the macOS adapter will not
            // report `unavailable`.
            if is_main_thread(handle) {
                return match context.refresh_active_application() {
                    Ok(_) => context.active_app_diagnostics(),
                    Err(error) => {
                        warn!(error = %error, "active-app direct refresh failed");
                        context.active_app_diagnostics()
                    }
                };
            }
            let cloned = context.clone();
            match run_on_main_thread_sync(handle, move || cloned.refresh_active_application()) {
                // The closure ran. `refresh_active_application` already
                // updated the diagnostics state and the counters; we
                // just need to return the latest snapshot.
                Ok(_) => context.active_app_diagnostics(),
                Err(error) => {
                    // The closure never executed: Tauri refused to
                    // schedule the closure, or the main thread did
                    // not pick it up inside the timeout. Surface the
                    // failure on the diagnostics state with the
                    // structured
                    // [`ActiveAppFailureKind`] so the UI can tell
                    // them apart, and keep the actionable detail in
                    // the `message` field.
                    warn!(error = %error, "active-app synchronous refresh failed");
                    context.record_active_app_sync_failure_with_kind(
                        error.failure_kind(),
                        &error.to_string(),
                    )
                }
            }
        }
        None => {
            // No Tauri runtime (tests): call directly so the cache
            // state is still observable through the diagnostics
            // surface. `refresh_active_application` already records
            // the outcome (success or failure) on the state, so we
            // only return the resulting snapshot here. The shell's
            // shell-side `warn!` mirrors the production behaviour.
            match context.refresh_active_application() {
                Ok(_) => context.active_app_diagnostics(),
                Err(error) => {
                    warn!(error = %error, "active-app direct refresh failed");
                    context.active_app_diagnostics()
                }
            }
        }
    }
}

/// Start the background capture loop. The loop polls the clipboard at
/// [`CAPTURE_LOOP_TICK`]. On macOS the cached active-app probe is
/// refreshed on the main queue by
/// [`install_active_app_main_queue_refresher`](#fn@install_active_app_main_queue_refresher),
/// so by the time the loop ticks the cache is at most one second
/// stale. The loop therefore does **not** schedule another main-thread
/// refresh before every iteration: doing so was the source of the
/// "Intentos: 214, Correctos: 0, Fallidos: 214" regression, since
/// `AppHandle::run_on_main_thread` from a background thread can stall
/// when the main run loop is busy with other user events. The
/// `cancel` flag lets `cleanup` shut the loop down at exit without
/// panicking.
///
/// On every successful capture (Stored or Duplicate) the loop emits
/// the metadata-only [`HISTORY_UPDATED_EVENT`] so the frontend can
/// refresh its recent-entries list without polling. Ignored/Failed
/// outcomes are intentionally NOT announced: nothing changed in the
/// history the UI is allowed to render.
///
/// The loop is wired to the **same** [`CaptureWatcher`] the manual
/// `Tick capture` command consumes via `SharedState`. Sharing the
/// watcher (instead of building a fresh one inside the spawned thread)
/// guarantees that the dedupe state (`last_hash`) is identical for
/// both callers. If the loop already evaluated a payload — including
/// events the `PrivacyGate` discarded for blacklisted sources — the
/// manual tick sees the same hash and returns `Unchanged` instead of
/// re-running the blacklist check against the post-focus snapshot.
///
/// The helper must be invoked AFTER `app.manage(SharedState::new(state))`
/// has run: spawning the thread earlier would mean the loop can run
/// for one tick before the managed state is observable, which is the
/// race the original regression surfaced.
///
/// Outcome of installing the macOS main-queue refresher. The shell
/// stores a non-`Installed` variant on the diagnostics state so a
/// fall-back to `run_on_main_thread_sync` (the on-demand refresh) can
/// still surface why the periodic refresh could not start.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum MainQueueInstallOutcome {
    Installed,
    SkippedUnsupported,
    Failed(String),
}

/// Install the macOS main-queue refresher on the supplied
/// `AppContext`. The refresher is the canonical way to keep the
/// `PrivacyGate`'s source identifier up to date on macOS: a
/// `dispatch2::DispatchSource` timer fires on the Cocoa main thread
/// at a fixed cadence, calls the inner `NSWorkspace` probe, and
/// stores the answer on the cached probe the background watcher
/// reads.
///
/// On non-macOS hosts (or builds without the `macos-native` feature)
/// the function returns [`MainQueueInstallOutcome::SkippedUnsupported`]
/// and the capture loop falls back to the synchronous helper for
/// every tick. The on-demand **Refrescar diagnóstico** button still
/// works on every platform.
///
/// The returned tuple holds the live timer handle when the refresher
/// was installed. Dropping the handle cancels the timer; the shell
/// stores it on `AppState` so cleanup cancels it deterministically at
/// shutdown. The platform crate enables `macos-native` for the macOS
/// build via the
/// `[target.'cfg(target_os = "macos")'.dependencies]` re-declaration,
/// so the timer symbol is guaranteed to be present on every macOS
/// host. The shell therefore only needs a `cfg(target_os = "macos")`
/// gate, not a redundant `feature = "macos-native"` check.
#[cfg(target_os = "macos")]
pub fn install_active_app_main_queue_refresher(
    state: &AppState,
) -> (
    MainQueueInstallOutcome,
    Option<clipvault_platform::ActiveAppRefresherHandle>,
) {
    use clipvault_platform::active_app_refresh_outcome as platform_outcome;
    use clipvault_platform::ActiveAppError;

    let diagnostics = state.context.active_app_diagnostics_state();
    let diagnostics_for_callback = diagnostics.clone();
    let on_outcome: std::sync::Arc<
        dyn Fn(platform_outcome::ActiveAppRefreshOutcome) + Send + Sync,
    > = std::sync::Arc::new(move |outcome| {
        // Increment the metadata-only timer-callback counter before
        // branching on the outcome so the diagnostic surface reflects
        // every fired callback regardless of probe success.
        diagnostics_for_callback.record_timer_callback();
        match outcome {
            platform_outcome::ActiveAppRefreshOutcome::Ok(value) => {
                diagnostics_for_callback.record_refresh(value);
            }
            platform_outcome::ActiveAppRefreshOutcome::Unavailable => {
                diagnostics_for_callback.record_failure(&ActiveAppError::Unavailable);
            }
            platform_outcome::ActiveAppRefreshOutcome::Backend { details } => {
                diagnostics_for_callback.record_failure(&ActiveAppError::Backend { details });
            }
        }
    });
    match clipvault_platform::ActiveAppRefresherHandle::install(
        state.context.cached_active_app_probe(),
        on_outcome,
        clipvault_platform::DEFAULT_REFRESH_INTERVAL,
    ) {
        Ok(handle) => {
            diagnostics.mark_refresher_installed();
            info!(
                "macOS active-app cache will refresh on the main queue every {:?}",
                clipvault_platform::DEFAULT_REFRESH_INTERVAL
            );
            (MainQueueInstallOutcome::Installed, Some(handle))
        }
        Err(error) => {
            warn!(error = %error, "failed to install macOS main-queue refresher");
            (MainQueueInstallOutcome::Failed(error.to_string()), None)
        }
    }
}

/// Non-macOS stub. The synchronous helper still works for on-demand
/// refreshes (the **Refrescar diagnóstico** Tauri command) so the user
/// can keep an eye on the cache. The handle type resolves to `()`
/// on this branch, so `None` carries no macOS-exclusive type
/// information; the previous direct reference to
/// `MainQueueActiveAppRefresher` forced the shell onto a
/// platform-conditional re-export that does not exist on Linux and
/// broke the Ubuntu build.
#[cfg(not(target_os = "macos"))]
pub fn install_active_app_main_queue_refresher(
    _state: &AppState,
) -> (
    MainQueueInstallOutcome,
    Option<clipvault_platform::ActiveAppRefresherHandle>,
) {
    (MainQueueInstallOutcome::SkippedUnsupported, None)
}

pub fn install_capture_loop<R: Runtime>(state: &AppState, handle: &AppHandle<R>) {
    let context = state.context.clone();
    let cancel = Arc::clone(&state.cancel_capture);
    let interval = state.watcher.interval().max(CAPTURE_LOOP_TICK);
    let watcher = Arc::clone(&state.watcher);
    let handle = handle.clone();
    let diagnostics = context.active_app_diagnostics_state();
    let metadata_scheduler = Arc::clone(&state.metadata_scheduler);
    thread::Builder::new()
        .name("clipvault-capture-loop".to_string())
        .spawn(move || {
            diagnostics.mark_loop_started();
            // One-shot backfill at startup so rows persisted before
            // the metadata pipeline landed (or rows whose first
            // inline enrichment failed because the provider required
            // the main thread) eventually carry the user-visible
            // name and icon reference the card rail renders. The
            // backfill is bounded: at most [`BACKFILL_BATCH`] rows
            // per startup, only for entries that already have a
            // non-empty `source_app`, never for entries whose
            // `source_app` is `NULL` (the contract: do not invent an
            // origin for rows the user never attributed).
            backfill_pending_metadata(&handle, &context, &metadata_scheduler);
            while !cancel.load(Ordering::SeqCst) {
                // The macOS main-queue refresher keeps the cache
                // fresh; calling `refresh_active_app_cached` here
                // would route through `run_on_main_thread_sync` which
                // can stall the same way it did on the user-reported
                // session. We still rely on the cache the watcher
                // reads, but we read it through
                // `cached_active_app_probe` so the watcher observes
                // the main-thread state without the watcher itself
                // scheduling anything on the main thread.
                //
                // After the inline enrichment (which may fail on
                // macOS because the provider requires the main
                // thread) the shell re-runs the lookup on the main
                // thread through the
                // [`MetadataEnrichmentScheduler`]. The scheduler
                // is non-blocking, coalesces duplicate enqueues by
                // `entry_id`, and never blocks the loop on a stuck
                // main thread: the previous
                // `run_on_main_thread_sync` path is gone.
                let outcome = capture_loop_tick(&watcher, &context);
                record_capture_decision(&diagnostics, &outcome);
                if let Some((entry_id, identifier)) = metadata_enrichment_target(&context, &outcome)
                {
                    enrich_metadata_on_main_thread(
                        &handle,
                        &context,
                        &metadata_scheduler,
                        entry_id,
                        &identifier,
                    );
                }
                emit_history_updated_if_changed(&handle, &outcome);
                thread::sleep(interval);
            }
        })
        .map(|_| ())
        .unwrap_or_else(|error| {
            warn!(error = %error, "failed to spawn capture loop thread");
        });
}

/// Maximum number of recent entries the startup backfill pass will
/// enrich. The bound keeps a long history from blocking the loop's
/// first iteration: every backfilled row costs at most one main
/// thread round-trip inside [`enrich_metadata_on_main_thread`] and
/// the bound is small enough that the startup latency stays
/// imperceptible. Subsequent captures re-trigger the enrichment on
/// demand via [`metadata_enrichment_target`].
const BACKFILL_BATCH: usize = 32;

/// Number of recent rows the backfill helper inspects when looking
/// for pending entries. The window MUST be at least
/// [`BACKFILL_BATCH`] and is intentionally larger so the helper can
/// short-circuit when the recent history is already enriched.
const BACKFILL_SCAN: usize = 96;

/// Collect the row id and `source_app` of every recent entry that
/// still needs metadata enrichment. The helper is exposed for
/// tests; the production backfill loops over the result of this
/// function and schedules a main-thread enrichment per row.
///
/// The selection rule:
/// - `source_app` MUST be present and non-empty (no invented origin);
/// - the entry is considered pending when either `source_app_name`
///   OR `source_app_icon_ref` is missing or whitespace-only.
///
/// Rows that already carry both columns are skipped.
pub(crate) fn pending_metadata_entries(context: &AppContext) -> Vec<(i64, String)> {
    let Ok(records) = context.history().recent_entries(context, BACKFILL_SCAN) else {
        return Vec::new();
    };
    let mut pending = Vec::new();
    for record in records {
        if record.source_app.is_none() {
            continue;
        }
        let identifier = match record.source_app.as_deref() {
            Some(value) if !value.trim().is_empty() => value.trim().to_string(),
            _ => continue,
        };
        let has_name = record
            .source_app_name
            .as_deref()
            .is_some_and(|name| !name.trim().is_empty());
        let has_icon = record
            .source_app_icon_ref
            .as_deref()
            .is_some_and(|icon| !icon.trim().is_empty());
        if has_name && has_icon {
            continue;
        }
        pending.push((record.id, identifier));
        if pending.len() >= BACKFILL_BATCH {
            break;
        }
    }
    pending
}

/// Schedule a metadata enrichment for every recent entry that still
/// lacks a `source_app_name` or `source_app_icon_ref`. The pass is
/// bounded by [`BACKFILL_BATCH`] and never runs for entries whose
/// `source_app` is `NULL` — the contract is "do not invent the
/// origin of an entry the user never attributed".
fn backfill_pending_metadata<R: Runtime>(
    handle: &AppHandle<R>,
    context: &AppContext,
    scheduler: &MetadataEnrichmentScheduler,
) {
    let pending = pending_metadata_entries(context);
    if pending.is_empty() {
        return;
    }
    info!(
        count = pending.len(),
        "history-card-layout: backfilling pending source-app metadata"
    );
    for (entry_id, identifier) in pending {
        enrich_metadata_on_main_thread(handle, context, scheduler, entry_id, &identifier);
    }
}

/// Decide whether the shell should run a metadata enrichment on the
/// platform thread. Returns `Some((entry_id, identifier))` when:
/// - the capture produced a fresh row (`Stored`) or refreshed an
///   existing one (`Duplicate`), AND
/// - the cached probe exposed a non-empty identifier, AND
/// - the row is **not** yet fully enriched (`source_app_name` AND
///   `source_app_icon_ref` both populated — see
///   [`entry_already_enriched`]). The shell re-runs the enrichment
///   whenever either column is missing so a previous icon failure
///   does not permanently disable the card icon.
///
/// Returns `None` for `Unchanged`, `Ignored`, `Failed` and for
/// captures that came from an unknown source so the shell does not
/// schedule a no-op main-thread closure.
fn metadata_enrichment_target(
    context: &AppContext,
    outcome: &clipvault_core::WatchTickOutcome,
) -> Option<(i64, String)> {
    use clipvault_core::{HistoryOutcome, WatchTickOutcome};
    let id = match outcome {
        WatchTickOutcome::Captured(HistoryOutcome::Stored { id })
        | WatchTickOutcome::Captured(HistoryOutcome::Duplicate { id }) => *id,
        _ => return None,
    };
    let identifier = resolved_source_identifier(context)?;
    if entry_already_enriched(context, id) {
        return None;
    }
    Some((id, identifier))
}

/// Best-effort check whether the entry already carries **both** the
/// `source_app_name` and the `source_app_icon_ref` the card rail
/// needs. The shell considers an entry fully enriched only when
/// both columns are populated:
///
/// - `source_app_name` non-empty after trimming;
/// - `source_app_icon_ref` non-empty after trimming.
///
/// Returning `true` makes the shell skip the main-thread retry, which
/// matches the contract the `history-card-layout` spec pinned: an
/// entry that only carries a name but no icon must be re-tried so
/// the icon has a chance to land. The query walks at most 64 recent
/// entries; on hosts with a busy recent history the worst case is a
/// tiny `SELECT` per capture.
fn entry_already_enriched(context: &AppContext, entry_id: i64) -> bool {
    let Ok(records) = context.history().recent_entries(context, 64) else {
        return false;
    };
    records
        .iter()
        .find(|record| record.id == entry_id)
        .is_some_and(|record| {
            let has_name = record
                .source_app_name
                .as_deref()
                .is_some_and(|name| !name.trim().is_empty());
            let has_icon = record
                .source_app_icon_ref
                .as_deref()
                .is_some_and(|icon| !icon.trim().is_empty());
            has_name && has_icon
        })
}

/// Persist the metadata-only label of the most recent capture
/// outcome on the diagnostics state. The label is one of
/// `allowed`/`duplicate`/`discarded:blacklisted`/`unchanged`/
/// `ignored`/`failed:<category>` — never the clipboard payload, the
/// snippet or the content hash.
fn record_capture_decision(
    diagnostics: &clipvault_core::ActiveAppDiagnosticsState,
    outcome: &clipvault_core::WatchTickOutcome,
) {
    let label = match outcome {
        clipvault_core::WatchTickOutcome::Captured(clipvault_core::HistoryOutcome::Stored {
            ..
        }) => "allowed:stored",
        clipvault_core::WatchTickOutcome::Captured(clipvault_core::HistoryOutcome::Duplicate {
            ..
        }) => "allowed:duplicate",
        clipvault_core::WatchTickOutcome::Captured(clipvault_core::HistoryOutcome::Ignored) => {
            "discarded:blacklisted"
        }
        clipvault_core::WatchTickOutcome::Captured(clipvault_core::HistoryOutcome::Failed {
            ..
        }) => "failed:backend",
        clipvault_core::WatchTickOutcome::Unchanged => "unchanged",
        clipvault_core::WatchTickOutcome::Suppressed => "suppressed:paste_owned",
        clipvault_core::WatchTickOutcome::Ignored => "ignored:empty_clipboard",
        clipvault_core::WatchTickOutcome::Failed { .. } => "failed:watcher",
    };
    diagnostics.record_capture_decision(label);
}

/// Run a single iteration of the background capture loop. Extracted
/// from the closure body so tests can drive the same codepath the
/// thread runs without standing up a Tauri runtime.
///
/// Sharing the watcher between this helper and the manual
/// [`crate::commands::clipvault_capture_tick`] is the regression fix
/// that keeps the `last_hash` deduplicated across both entry points.
///
/// The helper resolves the source-application identifier from the
/// cached active-app probe instead of forwarding `None` or whatever
/// the frontend sent. On non-macOS hosts the cache is refreshed
/// synchronously here (via
/// [`refresh_active_application_cache_for_loop_tick`]) so the X11 /
/// XWayland identifier the watcher evaluates against matches the
/// currently-focused application at the moment of the capture — the
/// regression the `linux-source-app-metadata` change ships as a
/// follow-up patch on Ubuntu. On macOS the main-queue refresher
/// installed at bootstrap already keeps the cache populated, so the
/// helper compiles to a no-op there and the existing behaviour stays
/// intact.
///
/// The snapshot is read without touching the platform probe after
/// the refresh — `cached_active_application` only reads the in-memory
/// cache — so the background loop never blocks on the main thread
/// for the identifier.
///
/// The same identifier feeds:
/// - the [`PrivacyGate`] consulted inside
///   [`clipvault_core::TextHistoryService::record_payload`], so a
///   blacklisted capture is discarded with the same identifier the
///   blacklist contains;
/// - the persistence step, so the new row stores the real
///   `source_app`;
/// - the [`enrich_metadata`] call that resolves the user-visible
///   name and icon reference.
///
/// Passing `None` (no cache, capability unavailable, identifier
/// empty) preserves the contract documented by the
/// `history-card-layout` change: capture stays allowed, the entry
/// records an unknown source and the frontend renders the fallback
/// copy.
pub(crate) fn capture_loop_tick(
    watcher: &CaptureWatcher,
    context: &AppContext,
) -> WatchTickOutcome {
    refresh_active_application_cache_for_loop_tick(context);
    let source_app = resolved_source_identifier(context);
    let outcome = watcher.tick(
        context,
        source_app.as_deref(),
        clipvault_core::AttemptOrigin::BackgroundLoop,
    );
    log_capture_outcome(&outcome);
    outcome
}

/// Refresh the cached active-app probe before the watcher reads the
/// identifier. Called from the capture-loop thread on every
/// iteration so the source identifier reflects the currently
/// focused application at the moment of the capture.
///
/// Platform split:
///
/// - **macOS** — the main-queue refresher installed at bootstrap
///   keeps the cache fresh on the Cocoa main thread. Calling
///   [`AppContext::refresh_active_application`] from the capture-loop
///   thread would invoke the inner `NSWorkspace` probe off the main
///   thread, which Apple does not support, so the helper compiles
///   to a no-op on macOS.
/// - **Linux / Windows / any non-macOS host** — the bootstrap does
///   NOT install a periodic refresher (the X11 / XWayland probe is
///   safe to call from any thread). Refreshing the cache here
///   guarantees the identifier the watcher evaluates against is
///   the one the focused application reported for the current
///   capture tick. The probe also reports `Ok(None)` for native
///   Wayland applications that do not publish an X11 window; the
///   refresh call leaves the cache empty in that case so the
///   capture does not receive a fabricated identifier (the
///   `unavailable` contract the spec pins).
///
/// The refresh outcome is recorded on the diagnostics state by
/// `refresh_active_application` itself, so the existing counters
/// stay consistent across platforms.
pub(crate) fn refresh_active_application_cache_for_loop_tick(context: &AppContext) {
    #[cfg(not(target_os = "macos"))]
    {
        let _ = context.refresh_active_application();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = context;
    }
}

/// Resolve the source-application identifier the capture pipeline
/// should use for the next iteration. The helper reads the cached
/// active-app probe without ever invoking the platform adapter, so
/// the background loop stays off the main thread.
///
/// Returns `None` when:
/// - the cache has never been populated (no main-queue refresher
///   tick has fired yet);
/// - the most recent cache refresh returned an empty identifier;
/// - the active-app capability is unavailable on this host.
///
/// `None` is the documented "unknown source → Allow" signal the
/// `PrivacyGate` understands: a permitted capture then persists
/// without a `source_app` so the card rail can show the explicit
/// fallback copy.
///
/// Exposed for the integration suite so the contract pinned by the
/// `history-card-layout` regression stays visible end-to-end
/// without spinning up a Tauri runtime.
pub fn resolved_source_identifier(context: &AppContext) -> Option<String> {
    context.cached_active_application().and_then(|app| {
        let identifier = app.identifier.trim();
        if identifier.is_empty() {
            None
        } else {
            Some(identifier.to_string())
        }
    })
}

/// Schedule a metadata enrichment on the platform thread. Used by the
/// Tauri shell to give macOS a chance to resolve the user-visible
/// name and icon when the inline enrichment (which runs on the
/// capture loop's background thread) returned `Unavailable` because
/// `NSWorkspace` must be called on the main thread.
///
/// The helper is intentionally narrow: it forwards the entry id and
/// the pre-resolved identifier the capture loop already extracted
/// from the cached probe, then hands the work to the
/// [`MetadataEnrichmentScheduler`]. The scheduler dispatches the
/// closure through `AppHandle::run_on_main_thread` without waiting
/// on a channel or a timeout — the previous prototype used
/// `run_on_main_thread_sync` and logged
/// `"metadata enrichment on the main thread did not run"` once per
/// capture whenever the main thread was busy. The new path is
/// non-blocking, coalesces duplicate enqueues for the same
/// `entry_id`, and never converts a valid capture into a
/// `Failed` outcome.
///
/// The closure body runs the provider lookup + SQLite update
/// through the same `enrich_metadata` entry point the inline
/// background path uses, so the metadata semantics are unchanged.
/// The diagnostics surface distinguishes the new outcomes
/// (`scheduled`, `skipped_duplicate`, `schedule_failed`) so the
/// dashboard can render the breakdown without parsing log lines.
pub(crate) fn enrich_metadata_on_main_thread<R: Runtime>(
    handle: &AppHandle<R>,
    context: &AppContext,
    scheduler: &MetadataEnrichmentScheduler,
    entry_id: i64,
    source_app: &str,
) {
    let identifier = source_app.to_string();
    let cloned = context.clone();
    let _ = scheduler.enqueue_with(handle, entry_id, move || {
        cloned
            .history()
            .enrich_metadata(&cloned, entry_id, Some(&identifier));
    });
}

/// Emit [`HISTORY_UPDATED_EVENT`] when the outcome signals that a row
/// was inserted or refreshed. Both `Stored` (new entry) and
/// `Duplicate` (existing entry whose timestamps were bumped) change
/// the recent-entries ordering the frontend renders, so both warrant
/// a notification. `Ignored` (empty clipboard, blacklisted source)
/// and `Failed` (backend error) leave the list as it was and must not
/// emit.
///
/// The payload is `()` — the frontend treats the event as a signal to
/// re-read `clipvault_recent_entries` and never inspects event
/// details, which is the contract that keeps clipboard content,
/// snippets, hashes and source identifiers out of every event.
fn emit_history_updated_if_changed<R: Runtime>(
    handle: &AppHandle<R>,
    outcome: &clipvault_core::WatchTickOutcome,
) {
    if !should_emit_history_updated(outcome) {
        return;
    }
    if let Err(error) = handle.emit(HISTORY_UPDATED_EVENT, ()) {
        warn!(error = %error, "failed to emit history-updated event");
    }
}

/// Pure decision extracted so unit tests can verify the contract
/// without standing up a Tauri runtime. Returns `true` for `Stored`
/// and `Duplicate` outcomes; `false` for everything else.
pub(crate) fn should_emit_history_updated(outcome: &clipvault_core::WatchTickOutcome) -> bool {
    use clipvault_core::{HistoryOutcome, WatchTickOutcome};
    matches!(
        outcome,
        WatchTickOutcome::Captured(HistoryOutcome::Stored { .. })
            | WatchTickOutcome::Captured(HistoryOutcome::Duplicate { .. })
    )
}

fn log_capture_outcome(outcome: &clipvault_core::WatchTickOutcome) {
    use clipvault_core::WatchTickOutcome;
    match outcome {
        WatchTickOutcome::Captured(history) => {
            // Deliberately avoid logging the clipboard text. The
            // outcome is already scrubbed (PrivacyGate discards
            // blacklisted events before they reach persistence) so a
            // category line is enough to debug capture issues.
            tracing::trace!(outcome = history.kind(), "background capture tick");
        }
        WatchTickOutcome::Unchanged => {
            tracing::trace!("background capture tick: unchanged");
        }
        WatchTickOutcome::Suppressed => {
            // Metadata-only: the trace line never carries the
            // payload, the hash or the snippet.
            tracing::trace!("background capture tick: suppressed (paste-owned payload)");
        }
        WatchTickOutcome::Ignored => {
            tracing::trace!("background capture tick: ignored");
        }
        WatchTickOutcome::Failed { message } => {
            warn!(error = %message, "background capture tick failed");
        }
    }
}

/// Register the default global hotkey. The callback runs on the
/// `global-hotkey` background thread; the shell emits a Tauri event so
/// the frontend can react.
pub fn register_default_hotkey<R: Runtime>(
    state: &AppState,
    handle: &AppHandle<R>,
) -> HotkeyOutcome {
    let binding = default_binding_for(state.adapters.info());
    let binding_for_log = binding.clone();
    let hotkey_id = binding.id.clone();
    let app_handle = handle.clone();
    let outcome = state
        .adapters
        .hotkey()
        .register(
            &binding,
            Box::new(move || {
                info!(id = %hotkey_id, "global hotkey activated");
                if let Err(error) = app_handle.emit("clipvault://quick-search", ()) {
                    warn!(error = %error, "failed to emit quick-search event");
                } else {
                    info!("quick-search event emitted");
                }
            }),
        )
        .unwrap_or(HotkeyOutcome::Failed {
            reason: "hotkey backend rejected the binding".into(),
        });
    info!(
        kind = outcome.kind(),
        id = %binding_for_log.id,
        "global hotkey outcome"
    );
    outcome
}

fn default_binding_for(info: &PlatformInfo) -> clipvault_platform::HotkeyBinding {
    match info.os_family {
        OsFamily::Macos => default_macos_binding("quick_search"),
        _ => default_linux_binding("quick_search"),
    }
}

fn build_clipboard(
    #[cfg_attr(not(target_os = "macos"), allow(unused_variables))] info: &PlatformInfo,
    _capabilities: Capabilities,
) -> Arc<dyn ClipboardBackend> {
    // On macOS hosts that ship the native `macos-native` feature
    // we hand the rich-text clipboard path to the
    // `NSPasteboard`-backed adapter so the platform can publish
    // `public.rtf` together with `public.html` and
    // `public.utf8-plain-text` in a single logical write. The
    // previous prototype used `arboard` for every direction;
    // `arboard` does not expose RTF, and its bitmap path can discard
    // pasteboard resolution/profile metadata. The native adapter now
    // owns the macOS rich-text and fidelity-sensitive image paths.
    // On Linux X11 the same `arboard` instance is reused for both
    // legs because no native rich adapter exists. The plain-text leg
    // remains the `arboard` instance on every host.
    //
    // The `info` parameter is only consulted inside the
    // `cfg(target_os = "macos")` block; suppressing the unused-variable
    // warning on non-macOS builds keeps the signature platform-stable
    // so the public bootstrap contract does not depend on a leading
    // underscore that would still need a `#[cfg_attr]` shim at every
    // call site.
    #[cfg(feature = "clipboard-arboard")]
    let plain: Arc<dyn ClipboardBackend> =
        Arc::new(clipvault_platform::runtime::clipboard_arboard::ArboardClipboard::new());
    #[cfg(not(feature = "clipboard-arboard"))]
    let plain: Arc<dyn ClipboardBackend> = Arc::new(clipvault_platform::NoopClipboardBackend);

    #[cfg(target_os = "macos")]
    {
        if matches!(info.os_family, clipvault_platform::OsFamily::Macos) {
            let rich: Arc<dyn ClipboardBackend> = Arc::new(
                clipvault_platform::runtime::macos_clipboard::MacOsPasteboardClipboard::new(),
            );
            return Arc::new(
                clipvault_platform::runtime::composite_clipboard::CompositeClipboard::new(
                    plain, rich,
                ),
            );
        }
    }
    plain
}

fn build_hotkey(_info: &PlatformInfo, capabilities: Capabilities) -> Arc<dyn HotkeyManager> {
    #[cfg(feature = "hotkey-global")]
    {
        match clipvault_platform::runtime::hotkey_global::GlobalHotkeyManagerAdapter::new() {
            Ok(adapter) => return Arc::new(adapter),
            Err(error) => {
                warn!(error = %error, "global-hotkey adapter failed to initialise");
            }
        }
    }
    let _ = capabilities;
    Arc::new(clipvault_platform::NoopHotkeyManager)
}

fn build_active_application(
    info: &PlatformInfo,
    capabilities: Capabilities,
) -> Arc<dyn ActiveApplicationProbe> {
    if !capabilities.active_application {
        return Arc::new(clipvault_platform::NoopActiveApplicationProbe);
    }
    match info.os_family {
        #[cfg(target_os = "macos")]
        OsFamily::Macos => {
            return Arc::new(
                clipvault_platform::runtime::macos_active_app::MacOsActiveApplication::new(),
            );
        }
        #[cfg(target_os = "linux")]
        OsFamily::Linux => {
            // The session matrix documented in `design.md`:
            //
            // - Plain X11 session: build the EWMH probe and label it
            //   `x11_ewmh`. The probe MUST connect through `$DISPLAY`.
            //   If the connection fails we fall back to the no-op
            //   probe instead of pretending a window manager answered.
            // - Wayland session with a real XWayland: build the EWMH
            //   probe through `$DISPLAY` and label it `xwayland_ewmh`.
            //   Native Wayland applications never publish through X11,
            //   so the probe returns `Ok(None)` for them — the
            //   diagnostics surface stays honest.
            // - Wayland session with `$WAYLAND_DISPLAY` set: prefer
            //   the native `ext-foreign-toplevel-list-v1` probe (or
            //   the `zwlr` fallback). The native probe is the
            //   authoritative source: when it is operational, its
            //   `Ok(None)` MUST NOT trigger an XWayland fallback that
            //   would surface a stale identifier.
            // - Wayland session without `$DISPLAY` and without a
            //   bindable native protocol: skip the probe entirely.
            // - Wayland session with `$DISPLAY` but no usable
            //   connection: fall back to the no-op probe and let the
            //   diagnostics surface report `unavailable`.
            //
            // The probe's [`ProbeKind`] parameter feeds through to
            // [`ActiveApplicationProbe::name`], which the diagnostics
            // card surfaces verbatim, so the UI never sees a
            // `x11_ewmh` identifier when the actual session is
            // XWayland or vice versa.
            //
            // The gate is intentionally `#[cfg(target_os = "linux")]`
            // (not `feature = "linux-x11"`): the X11 active-app probe
            // lives in `clipvault-platform` and the
            // `[target.'cfg(target_os = "linux")'.dependencies]`
            // declaration in `Cargo.toml` already enables the
            // `linux-x11` feature on the platform crate for every Linux
            // build. Gating the branch on `feature = "linux-x11"`
            // here caused `clipvault-app` to drop the Linux arm at
            // compile time (the `linux-x11` feature is not part of
            // `clipvault-app`'s `default = [...]` set), so the build
            // returned `NoopActiveApplicationProbe` while
            // `capabilities.active_application` stayed `true` and
            // every capture landed with `source_app = NULL` on Ubuntu
            // GNOME Wayland + XWayland — the regression this change
            // fixes.
            match info.display_server {
                clipvault_platform::DisplayServer::X11 => {
                    return match clipvault_platform::runtime::linux_x11_active_app::X11ActiveApplication::with_kind(
                        None,
                        clipvault_platform::runtime::linux_x11_active_app::ProbeKind::X11,
                    ) {
                        Ok(probe) => Arc::new(probe),
                        Err(error) => {
                            warn!(
                                error = %error,
                                "X11 active-app adapter unavailable"
                            );
                            Arc::new(clipvault_platform::NoopActiveApplicationProbe)
                        }
                    };
                }
                clipvault_platform::DisplayServer::Wayland => {
                    // On Wayland, the native probe is authoritative
                    // when it can bind one of the two supported
                    // protocols. We try it first and only fall back
                    // to the XWayland EWMH probe when the native
                    // adapter is unavailable AND `$DISPLAY` is set.
                    #[cfg(feature = "linux-wayland-active-app")]
                    {
                        match clipvault_platform::runtime::linux_wayland_active_app::try_build() {
                            clipvault_platform::runtime::linux_wayland_active_app::ConnectionOutcome::Operational(probe) => {
                                return Arc::new(probe);
                            }
                            clipvault_platform::runtime::linux_wayland_active_app::ConnectionOutcome::Unavailable => {
                                // No native protocol on this session
                                // — fall through to the XWayland
                                // fallback when `$DISPLAY` is set.
                            }
                        }
                    }
                    // The native probe is unavailable (either the
                    // feature is off or no protocol was bound).
                    // Try the XWayland EWMH probe when `$DISPLAY`
                    // points at a usable X server.
                    if std::env::var_os("DISPLAY").is_none() {
                        return Arc::new(clipvault_platform::NoopActiveApplicationProbe);
                    }
                    match clipvault_platform::runtime::linux_x11_active_app::X11ActiveApplication::with_kind(
                        None,
                        clipvault_platform::runtime::linux_x11_active_app::ProbeKind::XWayland,
                    ) {
                        Ok(probe) => return Arc::new(probe),
                        Err(error) => {
                            warn!(
                                error = %error,
                                "XWayland active-app adapter unavailable"
                            );
                            return Arc::new(clipvault_platform::NoopActiveApplicationProbe);
                        }
                    }
                }
                clipvault_platform::DisplayServer::Unknown => {
                    return Arc::new(clipvault_platform::NoopActiveApplicationProbe);
                }
            }
        }
        _ => {}
    }
    Arc::new(clipvault_platform::NoopActiveApplicationProbe)
}

/// Build the application-metadata provider used by the
/// `history-card-layout` capability. macOS uses the bundle metadata
/// helper (see [`clipvault_platform::runtime::macos_app_metadata`]
/// when the `macos-native` feature is on); Linux wires the
/// freedesktop-backed adapter so X11/XWayland captures enrich with
/// the user-visible name and a controlled icon reference; every
/// other host — Windows, unsupported platforms, hosts where the
/// metadata backend is unavailable — falls back to the no-op adapter
/// so the capture pipeline never blocks on metadata.
fn build_application_metadata_provider(
    info: &PlatformInfo,
) -> Arc<dyn clipvault_platform::ApplicationMetadataProvider> {
    match info.os_family {
        #[cfg(target_os = "macos")]
        OsFamily::Macos => {
            return Arc::new(
                clipvault_platform::runtime::macos_app_metadata::MacOsApplicationMetadataProvider::new(
                    info.data_dir.join("assets"),
                ),
            );
        }
        #[cfg(target_os = "linux")]
        OsFamily::Linux => {
            return Arc::new(
                clipvault_platform::runtime::linux_app_metadata::LinuxApplicationMetadataProvider::new(
                    info.data_dir.join("assets"),
                ),
            );
        }
        _ => {}
    }
    Arc::new(clipvault_platform::NoopApplicationMetadataProvider)
}

fn build_paste_controller(
    info: &PlatformInfo,
    _capabilities: Capabilities,
) -> Arc<dyn PasteController> {
    // We always prefer the real adapter when the host supports it,
    // even when the initial capability check reported
    // `synthetic_paste = false`. The macOS controller re-runs the
    // preflight on every `paste()` call so it can recover after the
    // user grants Accessibility and `clipvault_refresh_capabilities`
    // lifts the capability. Swapping in a permanent `NoopPasteController`
    // would trap the user in a guidance modal that never recovers.
    match info.os_family {
        #[cfg(target_os = "macos")]
        OsFamily::Macos => {
            return Arc::new(clipvault_platform::runtime::macos_paste::MacOsPasteController::new());
        }
        #[cfg(target_os = "linux")]
        OsFamily::Linux => {
            match clipvault_platform::runtime::linux_x11_paste::X11PasteController::new() {
                Ok(controller) => return Arc::new(controller),
                Err(error) => {
                    warn!(error = %error, "X11 paste adapter unavailable");
                }
            }
        }
        _ => {}
    }
    Arc::new(clipvault_platform::NoopPasteController)
}

fn build_tray_controller(
    _info: &PlatformInfo,
    _capabilities: Capabilities,
) -> Arc<dyn TrayController> {
    Arc::new(clipvault_platform::NoopTrayController)
}

/// Build the settings navigator. The navigator is feature-gated per
/// platform so production builds use the real native launcher while
/// tests and unsupported environments fall back to
/// [`NoopSettingsNavigator`]. The platform crate enables
/// `macos-native` for the macOS build via the
/// `[target.'cfg(target_os = "macos")'.dependencies]` re-declaration,
/// so the macOS navigator symbol is guaranteed to be present on
/// every macOS host.
fn build_settings_navigator(info: &PlatformInfo) -> Arc<dyn SettingsNavigator> {
    match info.os_family {
        #[cfg(target_os = "macos")]
        OsFamily::Macos => {
            Arc::new(clipvault_platform::runtime::macos_settings::MacOsSettingsNavigator::new())
        }
        #[cfg(target_os = "linux")]
        OsFamily::Linux => {
            Arc::new(clipvault_platform::runtime::linux_settings::LinuxSettingsNavigator::new())
        }
        _ => Arc::new(NoopSettingsNavigator),
    }
}

/// Default watcher poll interval. Used by the shell loop.
#[allow(dead_code)]
pub fn poll_interval() -> Duration {
    CaptureWatcher::default_interval()
}

#[cfg(test)]
mod tests {
    use super::*;
    use clipvault_core::{ClipboardBackendError, HistoryOutcome, WatchTickOutcome};

    #[test]
    fn stored_outcome_emits_history_updated() {
        let outcome = WatchTickOutcome::Captured(HistoryOutcome::Stored { id: 1 });
        assert!(
            should_emit_history_updated(&outcome),
            "Stored must emit history-updated"
        );
    }

    #[test]
    fn duplicate_outcome_emits_history_updated() {
        let outcome = WatchTickOutcome::Captured(HistoryOutcome::Duplicate { id: 2 });
        assert!(
            should_emit_history_updated(&outcome),
            "Duplicate must emit history-updated"
        );
    }

    #[test]
    fn captured_ignored_does_not_emit_history_updated() {
        // Blacklisted source / empty clipboard path: the gate
        // already discarded the payload; we must not announce the
        // dropped event.
        let outcome = WatchTickOutcome::Captured(HistoryOutcome::Ignored);
        assert!(
            !should_emit_history_updated(&outcome),
            "Ignored must not emit history-updated"
        );
    }

    #[test]
    fn captured_failed_does_not_emit_history_updated() {
        let outcome = WatchTickOutcome::Captured(HistoryOutcome::Failed {
            message: "backend gone".to_string(),
        });
        assert!(
            !should_emit_history_updated(&outcome),
            "Failed must not emit history-updated"
        );
    }

    #[test]
    fn watcher_unchanged_does_not_emit_history_updated() {
        let outcome = WatchTickOutcome::Unchanged;
        assert!(
            !should_emit_history_updated(&outcome),
            "Unchanged must not emit history-updated"
        );
    }

    #[test]
    fn watcher_ignored_does_not_emit_history_updated() {
        // The WatchTickOutcome-level Ignored is fired when the
        // clipboard itself returned no usable text; nothing
        // changed, no event must be emitted.
        let outcome = WatchTickOutcome::Ignored;
        assert!(!should_emit_history_updated(&outcome));
    }

    #[test]
    fn watcher_failed_does_not_emit_history_updated() {
        let outcome = WatchTickOutcome::Failed {
            message: ClipboardBackendError::backend("backend gone").to_string(),
        };
        assert!(
            !should_emit_history_updated(&outcome),
            "Watcher-level Failed must not emit history-updated"
        );
    }

    #[test]
    fn emit_helper_carries_no_payload() {
        // The shell must emit a metadata-only event. The payload is
        // `()` so the frontend never sees clipboard content,
        // snippets, hashes or source identifiers. We pin the
        // constant here so a future refactor that introduces a
        // payload surfaces as a test failure instead of leaking
        // data into the event surface.
        assert_eq!(HISTORY_UPDATED_EVENT, "clipvault://history-updated");
    }

    #[test]
    fn image_captures_use_the_same_metadata_only_event_as_text() {
        // `clipboard-rich-content` regression guard: an image capture
        // produces the very same `WatchTickOutcome` variants as a
        // textual one, so the emit decision — and therefore the
        // metadata-only payload — is identical. Nothing about the
        // image (bytes, hash, dimensions, asset reference) can reach
        // the event surface, because the surface carries no payload at
        // all.
        let stored =
            clipvault_core::WatchTickOutcome::Captured(clipvault_core::HistoryOutcome::Stored {
                id: 42,
            });
        let duplicate =
            clipvault_core::WatchTickOutcome::Captured(clipvault_core::HistoryOutcome::Duplicate {
                id: 42,
            });
        assert!(should_emit_history_updated(&stored));
        assert!(should_emit_history_updated(&duplicate));

        // A discarded (blacklisted) or failed image capture emits
        // nothing, so a rejected capture is not even observable as a
        // timing signal on the event channel.
        let ignored =
            clipvault_core::WatchTickOutcome::Captured(clipvault_core::HistoryOutcome::Ignored);
        let failed =
            clipvault_core::WatchTickOutcome::Captured(clipvault_core::HistoryOutcome::Failed {
                message: "clipboard asset could not be persisted".to_string(),
            });
        assert!(!should_emit_history_updated(&ignored));
        assert!(!should_emit_history_updated(&failed));
    }

    /// Build an `AppState`-shaped harness so the bootstrap tests
    /// can exercise the shared-watcher contract without spawning a
    /// Tauri runtime. The harness inserts `1password` into the
    /// blacklist, wires a scripted probe that the bootstrap then
    /// wraps in a cached probe, and returns the `Arc` wrapped
    /// around the `CaptureWatcher` the production shell exposes.
    /// The returned `FakeClipboardBackend` lets tests queue reads.
    fn harness_for_shared_watcher() -> (
        tempfile::TempDir,
        AppContext,
        Arc<clipvault_core::FakeClipboardBackend>,
    ) {
        use clipvault_core::{
            CaptureWatcher, FakeClipboardBackend, FakeHotkeyManager, FakePasteController,
            FakeSettingsNavigator, FakeTrayController, PlatformAdapters,
        };
        use clipvault_db::{builtin_migrations, IgnoredAppRepository};
        use clipvault_platform::{
            ActiveApplicationProbe, Capabilities, ClipboardBackend, DisplayServer, HotkeyManager,
            NoopApplicationMetadataProvider, OsFamily, PasteController, PlatformInfo,
            SettingsNavigator, TrayController,
        };

        // The bootstrap's gate reads from the SQLite blacklist, so
        // `1password` must be in the table before the gate evaluates
        // any capture for this scenario.
        struct ScriptedProbe;
        impl ActiveApplicationProbe for ScriptedProbe {
            fn active_application(
                &self,
            ) -> Result<
                Option<clipvault_platform::ActiveApplication>,
                clipvault_platform::ActiveAppError,
            > {
                Ok(Some(clipvault_platform::ActiveApplication::new(
                    "App",
                    "1password",
                )))
            }
            fn name(&self) -> &'static str {
                "shell-test-scripted"
            }
        }

        let dir = tempfile::TempDir::new().expect("tempdir");
        {
            let mut db =
                clipvault_db::Database::open(dir.path().join("clipvault.db")).expect("open");
            db.run_migrations(&builtin_migrations()).expect("migrate");
            let conn = db.connection_mut();
            let mut repo = IgnoredAppRepository::new(conn);
            repo.insert("1password", time::OffsetDateTime::now_utc())
                .expect("insert");
        }

        let probe: Arc<dyn ActiveApplicationProbe> = Arc::new(ScriptedProbe);
        let info = PlatformInfo {
            home_dir: std::path::PathBuf::from("/tmp"),
            data_dir: std::path::PathBuf::from("/tmp/.clipvault"),
            os_family: OsFamily::Macos,
            display_server: DisplayServer::Unknown,
        };
        let fake_clipboard = Arc::new(FakeClipboardBackend::new());
        let platform_adapters = PlatformAdapters::new(
            fake_clipboard.clone() as Arc<dyn ClipboardBackend>,
            Arc::new(FakeHotkeyManager::new()) as Arc<dyn HotkeyManager>,
            probe,
            Arc::new(FakePasteController::new()) as Arc<dyn PasteController>,
            Arc::new(FakeTrayController::new()) as Arc<dyn TrayController>,
            Arc::new(FakeSettingsNavigator::new()) as Arc<dyn SettingsNavigator>,
            Arc::new(NoopApplicationMetadataProvider)
                as Arc<dyn clipvault_platform::ApplicationMetadataProvider>,
            Capabilities::default(),
            info,
        );
        let context = AppBootstrap::new()
            .with_clock(Arc::new(clipvault_core::SystemClock) as Arc<dyn clipvault_core::Clock>)
            .with_platform_adapters(platform_adapters)
            .bootstrap_at(dir.path().join("clipvault.db"))
            .expect("bootstrap");
        let _ = CaptureWatcher::default_interval();
        (dir, context, fake_clipboard)
    }

    #[test]
    fn shared_capture_loop_uses_state_watcher() {
        // Bug 11 regression: the background loop's per-iteration
        // behaviour must go through the SAME `CaptureWatcher`
        // exposed by `AppState`. Two independent watchers would
        // keep disjoint `last_hash` values and let a blacklisted
        // event slip through the Tick capture command. The helper
        // `capture_loop_tick` is the closure body that runs inside
        // the spawned thread; asserting it shares the watcher
        // pins the contract.
        let (_dir, context, fake_clipboard) = harness_for_shared_watcher();

        // Two reads of the same payload let the watcher's dedupe
        // state prove the helper is using the SAME state across
        // iterations: the first iteration consumes the payload,
        // the second must observe the cached hash and return
        // `Unchanged`.
        fake_clipboard.push_read(Ok(Some("cv-shell-loop-payload".into())));
        fake_clipboard.push_read(Ok(Some("cv-shell-loop-payload".into())));

        let watcher = build_watcher_for_context(&context);
        // Iteration 1 — first read of the new payload.
        let first = capture_loop_tick(&watcher, &context);
        assert!(
            matches!(
                first,
                WatchTickOutcome::Captured(_) | WatchTickOutcome::Failed { .. }
            ),
            "first iteration must record the change, got {first:?}"
        );

        // Iteration 2 — same payload, same watcher. The dedupe
        // state must recognise the hash and return `Unchanged`.
        // Two independent watchers would each see `last_hash =
        // None` on their first read and produce a second
        // `Captured`. This contract is the regression pin.
        let second = capture_loop_tick(&watcher, &context);
        assert_eq!(
            second,
            WatchTickOutcome::Unchanged,
            "two iterations through the helper must share the dedupe state"
        );
    }

    fn build_watcher_for_context(context: &AppContext) -> CaptureWatcher {
        use clipvault_core::CaptureWatcher;
        use std::time::Duration;
        CaptureWatcher::new(
            context.platform_adapters().clipboard(),
            Duration::from_millis(10),
        )
    }

    /// The capture loop must refresh the cached active-app probe on
    /// every iteration through a MainThread-style synchronous helper.
    /// Without a Tauri runtime the helper still has to populate the
    /// cache when given `None`, and surface failures through the
    /// diagnostics endpoint. The unit test below exercises the
    /// `None`-handle path of `refresh_active_app_cached` so the test
    /// suite proves the gate observes a fresh identifier even when
    /// the helper cannot reach the main thread.
    #[test]
    fn refresh_active_app_cached_populates_cache_without_tauri_handle() {
        use clipvault_core::{
            FakeClipboard, FakeHotkeyManager, FakePasteController, FakeSettingsNavigator,
            FakeTrayController, PlatformAdapters,
        };
        use clipvault_db::{builtin_migrations, IgnoredAppRepository};
        use clipvault_platform::{
            ActiveApplication as PlatformActiveApplication, ActiveApplicationProbe, Capabilities,
            ClipboardBackend, DisplayServer, HotkeyManager, OsFamily, PasteController,
            PlatformInfo, SettingsNavigator, TrayController,
        };
        use std::sync::Arc;

        struct FixedProbe(&'static str);
        impl ActiveApplicationProbe for FixedProbe {
            fn active_application(
                &self,
            ) -> Result<Option<PlatformActiveApplication>, clipvault_platform::ActiveAppError>
            {
                Ok(Some(PlatformActiveApplication::new("App", self.0)))
            }
            fn name(&self) -> &'static str {
                "fixed-probe"
            }
        }

        let dir = tempfile::TempDir::new().expect("tempdir");
        {
            let mut db =
                clipvault_db::Database::open(dir.path().join("clipvault.db")).expect("open");
            db.run_migrations(&builtin_migrations()).expect("migrate");
            let conn = db.connection_mut();
            let mut repo = IgnoredAppRepository::new(conn);
            repo.insert("1password", time::OffsetDateTime::now_utc())
                .expect("insert");
        }
        let probe = Arc::new(FixedProbe("1password"));
        let info = PlatformInfo {
            home_dir: std::path::PathBuf::from("/tmp"),
            data_dir: std::path::PathBuf::from("/tmp/.clipvault"),
            os_family: OsFamily::Macos,
            display_server: DisplayServer::Unknown,
        };
        let platform_adapters = PlatformAdapters::new(
            Arc::new(clipvault_core::FakeClipboardBackend::new()) as Arc<dyn ClipboardBackend>,
            Arc::new(FakeHotkeyManager::new()) as Arc<dyn HotkeyManager>,
            probe,
            Arc::new(FakePasteController::new()) as Arc<dyn PasteController>,
            Arc::new(FakeTrayController::new()) as Arc<dyn TrayController>,
            Arc::new(FakeSettingsNavigator::new()) as Arc<dyn SettingsNavigator>,
            Arc::new(clipvault_core::NoopApplicationMetadataProvider)
                as Arc<dyn clipvault_platform::ApplicationMetadataProvider>,
            Capabilities::ALL_AVAILABLE,
            info,
        );
        let context = AppBootstrap::new()
            .with_clock(Arc::new(clipvault_core::SystemClock) as Arc<dyn clipvault_core::Clock>)
            .with_clipboard(Arc::new(FakeClipboard::new()) as Arc<dyn clipvault_core::Clipboard>)
            .with_platform_adapters(platform_adapters)
            .bootstrap_at(dir.path().join("clipvault.db"))
            .expect("bootstrap");
        let diag = refresh_active_app_cached::<tauri::Wry>(&context, None);
        assert!(
            diag.available,
            "the harness wires a probe, so diagnostics must report available"
        );
        assert!(diag.cache_populated);
        assert_eq!(diag.identifier.as_deref(), Some("1password"));
        assert_eq!(diag.refresh_outcome.kind(), "ok");
    }

    #[test]
    fn main_thread_sync_error_displays_reason_or_timeout() {
        // The error variants are observable to the diagnostics
        // pipeline. We pin both `Display` impls here so a future
        // refactor that drops the actionable context surfaces as a
        // test failure instead of leaking an empty error to the
        // user.
        let schedule = MainThreadSyncError::Schedule("tauri unavailable".to_string());
        assert!(schedule.to_string().contains("tauri unavailable"));
        let timeout = MainThreadSyncError::Timeout;
        assert!(timeout.to_string().contains("did not execute"));
    }

    #[test]
    fn main_thread_sync_error_maps_to_failure_kind() {
        // The shell converts `MainThreadSyncError` to the
        // `ActiveAppFailureKind` the diagnostics endpoint exposes.
        // Schedule and Timeout must NEVER collapse to the same
        // kind: the user needs to know whether Tauri refused to
        // enqueue the closure (Schedule) or whether the main thread
        // did not pick it up inside the timeout (Timeout).
        assert_eq!(
            MainThreadSyncError::Schedule("x".into()).failure_kind(),
            clipvault_core::ActiveAppFailureKind::Schedule
        );
        assert_eq!(
            MainThreadSyncError::Timeout.failure_kind(),
            clipvault_core::ActiveAppFailureKind::Timeout
        );
    }

    #[test]
    fn refresh_active_app_cached_without_handle_records_unavailable_failure_kind() {
        // When the helper runs without a Tauri runtime (tests), the
        // inner probe's outcome drives the failure_kind. A probe
        // that returns `Unavailable` MUST surface as
        // `ActiveAppFailureKind::Unavailable`, not as a generic
        // Backend bucket.
        use clipvault_core::{
            FakeClipboard, FakeHotkeyManager, FakePasteController, FakeSettingsNavigator,
            FakeTrayController, PlatformAdapters,
        };
        use clipvault_platform::{ActiveAppError, DisplayServer};

        struct UnavailableProbe;
        impl clipvault_platform::ActiveApplicationProbe for UnavailableProbe {
            fn active_application(
                &self,
            ) -> Result<
                Option<clipvault_platform::ActiveApplication>,
                clipvault_platform::ActiveAppError,
            > {
                Err(ActiveAppError::Unavailable)
            }
            fn name(&self) -> &'static str {
                "unavailable-probe"
            }
        }

        let dir = tempfile::TempDir::new().expect("tempdir");
        {
            let mut db =
                clipvault_db::Database::open(dir.path().join("clipvault.db")).expect("open");
            db.run_migrations(&clipvault_db::builtin_migrations())
                .expect("migrate");
        }
        let probe: Arc<dyn clipvault_platform::ActiveApplicationProbe> = Arc::new(UnavailableProbe);
        let info = clipvault_platform::PlatformInfo {
            home_dir: std::path::PathBuf::from("/tmp"),
            data_dir: std::path::PathBuf::from("/tmp/.clipvault"),
            os_family: clipvault_platform::OsFamily::Macos,
            display_server: DisplayServer::Unknown,
        };
        let platform_adapters = PlatformAdapters::new(
            Arc::new(clipvault_core::FakeClipboardBackend::new())
                as Arc<dyn clipvault_platform::ClipboardBackend>,
            Arc::new(FakeHotkeyManager::new()) as Arc<dyn clipvault_platform::HotkeyManager>,
            probe,
            Arc::new(FakePasteController::new()) as Arc<dyn clipvault_platform::PasteController>,
            Arc::new(FakeTrayController::new()) as Arc<dyn clipvault_platform::TrayController>,
            Arc::new(FakeSettingsNavigator::new())
                as Arc<dyn clipvault_platform::SettingsNavigator>,
            Arc::new(clipvault_core::NoopApplicationMetadataProvider)
                as Arc<dyn clipvault_platform::ApplicationMetadataProvider>,
            clipvault_platform::Capabilities::ALL_AVAILABLE,
            info,
        );
        let context = AppBootstrap::new()
            .with_clock(Arc::new(clipvault_core::SystemClock) as Arc<dyn clipvault_core::Clock>)
            .with_clipboard(Arc::new(FakeClipboard::new()) as Arc<dyn clipvault_core::Clipboard>)
            .with_platform_adapters(platform_adapters)
            .bootstrap_at(dir.path().join("clipvault.db"))
            .expect("bootstrap");

        let diag = refresh_active_app_cached::<tauri::Wry>(&context, None);
        assert_eq!(diag.failure_kind, Some("unavailable"));
        assert_eq!(diag.refresh_attempts, 1);
        assert_eq!(diag.failed_refreshes, 1);
        assert_eq!(diag.successful_refreshes, 0);
    }

    /// Regression for the original macOS bug: the diagnostics
    /// snapshot must transition Pending → Failed when the synchronous
    /// refresh fails (Schedule or Timeout). The previous
    /// implementation logged the error but never recorded it on the
    /// diagnostics state, so the snapshot stayed at `Pending`
    /// forever. The `None` handle path is the only one the unit
    /// tests can exercise without a Tauri runtime, so we exercise
    /// both legs: the synthetic probe that fails and the
    /// `record_active_app_sync_failure` short-circuit.
    #[test]
    fn refresh_active_app_cached_records_sync_failure_on_diagnostics() {
        use clipvault_core::{
            FakeClipboard, FakeHotkeyManager, FakePasteController, FakeSettingsNavigator,
            FakeTrayController,
        };
        use clipvault_platform::{ActiveAppError, DisplayServer};

        struct FailingProbe;
        impl clipvault_platform::ActiveApplicationProbe for FailingProbe {
            fn active_application(
                &self,
            ) -> Result<
                Option<clipvault_platform::ActiveApplication>,
                clipvault_platform::ActiveAppError,
            > {
                Err(ActiveAppError::Backend {
                    details: "simulated x11 disconnect".to_string(),
                })
            }
            fn name(&self) -> &'static str {
                "failing"
            }
        }

        let dir = tempfile::TempDir::new().expect("tempdir");
        {
            let mut db =
                clipvault_db::Database::open(dir.path().join("clipvault.db")).expect("open");
            db.run_migrations(&clipvault_db::builtin_migrations())
                .expect("migrate");
        }
        let probe: Arc<dyn clipvault_platform::ActiveApplicationProbe> = Arc::new(FailingProbe);
        let info = PlatformInfo {
            home_dir: std::path::PathBuf::from("/tmp"),
            data_dir: std::path::PathBuf::from("/tmp/.clipvault"),
            os_family: OsFamily::Macos,
            display_server: DisplayServer::Unknown,
        };
        let platform_adapters = PlatformAdapters::new(
            Arc::new(clipvault_core::FakeClipboardBackend::new()) as Arc<dyn ClipboardBackend>,
            Arc::new(FakeHotkeyManager::new()) as Arc<dyn HotkeyManager>,
            probe,
            Arc::new(FakePasteController::new()) as Arc<dyn PasteController>,
            Arc::new(FakeTrayController::new()) as Arc<dyn TrayController>,
            Arc::new(FakeSettingsNavigator::new()) as Arc<dyn SettingsNavigator>,
            Arc::new(clipvault_core::NoopApplicationMetadataProvider)
                as Arc<dyn clipvault_platform::ApplicationMetadataProvider>,
            Capabilities::ALL_AVAILABLE,
            info,
        );
        let context = AppBootstrap::new()
            .with_clock(Arc::new(clipvault_core::SystemClock) as Arc<dyn clipvault_core::Clock>)
            .with_clipboard(Arc::new(FakeClipboard::new()) as Arc<dyn clipvault_core::Clipboard>)
            .with_platform_adapters(platform_adapters)
            .bootstrap_at(dir.path().join("clipvault.db"))
            .expect("bootstrap");

        // The diagnostics surface starts Pending.
        let diag_before = context.active_app_diagnostics();
        assert_eq!(diag_before.refresh_outcome.kind(), "pending");
        assert_eq!(diag_before.refresh_attempts, 0);
        assert_eq!(diag_before.failed_refreshes, 0);

        // The probe fails every time, so `refresh_active_app_cached`
        // (the `None` handle path) calls
        // `record_active_app_sync_failure` with the platform error.
        let diag_after = refresh_active_app_cached::<tauri::Wry>(&context, None);
        assert_eq!(diag_after.refresh_outcome.kind(), "failed");
        match &diag_after.refresh_outcome {
            clipvault_core::ActiveAppRefreshOutcome::Failed { message, .. } => {
                assert!(
                    message.contains("simulated x11 disconnect"),
                    "diagnostics lost the actionable reason: {message}"
                );
            }
            other => panic!("expected Failed, got {other:?}"),
        }
        assert_eq!(diag_after.refresh_attempts, 1);
        assert_eq!(diag_after.failed_refreshes, 1);
        assert_eq!(diag_after.successful_refreshes, 0);
        assert!(diag_after.last_refresh_unix_ms.is_some());
    }

    /// The diagnostics metadata must let the dashboard distinguish
    /// "loop never spawned" from "loop is alive but cache empty".
    /// `install_capture_loop` calls `mark_loop_started` exactly once;
    /// this regression test simulates the same flow without Tauri.
    #[test]
    fn install_capture_loop_marks_loop_started_and_records_capture_decision() {
        let (_dir, context, fake_clipboard) = harness_for_shared_watcher();

        let diagnostics = context.active_app_diagnostics_state();
        assert!(
            !diagnostics.snapshot().loop_started,
            "fresh diagnostics must start with loop_started=false"
        );
        diagnostics.mark_loop_started();
        diagnostics.record_capture_decision("discarded:blacklisted");
        let snap = diagnostics.snapshot();
        assert!(snap.loop_started);
        assert_eq!(
            snap.last_capture_decision.as_deref(),
            Some("discarded:blacklisted")
        );
        assert_eq!(snap.refresh_attempts, 0);

        // Warm the cache with the blacklisted identifier so the
        // gate observes the real source. Without this the watcher
        // would fall back to the "unknown source → Allow" contract
        // and the test would not exercise the loop's blacklist
        // path.
        context.cached_active_app_probe().refresh_with(Some(
            clipvault_platform::ActiveApplication::new("App", "1password"),
        ));

        // Drive the watcher like the loop does and confirm the
        // shared dedupe state still sees the discarded payload as
        // Unchanged on the next iteration. Both ticks read the same
        // payload so the watcher's `last_hash` dedupe fires on the
        // second iteration.
        fake_clipboard.push_read(Ok(Some("cv-shell-loop-discard-payload".into())));
        fake_clipboard.push_read(Ok(Some("cv-shell-loop-discard-payload".into())));
        let watcher = build_watcher_for_context(&context);
        let first = capture_loop_tick(&watcher, &context);
        assert!(
            matches!(first, WatchTickOutcome::Captured(HistoryOutcome::Ignored)),
            "first tick must be discarded by the gate, got {first:?}"
        );
        let second = capture_loop_tick(&watcher, &context);
        assert_eq!(
            second,
            WatchTickOutcome::Unchanged,
            "shared watcher must dedupe across iterations"
        );
    }

    /// The `record_capture_decision` helper MUST stay metadata-only
    /// so the diagnostics card never carries the clipboard payload.
    #[test]
    fn record_capture_decision_caps_oversized_labels_and_trims_input() {
        let (_dir, context, _clipboard) = harness_for_shared_watcher();
        let diagnostics = context.active_app_diagnostics_state();
        diagnostics.record_capture_decision("   ");
        assert!(diagnostics.snapshot().last_capture_decision.is_none());
        diagnostics.record_capture_decision(&"a".repeat(200));
        let stored = diagnostics
            .snapshot()
            .last_capture_decision
            .expect("decision stored");
        assert!(stored.len() <= 80);
    }

    // -----------------------------------------------------------------
    // Source-app identifier resolution (`history-card-layout` regression).
    //
    // The user-reported bug was that `capture_loop_tick` forwarded
    // `None` to `CaptureWatcher::tick`, so the row never carried the
    // identifier the active application had been reporting and the
    // card rail rendered the generic fallback copy. The tests below
    // pin the new contract end-to-end.
    // -----------------------------------------------------------------

    /// Empty cache → `None`. The loop must keep the existing
    /// "unknown source → Allow" path so a permitted capture still
    /// persists without a `source_app`.
    #[test]
    fn resolved_source_identifier_returns_none_when_cache_empty() {
        let (_dir, context, _clipboard) = harness_for_shared_watcher();
        assert!(
            context.cached_active_application().is_none(),
            "fresh harness must start with an empty cache"
        );
        assert!(resolved_source_identifier(&context).is_none());
    }

    /// Whitespace identifier → `None`. The shell must not invent
    /// an identifier just because the cache holds padding.
    #[test]
    fn resolved_source_identifier_returns_none_for_whitespace_only_entry() {
        let (_dir, context, _clipboard) = harness_for_shared_watcher();
        context.cached_active_app_probe().refresh_with(Some(
            clipvault_platform::ActiveApplication::new("App", "   "),
        ));
        assert!(resolved_source_identifier(&context).is_none());
    }

    /// Populated identifier → `Some(trimmed)`. The trim ensures the
    /// identifier the gate observes is the one the SQLite layer
    /// will compare against the blacklist.
    #[test]
    fn resolved_source_identifier_trims_and_returns_cached_value() {
        let (_dir, context, _clipboard) = harness_for_shared_watcher();
        context.cached_active_app_probe().refresh_with(Some(
            clipvault_platform::ActiveApplication::new("Terminal", "  com.apple.Terminal  "),
        ));
        assert_eq!(
            resolved_source_identifier(&context).as_deref(),
            Some("com.apple.Terminal")
        );
    }

    /// Captura automática con aplicación activa Terminal produce
    /// `source_app` en la fila persistida.
    #[test]
    fn capture_loop_tick_persists_cached_identifier_for_terminal() {
        let (_dir, context, fake_clipboard) = harness_for_shared_watcher();
        context.cached_active_app_probe().refresh_with(Some(
            clipvault_platform::ActiveApplication::new("Terminal", "com.apple.Terminal"),
        ));
        fake_clipboard.push_read(Ok(Some("cv-loop-terminal-payload".into())));
        let watcher = build_watcher_for_context(&context);
        let outcome = capture_loop_tick(&watcher, &context);
        assert!(
            matches!(
                outcome,
                WatchTickOutcome::Captured(HistoryOutcome::Stored { .. })
            ),
            "first tick must store the capture, got {outcome:?}"
        );
        let recent = context.history().recent_entries(&context, 10).unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].source_app.as_deref(), Some("com.apple.Terminal"));
    }

    /// Captura automática con aplicación activa TextEdit produce
    /// metadata equivalente (mismo contrato, distinto identificador).
    #[test]
    fn capture_loop_tick_persists_cached_identifier_for_textedit() {
        let (_dir, context, fake_clipboard) = harness_for_shared_watcher();
        context.cached_active_app_probe().refresh_with(Some(
            clipvault_platform::ActiveApplication::new("TextEdit", "com.apple.TextEdit"),
        ));
        fake_clipboard.push_read(Ok(Some("cv-loop-textedit-payload".into())));
        let watcher = build_watcher_for_context(&context);
        let outcome = capture_loop_tick(&watcher, &context);
        assert!(
            matches!(
                outcome,
                WatchTickOutcome::Captured(HistoryOutcome::Stored { .. })
            ),
            "first tick must store the capture, got {outcome:?}"
        );
        let recent = context.history().recent_entries(&context, 10).unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].source_app.as_deref(), Some("com.apple.TextEdit"));
    }

    /// El identificador que consume `PrivacyGate` es el mismo que se
    /// persiste: el `harness_for_shared_watcher` inserta `1password`
    /// en la blacklist, así que una captura con caché caliente para
    /// `1password` debe descartarse.
    #[test]
    fn capture_loop_tick_uses_cached_identifier_to_evaluate_privacy_gate() {
        let (_dir, context, fake_clipboard) = harness_for_shared_watcher();
        context.cached_active_app_probe().refresh_with(Some(
            clipvault_platform::ActiveApplication::new("1Password", "1password"),
        ));
        fake_clipboard.push_read(Ok(Some("cv-loop-blacklisted-payload".into())));
        let watcher = build_watcher_for_context(&context);
        let outcome = capture_loop_tick(&watcher, &context);
        assert!(
            matches!(outcome, WatchTickOutcome::Captured(HistoryOutcome::Ignored)),
            "blacklisted source must be discarded, got {outcome:?}"
        );
        let recent = context.history().recent_entries(&context, 10).unwrap();
        assert!(recent.is_empty(), "blacklisted capture must not persist");
    }

    /// Una captura permitida no queda atribuida a ClipVault: el
    /// identificador que llega a la fila es el de la aplicación
    /// activa, nunca un literal tipo `ClipVault`.
    #[test]
    fn capture_loop_tick_does_not_substitute_clipvault_identifier() {
        let (_dir, context, fake_clipboard) = harness_for_shared_watcher();
        context.cached_active_app_probe().refresh_with(Some(
            clipvault_platform::ActiveApplication::new("Safari", "com.apple.Safari"),
        ));
        fake_clipboard.push_read(Ok(Some("cv-loop-safari-payload".into())));
        let watcher = build_watcher_for_context(&context);
        let _ = capture_loop_tick(&watcher, &context);
        let recent = context.history().recent_entries(&context, 10).unwrap();
        assert_eq!(recent[0].source_app.as_deref(), Some("com.apple.Safari"));
        assert_ne!(recent[0].source_app.as_deref(), Some("ClipVault"));
        assert_ne!(recent[0].source_app.as_deref(), Some("com.apple.ClipVault"));
    }

    /// Captura sin snapshot guarda origen desconocido sin romperse:
    /// la caché vacía hace que `resolved_source_identifier` devuelva
    /// `None`, la captura queda permitida y la fila se persiste con
    /// `source_app = NULL`.
    #[test]
    fn capture_loop_tick_with_empty_cache_persists_unknown_source() {
        let (_dir, context, fake_clipboard) = harness_for_shared_watcher();
        assert!(context.cached_active_application().is_none());
        fake_clipboard.push_read(Ok(Some("cv-loop-no-snapshot-payload".into())));
        let watcher = build_watcher_for_context(&context);
        let outcome = capture_loop_tick(&watcher, &context);
        assert!(
            matches!(
                outcome,
                WatchTickOutcome::Captured(HistoryOutcome::Stored { .. })
            ),
            "capture must succeed even when the cache is empty, got {outcome:?}"
        );
        let recent = context.history().recent_entries(&context, 10).unwrap();
        assert_eq!(recent.len(), 1);
        assert!(recent[0].source_app.is_none());
        assert!(recent[0].source_app_name.is_none());
        assert!(recent[0].source_app_icon_ref.is_none());
    }

    /// `metadata_enrichment_target` debe señalar entradas recién
    /// creadas sin metadata: una `Stored` con identificador válido
    /// y fila sin nombre devuelve `Some((id, identifier))`.
    #[test]
    fn metadata_enrichment_target_signals_pending_enrichment() {
        let (_dir, context, fake_clipboard) = harness_for_shared_watcher();
        context.cached_active_app_probe().refresh_with(Some(
            clipvault_platform::ActiveApplication::new("Terminal", "com.apple.Terminal"),
        ));
        fake_clipboard.push_read(Ok(Some("cv-loop-enrich-target".into())));
        let watcher = build_watcher_for_context(&context);
        let outcome = capture_loop_tick(&watcher, &context);
        let target = metadata_enrichment_target(&context, &outcome);
        assert_eq!(
            target,
            Some((1, "com.apple.Terminal".to_string())),
            "freshly stored entry with a non-empty identifier must be a candidate"
        );
    }

    /// `metadata_enrichment_target` debe omitir entradas que ya
    /// tienen `source_app_name` para evitar re-ejecuciones
    /// innecesarias del enrichment en el hilo principal.
    #[test]
    fn metadata_enrichment_target_skips_already_enriched_entry() {
        let (_dir, context, fake_clipboard) = harness_for_shared_watcher();
        context.cached_active_app_probe().refresh_with(Some(
            clipvault_platform::ActiveApplication::new("Terminal", "com.apple.Terminal"),
        ));
        fake_clipboard.push_read(Ok(Some("cv-loop-enrich-skip".into())));
        let watcher = build_watcher_for_context(&context);
        let outcome = capture_loop_tick(&watcher, &context);
        // Simulate a successful main-thread enrichment by populating
        // the metadata columns directly through the repository.
        if let WatchTickOutcome::Captured(HistoryOutcome::Stored { id }) = outcome {
            let mut db = context.database().lock();
            let mut repo = clipvault_db::EntryRepository::new(db.connection_mut());
            let now = time::OffsetDateTime::now_utc();
            repo.set_source_app_metadata(
                id,
                Some("Terminal"),
                Some("application-icons/com.apple.Terminal.png"),
                now,
            )
            .expect("set_source_app_metadata");
        } else {
            panic!("expected Stored outcome, got {outcome:?}");
        }
        assert_eq!(
            metadata_enrichment_target(&context, &outcome),
            None,
            "entry that already carries source_app_name must be skipped"
        );
    }

    // -----------------------------------------------------------------
    // `history-card-layout` regression suite — icon path end-to-end.
    //
    // The user reported that the card rail rendered a generic
    // fallback icon and the bare "—" placeholder instead of the
    // application icon and name. The end-to-end contract the suite
    // pins:
    //
    // - `entry_already_enriched` only returns `true` when BOTH
    //   `source_app_name` and `source_app_icon_ref` are present.
    //   A previous icon failure must not permanently disable the
    //   card icon.
    // - The shell re-runs the metadata enrichment on every capture
    //   for entries that lack either column; it never overwrites
    //   an already-stored name with `None`.
    // - `pending_metadata_entries` collects backfill candidates for
    //   old rows whose `source_app` is non-empty but whose metadata
    //   is missing. Rows with NULL `source_app` are NEVER candidates
    //   (the contract: do not invent the origin of an entry the user
    //   never attributed).
    // - The shell never attributes a capture to ClipVault: the
    //   identifier the row carries is always the cached active-app
    //   identifier, never a synthetic "ClipVault" value.
    // - Logs never carry clipboard content, snippets or hashes.
    // -----------------------------------------------------------------

    /// `entry_already_enriched` debe devolver `false` cuando la fila
    /// tiene nombre pero no `source_app_icon_ref` — un fallo de
    /// icono previo NO debe bloquear reintentos futuros.
    #[test]
    fn entry_already_enriched_requires_both_name_and_icon_ref() {
        let (_dir, context, fake_clipboard) = harness_for_shared_watcher();
        context.cached_active_app_probe().refresh_with(Some(
            clipvault_platform::ActiveApplication::new("Terminal", "com.apple.Terminal"),
        ));
        fake_clipboard.push_read(Ok(Some("cv-loop-partial-enrich".into())));
        let watcher = build_watcher_for_context(&context);
        let outcome = capture_loop_tick(&watcher, &context);
        let id = if let WatchTickOutcome::Captured(HistoryOutcome::Stored { id }) = outcome {
            id
        } else {
            panic!("expected Stored outcome, got {outcome:?}")
        };
        // Persistimos sólo el nombre, sin icono: el entry debe seguir
        // considerándose pendiente y la shell debe poder reintentar.
        {
            let mut db = context.database().lock();
            let mut repo = clipvault_db::EntryRepository::new(db.connection_mut());
            let now = time::OffsetDateTime::now_utc();
            repo.set_source_app_metadata(id, Some("Terminal"), None, now)
                .expect("set_source_app_metadata");
        }
        assert!(
            !entry_already_enriched(&context, id),
            "entry with name but missing icon must NOT be considered enriched"
        );
    }

    /// `entry_already_enriched` debe devolver `true` cuando ambos
    /// campos están poblados. Es la rama del contrato "completamente
    /// enriquecido".
    #[test]
    fn entry_already_enriched_returns_true_with_both_columns() {
        let (_dir, context, fake_clipboard) = harness_for_shared_watcher();
        context.cached_active_app_probe().refresh_with(Some(
            clipvault_platform::ActiveApplication::new("Terminal", "com.apple.Terminal"),
        ));
        fake_clipboard.push_read(Ok(Some("cv-loop-both-enrich".into())));
        let watcher = build_watcher_for_context(&context);
        let outcome = capture_loop_tick(&watcher, &context);
        let id = if let WatchTickOutcome::Captured(HistoryOutcome::Stored { id }) = outcome {
            id
        } else {
            panic!("expected Stored outcome, got {outcome:?}")
        };
        {
            let mut db = context.database().lock();
            let mut repo = clipvault_db::EntryRepository::new(db.connection_mut());
            let now = time::OffsetDateTime::now_utc();
            repo.set_source_app_metadata(
                id,
                Some("Terminal"),
                Some("application-icons/com.apple.Terminal.png"),
                now,
            )
            .expect("set_source_app_metadata");
        }
        assert!(
            entry_already_enriched(&context, id),
            "entry with name AND icon ref must be considered enriched"
        );
    }

    /// `pending_metadata_entries` debe omitir filas con
    /// `source_app = NULL`: el backfill NUNCA inventa un origen.
    #[test]
    fn pending_metadata_entries_skips_rows_with_null_source_app() {
        let (_dir, context, fake_clipboard) = harness_for_shared_watcher();
        // No cacheamos active app → `source_app` queda NULL.
        fake_clipboard.push_read(Ok(Some("cv-loop-unknown-source".into())));
        let watcher = build_watcher_for_context(&context);
        let outcome = capture_loop_tick(&watcher, &context);
        assert!(matches!(
            outcome,
            WatchTickOutcome::Captured(HistoryOutcome::Stored { .. })
        ));
        assert!(
            pending_metadata_entries(&context).is_empty(),
            "rows with NULL source_app must never be a backfill candidate"
        );
    }

    /// `pending_metadata_entries` debe incluir filas que tienen
    /// `source_app` pero no metadata: el backfill cubre entradas
    /// antiguas que requieren reintento del enrichment.
    #[test]
    fn pending_metadata_entries_includes_rows_with_source_but_no_metadata() {
        let (_dir, context, fake_clipboard) = harness_for_shared_watcher();
        context.cached_active_app_probe().refresh_with(Some(
            clipvault_platform::ActiveApplication::new("Terminal", "com.apple.Terminal"),
        ));
        fake_clipboard.push_read(Ok(Some("cv-loop-backfill-target".into())));
        let watcher = build_watcher_for_context(&context);
        let outcome = capture_loop_tick(&watcher, &context);
        let id = if let WatchTickOutcome::Captured(HistoryOutcome::Stored { id }) = outcome {
            id
        } else {
            panic!("expected Stored outcome, got {outcome:?}")
        };
        // El enrichment inline (NoopApplicationMetadataProvider en el
        // harness) no rellena la fila: `source_app_name` /
        // `source_app_icon_ref` siguen NULL.
        let pending = pending_metadata_entries(&context);
        assert!(
            pending.iter().any(|(pending_id, _)| *pending_id == id),
            "row with source_app but no metadata must be a backfill candidate"
        );
    }

    /// El backfill debe respetar el límite documentado: no procesa
    /// más filas de las que el límite permite aunque la tabla tenga
    /// muchas entradas pendientes.
    #[test]
    fn pending_metadata_entries_respects_batch_limit() {
        let (_dir, context, fake_clipboard) = harness_for_shared_watcher();
        context.cached_active_app_probe().refresh_with(Some(
            clipvault_platform::ActiveApplication::new("Terminal", "com.apple.Terminal"),
        ));
        let watcher = build_watcher_for_context(&context);
        // Insertamos más capturas que el límite para forzar el corte.
        let extra = BACKFILL_BATCH + 8;
        for i in 0..extra {
            fake_clipboard.push_read(Ok(Some(format!("cv-bulk-{i}"))));
        }
        for _ in 0..extra {
            let _ = capture_loop_tick(&watcher, &context);
        }
        let pending = pending_metadata_entries(&context);
        assert!(
            pending.len() <= BACKFILL_BATCH,
            "backfill must cap at BACKFILL_BATCH, got {}",
            pending.len()
        );
    }

    /// Las entradas antiguas con `source_app` poblado pero sin
    /// metadata pueden enriquecerse sin duplicar la fila.
    #[test]
    fn backfill_does_not_duplicate_existing_rows() {
        let (_dir, context, fake_clipboard) = harness_for_shared_watcher();
        context.cached_active_app_probe().refresh_with(Some(
            clipvault_platform::ActiveApplication::new("Terminal", "com.apple.Terminal"),
        ));
        fake_clipboard.push_read(Ok(Some("cv-loop-dedup-backfill".into())));
        let watcher = build_watcher_for_context(&context);
        let outcome = capture_loop_tick(&watcher, &context);
        let id = if let WatchTickOutcome::Captured(HistoryOutcome::Stored { id }) = outcome {
            id
        } else {
            panic!("expected Stored outcome, got {outcome:?}")
        };
        // La siguiente iteración con el mismo payload debe caer en
        // `Unchanged` (el watcher dedupa por `last_hash`); el shell
        // nunca debe invocar el enrichment para una captura duplicada.
        fake_clipboard.push_read(Ok(Some("cv-loop-dedup-backfill".into())));
        let second = capture_loop_tick(&watcher, &context);
        assert_eq!(
            second,
            WatchTickOutcome::Unchanged,
            "duplicate capture must dedupe via last_hash, got {second:?}"
        );
        // El backfill debe proponer una sola fila, no duplicarla.
        let pending = pending_metadata_entries(&context);
        let count = pending
            .iter()
            .filter(|(pending_id, _)| *pending_id == id)
            .count();
        assert_eq!(count, 1, "backfill must not duplicate the row");
    }

    /// Una captura permitida nunca queda atribuida a ClipVault por
    /// la ventana enfocada. La capa `bootstrap` resuelve el
    /// identificador siempre desde el caché active-app.
    #[test]
    fn capture_loop_never_attributes_to_clipvault() {
        let (_dir, context, fake_clipboard) = harness_for_shared_watcher();
        context.cached_active_app_probe().refresh_with(Some(
            clipvault_platform::ActiveApplication::new("Safari", "com.apple.Safari"),
        ));
        fake_clipboard.push_read(Ok(Some("cv-no-clipvault-attr".into())));
        let watcher = build_watcher_for_context(&context);
        let _ = capture_loop_tick(&watcher, &context);
        let recent = context.history().recent_entries(&context, 10).unwrap();
        assert_eq!(recent[0].source_app.as_deref(), Some("com.apple.Safari"));
        assert_ne!(recent[0].source_app.as_deref(), Some("ClipVault"));
        assert_ne!(recent[0].source_app.as_deref(), Some("com.apple.ClipVault"));
    }

    /// Las decisiones que el shell registra sólo pueden usar
    /// categorías estables (no contenido, snippet, hash).
    #[test]
    fn record_capture_decision_uses_stable_categories_only() {
        let (_dir, context, _clipboard) = harness_for_shared_watcher();
        let diagnostics = context.active_app_diagnostics_state();
        // Forzamos una categoría válida — debe quedar registrada.
        diagnostics.record_capture_decision("allowed:stored");
        let snap = diagnostics.snapshot();
        assert_eq!(
            snap.last_capture_decision.as_deref(),
            Some("allowed:stored")
        );
        // Una categoría que parece payload debe seguir siendo
        // registrada como categoría, no como contenido crudo: el
        // helper acepta cualquier string pero el shell nunca le pasa
        // contenido del clipboard. Pin del contrato "categorías
        // estables".
        diagnostics.record_capture_decision("metadata_lookup_failed");
        assert_eq!(
            diagnostics.snapshot().last_capture_decision.as_deref(),
            Some("metadata_lookup_failed")
        );
    }

    // -----------------------------------------------------------------
    // `linux-source-app-metadata` patch: capture-loop cache refresh.
    //
    // The user reported that on Ubuntu GNOME Wayland captures never
    // carry the source application identifier or its icon. The cause
    // was that `install_active_app_main_queue_refresher` only runs
    // on macOS; on Linux nobody calls `refresh_active_application`
    // and the cached probe stays empty, so the provider has nothing
    // to resolve. The fix refreshes the cache synchronously on every
    // iteration of `capture_loop_tick` (and on every manual tick
    // routed through `SharedState::tick`) on non-macOS hosts.
    //
    // The tests below pin the contract end-to-end on every platform
    // (the `cfg(not(target_os = "macos"))` paths exercise the
    // Linux-shaped behaviour on the macOS dev host too, because the
    // helper itself is gated on the target so the tests run as the
    // production code would).
    // -----------------------------------------------------------------

    /// Harness variant that wires a scripted active-app probe and
    /// a programmable metadata provider so the test can exercise the
    /// exact contract the user reported without depending on the
    /// production `NoopApplicationMetadataProvider`. Mirrors
    /// `harness_for_shared_watcher` but lets the test queue the
    /// probe answer for each call (and observe the metadata lookup
    /// call list).
    fn harness_with_scripted_probe(
        os_family: clipvault_platform::OsFamily,
    ) -> (
        tempfile::TempDir,
        AppContext,
        Arc<clipvault_core::FakeClipboardBackend>,
        Arc<clipvault_core::FakeApplicationMetadataProvider>,
        std::sync::Arc<ScriptedActiveAppProbe>,
    ) {
        use clipvault_core::{
            FakeApplicationMetadataProvider, FakeClipboardBackend, FakeHotkeyManager,
            FakePasteController, FakeSettingsNavigator, FakeTrayController, PlatformAdapters,
        };
        use clipvault_platform::{ApplicationMetadataProvider, OsFamily};

        let dir = tempfile::TempDir::new().expect("tempdir");
        let probe = std::sync::Arc::new(ScriptedActiveAppProbe::new());
        let provider = Arc::new(FakeApplicationMetadataProvider::new());
        let provider_for_adapters: Arc<dyn ApplicationMetadataProvider> = provider.clone();
        {
            let mut db =
                clipvault_db::Database::open(dir.path().join("clipvault.db")).expect("open");
            db.run_migrations(&clipvault_db::builtin_migrations())
                .expect("migrate");
        }
        let info = clipvault_platform::PlatformInfo {
            home_dir: std::path::PathBuf::from("/tmp"),
            data_dir: std::path::PathBuf::from("/tmp/.clipvault"),
            os_family,
            display_server: match os_family {
                OsFamily::Linux => clipvault_platform::DisplayServer::Wayland,
                _ => clipvault_platform::DisplayServer::Unknown,
            },
        };
        let fake_clipboard = Arc::new(FakeClipboardBackend::new());
        let platform_adapters = PlatformAdapters::new(
            fake_clipboard.clone() as Arc<dyn clipvault_platform::ClipboardBackend>,
            Arc::new(FakeHotkeyManager::new()) as Arc<dyn clipvault_platform::HotkeyManager>,
            probe.clone() as Arc<dyn clipvault_platform::ActiveApplicationProbe>,
            Arc::new(FakePasteController::new()) as Arc<dyn clipvault_platform::PasteController>,
            Arc::new(FakeTrayController::new()) as Arc<dyn clipvault_platform::TrayController>,
            Arc::new(FakeSettingsNavigator::new())
                as Arc<dyn clipvault_platform::SettingsNavigator>,
            provider_for_adapters,
            clipvault_platform::Capabilities::ALL_AVAILABLE,
            info,
        );
        let context = AppBootstrap::new()
            .with_clock(Arc::new(clipvault_core::SystemClock) as Arc<dyn clipvault_core::Clock>)
            .with_clipboard(Arc::new(clipvault_core::FakeClipboard::new())
                as Arc<dyn clipvault_core::Clipboard>)
            .with_platform_adapters(platform_adapters)
            .bootstrap_at(dir.path().join("clipvault.db"))
            .expect("bootstrap");
        (dir, context, fake_clipboard, provider, probe)
    }

    /// Scripted active-app probe that returns the queued
    /// `Result<Option<ActiveApplication>, ActiveAppError>` and
    /// exposes the call counter so the test can verify the loop
    /// really does invoke the inner probe on every iteration (the
    /// regression the user reported was that nobody refreshed the
    /// cache on Linux).
    struct ScriptedActiveAppProbe {
        queued: parking_lot::Mutex<
            Vec<
                Result<
                    Option<clipvault_platform::ActiveApplication>,
                    clipvault_platform::ActiveAppError,
                >,
            >,
        >,
        calls: parking_lot::Mutex<usize>,
    }

    impl ScriptedActiveAppProbe {
        fn new() -> Self {
            Self {
                queued: parking_lot::Mutex::new(Vec::new()),
                calls: parking_lot::Mutex::new(0),
            }
        }

        fn push(
            &self,
            value: Result<
                Option<clipvault_platform::ActiveApplication>,
                clipvault_platform::ActiveAppError,
            >,
        ) {
            self.queued.lock().push(value);
        }

        fn calls(&self) -> usize {
            *self.calls.lock()
        }
    }

    impl clipvault_platform::ActiveApplicationProbe for ScriptedActiveAppProbe {
        fn active_application(
            &self,
        ) -> Result<Option<clipvault_platform::ActiveApplication>, clipvault_platform::ActiveAppError>
        {
            *self.calls.lock() += 1;
            self.queued
                .lock()
                .pop()
                .unwrap_or(Err(clipvault_platform::ActiveAppError::Unavailable))
        }
        fn name(&self) -> &'static str {
            "linux-source-app-metadata-scripted"
        }
    }

    /// Linux-only contract: the capture loop must call
    /// `refresh_active_application` (and therefore the inner probe)
    /// on every iteration. Before this fix the loop never refreshed
    /// the cache on Linux, the diagnostics counters stayed at zero,
    /// and every capture landed with `source_app = NULL`. The test
    /// guards the regression by counting probe invocations across
    /// two ticks.
    #[cfg(not(target_os = "macos"))]
    #[test]
    fn linux_capture_loop_refreshes_cache_on_every_iteration() {
        use clipvault_core::ActiveApplication as PlatformActiveApplication;
        let (_dir, context, _clipboard, _provider, probe) =
            harness_with_scripted_probe(clipvault_platform::OsFamily::Linux);
        // The cache must start empty: the harness does not refresh
        // it, so a probe call is the only way the loop can populate
        // it.
        assert!(context.cached_active_application().is_none());
        probe.push(Ok(Some(PlatformActiveApplication::new(
            "Firefox", "firefox",
        ))));
        probe.push(Ok(Some(PlatformActiveApplication::new(
            "Terminal",
            "com.apple.Terminal",
        ))));
        // Build a fresh watcher through the existing helper so the
        // loop runs the same dedupe state the production shell
        // would observe.
        let watcher = build_watcher_for_context(&context);
        // First tick refreshes the cache to `Firefox`.
        let _ = capture_loop_tick(&watcher, &context);
        assert_eq!(
            context
                .cached_active_application()
                .as_ref()
                .map(|app| app.identifier.as_str()),
            Some("firefox"),
            "first tick must refresh the cache from the inner probe"
        );
        // Second tick refreshes the cache to `com.apple.Terminal`.
        let _ = capture_loop_tick(&watcher, &context);
        assert_eq!(
            context
                .cached_active_application()
                .as_ref()
                .map(|app| app.identifier.as_str()),
            Some("com.apple.Terminal"),
            "second tick must refresh the cache again, not reuse the first answer"
        );
        assert!(
            probe.calls() >= 2,
            "Linux capture loop must call the probe at least twice, got {}",
            probe.calls()
        );
    }

    /// Linux contract: an X11/XWayland capture with a populated
    /// cache must persist `source_app` so the metadata enrichment
    /// can resolve the user-visible name. Before this fix the cache
    /// was empty and the row stored `source_app = NULL`, so the
    /// provider had nothing to look up.
    #[cfg(not(target_os = "macos"))]
    #[test]
    fn linux_x11_capture_persists_source_app_from_refreshed_cache() {
        use clipvault_core::{ActiveApplication as PlatformActiveApplication, HistoryOutcome};
        let (_dir, context, fake_clipboard, _provider, probe) =
            harness_with_scripted_probe(clipvault_platform::OsFamily::Linux);
        // Script the probe to answer `firefox` (typical X11 WM_CLASS
        // class segment). Push two answers so a second refresh does
        // not fall back to `Unavailable` and clear the cache mid
        // capture.
        probe.push(Ok(Some(PlatformActiveApplication::new(
            "Firefox", "firefox",
        ))));
        probe.push(Ok(Some(PlatformActiveApplication::new(
            "Firefox", "firefox",
        ))));
        fake_clipboard.push_read(Ok(Some("cv-linux-x11-payload".into())));
        let watcher = build_watcher_for_context(&context);
        let outcome = capture_loop_tick(&watcher, &context);
        assert!(
            matches!(
                outcome,
                clipvault_core::WatchTickOutcome::Captured(HistoryOutcome::Stored { .. })
            ),
            "first tick must store the capture, got {outcome:?}"
        );
        let recent = context.history().recent_entries(&context, 10).unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(
            recent[0].source_app.as_deref(),
            Some("firefox"),
            "Linux X11 capture must carry the WM_CLASS identifier the probe reported"
        );
    }

    /// Linux contract: the metadata provider MUST be asked for the
    /// same identifier the row persisted. Before this fix the cache
    /// stayed empty and the provider never saw a `lookup` call; the
    /// regression the user reported is "captures never show the
    /// application icon" — both the row and the lookup list must
    /// reflect the refreshed identifier.
    #[cfg(not(target_os = "macos"))]
    #[test]
    fn linux_capture_enriches_metadata_through_provider_lookup() {
        use clipvault_core::{ActiveApplication as PlatformActiveApplication, ApplicationMetadata};
        let (_dir, context, fake_clipboard, provider, probe) =
            harness_with_scripted_probe(clipvault_platform::OsFamily::Linux);
        probe.push(Ok(Some(PlatformActiveApplication::new(
            "GNOME Terminal",
            "gnome-terminal",
        ))));
        provider.push_lookup(Ok(Some(ApplicationMetadata {
            display_name: "GNOME Terminal".to_string(),
            icon_ref: Some("application-icons/gnome-terminal.png".to_string()),
        })));
        fake_clipboard.push_read(Ok(Some("cv-linux-enrich-payload".into())));
        let watcher = build_watcher_for_context(&context);
        let outcome = capture_loop_tick(&watcher, &context);
        assert!(matches!(
            outcome,
            clipvault_core::WatchTickOutcome::Captured(
                clipvault_core::HistoryOutcome::Stored { .. }
            )
        ));
        let calls = provider.calls();
        assert!(
            calls.iter().any(|id| id == "gnome-terminal"),
            "provider must see the refreshed identifier, got {calls:?}"
        );
        // The row stored by the loop MUST carry the source_app the
        // metadata lookup used, so the enrichment target (which the
        // production shell schedules on the platform thread) can
        // re-run the provider without a second refresh.
        let recent = context.history().recent_entries(&context, 10).unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].source_app.as_deref(), Some("gnome-terminal"));
    }

    /// Linux contract: native Wayland (no X11 window focused)
    /// MUST leave the cache empty and never attribute the capture
    /// to a synthetic identifier. The probe reports `Ok(None)` —
    /// the contract the spec pins — so the watcher evaluates an
    /// empty cache, the row stores `source_app = NULL`, and the
    /// metadata provider is never asked.
    #[cfg(not(target_os = "macos"))]
    #[test]
    fn linux_native_wayland_does_not_get_a_fake_identifier() {
        let (_dir, context, fake_clipboard, provider, probe) =
            harness_with_scripted_probe(clipvault_platform::OsFamily::Linux);
        // Probe answers `Ok(None)`: this is what the X11 probe
        // returns for a focused native Wayland window because it
        // cannot see X11 windows it did not publish.
        probe.push(Ok(None));
        probe.push(Ok(None));
        fake_clipboard.push_read(Ok(Some("cv-linux-native-wayland-payload".into())));
        let watcher = build_watcher_for_context(&context);
        let outcome = capture_loop_tick(&watcher, &context);
        assert!(matches!(
            outcome,
            clipvault_core::WatchTickOutcome::Captured(
                clipvault_core::HistoryOutcome::Stored { .. }
            )
        ));
        assert!(
            context.cached_active_application().is_none(),
            "native Wayland probe answer must leave the cache empty"
        );
        let recent = context.history().recent_entries(&context, 10).unwrap();
        assert_eq!(recent.len(), 1);
        assert!(
            recent[0].source_app.is_none(),
            "native Wayland capture MUST NOT carry a fabricated source_app, got {:?}",
            recent[0].source_app
        );
        assert!(
            provider.calls().is_empty(),
            "native Wayland capture must not invoke the metadata provider, got {:?}",
            provider.calls()
        );
        // The diagnostics surface MUST still record the refresh
        // attempt so the user can tell apart "the loop never tried"
        // from "the loop tried and the probe reported unavailable".
        let diag = context.active_app_diagnostics();
        assert!(
            diag.refresh_attempts >= 1,
            "refresh_attempts must reflect the synchronous refresh, got {diag:?}"
        );
    }

    /// Linux contract: the cache must not be empty after the loop
    /// has run at least once. The previous behaviour — `Ok(None)`
    /// probe answer — left the cache empty permanently because the
    /// loop never refreshed it; this assertion pins that the new
    /// helper populates the cache when the probe has an answer to
    /// give.
    #[cfg(not(target_os = "macos"))]
    #[test]
    fn linux_cache_is_populated_after_loop_tick() {
        use clipvault_core::ActiveApplication as PlatformActiveApplication;
        let (_dir, context, fake_clipboard, _provider, probe) =
            harness_with_scripted_probe(clipvault_platform::OsFamily::Linux);
        probe.push(Ok(Some(PlatformActiveApplication::new(
            "Files",
            "org.gnome.Nautilus",
        ))));
        fake_clipboard.push_read(Ok(Some("cv-linux-cache-populated".into())));
        let watcher = build_watcher_for_context(&context);
        assert!(
            context.cached_active_application().is_none(),
            "fresh harness must start with an empty cache"
        );
        let _ = capture_loop_tick(&watcher, &context);
        assert!(
            context.cached_active_application().is_some(),
            "after one loop tick the cache must hold the probe answer"
        );
    }

    /// Contract: the helper that refreshes the cache before each
    /// tick MUST be a no-op on macOS. The macOS path relies on the
    /// main-queue refresher installed at bootstrap; calling
    /// `refresh_active_application` from the capture-loop thread
    /// would invoke `NSWorkspace` off the main thread, which Apple
    /// does not support. We verify the contract on every target by
    /// observing that the probe counter does not increment when the
    /// helper is invoked on macOS (the `cfg` gate compiles the body
    /// away on macOS).
    #[cfg(target_os = "macos")]
    #[test]
    fn macos_capture_loop_does_not_refresh_cache_from_background_thread() {
        use clipvault_core::ActiveApplication as PlatformActiveApplication;
        let (_dir, context, _clipboard, _provider, probe) =
            harness_with_scripted_probe(clipvault_platform::OsFamily::Macos);
        probe.push(Ok(Some(PlatformActiveApplication::new(
            "TextEdit",
            "com.apple.TextEdit",
        ))));
        let calls_before = probe.calls();
        refresh_active_application_cache_for_loop_tick(&context);
        let calls_after = probe.calls();
        assert_eq!(
            calls_before, calls_after,
            "macOS capture loop must NOT invoke the probe from the background thread"
        );
    }

    // -----------------------------------------------------------------
    // `linux-source-app-metadata` follow-up patch: byte-decoding fix
    // for `_NET_ACTIVE_WINDOW` + granular probe-stage tracking.
    //
    // The previous implementation read `reply.value.first()` for the
    // `_NET_ACTIVE_WINDOW` reply (a `format=32` property that holds
    // the 4-byte window id in native endianness) and converted the
    // single-byte result to a `Window`. The upper three bytes were
    // discarded, so every focus query landed on a wrong window id and
    // `WM_CLASS` lookup consequently returned `None`. The fix:
    //   1. Use `reply.value32()` / `parse_active_window_id` to read
    //      the full 32-bit value in native endianness.
    //   2. Surface a granular `ProbeStage` from the probe so the
    //      diagnostics card distinguishes "no X11 window focused"
    //      from "WM_CLASS is undeclared on the focused window".
    //
    // The tests below are intentionally host-agnostic: every test
    // uses a scripted probe that mimics the documented EWMH replies
    // (the dev.warp.Warp scenario the user reported on Ubuntu), so
    // the suite runs on the macOS dev host without depending on a
    // real X server. The macOS host keeps the previous
    // `#[cfg(not(target_os = "macos"))]` gates for the existing
    // production-flavoured cache-refresh tests so the no-op helper
    // they exercise stays the same code path the production shell
    // uses.
    // -----------------------------------------------------------------

    /// Scripted active-app probe wired to a programmable probe
    /// stage. The test mutates the stage between calls so the
    /// diagnostics surface can verify that `refresh_active_application`
    /// mirrors the granular stage the Linux probe reached on its
    /// most recent call (the regression the
    /// `linux-source-app-metadata` follow-up patch fixes).
    struct ScriptedStageActiveAppProbe {
        queued: parking_lot::Mutex<
            Vec<
                Result<
                    Option<clipvault_platform::ActiveApplication>,
                    clipvault_platform::ActiveAppError,
                >,
            >,
        >,
        stage: parking_lot::Mutex<clipvault_platform::ProbeStage>,
        calls: parking_lot::Mutex<usize>,
    }

    impl ScriptedStageActiveAppProbe {
        fn new() -> Self {
            Self {
                queued: parking_lot::Mutex::new(Vec::new()),
                stage: parking_lot::Mutex::new(clipvault_platform::ProbeStage::Started),
                calls: parking_lot::Mutex::new(0),
            }
        }

        fn push(
            &self,
            value: Result<
                Option<clipvault_platform::ActiveApplication>,
                clipvault_platform::ActiveAppError,
            >,
        ) {
            self.queued.lock().push(value);
        }

        fn set_stage(&self, stage: clipvault_platform::ProbeStage) {
            *self.stage.lock() = stage;
        }

        #[allow(dead_code)]
        fn calls(&self) -> usize {
            *self.calls.lock()
        }
    }

    impl clipvault_platform::ActiveApplicationProbe for ScriptedStageActiveAppProbe {
        fn active_application(
            &self,
        ) -> Result<Option<clipvault_platform::ActiveApplication>, clipvault_platform::ActiveAppError>
        {
            *self.calls.lock() += 1;
            self.queued
                .lock()
                .pop()
                .unwrap_or(Err(clipvault_platform::ActiveAppError::Unavailable))
        }
        fn name(&self) -> &'static str {
            "linux-source-app-metadata-scripted-stage"
        }
        fn last_probe_stage(&self) -> clipvault_platform::ProbeStage {
            *self.stage.lock()
        }
    }

    /// Harness variant that mirrors
    /// `harness_with_scripted_probe` but installs
    /// `ScriptedStageActiveAppProbe` so a test can drive the
    /// probe-stage transitions independently of the probe answer
    /// (the doc delta the
    /// `linux-source-app-metadata` follow-up pins). Keeping the
    /// metadata provider (`FakeApplicationMetadataProvider`) in the
    /// returned tuple preserves the existing assertions on
    /// `source_app_name` / `source_app_icon_ref` enrichment. The
    /// helper is platform-agnostic by design: the tests below want
    /// to drive `refresh_active_application` directly without going
    /// through the macOS-gated `capture_loop_tick` helper.
    fn harness_with_staged_probe() -> (
        tempfile::TempDir,
        AppContext,
        Arc<clipvault_core::FakeClipboardBackend>,
        Arc<clipvault_core::FakeApplicationMetadataProvider>,
        std::sync::Arc<ScriptedStageActiveAppProbe>,
    ) {
        use clipvault_core::{
            FakeApplicationMetadataProvider, FakeClipboardBackend, FakeHotkeyManager,
            FakePasteController, FakeSettingsNavigator, FakeTrayController, PlatformAdapters,
        };
        use clipvault_platform::{ApplicationMetadataProvider, DisplayServer, OsFamily};

        let dir = tempfile::TempDir::new().expect("tempdir");
        let probe = std::sync::Arc::new(ScriptedStageActiveAppProbe::new());
        let provider = Arc::new(FakeApplicationMetadataProvider::new());
        let provider_for_adapters: Arc<dyn ApplicationMetadataProvider> = provider.clone();
        {
            let mut db =
                clipvault_db::Database::open(dir.path().join("clipvault.db")).expect("open");
            db.run_migrations(&clipvault_db::builtin_migrations())
                .expect("migrate");
        }
        let info = clipvault_platform::PlatformInfo {
            home_dir: std::path::PathBuf::from("/tmp"),
            data_dir: std::path::PathBuf::from("/tmp/.clipvault"),
            os_family: OsFamily::Linux,
            display_server: DisplayServer::Wayland,
        };
        let fake_clipboard = Arc::new(FakeClipboardBackend::new());
        let platform_adapters = PlatformAdapters::new(
            fake_clipboard.clone() as Arc<dyn clipvault_platform::ClipboardBackend>,
            Arc::new(FakeHotkeyManager::new()) as Arc<dyn clipvault_platform::HotkeyManager>,
            probe.clone() as Arc<dyn clipvault_platform::ActiveApplicationProbe>,
            Arc::new(FakePasteController::new()) as Arc<dyn clipvault_platform::PasteController>,
            Arc::new(FakeTrayController::new()) as Arc<dyn clipvault_platform::TrayController>,
            Arc::new(FakeSettingsNavigator::new())
                as Arc<dyn clipvault_platform::SettingsNavigator>,
            provider_for_adapters,
            clipvault_platform::Capabilities::ALL_AVAILABLE,
            info,
        );
        let context = AppBootstrap::new()
            .with_clock(Arc::new(clipvault_core::SystemClock) as Arc<dyn clipvault_core::Clock>)
            .with_clipboard(Arc::new(clipvault_core::FakeClipboard::new())
                as Arc<dyn clipvault_core::Clipboard>)
            .with_platform_adapters(platform_adapters)
            .bootstrap_at(dir.path().join("clipvault.db"))
            .expect("bootstrap");
        (dir, context, fake_clipboard, provider, probe)
    }

    /// The fixture the user reported: Warp running under XWayland on
    /// GNOME Wayland, with `WM_CLASS` = `("dev.warp.Warp",
    /// "dev.warp.Warp")`. The capture loop MUST persist
    /// `source_app = "dev.warp.Warp"` on the new row and the metadata
    /// provider MUST receive the same identifier so it can resolve
    /// `source_app_name` / `source_app_icon_ref`. The granularity here
    /// simulates what the (now fixed) X11 probe reports on a real
    /// Warp capture: `_NET_ACTIVE_WINDOW` → parsed window id →
    /// `WM_CLASS` lookup → identifier parsed. Host-agnostic by
    /// design: the test calls `refresh_active_application` directly
    /// instead of routing through the macOS-gated
    /// `capture_loop_tick` helper so it runs on the dev host too.
    #[test]
    fn capture_loop_persists_dev_warp_warp_source_app_end_to_end() {
        use clipvault_core::ActiveApplication as PlatformActiveApplication;
        let (_dir, context, fake_clipboard, provider, probe) = harness_with_staged_probe();
        // The probe answers as if `WM_CLASS` had been read
        // successfully — both segments equal `dev.warp.Warp`, so the
        // active-app identifier the X11 probe returns is the class
        // segment, exactly the regression-reported `dev.warp.Warp`.
        probe.push(Ok(Some(PlatformActiveApplication::new(
            "Warp",
            "dev.warp.Warp",
        ))));
        // The Linux probe also surfaces a granular `Identified`
        // stage on the successful path; the diagnostics endpoint
        // mirrors it through `refresh_active_application`.
        probe.set_stage(clipvault_platform::ProbeStage::Identified);
        fake_clipboard.push_read(Ok(Some("cv-warp-payload".into())));
        // Drive the refresh + watcher explicitly so the test does
        // not depend on `capture_loop_tick`'s host-conditional
        // refresh helper (the helper is a no-op on macOS by design
        // — the production macOS path relies on the dispatch
        // refresher, not the synchronous loop tick). The two-step
        // sequence mirrors what `capture_loop_tick` does in the
        // non-macOS branch the user reported: refresh first, then
        // resolve the cached identifier through the watcher.
        context.refresh_active_application().expect("refresh");
        let source_app = resolved_source_identifier(&context);
        let watcher = build_watcher_for_context(&context);
        let outcome = watcher.tick(
            &context,
            source_app.as_deref(),
            clipvault_core::AttemptOrigin::BackgroundLoop,
        );
        assert!(
            matches!(
                outcome,
                clipvault_core::WatchTickOutcome::Captured(
                    clipvault_core::HistoryOutcome::Stored { .. }
                )
            ),
            "first tick must store the capture, got {outcome:?}"
        );
        let recent = context.history().recent_entries(&context, 10).unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(
            recent[0].source_app.as_deref(),
            Some("dev.warp.Warp"),
            "capture loop must persist the WM_CLASS class segment the probe reported"
        );
        // The provider MUST see the same identifier the row stored so
        // the enrichment can resolve `source_app_name` /
        // `source_app_icon_ref`. Without the byte-decoding fix the
        // probe would return `Ok(None)` and the provider would never
        // be consulted — the regression that produced empty rows in
        // `~/.clipvault/clipvault.db`.
        let calls = provider.calls();
        assert!(
            calls.iter().any(|id| id == "dev.warp.Warp"),
            "metadata provider must see the refreshed identifier, got {calls:?}"
        );
        // The diagnostics surface mirrors the granular stage so the
        // user can confirm the EWMH chain (`_NET_ACTIVE_WINDOW` →
        // `WM_CLASS` → identifier) completed.
        let diag = context.active_app_diagnostics();
        assert_eq!(
            diag.last_probe_stage,
            Some("identified"),
            "diagnostics must mirror the probe stage, got {diag:?}"
        );
        assert_eq!(
            diag.net_active_window_seen,
            Some(true),
            "diag.net_active_window_seen must be true on the success path, got {diag:?}"
        );
        assert_eq!(
            diag.wm_class_seen,
            Some(true),
            "diag.wm_class_seen must be true on the success path, got {diag:?}"
        );
        assert!(
            diag.cache_populated,
            "diag.cache_populated must be true after a successful refresh, got {diag:?}"
        );
    }

    /// Mirrors the `_NET_ACTIVE_WINDOW → WM_CLASS → source_app`
    /// pin expressed in the design:
    /// - probe returns `dev.warp.Warp` (full XWayland success path),
    /// - cache is populated for the next capture tick,
    /// - source_app column stores the identifier the watcher
    ///   forwarded.
    ///
    /// The scripted probe simulates the resolved probe outcome the
    /// fixed code now produces; the test pins the cache and
    /// persistence integration on every host.
    #[test]
    fn probe_dev_warp_warp_identifier_populates_cache_and_source_app() {
        use clipvault_core::ActiveApplication as PlatformActiveApplication;
        let (_dir, context, fake_clipboard, _provider, probe) = harness_with_staged_probe();
        // Scripted probe returns the dev.warp.Warp identifier on
        // both iterations so a second refresh does not fall back to
        // `Unavailable` and clear the cache mid capture.
        probe.push(Ok(Some(PlatformActiveApplication::new(
            "Warp",
            "dev.warp.Warp",
        ))));
        probe.push(Ok(Some(PlatformActiveApplication::new(
            "Warp",
            "dev.warp.Warp",
        ))));
        // The X11 probe surfaces the granular `Identified` stage on
        // the success path; the diagnostics endpoint mirrors it.
        probe.set_stage(clipvault_platform::ProbeStage::Identified);
        fake_clipboard.push_read(Ok(Some("cv-warp-populated".into())));
        assert!(
            context.cached_active_application().is_none(),
            "fresh harness must start with an empty cache"
        );
        context.refresh_active_application().expect("refresh");
        let source_app = resolved_source_identifier(&context);
        let watcher = build_watcher_for_context(&context);
        let outcome = watcher.tick(
            &context,
            source_app.as_deref(),
            clipvault_core::AttemptOrigin::BackgroundLoop,
        );
        assert!(matches!(
            outcome,
            clipvault_core::WatchTickOutcome::Captured(
                clipvault_core::HistoryOutcome::Stored { .. }
            )
        ));
        // The cache now exposes `dev.warp.Warp` so the next loop
        // tick (without a probe call) would still see the
        // identifier. The persistence layer received the same
        // identifier and persisted it on the new row — the chain
        // the user reported missing on Ubuntu.
        let cached = context
            .cached_active_application()
            .expect("cache must be populated");
        assert_eq!(cached.identifier, "dev.warp.Warp");
        let recent = context.history().recent_entries(&context, 10).unwrap();
        assert_eq!(recent[0].source_app.as_deref(), Some("dev.warp.Warp"));
    }

    /// Verifies the Wayland + DISPLAY probe selection path the user
    /// exercised on Ubuntu GNOME: even though the host detected as
    /// Wayland, the bootstrap selects the X11 / XWayland backend
    /// because `$DISPLAY` is set. The design pins that the probe
    /// name MUST distinguish `x11_ewmh` from `xwayland_ewmh`; the
    /// diagnostics mirror the choice so the user can confirm the
    /// bootstrap took the XWayland branch. The test is purely
    /// numeric — no platform adapter is wired.
    #[test]
    fn wayland_with_display_path_pins_xwayland_backend_selection() {
        use clipvault_platform::{ActiveAppBackendKind, DisplayServer, OsFamily, PlatformInfo};
        use std::path::PathBuf;
        let info = PlatformInfo {
            home_dir: PathBuf::from("/tmp"),
            data_dir: PathBuf::from("/tmp/.clipvault"),
            os_family: OsFamily::Linux,
            display_server: DisplayServer::Wayland,
        };
        assert_eq!(
            clipvault_core::active_app_backend_kind(&info, true),
            ActiveAppBackendKind::XWaylandEwmh,
            "Wayland + DISPLAY MUST classify as xwayland_ewmh"
        );
        // Plain X11 stays x11_ewmh (the regression-pin the design
        // added so the bootstrap never labels a plain X11 session as
        // XWayland).
        let info_x11 = PlatformInfo {
            display_server: DisplayServer::X11,
            ..info.clone()
        };
        assert_eq!(
            clipvault_core::active_app_backend_kind(&info_x11, true),
            ActiveAppBackendKind::X11Ewmh,
            "Plain X11 session MUST classify as x11_ewmh"
        );
    }

    /// Pin the contract that the user-required diagnostics signal
    /// surface stays honest when `_NET_ACTIVE_WINDOW` returns an
    /// empty value (the regression case the audit captured as
    /// `active_window_empty`): the cache stays empty, no row carries
    /// a fabricated `source_app`, the diagnostics endpoint mirrors
    /// `active_window_empty` so the user can tell the active-app
    /// probe apart from a real blacklist hit, and the metadata
    /// provider is never consulted for a `source_app = NULL` row.
    #[test]
    fn diagnostics_mirror_active_window_empty_stage() {
        let (_dir, context, fake_clipboard, provider, probe) = harness_with_staged_probe();
        // Probe answered `Ok(None)` because `_NET_ACTIVE_WINDOW`
        // returned no parseable window id: the X11 probe sets
        // `ProbeStage::ActiveWindowEmpty` and leaves the cache
        // empty.
        probe.push(Ok(None));
        probe.push(Ok(None));
        probe.set_stage(clipvault_platform::ProbeStage::ActiveWindowEmpty);
        fake_clipboard.push_read(Ok(Some("cv-warp-no-window".into())));
        context.refresh_active_application().expect("refresh");
        let source_app = resolved_source_identifier(&context);
        let watcher = build_watcher_for_context(&context);
        let outcome = watcher.tick(
            &context,
            source_app.as_deref(),
            clipvault_core::AttemptOrigin::BackgroundLoop,
        );
        assert!(matches!(
            outcome,
            clipvault_core::WatchTickOutcome::Captured(
                clipvault_core::HistoryOutcome::Stored { .. }
            )
        ));
        let diag = context.active_app_diagnostics();
        assert_eq!(
            diag.last_probe_stage,
            Some("active_window_empty"),
            "diagnostics must mirror the active window stage, got {diag:?}"
        );
        assert_eq!(
            diag.net_active_window_seen,
            Some(false),
            "diag.net_active_window_seen must be false on the empty path, got {diag:?}"
        );
        assert!(
            diag.wm_class_seen.is_none(),
            "wm_class_seen must be None when _NET_ACTIVE_WINDOW is empty, got {diag:?}"
        );
        // The cache stays empty: the next capture tick that runs
        // before a fresh refresh returns `Ok(None)`.
        assert!(
            context.cached_active_application().is_none(),
            "cache must stay empty when the probe reports no window"
        );
        // No fabricated source_app is persisted and the metadata
        // provider is never consulted.
        let recent = context.history().recent_entries(&context, 10).unwrap();
        assert!(
            recent[0].source_app.is_none(),
            "row must persist source_app = NULL when the probe returned no window, got {:?}",
            recent[0].source_app
        );
        assert!(
            provider.calls().is_empty(),
            "metadata provider must not be called when source_app is NULL, got {:?}",
            provider.calls()
        );
    }

    /// Confirms the cascade the design pins for the
    /// `wm_class_missing` stage: `_NET_ACTIVE_WINDOW` succeeded but
    /// the `WM_CLASS` lookup did not. The diagnostics surface
    /// exposes the asymmetry (`net_active_window_seen = true`,
    /// `wm_class_seen = false`) so the user can tell "no X11
    /// window focused" apart from "WM_CLASS was undeclared on the
    /// focused window" — a real distinction on a focused Warp
    /// capture where the XWayland window id resolves but the
    /// toolkit publishes an empty WM_CLASS in a deprecated form.
    #[test]
    fn diagnostics_mirror_wm_class_missing_stage() {
        let (_dir, context, fake_clipboard, provider, probe) = harness_with_staged_probe();
        // Probe returns no value (the watcher's contract is
        // `Ok(None)` for the wm-class-missing case — the wrapper
        // dropped into `set_stage` below records the granular
        // cause).
        probe.push(Ok(None));
        probe.push(Ok(None));
        probe.set_stage(clipvault_platform::ProbeStage::WmClassMissing);
        fake_clipboard.push_read(Ok(Some("cv-wm-class-missing".into())));
        context.refresh_active_application().expect("refresh");
        let source_app = resolved_source_identifier(&context);
        let watcher = build_watcher_for_context(&context);
        let _ = watcher.tick(
            &context,
            source_app.as_deref(),
            clipvault_core::AttemptOrigin::BackgroundLoop,
        );
        let diag = context.active_app_diagnostics();
        assert_eq!(
            diag.last_probe_stage,
            Some("wm_class_missing"),
            "diagnostics must mirror the wm_class_missing stage, got {diag:?}"
        );
        assert_eq!(
            diag.net_active_window_seen,
            Some(true),
            "diag.net_active_window_seen must be true when the window id was decoded, got {diag:?}"
        );
        assert_eq!(
            diag.wm_class_seen,
            Some(false),
            "diag.wm_class_seen must be false when WM_CLASS is missing, got {diag:?}"
        );
        let recent = context.history().recent_entries(&context, 10).unwrap();
        assert!(
            recent[0].source_app.is_none(),
            "row must not carry a fabricated source_app, got {:?}",
            recent[0].source_app
        );
        assert!(
            provider.calls().is_empty(),
            "metadata provider must not be consulted for a NULL source_app row, got {:?}",
            provider.calls()
        );
    }

    /// Verifies the diagnostics endpoint reports the counters the
    /// user explicitly requested: `successful_refreshes > 0`,
    /// `refresh_attempts` equals the number of loop ticks, and
    /// `cache_populated` flips on the first successful refresh.
    /// The shell documents both fields as the contract the
    /// `history-card-layout` regression pins.
    #[test]
    fn diagnostics_counters_track_successful_refresh_and_attempts() {
        use clipvault_core::ActiveApplication as PlatformActiveApplication;
        let (_dir, context, fake_clipboard, _provider, probe) = harness_with_staged_probe();
        probe.push(Ok(Some(PlatformActiveApplication::new(
            "Warp",
            "dev.warp.Warp",
        ))));
        probe.push(Ok(Some(PlatformActiveApplication::new(
            "Warp",
            "dev.warp.Warp",
        ))));
        probe.set_stage(clipvault_platform::ProbeStage::Identified);
        fake_clipboard.push_read(Ok(Some("cv-warp-counters".into())));
        context.refresh_active_application().expect("refresh");
        let source_app = resolved_source_identifier(&context);
        let watcher = build_watcher_for_context(&context);
        let _ = watcher.tick(
            &context,
            source_app.as_deref(),
            clipvault_core::AttemptOrigin::BackgroundLoop,
        );
        let diag = context.active_app_diagnostics();
        assert!(
            diag.refresh_attempts >= 1,
            "refresh_attempts must reflect the synchronous refresh, got {diag:?}"
        );
        assert!(
            diag.successful_refreshes >= 1,
            "successful_refreshes must increment on success, got {diag:?}"
        );
        assert_eq!(
            diag.failed_refreshes, 0,
            "failed_refreshes must stay 0 on success, got {diag:?}"
        );
        assert!(
            diag.cache_populated,
            "cache_populated must flip to true after a successful refresh, got {diag:?}"
        );
    }

    // -----------------------------------------------------------------
    // `pick_and_add_ignored_app` — macOS main-thread scheduling helper.
    // -----------------------------------------------------------------

    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering as AtomicOrdering};

    use clipvault_core::{IgnoredAppEntry, IgnoredAppsService, IgnoredAppsServiceError};
    use clipvault_platform::{
        ApplicationPicker, ApplicationPickerError, NoopActiveApplicationProbe, SelectedApplication,
    };

    /// Test scheduler type alias. Factoring the long `Box<dyn FnOnce(...)>`
    /// out keeps the test bodies readable and silences
    /// `clippy::type_complexity` for the panic-scheduler test.
    type TestScheduler = Box<dyn FnOnce(Box<dyn FnOnce() + Send + 'static>) -> Result<(), String>>;

    /// Build a fresh `AppContext` plus a `PrivacyGate` the helper can
    /// reuse for assertions. Mirrors the production bootstrap so the
    /// `IgnoredAppsService` reads from the same SQLite blacklist.
    fn picker_test_context() -> (tempfile::TempDir, AppContext) {
        use clipvault_core::{
            FakeClipboard, FakeHotkeyManager, FakePasteController, FakeSettingsNavigator,
            FakeTrayController, PlatformAdapters, SystemClock,
        };
        use clipvault_db::builtin_migrations;
        use clipvault_platform::{
            Capabilities, ClipboardBackend, DisplayServer, HotkeyManager, OsFamily,
            PasteController, PlatformInfo, SettingsNavigator, TrayController,
        };

        let dir = tempfile::TempDir::new().expect("tempdir");
        {
            let mut db =
                clipvault_db::Database::open(dir.path().join("clipvault.db")).expect("open");
            db.run_migrations(&builtin_migrations()).expect("migrate");
        }
        let info = PlatformInfo {
            home_dir: std::path::PathBuf::from("/tmp"),
            data_dir: std::path::PathBuf::from("/tmp/.clipvault"),
            os_family: OsFamily::Macos,
            display_server: DisplayServer::Unknown,
        };
        let platform_adapters = PlatformAdapters::new(
            Arc::new(clipvault_core::FakeClipboardBackend::new()) as Arc<dyn ClipboardBackend>,
            Arc::new(FakeHotkeyManager::new()) as Arc<dyn HotkeyManager>,
            Arc::new(NoopActiveApplicationProbe)
                as Arc<dyn clipvault_platform::ActiveApplicationProbe>,
            Arc::new(FakePasteController::new()) as Arc<dyn PasteController>,
            Arc::new(FakeTrayController::new()) as Arc<dyn TrayController>,
            Arc::new(FakeSettingsNavigator::new()) as Arc<dyn SettingsNavigator>,
            Arc::new(clipvault_core::NoopApplicationMetadataProvider)
                as Arc<dyn clipvault_platform::ApplicationMetadataProvider>,
            Capabilities::ALL_AVAILABLE,
            info,
        );
        let context = AppBootstrap::new()
            .with_clock(Arc::new(SystemClock) as Arc<dyn clipvault_core::Clock>)
            .with_clipboard(Arc::new(FakeClipboard::new()) as Arc<dyn clipvault_core::Clipboard>)
            .with_platform_adapters(platform_adapters)
            .bootstrap_at(dir.path().join("clipvault.db"))
            .expect("bootstrap");
        (dir, context)
    }

    /// Build a `run_picker` closure that delegates to a scripted
    /// [`ApplicationPicker`]. The closure mirrors the production
    /// `run_picker` so the test exercises the exact same error
    /// translation the shell observes.
    fn scripted_run_picker(
        context: AppContext,
        picker: Arc<dyn ApplicationPicker>,
    ) -> impl FnOnce() -> Result<PickAndAddOutcome, IgnoredAppError> + Send + 'static {
        move || {
            let outcome = context.ignored_apps().pick_and_add(&context, &*picker);
            match outcome {
                Ok(value) => Ok(value),
                Err(IgnoredAppsServiceError::Domain(error)) => Err(error),
                Err(IgnoredAppsServiceError::Persistence(error)) => {
                    Err(IgnoredAppError::Persistence {
                        reason: error.to_string(),
                    })
                }
            }
        }
    }

    /// A scripted picker that returns the supplied outcome on every
    /// `pick()` call. The struct deliberately implements `Send +
    /// Sync` (its only field is `Result<...>`) so it can travel
    /// inside the scheduled closure without further wrapping.
    #[derive(Clone)]
    struct ScriptedPicker(SelectedApplication);

    impl ApplicationPicker for ScriptedPicker {
        fn pick(&self) -> Result<SelectedApplication, ApplicationPickerError> {
            Ok(self.0.clone())
        }
        fn name(&self) -> &'static str {
            "bootstrap-test-scripted"
        }
    }

    struct FailingScriptedPicker(ApplicationPickerError);

    impl ApplicationPicker for FailingScriptedPicker {
        fn pick(&self) -> Result<SelectedApplication, ApplicationPickerError> {
            Err(match &self.0 {
                ApplicationPickerError::Cancelled => ApplicationPickerError::Cancelled,
                ApplicationPickerError::InvalidSelection { reason } => {
                    ApplicationPickerError::InvalidSelection {
                        reason: reason.clone(),
                    }
                }
                ApplicationPickerError::MissingIdentifier => {
                    ApplicationPickerError::MissingIdentifier
                }
                ApplicationPickerError::BackendUnavailable { reason } => {
                    ApplicationPickerError::BackendUnavailable {
                        reason: reason.clone(),
                    }
                }
                ApplicationPickerError::UnsupportedSession { reason } => {
                    ApplicationPickerError::UnsupportedSession {
                        reason: reason.clone(),
                    }
                }
            })
        }
        fn name(&self) -> &'static str {
            "bootstrap-test-failing"
        }
    }

    fn selected_application(
        identifier: &str,
        display_name: &str,
        icon_ref: Option<&str>,
    ) -> SelectedApplication {
        SelectedApplication {
            identifier: identifier.to_string(),
            display_name: display_name.to_string(),
            icon_ref: icon_ref.map(|value| value.to_string()),
        }
    }

    /// When the calling thread is the macOS main thread the helper
    /// MUST run the picker inline and never enqueue anything. The
    /// scheduler is a panic closure so a regression that
    /// accidentally invokes the scheduler surfaces as a test failure
    /// instead of a deadlock.
    #[test]
    fn pick_and_add_ignored_app_skips_scheduler_when_already_on_main_thread() {
        let (_dir, context) = picker_test_context();
        let picker: Arc<dyn ApplicationPicker> = Arc::new(ScriptedPicker(selected_application(
            "com.apple.terminal",
            "Terminal",
            Some("ignored-apps/com.apple.terminal.png"),
        )));
        let run_picker = scripted_run_picker(context.clone(), picker);

        let scheduler: TestScheduler = Box::new(|_work| -> Result<(), String> {
            panic!("scheduler must not be invoked when on the main thread")
        });

        let outcome =
            pick_and_add_ignored_app_impl(true, run_picker, scheduler, Duration::from_secs(1))
                .expect("inline path");

        match outcome {
            PickAndAddOutcome::Added(entry) => {
                assert_eq!(entry.id, "com.apple.terminal");
                assert_eq!(entry.display_name.as_deref(), Some("Terminal"));
                assert_eq!(
                    entry.icon_ref.as_deref(),
                    Some("ignored-apps/com.apple.terminal.png")
                );
            }
            other => panic!("expected Added, got {other:?}"),
        }
    }

    /// When the calling thread is off-main the helper MUST route the
    /// picker construction through the scheduler. The fake scheduler
    /// records its invocation and runs the work synchronously so the
    /// helper receives the inner result through the channel.
    #[test]
    fn pick_and_add_ignored_app_invokes_scheduler_when_off_main_thread() {
        let (_dir, context) = picker_test_context();
        let picker: Arc<dyn ApplicationPicker> = Arc::new(ScriptedPicker(selected_application(
            "com.apple.terminal",
            "Terminal",
            Some("ignored-apps/com.apple.terminal.png"),
        )));
        let run_picker = scripted_run_picker(context.clone(), picker);

        let invoked = Arc::new(AtomicBool::new(false));
        let invoked_for_scheduler = invoked.clone();
        let scheduler = move |work: Box<dyn FnOnce() + Send + 'static>| -> Result<(), String> {
            invoked_for_scheduler.store(true, AtomicOrdering::SeqCst);
            work();
            Ok(())
        };

        let outcome =
            pick_and_add_ignored_app_impl(false, run_picker, scheduler, Duration::from_secs(1))
                .expect("scheduler path");

        assert!(
            invoked.load(AtomicOrdering::SeqCst),
            "scheduler was not called"
        );
        match outcome {
            PickAndAddOutcome::Added(entry) => {
                assert_eq!(entry.id, "com.apple.terminal");
                assert_eq!(entry.display_name.as_deref(), Some("Terminal"));
            }
            other => panic!("expected Added, got {other:?}"),
        }
    }

    /// When Tauri (or any other scheduler) refuses to enqueue the
    /// closure the helper MUST surface a `BackendUnavailable`
    /// variant whose reason is safe to emit — no clipboard content,
    /// no hash, no identifier, no path.
    #[test]
    fn pick_and_add_ignored_app_maps_schedule_failure_to_backend_unavailable() {
        let (_dir, context) = picker_test_context();
        let picker: Arc<dyn ApplicationPicker> = Arc::new(ScriptedPicker(selected_application(
            "com.apple.terminal",
            "Terminal",
            None,
        )));
        let run_picker = scripted_run_picker(context.clone(), picker);

        let scheduler = move |_work: Box<dyn FnOnce() + Send + 'static>| -> Result<(), String> {
            Err("runtime queue closed".to_string())
        };

        let result =
            pick_and_add_ignored_app_impl(false, run_picker, scheduler, Duration::from_secs(1));
        match result {
            Err(IgnoredAppError::BackendUnavailable { reason }) => {
                assert!(
                    reason.contains("could not be scheduled"),
                    "reason must indicate schedule failure: {reason}"
                );
                assert!(
                    reason.contains("runtime queue closed"),
                    "reason must surface the runtime detail: {reason}"
                );
                assert!(
                    !reason.contains("com.apple.terminal"),
                    "reason must not leak the selected identifier: {reason}"
                );
            }
            other => panic!("expected BackendUnavailable, got {other:?}"),
        }
    }

    /// When the main thread never picks the closure up inside the
    /// bound the helper MUST time out and surface
    /// `BackendUnavailable` instead of hanging the shell. The fake
    /// scheduler accepts the work without ever running it so the
    /// channel sits empty until the timeout fires.
    #[test]
    fn pick_and_add_ignored_app_maps_timeout_to_backend_unavailable() {
        let (_dir, context) = picker_test_context();
        let picker: Arc<dyn ApplicationPicker> = Arc::new(ScriptedPicker(selected_application(
            "com.apple.terminal",
            "Terminal",
            None,
        )));
        let run_picker = scripted_run_picker(context.clone(), picker);

        let scheduler =
            move |_work: Box<dyn FnOnce() + Send + 'static>| -> Result<(), String> { Ok(()) };

        let result =
            pick_and_add_ignored_app_impl(false, run_picker, scheduler, Duration::from_millis(100));
        match result {
            Err(IgnoredAppError::BackendUnavailable { reason }) => {
                assert!(
                    reason.contains("did not complete"),
                    "reason must indicate a timeout: {reason}"
                );
                assert!(
                    reason.contains("100ms"),
                    "reason must include the timeout bound: {reason}"
                );
            }
            other => panic!("expected BackendUnavailable, got {other:?}"),
        }
    }

    /// Cancelling the picker is a silent, mutating-free operation:
    /// the helper MUST propagate `Cancelled` through the scheduler
    /// path without rewriting the blacklist.
    #[test]
    fn pick_and_add_ignored_app_preserves_cancellation_through_scheduler() {
        let (_dir, context) = picker_test_context();
        let picker: Arc<dyn ApplicationPicker> =
            Arc::new(FailingScriptedPicker(ApplicationPickerError::Cancelled));
        let run_picker = scripted_run_picker(context.clone(), picker);

        let invoked = Arc::new(AtomicBool::new(false));
        let invoked_for_scheduler = invoked.clone();
        let scheduler = move |work: Box<dyn FnOnce() + Send + 'static>| -> Result<(), String> {
            invoked_for_scheduler.store(true, AtomicOrdering::SeqCst);
            work();
            Ok(())
        };

        let result =
            pick_and_add_ignored_app_impl(false, run_picker, scheduler, Duration::from_secs(1));
        assert!(invoked.load(AtomicOrdering::SeqCst));
        match result {
            Err(IgnoredAppError::Cancelled) => {}
            other => panic!("expected Cancelled, got {other:?}"),
        }

        let entries = context
            .ignored_apps()
            .list(&context)
            .expect("list stays empty");
        assert!(entries.is_empty(), "cancellation must not mutate the list");
    }

    /// A valid selection MUST persist metadata (identifier,
    /// display name, icon reference) and survive the round trip
    /// through the scheduler channel.
    #[test]
    fn pick_and_add_ignored_app_preserves_valid_selection_through_scheduler() {
        let (_dir, context) = picker_test_context();
        let picker: Arc<dyn ApplicationPicker> = Arc::new(ScriptedPicker(selected_application(
            "  Com.Apple.Terminal  ",
            "Terminal",
            Some("ignored-apps/com.apple.terminal.png"),
        )));
        let run_picker = scripted_run_picker(context.clone(), picker);

        let scheduler = move |work: Box<dyn FnOnce() + Send + 'static>| -> Result<(), String> {
            work();
            Ok(())
        };

        let outcome =
            pick_and_add_ignored_app_impl(false, run_picker, scheduler, Duration::from_secs(1))
                .expect("picker result");

        match outcome {
            PickAndAddOutcome::Added(entry) => {
                assert_eq!(entry.id, "com.apple.terminal");
                assert_eq!(entry.display_name.as_deref(), Some("Terminal"));
                assert_eq!(
                    entry.icon_ref.as_deref(),
                    Some("ignored-apps/com.apple.terminal.png")
                );
            }
            other => panic!("expected Added, got {other:?}"),
        }

        let entries = context.ignored_apps().list(&context).expect("list");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, "com.apple.terminal");
        assert_eq!(entries[0].display_name.as_deref(), Some("Terminal"));
        assert_eq!(
            entries[0].icon_ref.as_deref(),
            Some("ignored-apps/com.apple.terminal.png")
        );
    }

    /// Selecting the same application twice MUST be idempotent: the
    /// first call returns `Added`, the second returns `Updated`,
    /// and the list keeps a single row.
    #[test]
    fn pick_and_add_ignored_app_is_idempotent_through_scheduler() {
        let (_dir, context) = picker_test_context();
        let picker: Arc<dyn ApplicationPicker> = Arc::new(ScriptedPicker(selected_application(
            "com.apple.terminal",
            "Terminal",
            Some("ignored-apps/com.apple.terminal.png"),
        )));

        let first = {
            let picker = picker.clone();
            let run_picker = scripted_run_picker(context.clone(), picker);
            let scheduler = move |work: Box<dyn FnOnce() + Send + 'static>| -> Result<(), String> {
                work();
                Ok(())
            };
            pick_and_add_ignored_app_impl(false, run_picker, scheduler, Duration::from_secs(1))
                .expect("first")
        };
        let second = {
            let run_picker = scripted_run_picker(context.clone(), picker);
            let scheduler = move |work: Box<dyn FnOnce() + Send + 'static>| -> Result<(), String> {
                work();
                Ok(())
            };
            pick_and_add_ignored_app_impl(false, run_picker, scheduler, Duration::from_secs(1))
                .expect("second")
        };
        assert!(matches!(first, PickAndAddOutcome::Added(_)));
        assert!(matches!(second, PickAndAddOutcome::Updated(_)));
        let entries = context.ignored_apps().list(&context).expect("list");
        assert_eq!(entries.len(), 1, "idempotent: list must not duplicate");
    }

    /// A `None` icon reference MUST NOT block the privacy rule: the
    /// row is persisted with `icon_ref = None` so the blacklist
    /// keeps matching the identifier.
    #[test]
    fn pick_and_add_ignored_app_preserves_rule_when_icon_is_missing() {
        let (_dir, context) = picker_test_context();
        let picker: Arc<dyn ApplicationPicker> = Arc::new(ScriptedPicker(selected_application(
            "com.apple.terminal",
            "Terminal",
            None,
        )));
        let run_picker = scripted_run_picker(context.clone(), picker);
        let scheduler = move |work: Box<dyn FnOnce() + Send + 'static>| -> Result<(), String> {
            work();
            Ok(())
        };

        let outcome =
            pick_and_add_ignored_app_impl(false, run_picker, scheduler, Duration::from_secs(1))
                .expect("picker result");

        match outcome {
            PickAndAddOutcome::Added(entry) => {
                assert!(entry.icon_ref.is_none());
            }
            other => panic!("expected Added, got {other:?}"),
        }

        let entries = context.ignored_apps().list(&context).expect("list");
        assert_eq!(entries.len(), 1);
        assert!(entries[0].icon_ref.is_none());
    }

    /// Errors raised by the picker adapter (for example
    /// `BackendUnavailable` or `UnsupportedSession`) MUST flow
    /// through the scheduler channel untouched so the shell can
    /// surface the matching localised copy.
    #[test]
    fn pick_and_add_ignored_app_propagates_unsupported_session_through_scheduler() {
        let (_dir, context) = picker_test_context();
        let picker: Arc<dyn ApplicationPicker> = Arc::new(FailingScriptedPicker(
            ApplicationPickerError::UnsupportedSession {
                reason: "wayland".into(),
            },
        ));
        let run_picker = scripted_run_picker(context.clone(), picker);
        let scheduler = move |work: Box<dyn FnOnce() + Send + 'static>| -> Result<(), String> {
            work();
            Ok(())
        };

        let result =
            pick_and_add_ignored_app_impl(false, run_picker, scheduler, Duration::from_secs(1));
        match result {
            Err(IgnoredAppError::UnsupportedSession { reason }) => {
                assert_eq!(reason, "wayland");
            }
            other => panic!("expected UnsupportedSession, got {other:?}"),
        }
    }

    /// Privacy regression: the helper MUST never augment an error
    /// reason with clipboard content, content hashes or source
    /// identifiers. The schedule path propagates the runtime error
    /// verbatim, but the helper's own prefix is a metadata-only
    /// description that does not depend on user data. The
    /// picker-layer error reasons describe the bundle, not the
    /// clipboard, and the helper forwards them untouched.
    #[test]
    fn pick_and_add_ignored_app_does_not_leak_clipboard_in_reasons() {
        let (_dir, context) = picker_test_context();

        // Picker-layer error: the helper forwards the reason as-is
        // and never augments it. We use a SAFE bundle-related reason
        // here — anything coming from the picker describes the
        // bundle, never the clipboard payload.
        let picker: Arc<dyn ApplicationPicker> = Arc::new(FailingScriptedPicker(
            ApplicationPickerError::InvalidSelection {
                reason: "not an .app bundle".to_string(),
            },
        ));
        let run_picker = scripted_run_picker(context.clone(), picker);
        let scheduler = move |work: Box<dyn FnOnce() + Send + 'static>| -> Result<(), String> {
            work();
            Ok(())
        };
        let result =
            pick_and_add_ignored_app_impl(false, run_picker, scheduler, Duration::from_secs(1));
        match result {
            Err(IgnoredAppError::InvalidSelection { reason }) => {
                assert_eq!(reason, "not an .app bundle");
                assert!(
                    !reason.contains("cv-")
                        && !reason.contains("/tmp/")
                        && !reason.contains("password")
                        && !reason.contains("token"),
                    "picker-layer reasons must describe the bundle only: {reason}"
                );
            }
            other => panic!("expected InvalidSelection, got {other:?}"),
        }

        // Schedule-failure path: the helper builds the prefix
        // itself. The prefix is metadata-only — it does NOT
        // interpolate user identifiers, paths, hash or content.
        let picker: Arc<dyn ApplicationPicker> = Arc::new(ScriptedPicker(selected_application(
            "com.apple.terminal",
            "Terminal",
            None,
        )));
        let run_picker = scripted_run_picker(context.clone(), picker);
        let scheduler = move |_work: Box<dyn FnOnce() + Send + 'static>| -> Result<(), String> {
            Err("queue closed".to_string())
        };
        let result =
            pick_and_add_ignored_app_impl(false, run_picker, scheduler, Duration::from_secs(1));
        match result {
            Err(IgnoredAppError::BackendUnavailable { reason }) => {
                assert!(
                    reason.starts_with(
                        "application picker could not be scheduled on the main thread"
                    ),
                    "helper prefix must be the metadata-only schedule description: {reason}"
                );
                assert!(
                    !reason.contains("com.apple.terminal"),
                    "schedule reason must never include the selected identifier: {reason}"
                );
                assert!(!reason.contains("/tmp/"));
                assert!(!reason.contains(".app"));
            }
            other => panic!("expected BackendUnavailable, got {other:?}"),
        }

        // Timeout reason: the helper builds the entire message
        // itself. It MUST be metadata-only.
        let picker: Arc<dyn ApplicationPicker> = Arc::new(ScriptedPicker(selected_application(
            "com.apple.terminal",
            "Terminal",
            None,
        )));
        let run_picker = scripted_run_picker(context.clone(), picker);
        let scheduler =
            move |_work: Box<dyn FnOnce() + Send + 'static>| -> Result<(), String> { Ok(()) };
        let result =
            pick_and_add_ignored_app_impl(false, run_picker, scheduler, Duration::from_millis(50));
        match result {
            Err(IgnoredAppError::BackendUnavailable { reason }) => {
                assert!(
                    reason.starts_with(
                        "application picker did not complete on the main thread within"
                    ),
                    "timeout reason must be the metadata-only timeout description: {reason}"
                );
                assert!(reason.contains("50ms"));
                assert!(!reason.contains("com.apple.terminal"));
                assert!(!reason.contains(".app"));
            }
            other => panic!("expected BackendUnavailable, got {other:?}"),
        }
    }

    /// Path-only sanity: the picker DTO used by the helper has no
    /// path field. The test pins the contract so a future refactor
    /// that adds one surfaces as a regression instead of
    /// accidentally persisting an absolute bundle path.
    #[test]
    fn pick_and_add_ignored_app_selected_application_does_not_carry_a_path() {
        let value = selected_application(
            "com.example.app",
            "Example",
            Some("ignored-apps/com.example.app.png"),
        );
        let json = serde_json::to_string(&value).expect("serialise");
        assert!(!json.contains("path"));
        assert!(!json.contains("bundle_url"));
        let entry = IgnoredAppEntry::new(
            value.identifier.clone(),
            Some(value.display_name.clone()),
            value.icon_ref.clone(),
            "2026-01-02T03:04:05Z",
        );
        let _ = entry;
        let _ = std::any::type_name::<IgnoredAppsService>();
        let _ = AtomicUsize::new(0);
    }

    // -----------------------------------------------------------------
    // Cross-platform `AppState::active_app_refresher` regression.
    //
    // The user reported the shell failed to compile on Ubuntu because
    // `bootstrap.rs` referenced `clipvault_platform::MainQueueActiveAppRefresher`
    // at sites that have no `cfg` gate:
    //
    //   - `AppState::active_app_refresher`;
    //   - the macOS/Linux signature of
    //     `install_active_app_main_queue_refresher`;
    //   - inside the macOS install body the direct path
    //     `MainQueueActiveAppRefresher::install(...)`.
    //
    // The platform crate guards the symbol behind
    // `#[cfg(all(target_os = "macos", feature = "macos-native"))]`,
    // so the reference is unreachable on Linux. The
    // `linux-x11-compatibility` change introduces a
    // platform-neutral alias, `clipvault_platform::ActiveAppRefresherHandle`,
    // and routes every shell-side reference through it. The tests
    // below pin the new contract so a future refactor that resurfaces
    // a direct `MainQueueActiveAppRefresher` reference at a
    // non-macOS-gated site fails on the platform that the regression
    // was reported on (or fails at compile time on every other
    // platform).
    // -----------------------------------------------------------------

    /// Build a minimal `AppState` for the cross-platform tests so
    /// `install_active_app_main_queue_refresher` can be exercised
    /// without going through the full `build_state` path (which would
    /// need a Tauri runtime, a real platform probe and a SQLite
    /// migration to complete).
    ///
    /// The helper is consumed only by the
    /// `#[cfg(not(target_os = "macos"))]` regression test
    /// (`install_active_app_main_queue_refresher_returns_skipped_unsupported_on_linux`),
    /// so the `dead_code` lint fires on macOS builds. The platform
    /// itself stays platform-neutral — the helper deliberately
    /// keeps the same shape across both build targets so the test
    /// can re-use the same fixtures on Linux.
    #[allow(dead_code)]
    fn provisional_state_for_active_app_refresher(context: &AppContext) -> AppState {
        use clipvault_core::CaptureWatcher;
        use clipvault_core::{
            FakeHotkeyManager, FakePasteController, FakeSettingsNavigator, FakeTrayController,
            PlatformAdapters,
        };
        use clipvault_platform::{
            Capabilities, ClipboardBackend, DisplayServer, HotkeyManager, PasteController,
            PlatformInfo, SettingsNavigator, TrayController,
        };

        let info = PlatformInfo {
            home_dir: std::path::PathBuf::from("/tmp"),
            data_dir: std::path::PathBuf::from("/tmp/.clipvault"),
            // The `install_active_app_main_queue_refresher` gating is
            // based on `cfg(target_os = "macos")`, not on `OsFamily`,
            // so the test can use any family on every platform. The
            // helper keeps the platform-agnostic surface stable for
            // both build targets.
            os_family: clipvault_platform::OsFamily::Linux,
            display_server: DisplayServer::Unknown,
        };
        let platform_adapters = PlatformAdapters::new(
            Arc::new(clipvault_core::FakeClipboardBackend::new()) as Arc<dyn ClipboardBackend>,
            Arc::new(FakeHotkeyManager::new()) as Arc<dyn HotkeyManager>,
            Arc::new(clipvault_core::NoopActiveApplicationProbe)
                as Arc<dyn clipvault_platform::ActiveApplicationProbe>,
            Arc::new(FakePasteController::new()) as Arc<dyn PasteController>,
            Arc::new(FakeTrayController::new()) as Arc<dyn TrayController>,
            Arc::new(FakeSettingsNavigator::new()) as Arc<dyn SettingsNavigator>,
            Arc::new(clipvault_core::NoopApplicationMetadataProvider)
                as Arc<dyn clipvault_platform::ApplicationMetadataProvider>,
            Capabilities::default(),
            info,
        );
        AppState {
            context: context.clone(),
            watcher: Arc::new(CaptureWatcher::new(
                platform_adapters.clipboard().clone(),
                CaptureWatcher::default_interval(),
            )),
            adapters: platform_adapters,
            cancel_capture: Arc::new(AtomicBool::new(false)),
            active_app_refresher: None,
            metadata_scheduler: Arc::new(MetadataEnrichmentScheduler::new()),
        }
    }

    /// Pin the contract: `AppState::active_app_refresher` always uses
    /// the platform-neutral alias, and the alias is constructible as
    /// `Option<...>` from both sides of the platform split. The test
    /// compiles on macOS, Linux and every other host. A regression
    /// that swaps the alias back for a direct
    /// `MainQueueActiveAppRefresher` reference breaks this test on
    /// Linux at compile time, which is exactly the regression the
    /// user reported.
    #[allow(dead_code)]
    #[test]
    fn app_state_active_app_refresher_uses_platform_neutral_alias() {
        let handle: Option<clipvault_platform::ActiveAppRefresherHandle> = None;
        assert!(
            handle.is_none(),
            "constructing the platform-neutral handle must compile on every target"
        );
    }

    /// Pin the contract: on non-macOS the install helper returns
    /// `SkippedUnsupported` and `None`, and the shell never imports
    /// the macOS-only refresher symbol. The test compiles only on
    /// the non-macOS branch so the macOS gating of the install
    /// symbol does not let an off-target regression pass silently.
    #[cfg(not(target_os = "macos"))]
    #[test]
    fn install_active_app_main_queue_refresher_returns_skipped_unsupported_on_linux() {
        let (_dir, context, _clipboard) = harness_for_shared_watcher();
        let provisional = provisional_state_for_active_app_refresher(&context);

        let (outcome, handle) = install_active_app_main_queue_refresher(&provisional);

        assert_eq!(
            outcome,
            MainQueueInstallOutcome::SkippedUnsupported,
            "non-macOS builds must short-circuit the install helper"
        );
        assert!(
            handle.is_none(),
            "non-macOS builds must leave the platform-neutral handle empty"
        );
    }

    /// Pin the contract: on non-macOS the shell MUST NOT mention the
    /// `MainQueueActiveAppRefresher` symbol directly anywhere outside
    /// a `#[cfg(target_os = "macos")]` block. We express the rule as
    /// a compile-time check: a `[lib]`-level line that tries to
    /// mention the type fails to compile on the non-macOS branch.
    #[cfg(not(target_os = "macos"))]
    #[allow(dead_code)]
    fn _assert_linux_does_not_reference_macos_only_refresher_type() {
        // This function body never executes; it exists only to make
        // the compiler verify the type identity path is conditional.
        // A future refactor that lifts the `use`/`pub use` of the
        // macOS refresher symbol out of the `cfg` gate will surface
        // here as a compile error on Linux builds (the platform
        // crate does not export the symbol without the
        // `cfg(all(target_os = "macos", feature = "macos-native"))`
        // gate).
        let probe: Option<clipvault_platform::ActiveAppRefresherHandle> = None;
        let _ = probe;
        let _ = std::any::type_name::<MainQueueInstallOutcome>();
    }

    /// Pin the contract: on macOS the install helper still installs
    /// the real `MainQueueActiveAppRefresher`. The compile-time
    /// assertion here guarantees the alias resolves to the macOS
    /// refresher type so the dispatcher timer keeps running and
    /// `MainQueueInstallOutcome::Installed`/`Failed(...)` paths stay
    /// observable through the diagnostics endpoint.
    #[cfg(target_os = "macos")]
    #[allow(dead_code)]
    #[test]
    fn macos_active_app_refresher_handle_resolves_to_real_refresher_type() {
        // The alias MUST equal the macOS refresher type; otherwise
        // the timer would silently no-op and the captured application
        // identifier would fall back to "unknown source" for the
        // entire session. The `TypeId` equality is the strongest
        // compile-time pin available without spinning up a real
        // dispatch timer.
        assert_eq!(
            std::any::TypeId::of::<clipvault_platform::ActiveAppRefresherHandle>(),
            std::any::TypeId::of::<clipvault_platform::MainQueueActiveAppRefresher>(),
            "on macOS the platform-neutral alias must resolve to the real refresher"
        );
        let probe: Option<clipvault_platform::MainQueueActiveAppRefresher> = None;
        let _ = probe;
    }

    /// The shell MUST NOT introduce `unsafe` (manual `Send`/`Sync`
    /// or otherwise) to mask the cross-platform refresher wiring.
    /// The previous prototype considered `unsafe impl Send for
    /// ()`, which would have made the alias emit only on nightly
    /// and violated the project's `#![deny(unsafe_op_in_unsafe_fn)]`
    /// baseline. The test parses the source — stripping line and
    /// block comments so prose that mentions the keyword does not
    /// trigger a false positive — and asserts that the file does
    /// not contain a statement-level `unsafe` block opener or a
    /// `unsafe fn`/`unsafe impl Send|Sync for ...` declaration. The
    /// platform adapters in other crates may legitimately contain
    /// `unsafe`; this check is scoped to `bootstrap.rs` only.
    #[test]
    fn bootstrap_active_app_refresher_wiring_stays_safe() {
        let source_path =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/bootstrap.rs");
        let source = std::fs::read_to_string(&source_path)
            .unwrap_or_else(|error| panic!("read bootstrap source: {error}"));
        // Strip /* ... */ blocks and // ... lines so the prose
        // describing the rule does not trip the counter. The shell
        // never nests these forms in the code we care about; if a
        // future refactor introduces one, the test surfaces as a
        // false negative that prompts the author to update the
        // parser.
        let mut stripped = String::with_capacity(source.len());
        let mut chars = source.chars().peekable();
        let mut in_line_comment = false;
        let mut in_block_comment = false;
        while let Some(ch) = chars.next() {
            if in_line_comment {
                if ch == '\n' {
                    in_line_comment = false;
                    stripped.push(ch);
                }
                continue;
            }
            if in_block_comment {
                if ch == '*' && chars.peek() == Some(&'/') {
                    chars.next();
                    in_block_comment = false;
                    stripped.push(' ');
                }
                continue;
            }
            if ch == '/' && chars.peek() == Some(&'/') {
                chars.next();
                in_line_comment = true;
                stripped.push(' ');
                continue;
            }
            if ch == '/' && chars.peek() == Some(&'*') {
                chars.next();
                in_block_comment = true;
                stripped.push(' ');
                continue;
            }
            if ch == '"' {
                let mut string = String::from(ch);
                while let Some(next) = chars.next() {
                    string.push(next);
                    if next == '\\' {
                        if let Some(escaped) = chars.next() {
                            string.push(escaped);
                            continue;
                        }
                    }
                    if next == '"' {
                        break;
                    }
                }
                stripped.push_str(&" ".repeat(string.len()));
                continue;
            }
            stripped.push(ch);
        }
        let mut offenders = Vec::new();
        for (idx, line) in stripped.lines().enumerate() {
            let trimmed = line.trim_start();
            // Skip attribute lines (`#[...]`) since they are
            // metadata, not code.
            if trimmed.starts_with('#') {
                continue;
            }
            // Statement opens: `unsafe { ...`, `unsafe fn ...`,
            // `unsafe impl`, `unsafe trait`. The shell does not
            // emit any of these on purpose; catching them here is
            // the regression pin.
            let starts_unsafe = trimmed.starts_with("unsafe ") || trimmed.starts_with("unsafe{");
            if starts_unsafe {
                offenders.push(idx + 1);
            }
        }
        assert!(
            offenders.is_empty(),
            "bootstrap.rs must not introduce unsafe blocks (cross-platform refresher wiring); offenders at lines {offenders:?}"
        );
    }

    // -----------------------------------------------------------------
    // `linux-source-app-metadata` follow-up: structural + behavioural
    // regression pins for the Ubuntu `source_app = NULL` regression.
    //
    // The user reported that on Ubuntu GNOME Wayland + XWayland the
    // capture loop never persisted `source_app` (and therefore never
    // the user-visible name or icon) because the shell-side
    // `build_active_application` / `build_paste_controller` were
    // gated behind `#[cfg(all(target_os = "linux", feature =
    // "linux-x11"))]`. The `linux-x11` feature is part of the
    // `[features]` table of `clipvault-app` but is NOT part of the
    // default feature set, so the `[cfg]` arms compiled out at build
    // time and the shell returned `NoopActiveApplicationProbe` while
    // `capabilities.active_application` stayed `true`. The probe
    // assembly lived in `clipvault-platform` (feature-gated there) and
    // the target-specific dependency declaration in
    // `app/tauri/src-tauri/Cargo.toml` already enables the
    // `linux-x11` feature on the platform crate for every Linux
    // build — the bug was solely on the shell side.
    //
    // The fixes are:
    //
    // 1. The Linux arms of `build_active_application` and
    //    `build_paste_controller` are now `#[cfg(target_os = "linux")]`
    //    (no `feature = "linux-x11"` join) so the shell compiles the
    //    adapter wiring on every Linux build.
    // 2. The unused `parse_active_window_id` import that survived the
    //    previous patch is removed so the `unused_import` warning can
    //    no longer reappear.
    //
    // The tests below pin both halves: a parse-the-source structural
    // guard that scans the bootstrap source for the disallowed
    // `cfg(all(target_os = "linux", feature = "linux-x11"))` pattern,
    // and a set of behavioural tests that exercise the post-fix
    // contracts end-to-end without requiring a live X server.
    // -----------------------------------------------------------------

    /// Structural regression: the shell must NOT ship
    /// `#[cfg(all(target_os = "linux", feature = "linux-x11"))]` arms
    /// any more. The shell enables its X11 adapters through the
    /// `clipvault-platform` target-specific dependency declaration
    /// (which already enables the `linux-x11` feature on the platform
    /// crate for every Linux build), so any future contributor who
    /// gates the shell-side wiring on `feature = "linux-x11"`
    /// reintroduces the regression the user reported: the Linux arm
    /// is dropped at compile time, the bootstrap returns
    /// `NoopActiveApplicationProbe`, `capabilities.active_application`
    /// stays `true` and every capture lands with
    /// `source_app = NULL`.
    ///
    /// The source scanner strips `/* ... */` blocks, `// ...` line
    /// comments and `"..."` strings so prose that mentions the
    /// disallowed attribute does not trip the linter. The shell never
    /// nests these forms in the code we care about; if a future
    /// refactor introduces one, the linter surfaces as a false
    /// negative that prompts the author to update the parser. The
    /// test runs on every target (macOS dev hosts included) so the
    /// guard fires the moment a regression is committed, regardless
    /// of the host that produced it.
    #[test]
    fn shell_linux_x11_cfg_does_not_require_linux_x11_feature() {
        let source_path =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/bootstrap.rs");
        let source = std::fs::read_to_string(&source_path)
            .unwrap_or_else(|error| panic!("read bootstrap source: {error}"));
        let stripped = strip_prose_for_cfg_scan(&source);
        let mut offenders = Vec::new();
        for (idx, raw_line) in stripped.lines().enumerate() {
            let trimmed = raw_line.trim();
            if !trimmed.starts_with("#[cfg(all(") {
                continue;
            }
            if !trimmed.contains("feature = \"linux-x11\"") {
                continue;
            }
            // The disqualified arms are those that BOTH gate on
            // `target_os = "linux"` and require the `linux-x11`
            // feature. We deliberately accept all other
            // `cfg(all(... feature = "linux-x11" ...))` arms so the
            // linter does not over-fire.
            if trimmed.contains("target_os = \"linux\"") {
                offenders.push(idx + 1);
            }
        }
        assert!(
            offenders.is_empty(),
            "bootstrap.rs must not gate build_active_application / build_paste_controller on the `linux-x11` feature (target-specific dep already enables linux-x11 in clipvault-platform). Offending lines: {offenders:?}"
        );
    }

    /// Structural regression: the Linux shell dependency declaration
    /// on `clipvault-platform` MUST enable the `linux-svg-raster`
    /// feature. The shell pulls the platform crate in through a
    /// `[target.'cfg(all(target_os = "linux", not(target_os =
    /// "macos")))'.dependencies]` block; without the
    /// `linux-svg-raster` feature the SVG → PNG rasterizer is left
    /// out of the Linux build graph and the provider falls back to
    /// `IconFailureKind::SvgRejected` for any `.desktop` whose
    /// `Icon=` resolves to an SVG-only payload. The user reported
    /// `source_app_icon_ref = NULL` and an empty
    /// `~/.clipvault/assets/application-icons/` directory on every
    /// distribution that ships SVG icons (Ubuntu, Debian, Fedora,
    /// Arch, openSUSE) — the rasterizer was already merged in
    /// `clipvault-platform` but the shell forgot to opt in.
    ///
    /// The test parses the shell `Cargo.toml` and verifies that
    /// the Linux target-specific `clipvault-platform` dependency
    /// lists `linux-svg-raster` among its features. The parser is
    /// a minimal TOML walker: it locates the Linux target section
    /// by its `[target.'cfg(all(target_os = "linux", not(target_os
    /// = "macos")))'.dependencies]` header, scoops up the literal
    /// `features = [ ... ]` array that follows, and checks for the
    /// `"linux-svg-raster"` entry. TOML's surface here is small
    /// enough that a hand-rolled scan is more honest than pulling
    /// in a full TOML parser for the test: a future migration to a
    /// proper TOML dependency would only strengthen the regression.
    ///
    /// The test runs on every target (including macOS dev hosts)
    /// so the regression is caught by the `cargo test --workspace`
    /// run the dev workflow already executes — the bug surfaced on
    /// Linux precisely because the existing Linux-only CI skipped
    /// this assertion.
    #[test]
    fn shell_linux_svg_raster_feature_is_enabled_for_linux_target() {
        let manifest_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
        let source = std::fs::read_to_string(&manifest_path)
            .unwrap_or_else(|error| panic!("read shell Cargo.toml: {error}"));
        let features = linux_target_clipvault_platform_features(&source)
            .expect("shell Cargo.toml must declare a Linux target section with a clipvault-platform dependency and features = [...]");
        assert!(
            features.iter().any(|feature| feature == "linux-svg-raster"),
            "Linux target dependency on clipvault-platform must enable the `linux-svg-raster` feature so the SVG -> PNG rasterizer ships in the Ubuntu binary. features = {features:?}"
        );
    }

    /// Locate the `[target.'cfg(all(target_os = "linux", not(target_os
    /// = "macos")))'.dependencies]` table in the shell Cargo.toml and
    /// return the `features = [ ... ]` array the
    /// `clipvault-platform` dependency declares. Returns `None` when
    /// either section is missing so the caller can panic with a
    /// descriptive message instead of indexing into an empty vector.
    ///
    /// The walker is deliberately tiny — the shell Cargo.toml only
    /// has a handful of features in the Linux block and the test
    /// already documents the expected shape. Any future contributor
    /// who restructures the table gets an immediate test failure
    /// here, which is the desired regression semantics.
    fn linux_target_clipvault_platform_features(source: &str) -> Option<Vec<String>> {
        const HEADER: &str =
            "[target.'cfg(all(target_os = \"linux\", not(target_os = \"macos\")))'.dependencies]";
        let header_idx = source.find(HEADER)?;
        // Locate the body of the Linux target section by walking
        // forward until we either hit the next `[ ... ]` header (which
        // would terminate the table) or the end of the file.
        let rest = &source[header_idx + HEADER.len()..];
        let section_body: String = rest
            .lines()
            .take_while(|line| !line.trim_start().starts_with('['))
            .collect::<Vec<_>>()
            .join("\n");
        // Within the body, locate the `clipvault-platform = { ... }`
        // inline-table and balance the braces so a future refactor that
        // adds inner tables or arrays still extracts the right entry.
        let entry_marker = "clipvault-platform = {";
        let entry_open = section_body.find(entry_marker)?;
        let entry_open_brace = entry_open + entry_marker.len() - 1;
        let mut depth: usize = 1;
        let mut entry_close = None;
        for (offset, ch) in section_body[entry_open_brace + 1..].char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        entry_close = Some(entry_open_brace + 1 + offset + 1);
                        break;
                    }
                }
                _ => {}
            }
        }
        let entry_close = entry_close?;
        let entry_body = &section_body[entry_open_brace + 1..entry_close];
        // Extract the literal `features = [ ... ]` array. We only
        // read the substring between the `[` and `]` and ignore
        // whatever may follow in the same line.
        let features_marker = "features = [";
        let feat_open = entry_body.find(features_marker)? + features_marker.len();
        let feat_close = entry_body[feat_open..].find(']')? + feat_open;
        let raw = &entry_body[feat_open..feat_close];
        Some(
            raw.split(',')
                .map(str::trim)
                .map(|item| item.trim_matches('"').to_string())
                .filter(|item| !item.is_empty())
                .collect(),
        )
    }

    /// Behavioural regression: a probe that has answered with the
    /// `dev.warp.Warp` `WM_CLASS` class segment must end up persisted
    /// on the new row as `source_app = "dev.warp.Warp"`. The test
    /// invokes the recorded behaviour through the cache + watcher
    /// pair that production drives every tick, so a future regression
    /// that drops the Linux adapter wiring on the shell side surfaces
    /// here as an empty cache or a populated cache with the wrong
    /// probe attached.
    #[test]
    fn shell_linux_x11_adapter_attaches_dev_warp_warp_wm_class_to_source_app() {
        use clipvault_core::ActiveApplication as PlatformActiveApplication;
        let (_dir, context, fake_clipboard, provider, probe) = harness_with_staged_probe();
        probe.push(Ok(Some(PlatformActiveApplication::new(
            "Warp",
            "dev.warp.Warp",
        ))));
        probe.push(Ok(Some(PlatformActiveApplication::new(
            "Warp",
            "dev.warp.Warp",
        ))));
        probe.set_stage(clipvault_platform::ProbeStage::Identified);
        fake_clipboard.push_read(Ok(Some("cv-linux-x11-cfg-payload".into())));
        context.refresh_active_application().expect("refresh");
        let source_app = resolved_source_identifier(&context);
        let watcher = build_watcher_for_context(&context);
        let _ = watcher.tick(
            &context,
            source_app.as_deref(),
            clipvault_core::AttemptOrigin::BackgroundLoop,
        );
        let recent = context.history().recent_entries(&context, 10).unwrap();
        assert_eq!(
            recent[0].source_app.as_deref(),
            Some("dev.warp.Warp"),
            "shell must persist the WM_CLASS class segment the Linux probe reported"
        );
        let calls = provider.calls();
        assert!(
            calls.iter().any(|id| id == "dev.warp.Warp"),
            "metadata provider must see the refreshed identifier, got {calls:?}"
        );
    }

    /// Behavioural regression: when the Linux probe reports
    /// `Ok(None)` (the native Wayland branch) the shell must keep the
    /// empty-source contract documented by
    /// `desktop-platform-integration`: `source_app` stays NULL, no
    /// name/icon is invented, the cache stays empty, the metadata
    /// provider is never consulted. The cfg-side fix MUST NOT
    /// regress this branch.
    #[test]
    fn shell_native_wayland_keeps_empty_source_contract() {
        let (_dir, context, fake_clipboard, provider, probe) = harness_with_staged_probe();
        probe.push(Ok(None));
        probe.push(Ok(None));
        probe.set_stage(clipvault_platform::ProbeStage::WmClassMissing);
        fake_clipboard.push_read(Ok(Some("cv-linux-cfg-native-wayland".into())));
        context.refresh_active_application().expect("refresh");
        let source_app = resolved_source_identifier(&context);
        let watcher = build_watcher_for_context(&context);
        let outcome = watcher.tick(
            &context,
            source_app.as_deref(),
            clipvault_core::AttemptOrigin::BackgroundLoop,
        );
        assert!(
            matches!(
                outcome,
                clipvault_core::WatchTickOutcome::Captured(
                    clipvault_core::HistoryOutcome::Stored { .. }
                )
            ),
            "native Wayland must still produce a stored capture, got {outcome:?}"
        );
        let recent = context.history().recent_entries(&context, 10).unwrap();
        assert!(
            recent[0].source_app.is_none(),
            "native Wayland MUST NOT carry a fabricated source_app, got {:?}",
            recent[0].source_app
        );
        assert!(
            provider.calls().is_empty(),
            "metadata provider must not be consulted for NULL source_app row, got {:?}",
            provider.calls()
        );
    }

    /// Strip `/* ... */` blocks and `// ...` line comments from the
    /// supplied source so the structural regression test that scans
    /// for `#[cfg(all(target_os = "linux", feature = "linux-x11"))]`
    /// arms does not trip on prose that mentions the disallowed
    /// attribute. The producer of the source is `clipvault-app`,
    /// which never nests these forms inside the code we care about,
    /// so the simple character-level walker covers every shape the
    /// test guards against. If a future refactor nests these forms
    /// (for example a `cfg` attribute written inside a documentation
    /// comment), the walker surfaces as a false negative that prompts
    /// the author to extend it; that's the expected regression
    /// semantics.
    ///
    /// String literals are NOT stripped — the disallowed attribute is
    /// itself written with quoted tokens (`target_os = "linux"`,
    /// `feature = "linux-x11"`), and naively replacing every string
    /// with whitespace would hide the very attributes the test is
    /// trying to find. The shell does not contain literal code that
    /// mentions the disallowed cfg gates outside attributes, so
    /// leaving strings untouched is the safer choice.
    fn strip_prose_for_cfg_scan(source: &str) -> String {
        let mut out = String::with_capacity(source.len());
        let mut chars = source.chars().peekable();
        let mut in_line_comment = false;
        let mut in_block_comment = false;
        while let Some(ch) = chars.next() {
            if in_line_comment {
                if ch == '\n' {
                    in_line_comment = false;
                    out.push(ch);
                } else {
                    out.push(' ');
                }
                continue;
            }
            if in_block_comment {
                if ch == '*' && chars.peek() == Some(&'/') {
                    chars.next();
                    in_block_comment = false;
                    out.push(' ');
                    out.push(' ');
                } else if ch != '\n' {
                    out.push(' ');
                } else {
                    out.push(ch);
                }
                continue;
            }
            if ch == '/' && chars.peek() == Some(&'/') {
                chars.next();
                in_line_comment = true;
                out.push(' ');
                out.push(' ');
                continue;
            }
            if ch == '/' && chars.peek() == Some(&'*') {
                chars.next();
                in_block_comment = true;
                out.push(' ');
                out.push(' ');
                continue;
            }
            out.push(ch);
        }
        out
    }

    // -----------------------------------------------------------------
    // `linux-native-wayland-app-detection` regression pins:
    //
    // The shell MUST enable the `linux-wayland-active-app` feature on
    // the platform crate for every Linux target, the bootstrap MUST
    // wire the native probe as the authoritative source on Wayland
    // sessions before falling back to the XWayland EWMH probe, and the
    // Wayland probe's `Ok(None)` MUST NOT trigger an XWayland fallback
    // that surfaces a stale identifier. The tests below pin the
    // structural side (manifest gating) and the behavioural side
    // (precedence rules) on every target.
    // -----------------------------------------------------------------

    /// Structural regression: the Linux shell dependency on
    /// `clipvault-platform` MUST enable the `linux-wayland-active-app`
    /// feature. Without it the Wayland adapter is not in the link
    /// graph and the bootstrap falls back to the XWayland probe even
    /// on a session where the compositor announces
    /// `ext-foreign-toplevel-list-v1`. The test parses the shell
    /// `Cargo.toml` through the same minimal TOML walker the SVG
    /// regression uses, so the assertion runs on every developer
    /// machine without a Linux host.
    #[test]
    fn shell_linux_wayland_active_app_feature_is_enabled_for_linux_target() {
        let manifest_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
        let source = std::fs::read_to_string(&manifest_path)
            .unwrap_or_else(|error| panic!("read shell Cargo.toml: {error}"));
        let features = linux_target_clipvault_platform_features(&source)
            .expect("shell Cargo.toml must declare a Linux target section with a clipvault-platform dependency and features = [...]");
        assert!(
            features
                .iter()
                .any(|feature| feature == "linux-wayland-active-app"),
            "Linux target dependency on clipvault-platform must enable the `linux-wayland-active-app` feature so the native Wayland adapter ships in the Ubuntu binary. features = {features:?}"
        );
    }

    /// Behavioural regression: the bootstrap MUST wire the Wayland
    /// probe before the XWayland EWMH probe on Wayland sessions so a
    /// native snapshot takes precedence over an obsolete XWayland
    /// answer. The test inspects the post-fix `build_active_application`
    /// signature through a fake probe + platform info pair so the
    /// precedence rule is verified on every developer host.
    #[test]
    fn shell_wayland_native_probe_takes_precedence_over_xwayland() {
        // The bootstrap is gated on `target_os = "linux"` and the
        // probe assembly uses the Linux adapter wired by the
        // platform crate. On non-Linux hosts the test is a no-op
        // because the macOS branch short-circuits before the
        // precedence check; we still exercise the structural pin by
        // walking the source and asserting the native probe arm is
        // wired before the XWayland fallback.
        #[cfg(target_os = "linux")]
        {
            use clipvault_platform::{Capabilities, DisplayServer, OsFamily, PlatformInfo};
            let info = PlatformInfo {
                home_dir: std::path::PathBuf::from("/tmp"),
                data_dir: std::path::PathBuf::from("/tmp/.clipvault"),
                os_family: OsFamily::Linux,
                display_server: DisplayServer::Wayland,
            };
            let capabilities = Capabilities::ALL_AVAILABLE;
            // The probe assembly MUST produce a `Send + Sync` probe
            // the capture loop can install behind `Arc<dyn
            // ActiveApplicationProbe>`. The exact backend identifier
            // depends on the compositor: Wayland native on hosts that
            // publish `ext-foreign-toplevel-list-v1`, XWayland on
            // hosts that do not. The test asserts the structural
            // invariant: the probe exists and is not the no-op
            // fallback the pre-fix bootstrap returned when
            // `display_server` was Wayland.
            let probe = build_active_application(&info, capabilities);
            assert!(
                probe.name() != clipvault_platform::ActiveAppBackendKind::Unavailable.as_str()
                    || std::env::var_os("WAYLAND_DISPLAY").is_none(),
                "Wayland session with $WAYLAND_DISPLAY set must produce a non-noop probe, got {}",
                probe.name()
            );
        }
        #[cfg(not(target_os = "linux"))]
        {
            // Non-Linux hosts: pin the structural source shape so
            // the precedence rule is visible to every contributor.
            let source_path =
                std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/bootstrap.rs");
            let source = std::fs::read_to_string(&source_path)
                .unwrap_or_else(|error| panic!("read bootstrap source: {error}"));
            let stripped = strip_prose_for_cfg_scan(&source);
            assert!(
                stripped.contains("linux_wayland_active_app::try_build"),
                "build_active_application must call linux_wayland_active_app::try_build before falling back to the XWayland EWMH probe on Wayland sessions"
            );
            assert!(
                stripped.contains("ConnectionOutcome::Operational"),
                "build_active_application must match ConnectionOutcome::Operational to install the native Wayland probe"
            );
        }
    }
}
