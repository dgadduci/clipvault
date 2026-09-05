//! Shared test support for building fully isolated [`AppContext`]
//! instances.
//!
//! The helpers in this module produce an [`AppContext`] whose asset
//! stores live inside a [`tempfile::TempDir`], so destructive
//! operations ([`HistoryManagementService::delete_entry`],
//! [`HistoryManagementService::clear_non_favorites`],
//! [`HistoryManagementService::apply_retention`],
//! [`HistoryManagementService::clear_unorganized_history`], the
//! asset collector, …) can never touch the developer's real
//! `~/.clipvault/assets/clipboard`.
//!
//! ## Why this module exists
//!
//! The previous design let `AppBootstrap::bootstrap_at(tempdir/clipvault.db)`
//! fall back to [`DefaultPlatform::detect`] when no
//! [`PlatformAdapters`] were injected. That fallback resolved
//! `data_dir` to `~/.clipvault` regardless of where the SQLite file
//! lived, which meant a test pointing the database at a tempdir and
//! exercising any destructive operation deleted every PNG the
//! collector found under the real `~/.clipvault`. The regression
//! was caught at 11:20:44 by a filesystem audit — the test binary
//! emitted four `unlink` calls against
//! `~/.clipvault/assets/clipboard/*.png` even though SQLite was
//! looking at a temporary database with zero live references.
//!
//! The fix has two halves:
//!
//! 1. [`AppBootstrap`] refuses to build the context unless
//!    [`PlatformAdapters`] are explicitly injected (see
//!    [`crate::bootstrap::BootstrapError::MissingPlatformAdapters`]).
//! 2. This module is the single helper that wires those adapters with
//!    a [`PlatformInfo`] whose `home_dir` and `data_dir` live inside
//!    a [`tempfile::TempDir`]. Every destructive operation in the
//!    rest of the suite runs through an [`IsolatedTestHarness`], so
//!    the asset collector cannot reach outside the tempdir.
//!
//! ## How to use it
//!
//! The simplest entry point is [`isolated_harness`] /
//! [`isolated_harness_with_clock`]. They return a [`TempDir`] plus an
//! [`AppContext`] ready for the destructive operations the rest of
//! the test exercises. The richer [`IsolatedTestHarness`] struct is
//! exposed for the asset-isolation regression tests so they can poke
//! at the underlying `data_dir`, the asset stores and the
//! `PlatformAdapters` directly.
//!
//! ```ignore
//! use clipvault_core::test_support;
//!
//! let (dir, context) = test_support::isolated_harness_with_clock(clock);
//! // `dir.path()` is the tempdir; the asset stores live inside it.
//! // The SQLite file is `dir.path().join("clipvault.db")`.
//! ```

use std::path::{Path, PathBuf};
use std::sync::Arc;

use clipvault_platform::{
    ActiveApplicationProbe, ApplicationMetadataProvider, Capabilities, ClipboardBackend,
    DisplayServer, HotkeyManager, NoopApplicationMetadataProvider, OsFamily, PasteController,
    PlatformInfo, SettingsNavigator, TrayController,
};

use crate::bootstrap::{AppBootstrap, AppContext};
use crate::clipboard::FakeClipboard;
use crate::clipboard_assets::ClipboardAssetStore;
use crate::clock::{Clock, SystemClock};
use crate::fakes::{
    FakeActiveApplication, FakeClipboardBackend, FakeHotkeyManager, FakePasteController,
    FakeSettingsNavigator, FakeTrayController,
};
use crate::platform_adapters::PlatformAdapters;
use crate::rich_text::RichTextAssetStore;

/// Fully isolated harness for tests that exercise destructive code
/// paths or the asset collector.
///
/// Every field is metadata-only: the harness never holds clipboard
/// content, content hashes or absolute paths beyond the tempdir it
/// owns. The struct's `Debug` impl does not log the tempdir path —
/// callers that need to assert on it can read [`Self::data_dir`] or
/// [`Self::home_dir`] directly.
pub struct IsolatedTestHarness {
    /// Tempdir that backs every namespace the harness exposes. The
    /// harness keeps the dir alive for its lifetime; dropping the
    /// harness triggers the tempdir cleanup.
    pub dir: tempfile::TempDir,
    /// Resolved `home_dir` the [`PlatformInfo`] advertises. Lives
    /// inside [`Self::dir`].
    pub home_dir: PathBuf,
    /// Resolved `data_dir` the [`PlatformInfo`] advertises. The
    /// `ClipboardAssetStore` and `RichTextAssetStore` derive their
    /// roots from this path, so the collector can only touch files
    /// under it.
    pub data_dir: PathBuf,
    /// The fully-wired [`AppContext`]. Cloneable / cheap to share.
    pub context: AppContext,
    /// The [`PlatformAdapters`] bundle the bootstrap adopted. Kept
    /// around so the audit-style regression tests can introspect the
    /// capabilities / info / individual adapters.
    pub adapters: PlatformAdapters,
    /// Asset store the bootstrap wired for the image namespace.
    /// Re-derived from [`Self::data_dir`] so callers can probe the
    /// collector or write test fixtures without going through the
    /// `AppContext` services.
    pub asset_store: ClipboardAssetStore,
    /// Asset store the bootstrap wired for the rich-text namespace.
    pub rich_asset_store: RichTextAssetStore,
}

