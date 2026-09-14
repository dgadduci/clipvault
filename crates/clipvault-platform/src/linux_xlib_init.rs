//! Process-wide Xlib thread-safety preflight for Linux.
//!
//! [`global_hotkey`] spins up a worker thread that opens Xlib on Linux
//! and dispatches key events. When Tauri, GTK or the WebView has
//! already performed Xlib work from a different thread, that race can
//! trigger the XCB assertion
//! `[xcb] Unknown sequence number while processing queue` and abort
//! the process. The canonical fix is to call `XInitThreads` once,
//! before any other Xlib call in the process. `XInitThreads` itself
//! is idempotent at the OS level, but the rule it enables is
//! process-wide so it MUST run from a single thread of control.
//!
//! # Xlib ABI contract
//!
//! The Xlib prototype declares
//! [`Status XInitThreads(void)`](https://gitlab.freedesktop.org/xorg/lib/libx11)
//! and the header comment in
//! `/usr/include/X11/Xlib.h:1734` is explicit:
//!
//! > The XInitThreads() function returns a Status (non-zero on
//! > success, zero on failure).
//!
//! `x11-dl` reflects the same prototype
//! (`pub fn XInitThreads() -> c_int`, see
//! `~/.cargo/registry/.../x11-dl-2.21.0/src/xlib.rs`). The platform
//! crate MUST honour that contract: a non-zero return maps to
//! [`XlibInitOutcome::Initialized`] and a zero return maps to
//! [`XlibInitOutcome::InitFailed`]. Inverting the check was the root
//! cause of the previous regression that left every X11 host wired
//! to the no-op hotkey manager instead of the real Xlib backend.
//!
//! # Module contract
//!
//! This module is the single place the shell calls. It:
//!
//! - Loads `libX11.so` via [`x11_dl::Xlib::open`] and resolves
//!   `XInitThreads`.
//! - Calls `XInitThreads` exactly once (per process) and caches the
//!   result in a process-wide cell guarded by a single critical
//!   section. There is no window between the cache check and the
//!   native call, so two concurrent callers cannot both invoke
//!   `XInitThreads`.
//! - Returns a typed [`XlibInitOutcome`] distinguishing
//!   `Initialized`, `LibraryUnavailable` and `InitFailed` so the
//!   shell can fall back to the no-op hotkey manager without
//!   aborting the application.
//! - Exposes a [`XlibLoader`] trait so unit tests can inject a
//!   fake without ever touching libX11 or a real display.
//!
//! The module is gated behind `cfg(target_os = "linux")` AND the
//! `linux-xlib-init` feature (which in turn requires
//! `hotkey-global`). macOS, Windows and builds compiled without
//! the Xlib hotkey backend do not link or call any X11 symbol.
//!
//! [`global_hotkey`]: https://docs.rs/global-hotkey

#![cfg(all(target_os = "linux", feature = "linux-xlib-init"))]

use std::sync::Arc;

use parking_lot::Mutex;
use thiserror::Error;
use tracing::warn;

/// Outcome of the process-wide Xlib thread-safety preflight.
///
/// The variant carries no Xlib pointer and never includes clipboard
/// content, snippets, hashes or absolute paths so the value is safe
/// to surface through the diagnostics endpoint and through logs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum XlibInitOutcome {
    /// `XInitThreads` was invoked successfully (it returned a
    /// non-zero `Status`). The process can share Xlib across
    /// threads and the shell MUST build the real Xlib hotkey
    /// manager.
    Initialized,
    /// `libX11.so` could not be loaded (host without X11, sandbox,
    /// missing system package, …). The shell should treat the
    /// global-hotkey backend as unavailable.
    LibraryUnavailable { reason: String },
    /// Xlib was loaded but `XInitThreads` returned `0`, meaning
    /// the call was rejected. The shell MUST NOT retry from the
    /// hotkey callback; it should fall back to the no-op hotkey
    /// manager. A retry would never turn a rejected call into a
    /// successful one.
    InitFailed { reason: String },
}