impl IsolatedTestHarness {
    /// Build a fresh harness backed by a deterministic
    /// [`SystemClock`]. Most callers should prefer
    /// [`isolated_harness`] or [`isolated_harness_with_clock`] which
    /// return a `(TempDir, AppContext)` pair matching the existing
    /// `bootstrap_with_clock` helper shape.
    pub fn new(when: time::OffsetDateTime) -> Self {
        Self::with_clock(Arc::new(SystemClock), when)
    }

    /// Build a fresh harness with a caller-supplied clock.
    pub fn with_clock(clock: Arc<dyn Clock>, _when: time::OffsetDateTime) -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let home_dir = dir.path().to_path_buf();
        let data_dir = dir.path().join("data");
        std::fs::create_dir_all(&data_dir).expect("data dir");

        let adapters = build_isolated_adapters(&home_dir, &data_dir);
        let context = AppBootstrap::new()
            .with_clock(clock)
            .with_clipboard(Arc::new(FakeClipboard::new()))
            .with_platform_adapters(adapters.clone())
            .bootstrap_at(dir.path().join("clipvault.db"))
            .expect("isolated bootstrap");

        Self {
            asset_store: ClipboardAssetStore::new(data_dir.clone()),
            rich_asset_store: RichTextAssetStore::new(data_dir.clone()),
            dir,
            home_dir,
            data_dir,
            context,
            adapters,
        }
    }
}

/// Build an [`IsolatedTestHarness`] with the system clock.
pub fn isolated_harness() -> (tempfile::TempDir, AppContext) {
    let harness = IsolatedTestHarness::new(time::OffsetDateTime::UNIX_EPOCH);
    (harness.dir, harness.context)
}

/// Build an [`IsolatedTestHarness`] anchored at the supplied clock.
/// Mirrors the `(TempDir, AppContext)` shape every existing test
/// helper exposes so swapping a local `bootstrap_with_clock` helper
/// for this one is a one-line change.
pub fn isolated_harness_with_clock(clock: Arc<dyn Clock>) -> (tempfile::TempDir, AppContext) {
    let harness = IsolatedTestHarness::with_clock(clock, time::OffsetDateTime::UNIX_EPOCH);
    (harness.dir, harness.context)
}

/// Build an [`IsolatedTestHarness`] anchored at the supplied
/// `OffsetDateTime`. Convenience for tests that want a deterministic
/// wall clock without dragging an `Arc<dyn Clock>` around.
pub fn isolated_harness_at(when: time::OffsetDateTime) -> (tempfile::TempDir, AppContext) {
    let harness = IsolatedTestHarness::new(when);
    (harness.dir, harness.context)
}

/// Build the [`PlatformAdapters`] bundle an isolated test should
/// inject. Both `home_dir` and `data_dir` resolve inside the same
/// tempdir so the asset stores cannot reach outside it. Capabilities
/// are pinned to [`Capabilities::ALL_AVAILABLE`] — the destructive
/// tests do not exercise the capability gate, and the audit asserts
/// every code path sees the same root.
pub fn build_isolated_adapters(home_dir: &Path, data_dir: &Path) -> PlatformAdapters {
    let info = PlatformInfo {
        home_dir: home_dir.to_path_buf(),
        data_dir: data_dir.to_path_buf(),
        os_family: OsFamily::Macos,
        display_server: DisplayServer::Unknown,
    };

    let clipboard: Arc<dyn ClipboardBackend> = Arc::new(FakeClipboardBackend::new());
    let hotkey: Arc<dyn HotkeyManager> = Arc::new(FakeHotkeyManager::new());
    let active_app: Arc<dyn ActiveApplicationProbe> = Arc::new(FakeActiveApplication::new());
    let paste: Arc<dyn PasteController> = Arc::new(FakePasteController::new());
    let tray: Arc<dyn TrayController> = Arc::new(FakeTrayController::new());
    let settings_navigator: Arc<dyn SettingsNavigator> = Arc::new(FakeSettingsNavigator::new());
    let app_metadata: Arc<dyn ApplicationMetadataProvider> =
        Arc::new(NoopApplicationMetadataProvider {});

    PlatformAdapters::new(
        clipboard,
        hotkey,
        active_app,
        paste,
        tray,
        settings_navigator,
        app_metadata,
        Capabilities::ALL_AVAILABLE,
        info,
    )
}

/// Construct a fresh `FixedClock`-style clock at `when` for tests that
/// want to anchor the harness to a specific `OffsetDateTime` while
/// keeping the `Arc<dyn Clock>` shape every other helper expects.
pub fn fixed_clock(when: time::OffsetDateTime) -> Arc<dyn Clock> {
    Arc::new(FixedClock::new(when))
}

/// Tiny `Clock` impl used by [`fixed_clock`]. Mirrors the private
/// structs each test file used to declare inline; keeping it in the
/// shared module lets every test bootstrap with a single import.
#[derive(Debug, Clone)]
pub struct FixedClock {
    instant: time::OffsetDateTime,
}

impl FixedClock {
    pub fn new(instant: time::OffsetDateTime) -> Self {
        Self { instant }
    }
}

impl Clock for FixedClock {
    fn now(&self) -> time::OffsetDateTime {
        self.instant
    }
}