impl XlibInitOutcome {
    /// Stable, snake_case identifier the diagnostics surface and
    /// the structured logs can rely on. The value never contains
    /// content from the user's environment so it is safe to print.
    pub fn kind(&self) -> &'static str {
        match self {
            XlibInitOutcome::Initialized => "initialized",
            XlibInitOutcome::LibraryUnavailable { .. } => "library_unavailable",
            XlibInitOutcome::InitFailed { .. } => "init_failed",
        }
    }

    /// `true` only when the preflight reached `XInitThreads` and
    /// the call returned a non-zero `Status`. The shell uses this to
    /// decide whether the Xlib hotkey manager is safe to build.
    pub fn is_initialized(&self) -> bool {
        matches!(self, XlibInitOutcome::Initialized)
    }

    /// Safe, metadata-only reason. Never includes Xlib pointers,
    /// display handles or user content. The returned string is
    /// guaranteed to be a short, sanitised label rather than the
    /// raw `dlerror`/`OpenError` text from the loader.
    pub fn reason(&self) -> Option<&str> {
        match self {
            XlibInitOutcome::Initialized => None,
            XlibInitOutcome::LibraryUnavailable { reason }
            | XlibInitOutcome::InitFailed { reason } => Some(reason.as_str()),
        }
    }
}

/// Typed error the loader abstraction reports when it cannot open
/// `libX11.so`. The crate never exposes the raw `dlerror` payload
/// through the public API; the helper sanitises the reason into a
/// short, metadata-only label so logs and diagnostics stay safe.
#[derive(Debug, Clone, Error)]
pub enum XlibLoadError {
    /// `libX11.so` could not be located or opened.
    #[error("libX11.so could not be opened")]
    LibraryNotFound,
}

/// Handle to the Xlib symbols the preflight actually uses. The
/// struct intentionally holds only the function pointer the helper
/// needs; any future caller that needs more symbols should grow the
/// surface here rather than reaching into the global `Xlib` struct.
#[derive(Clone, Copy)]
pub struct XlibHandle {
    /// Resolved `XInitThreads` symbol. Calling it is `unsafe`
    /// because the symbol lives in a foreign library; the safety
    /// contract is documented on the field.
    pub init_threads: unsafe extern "C" fn() -> i32,
}

/// Loader abstraction the preflight consults. The default
/// implementation wraps [`x11_dl::Xlib::open`]; tests substitute a
/// fake so they can drive the preflight without loading libX11 or
/// owning a display.
///
/// The trait is `Send + Sync` because the loader is installed in a
/// process-wide cell that the preflight may consult from any
/// thread that runs the shell bootstrap.
pub trait XlibLoader: Send + Sync {
    /// Open Xlib and resolve the symbols the preflight needs.
    fn open(&self) -> Result<XlibHandle, XlibLoadError>;
}

struct DefaultXlibLoader;

impl XlibLoader for DefaultXlibLoader {
    fn open(&self) -> Result<XlibHandle, XlibLoadError> {
        // `x11_dl::Xlib::open` calls `dlopen` lazily and caches the
        // resolved symbols in a `OnceCell` inside the crate. The
        // crate's own `unsafe impl Send/Sync` for `Xlib` is not
        // relied on here because we copy out the single function
        // pointer the preflight needs and let the handle live in
        // our own process-wide cell.
        let xlib = x11_dl::xlib::Xlib::open().map_err(|_| XlibLoadError::LibraryNotFound)?;
        let init_threads = xlib.XInitThreads;
        Ok(XlibHandle { init_threads })
    }
}

/// Process-wide cache the preflight uses to enforce the
/// "XInitThreads is called at most once" contract. The mutex is
/// held for the **entire** preflight (cache check, native call,
/// store) so two threads cannot race past the cache check and both
/// invoke `XInitThreads`. Holding the mutex during the native call
/// is safe because the call is fast (a single Status return) and
/// only races against itself.
static INIT_RESULT: Mutex<Option<XlibInitOutcome>> = Mutex::new(None);

/// Test-only cell the helpers consult to inject a scripted loader.
/// Compiled out of release builds so production callers always go
/// through [`DefaultXlibLoader`].
#[cfg(test)]
static TEST_LOADER: Mutex<Option<Arc<dyn XlibLoader>>> = Mutex::new(None);

fn resolve_loader() -> Arc<dyn XlibLoader> {
    #[cfg(test)]
    {
        if let Some(loader) = TEST_LOADER.lock().as_ref() {
            return loader.clone();
        }
    }
    Arc::new(DefaultXlibLoader)
}

fn run_preflight(loader: &dyn XlibLoader) -> XlibInitOutcome {
    match loader.open() {
        Ok(handle) => {
            // SAFETY: `XInitThreads` is documented to be safe to
            // call from any thread; the only requirement is that
            // no other Xlib call has raced ahead of it (the
            // process-wide critical section below prevents
            // concurrent preflights from racing each other). The
            // function pointer came from `x11_dl`, which resolves
            // it from the loaded `libX11.so` so the symbol
            // matches the X11 ABI.
            let code = unsafe { (handle.init_threads)() };
            // Real Xlib ABI: `XInitThreads` returns a `Status` —
            // non-zero on success, zero on failure. See
            // `/usr/include/X11/Xlib.h:1734` and the `x11-dl`
            // prototype (`pub fn XInitThreads() -> c_int`).
            if code != 0 {
                XlibInitOutcome::Initialized
            } else {
                XlibInitOutcome::InitFailed {
                    reason: "XInitThreads returned zero".into(),
                }
            }
        }
        Err(error) => {
            // We deliberately drop the raw `dlerror` payload and
            // replace it with the short, metadata-only label the
            // enum exposes. Logging more would risk leaking
            // filesystem paths from the loader.
            let reason = match error {
                XlibLoadError::LibraryNotFound => "libx11_unavailable",
            };
            XlibInitOutcome::LibraryUnavailable {
                reason: reason.into(),
            }
        }
    }
}

/// Run the preflight exactly once per process and return the
/// cached outcome on subsequent calls.
///
/// The function is safe to call from any thread. The process-wide
/// mutex that backs [`INIT_RESULT`] is held for the entire
/// preflight — cache check, loader resolution, native call, and
/// store — so two concurrent bootstrap paths cannot both invoke
/// `XInitThreads`. The shell invokes it **before**
/// `tauri::Builder::default()`, **before**
/// [`crate::bootstrap::build_state`] and **before** the
/// `GlobalHotkeyManagerAdapter` constructor so the rule
/// `XInitThreads` enables (process-wide thread coordination) is
/// already in place when any other component opens Xlib.
pub fn xlib_init_once() -> XlibInitOutcome {
    // Single critical section: hold the result mutex for the
    // entire preflight so two threads cannot both pass the cache
    // check and both invoke `XInitThreads`. The native call is a
    // single Status return so the lock is held for microseconds.
    let mut guard = INIT_RESULT.lock();
    if let Some(outcome) = guard.as_ref() {
        return outcome.clone();
    }
    let loader = resolve_loader();
    let outcome = run_preflight(&*loader);
    *guard = Some(outcome.clone());
    drop(guard);

    // Surface a metadata-only log line at the level the operator
    // configures; the reason text is one of three short labels
    // declared on the outcome and never carries clipboard
    // content, secrets, hashes or paths.
    if !outcome.is_initialized() {
        warn!(
            backend = "xlib",
            kind = outcome.kind(),
            reason = outcome.reason().unwrap_or(""),
            "Xlib thread-safety preflight did not initialise; falling back to the no-op hotkey manager"
        );
    }

    outcome
}

/// Install a custom loader. The loader is installed in the
/// process-wide cell that [`xlib_init_once`] reads; the cache that
/// stores the outcome is cleared so the next call observes the new
/// loader.
///
/// Calling this from production code is allowed but discouraged:
/// the preflight is meant to be transparent, and replacing the
/// loader from a non-test path would defeat the diagnostic
/// value of the cached outcome.
#[doc(hidden)]
#[cfg(test)]
pub fn install_loader_for_test(loader: Arc<dyn XlibLoader>) {
    *TEST_LOADER.lock() = Some(loader);
    // Clear the cached outcome so the next `xlib_init_once` call
    // routes through the freshly installed loader instead of
    // returning the previous result.
    *INIT_RESULT.lock() = None;
}

/// Reset the cached outcome and the installed loader. Test-only
/// helper so individual unit tests can start from a clean slate.
#[doc(hidden)]
#[cfg(test)]
pub fn reset_for_test() {
    *TEST_LOADER.lock() = None;
    *INIT_RESULT.lock() = None;
}

#[cfg(test)]
mod tests {
    use super::*;
    use parking_lot::MutexGuard;
    use std::sync::atomic::{AtomicI32, AtomicUsize, Ordering};
    use std::thread;

    /// Serialise the preflight tests so the global fixture slot
    /// never carries the configuration of a parallel test. The
    /// Rust test harness runs each `#[test]` in its own thread by
    /// default; without the lock, two tests could swap the active
    /// fixture between `xlib_init_once` and the shim and observe
    /// a foreign pointer. The lock is held for the lifetime of
    /// each test body so the fixture is stable until the guard
    /// drops.
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    /// Process-wide configuration the fake `XInitThreads` reads
    /// when the preflight calls it. The atomic counters let the
    /// test assertions verify the preflight really invoked the
    /// symbol (vs. short-circuiting on the cached outcome).
    struct FakeInitFixture {
        return_value: AtomicI32,
        call_count: AtomicUsize,
    }

    impl FakeInitFixture {
        fn new(code: i32) -> Self {
            Self {
                return_value: AtomicI32::new(code),
                call_count: AtomicUsize::new(0),
            }
        }
    }

    /// Active fixture pointer the fake `XInitThreads` reads.
    /// The pointer is `null` until the first test sets it; the
    /// tests serialise through [`TEST_LOCK`] and each installs
    /// its own fixture before invoking the preflight, so the
    /// pointer is always safe to dereference from inside the
    /// shim.
    static ACTIVE_FIXTURE: std::sync::atomic::AtomicPtr<FakeInitFixture> =
        std::sync::atomic::AtomicPtr::new(std::ptr::null_mut());

    fn set_active_fixture(fixture: *mut FakeInitFixture) {
        ACTIVE_FIXTURE.store(fixture, Ordering::SeqCst);
    }

    /// Build an `unsafe extern "C" fn() -> i32` that bumps the
    /// counter on the active fixture and returns its configured
    /// value. The pointer is the same across every test; the
    /// fixture the pointer reads is what changes between tests.
    fn fake_init_threads() -> unsafe extern "C" fn() -> i32 {
        unsafe extern "C" fn shim() -> i32 {
            let fixture_ptr = ACTIVE_FIXTURE.load(Ordering::SeqCst);
            assert!(
                !fixture_ptr.is_null(),
                "fake XInitThreads invoked without an active fixture",
            );
            // SAFETY: the fixture pointer is installed by
            // `FixtureGuard::new` and removed before the next
            // test starts. The atomic pointer is updated with
            // `Ordering::SeqCst` and the underlying `Box` is
            // pinned for the lifetime of the active guard.
            let fixture = unsafe { &*fixture_ptr };
            fixture.call_count.fetch_add(1, Ordering::SeqCst);
            fixture.return_value.load(Ordering::SeqCst)
        }
        shim
    }

    /// Loader fake that records the number of `open` calls and
    /// returns a configured outcome (either an `Ok(handle)` or
    /// an `Err(...)`).
    struct ScriptedLoader {
        open_calls: Arc<AtomicUsize>,
        outcome: Result<unsafe extern "C" fn() -> i32, XlibLoadError>,
    }

    impl XlibLoader for ScriptedLoader {
        fn open(&self) -> Result<XlibHandle, XlibLoadError> {
            self.open_calls.fetch_add(1, Ordering::SeqCst);
            match &self.outcome {
                Ok(init_threads) => Ok(XlibHandle {
                    init_threads: *init_threads,
                }),
                Err(error) => Err(error.clone()),
            }
        }
    }

    /// RAII guard that takes the test lock, installs the supplied
    /// fixture as the active fake, and clears the active fixture
    /// on drop. Tests use this so the process-wide pointer and
    /// lock cannot leak between tests.
    struct FixtureGuard {
        _lock: MutexGuard<'static, ()>,
        fixture: Box<FakeInitFixture>,
    }

    impl FixtureGuard {
        fn new(code: i32) -> Self {
            let lock = TEST_LOCK.lock();
            let fixture = Box::new(FakeInitFixture::new(code));
            set_active_fixture(Box::as_ref(&fixture) as *const _ as *mut _);
            Self {
                _lock: lock,
                fixture,
            }
        }

        fn call_count(&self) -> usize {
            self.fixture.call_count.load(Ordering::SeqCst)
        }
    }

    impl Drop for FixtureGuard {
        fn drop(&mut self) {
            set_active_fixture(std::ptr::null_mut());
        }
    }

    fn outcome_with_loader(loader: Arc<dyn XlibLoader>) -> XlibInitOutcome {
        reset_for_test();
        install_loader_for_test(loader);
        xlib_init_once()
    }

    #[test]
    fn initialized_when_xinit_threads_returns_nonzero() {
        // ABI contract: `XInitThreads` returns a non-zero `Status`
        // on success (see `/usr/include/X11/Xlib.h:1734`). The
        // preflight MUST report `Initialized` in this case so the
        // shell can wire the real Xlib hotkey manager.
        let fixture = FixtureGuard::new(1);
        let open_calls = Arc::new(AtomicUsize::new(0));
        let loader: Arc<dyn XlibLoader> = Arc::new(ScriptedLoader {
            open_calls: Arc::clone(&open_calls),
            outcome: Ok(fake_init_threads()),
        });
        let outcome = outcome_with_loader(loader);
        assert_eq!(outcome, XlibInitOutcome::Initialized);
        assert!(outcome.is_initialized());
        assert_eq!(outcome.kind(), "initialized");
        assert_eq!(outcome.reason(), None);
        // The `open` call ran exactly once: `xlib_init_once`
        // returned the cached value on subsequent invocations.
        assert_eq!(open_calls.load(Ordering::SeqCst), 1);
        assert_eq!(fixture.call_count(), 1);
        let second = xlib_init_once();
        assert_eq!(second, XlibInitOutcome::Initialized);
        assert_eq!(
            open_calls.load(Ordering::SeqCst),
            1,
            "second call must not re-invoke the loader"
        );
        assert_eq!(
            fixture.call_count(),
            1,
            "second call must not re-invoke XInitThreads"
        );
    }

    #[test]
    fn init_failed_when_xinit_threads_returns_zero() {
        // ABI contract: a zero `Status` from `XInitThreads`
        // means the call was rejected. The preflight MUST
        // surface this as `InitFailed` so the shell selects
        // the no-op hotkey manager; it MUST NOT retry the call
        // from the hotkey callback.
        let fixture = FixtureGuard::new(0);
        let open_calls = Arc::new(AtomicUsize::new(0));
        let loader: Arc<dyn XlibLoader> = Arc::new(ScriptedLoader {
            open_calls: Arc::clone(&open_calls),
            outcome: Ok(fake_init_threads()),
        });
        let outcome = outcome_with_loader(loader);
        match &outcome {
            XlibInitOutcome::InitFailed { reason } => {
                assert_eq!(reason, "XInitThreads returned zero");
            }
            other => panic!("expected InitFailed, got {other:?}"),
        }
        assert_eq!(outcome.kind(), "init_failed");
        assert!(!outcome.is_initialized());
        assert_eq!(fixture.call_count(), 1);
    }

    #[test]
    fn library_unavailable_when_loader_returns_error() {
        // Xlib could not be loaded: the preflight must surface
        // `LibraryUnavailable` with a sanitised reason (no raw
        // `dlerror` payload, no host paths, no secrets).
        let _fixture = FixtureGuard::new(0);
        let open_calls = Arc::new(AtomicUsize::new(0));
        let loader: Arc<dyn XlibLoader> = Arc::new(ScriptedLoader {
            open_calls: Arc::clone(&open_calls),
            outcome: Err(XlibLoadError::LibraryNotFound),
        });
        let outcome = outcome_with_loader(loader);
        match &outcome {
            XlibInitOutcome::LibraryUnavailable { reason } => {
                assert_eq!(reason, "libx11_unavailable");
                assert!(reason.len() < 64, "reason must stay short, got {reason:?}");
            }
            other => panic!("expected LibraryUnavailable, got {other:?}"),
        }
        assert_eq!(outcome.kind(), "library_unavailable");
        assert!(!outcome.is_initialized());
        assert_eq!(open_calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn idempotent_across_repeated_calls() {
        // The preflight contract: `XInitThreads` is called at
        // most once per process regardless of how many times
        // `xlib_init_once` runs. The test asserts both the open
        // count and the init count stay at one.
        let fixture = FixtureGuard::new(1);
        let open_calls = Arc::new(AtomicUsize::new(0));
        let loader: Arc<dyn XlibLoader> = Arc::new(ScriptedLoader {
            open_calls: Arc::clone(&open_calls),
            outcome: Ok(fake_init_threads()),
        });
        reset_for_test();
        install_loader_for_test(loader);
        let first = xlib_init_once();
        let second = xlib_init_once();
        let third = xlib_init_once();
        assert_eq!(first, XlibInitOutcome::Initialized);
        assert_eq!(second, XlibInitOutcome::Initialized);
        assert_eq!(third, XlibInitOutcome::Initialized);
        assert_eq!(open_calls.load(Ordering::SeqCst), 1);
        assert_eq!(fixture.call_count(), 1);
    }

    #[test]
    fn concurrent_calls_invoke_xinit_threads_at_most_once() {
        // Stress the single critical section: sixteen threads
        // race into `xlib_init_once` and every thread must
        // observe the same `XInitThreads` invocation. If the
        // mutex were released between the cache check and the
        // native call (the previous regression), this test
        // would observe multiple `XInitThreads` invocations and
        // the assertion below would fire.
        let fixture = FixtureGuard::new(1);
        let open_calls = Arc::new(AtomicUsize::new(0));
        let loader: Arc<dyn XlibLoader> = Arc::new(ScriptedLoader {
            open_calls: Arc::clone(&open_calls),
            outcome: Ok(fake_init_threads()),
        });
        reset_for_test();
        install_loader_for_test(loader);

        let mut handles = Vec::new();
        for _ in 0..16 {
            let handle = thread::spawn(xlib_init_once);
            handles.push(handle);
        }

        let outcomes: Vec<XlibInitOutcome> = handles
            .into_iter()
            .map(|h| h.join().expect("thread join"))
            .collect();
        assert_eq!(outcomes.len(), 16);
        for outcome in &outcomes {
            assert_eq!(
                *outcome,
                XlibInitOutcome::Initialized,
                "every concurrent caller must observe Initialized"
            );
        }
        // Exactly one `XInitThreads` invocation even though 16
        // threads entered the preflight.
        assert_eq!(
            fixture.call_count(),
            1,
            "XInitThreads must run exactly once under concurrency"
        );
        assert_eq!(
            open_calls.load(Ordering::SeqCst),
            1,
            "the loader's open() must run exactly once under concurrency"
        );
    }

    #[test]
    fn reset_for_test_clears_cached_outcome() {
        // Tests must be able to drive the preflight from a clean
        // slate. A first call with a failing loader must NOT
        // poison a subsequent test that installs a successful
        // loader.
        let _fixture = FixtureGuard::new(1);
        reset_for_test();
        let failing: Arc<dyn XlibLoader> = Arc::new(ScriptedLoader {
            open_calls: Arc::new(AtomicUsize::new(0)),
            outcome: Err(XlibLoadError::LibraryNotFound),
        });
        install_loader_for_test(failing);
        let first = xlib_init_once();
        assert!(matches!(first, XlibInitOutcome::LibraryUnavailable { .. }));

        reset_for_test();
        let succeeding: Arc<dyn XlibLoader> = Arc::new(ScriptedLoader {
            open_calls: Arc::new(AtomicUsize::new(0)),
            outcome: Ok(fake_init_threads()),
        });
        install_loader_for_test(succeeding);
        let second = xlib_init_once();
        assert_eq!(second, XlibInitOutcome::Initialized);
    }

    #[test]
    fn reason_strings_are_metadata_only() {
        // The reason MUST never carry raw error payloads that
        // could leak filesystem paths, display numbers or user
        // identifiers. We assert every known reason against a
        // conservative allow-list of forbidden substrings.
        let _fixture = FixtureGuard::new(0);
        for outcome in [
            XlibInitOutcome::Initialized,
            XlibInitOutcome::LibraryUnavailable {
                reason: "libx11_unavailable".into(),
            },
            XlibInitOutcome::InitFailed {
                reason: "XInitThreads returned zero".into(),
            },
        ] {
            if let Some(reason) = outcome.reason() {
                for forbidden in ["/", "DISPLAY", "0.0", "127.0.0.1", "Error", ":0"] {
                    assert!(
                        !reason.contains(forbidden),
                        "reason {reason:?} contains forbidden substring {forbidden:?}"
                    );
                }
            }
        }
    }

    /// Integration smoke test: when the test loader is NOT
    /// installed, `xlib_init_once` falls back to the default
    /// loader which calls `x11_dl::Xlib::open` and runs
    /// `XInitThreads`. The test asserts the loader runs end-to-end
    /// without panicking and the resulting outcome carries a
    /// metadata-only reason so the diagnostics surface stays safe
    /// regardless of the host's X11 stack. The test is portable:
    /// hosts without libX11 see `LibraryUnavailable`, hosts with
    /// a working Xlib thread layer see `Initialized`, and hosts
    /// where `XInitThreads` rejects the call see `InitFailed`.
    /// Each branch is a valid configuration.
    #[test]
    fn default_loader_drives_xinit_threads_on_x11_host() {
        reset_for_test();
        let outcome = xlib_init_once();
        match &outcome {
            XlibInitOutcome::Initialized => {
                assert_eq!(outcome.kind(), "initialized");
                assert_eq!(outcome.reason(), None);
            }
            XlibInitOutcome::LibraryUnavailable { reason } => {
                assert!(
                    reason == "libx11_unavailable",
                    "sanitised reason expected, got {reason:?}"
                );
            }
            XlibInitOutcome::InitFailed { reason } => {
                assert!(
                    reason == "XInitThreads returned zero",
                    "sanitised reason expected, got {reason:?}"
                );
            }
        }
    }
}
