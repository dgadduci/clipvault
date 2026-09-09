//! Application bootstrap: assembles the database, platform info and
//! clock behind a stable [`AppContext`] that the shell (Tauri or future
//! CLI) can consume.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::Mutex;
use thiserror::Error;
use time::OffsetDateTime;
use tracing::info;

use clipvault_db::{
    builtin_migrations, AppSettingsRepository, Database, DbError, IgnoredAppRepository, Migration,
};
use clipvault_platform::{
    ActiveApplicationProbe, CachedActiveApplication, HotkeyManager, PasteController, TrayController,
};
use clipvault_platform::{Capabilities, DefaultPlatform, PlatformError, PlatformInfo};

use crate::active_app_diagnostics::ActiveAppDiagnosticsState;
use crate::clipboard::{Clipboard, FakeClipboard};
use crate::clipboard_assets::ClipboardAssetStore;
use crate::clock::{Clock, SystemClock};
use crate::code_language_service::CodeLanguageService;
use crate::history::TextHistoryService;
use crate::ignored_apps_service::IgnoredAppsService;
use crate::management::{HistoryManagementService, DEFAULT_RETENTION, RETENTION_SETTING_KEY};
use crate::organization::OrganizationService;
use crate::paste::PasteService;
use crate::paste_suppression::PasteSuppression;
use crate::platform_adapters::PlatformAdapters;
use crate::privacy::{CoreBlacklistMatcher, PrivacyGate};
use crate::rich_text::RichTextAssetStore;
use crate::search::SearchService;
use crate::settings_service::SettingsService;

#[derive(Debug, Error)]
pub enum BootstrapError {
    #[error("database error: {0}")]
    Database(#[from] DbError),

    #[error("platform error: {0}")]
    Platform(#[from] PlatformError),

    /// The bootstrap was asked to build an [`AppContext`] without an
    /// explicit [`PlatformAdapters`] bundle. The previous design
    /// silently fell back to [`DefaultPlatform::detect`], which
    /// resolved `data_dir` to `~/.clipvault` regardless of where the
    /// SQLite file lived. A test pointing the database at a tempdir
    /// and exercising `delete_entry`, `clear_non_favorites` or
    /// `apply_retention` could therefore ask the asset collector to
    /// sweep the developer's real PNG assets (audit captured the
    /// regression at 11:20:44 with four `unlink` calls against
    /// `~/.clipvault/assets/clipboard/*.png`).
    ///
    /// The fix refuses the build and forces every caller — shell or
    /// test — to either inject a [`PlatformAdapters`] bundle with a
    /// `PlatformInfo` whose `data_dir` is the same namespace as the
    /// database, or opt into the host-detected platform explicitly
    /// through
    /// [`AppBootstrap::with_default_platform_adapters`]. The two
    /// paths mirror the two safe directions:
    ///
    /// - **Production shells** use the host's `~/.clipvault`. Call
    ///   `with_default_platform_adapters` so the resolved
    ///   `PlatformInfo` and the SQLite path agree.
    /// - **Tests** use a `tempfile::TempDir`. Call
    ///   `with_platform_adapters` with a synthetic `PlatformInfo`
    ///   that points `home_dir` and `data_dir` at the tempdir (see
    ///   `crate::test_support::IsolatedTestHarness` for the shared
    ///   helper).
    ///
    /// Either path guarantees that `ClipboardAssetStore::root()` and
    /// `RichTextAssetStore::root()` resolve inside the same
    /// namespace the SQLite file lives in, so the asset collector
    /// cannot reach a different filesystem root.
    #[error(
        "AppBootstrap requires explicit PlatformAdapters; refusing to auto-detect the host data \
         directory. Call `with_platform_adapters` (tests) or `with_default_platform_adapters` \
         (production shells) before `bootstrap_at`, `bootstrap_default` or \
         `bootstrap_with_database`."
    )]
    MissingPlatformAdapters,
}

/// Knobs the shell can pass in. Most callers only need the defaults.
pub struct BootstrapOptions {
    pub clock: Arc<dyn Clock>,
    pub clipboard: Arc<dyn Clipboard>,
    /// Aggregated platform adapters. When `None`, the bootstrap builds
    /// a stub [`PlatformAdapters`] populated with no-op adapters and
    /// a fake clipboard. Production shells construct one with real
    /// adapters and pass it through.
    pub platform_adapters: Option<PlatformAdapters>,
}

impl Default for BootstrapOptions {
    fn default() -> Self {
        Self {
            clock: Arc::new(SystemClock),
            clipboard: Arc::new(FakeClipboard::new()),
            platform_adapters: None,
        }
    }
}

/// Shared, owned context that the desktop shell hands to Tauri
/// commands. Cloning is cheap: every field is either `Arc` or already
/// shareable across threads.
#[derive(Clone)]
pub struct AppContext {
    database: Arc<Mutex<Database>>,
    platform: PlatformInfo,
    capabilities: Arc<Mutex<Capabilities>>,
    clock: Arc<dyn Clock>,
    clipboard: Arc<dyn Clipboard>,
    platform_adapters: PlatformAdapters,
    history: TextHistoryService,
    management: HistoryManagementService,
    organization: OrganizationService,
    paste: PasteService,
    search: SearchService,
    settings: SettingsService,
    ignored_apps: IgnoredAppsService,
    started_at: OffsetDateTime,
    version: &'static str,
    /// Cached probe that the background watcher consults. The shell
    /// is responsible for keeping it fresh on the platform thread
    /// (see [`Self::refresh_active_application`]).
    cached_active_app: CachedActiveApplication,
    /// Shared state that tracks the most recent refresh outcome so the
    /// shell can surface stale / failed / pending refreshes through a
    /// metadata-only Tauri command.
    active_app_diagnostics: ActiveAppDiagnosticsState,
    /// Shared suppression registry consulted by the capture watcher
    /// before persisting any payload. The paste service arms a
    /// metadata-only fingerprint on the registry before writing the
    /// clipboard; the watcher consumes the token when the next
    /// observation matches and skips persistence. The state is held
    /// here so every entry point (background loop, manual tick
    /// command, paste command) sees the same registry without
    /// threading a separate handle through the call sites.
    paste_suppression: PasteSuppression,
}

impl AppContext {
    pub fn database(&self) -> &Arc<Mutex<Database>> {
        &self.database
    }

    pub fn platform(&self) -> &PlatformInfo {
        &self.platform
    }

    pub fn capabilities(&self) -> Capabilities {
        *self.capabilities.lock()
    }

    /// Replace the cached [`Capabilities`] matrix. The shell calls
    /// this after refreshing capabilities from the platform layer so
    /// subsequent `capabilities()` reads see the new state.
    pub fn set_capabilities(&self, capabilities: Capabilities) {
        *self.capabilities.lock() = capabilities;
    }

    /// Read the latest [`Capabilities`] matrix without locking. Kept
    /// for hot paths; `capabilities()` is the canonical accessor.
    pub fn capabilities_snapshot(&self) -> Capabilities {
        *self.capabilities.lock()
    }

    pub fn clock(&self) -> Arc<dyn Clock> {
        Arc::clone(&self.clock)
    }

    pub fn clipboard(&self) -> Arc<dyn Clipboard> {
        Arc::clone(&self.clipboard)
    }

    pub fn platform_adapters(&self) -> &PlatformAdapters {
        &self.platform_adapters
    }

    pub fn history(&self) -> &TextHistoryService {
        &self.history
    }

    /// Handle the persistence layer uses for the canonical
    /// `code_language` metadata. The service is stateless so the
    /// accessor returns a fresh, zero-cost handle on every call.
    pub fn code_language(&self) -> CodeLanguageService {
        CodeLanguageService::new()
    }

    pub fn management(&self) -> &HistoryManagementService {
        &self.management
    }

    pub fn organization(&self) -> &OrganizationService {
        &self.organization
    }

    pub fn paste(&self) -> &PasteService {
        &self.paste
    }

    pub fn search(&self) -> &SearchService {
        &self.search
    }

    pub fn settings(&self) -> &SettingsService {
        &self.settings
    }

    pub fn ignored_apps(&self) -> &IgnoredAppsService {
        &self.ignored_apps
    }

    pub fn started_at(&self) -> OffsetDateTime {
        self.started_at
    }

    pub fn version(&self) -> &'static str {
        self.version
    }

    /// Refresh the cached active-application probe by calling the
    /// inner platform adapter. Must be invoked on the platform thread
    /// (e.g. the main thread on macOS) so the underlying
    /// `NSWorkspace` API stays on the thread Apple requires. Returns
    /// the freshly cached value so the shell can log transitions
    /// without inspecting the inner probe.
    pub fn refresh_active_application(
        &self,
    ) -> Result<Option<clipvault_platform::ActiveApplication>, clipvault_platform::ActiveAppError>
    {
        let fresh = self.cached_active_app.refresh();
        match &fresh {
            Ok(value) => {
                self.active_app_diagnostics.record_refresh(value.clone());
            }
            Err(error) => {
                self.active_app_diagnostics.record_failure(error);
            }
        }
        // Snapshot the granular probe stage the wrapped inner
        // probe reached on the most recent call. The X11 / XWayland
        // probe stores `active_window_missing`, `wm_class_missing`,
        // `identified`, … behind the cached wrapper; macOS / no-op
        // probes report `not_applicable` and the helper collapses
        // that into a `None` JSON slot. Recording the stage after
        // `record_refresh` / `record_failure` keeps the field
        // aligned with whatever the most recent platform call
        // produced — the regression the `linux-source-app-metadata`
        // follow-up patch ships.
        let stage = self.cached_active_app.last_probe_stage();
        self.active_app_diagnostics.record_probe_stage(stage);
        fresh
    }

    /// Snapshot of the cached active-application probe. Safe to call
    /// from any thread; returns `None` until the first refresh has
    /// populated the cache.
    pub fn cached_active_application(&self) -> Option<clipvault_platform::ActiveApplication> {
        self.cached_active_app.cached()
    }

    /// Metadata-only diagnostic snapshot of the cached probe. Never
    /// returns clipboard content, hashes or source identifiers from
    /// past captures — only the platform adapter's current answer and
    /// the outcome of the most recent refresh.
    pub fn active_app_diagnostics(&self) -> crate::ActiveAppDiagnostics {
        self.active_app_diagnostics.snapshot()
    }

    /// Direct handle to the cached probe. Used by the background
    /// watcher to read the latest identifier on every tick.
    pub fn cached_active_app_probe(&self) -> CachedActiveApplication {
        self.cached_active_app.clone()
    }

    /// Shared diagnostics state. Used by the shell to record the
    /// loop-start sentinel and the metadata-only capture decision
    /// without going through `refresh_active_application`. The
    /// returned handle is cheap to clone and lives as long as the
    /// context.
    pub fn active_app_diagnostics_state(&self) -> ActiveAppDiagnosticsState {
        self.active_app_diagnostics.clone()
    }

    /// Shared suppression registry consulted by the capture watcher.
    /// Both the paste service (which arms a token before writing)
    /// and the watcher (which consumes the token on a matching
    /// observation) keep a handle on the same instance through the
    /// `AppContext`.
    pub fn paste_suppression(&self) -> &PasteSuppression {
        &self.paste_suppression
    }

    /// Record a synchronous-refresh failure observed by the shell
    /// (for example `MainThreadSyncError::Schedule` or `Timeout`).
    /// The error is sanitised to an `ActiveAppError::Backend` with a
    /// caller-supplied, action-oriented message; the cache and the
    /// counters update so the diagnostics surface reflects the
    /// failure without leaking the original platform-layer detail.
    /// Returns the new snapshot so the shell can decide whether to
    /// emit it.
    pub fn record_active_app_sync_failure(&self, message: &str) -> crate::ActiveAppDiagnostics {
        let error = clipvault_platform::ActiveAppError::Backend {
            details: message.to_string(),
        };
        self.active_app_diagnostics.record_failure(&error)
    }

    /// Record a synchronous-refresh failure that originated before the
    /// platform probe ran (for example Tauri refusing to enqueue the
    /// closure, or the main thread not picking it up inside the
    /// timeout). Lets the shell surface the exact [`ActiveAppFailureKind`]
    /// — `Schedule` vs `Timeout` — so the diagnostics card no longer
    /// collapses every sync error into a single `Backend` bucket.
    pub fn record_active_app_sync_failure_with_kind(
        &self,
        kind: crate::active_app_diagnostics::ActiveAppFailureKind,
        message: &str,
    ) -> crate::ActiveAppDiagnostics {
        self.active_app_diagnostics
            .record_failure_with_kind(kind, message.to_string())
    }
}

/// Builder for [`AppContext`].
pub struct AppBootstrap {
    options: BootstrapOptions,
}

impl AppBootstrap {
    pub fn new() -> Self {
        Self {
            options: BootstrapOptions::default(),
        }
    }

    pub fn with_clock(mut self, clock: Arc<dyn Clock>) -> Self {
        self.options.clock = clock;
        self
    }

    pub fn with_clipboard(mut self, clipboard: Arc<dyn Clipboard>) -> Self {
        self.options.clipboard = clipboard.clone();
        // Mirror into the platform clipboard if the caller has not
        // supplied an explicit set of adapters yet.
        if self.options.platform_adapters.is_none() {
            // The bootstrap keeps the existing core Clipboard for
            // backward compat; the platform adapter slot remains a
            // default until the shell injects the real adapter set.
            let _ = clipboard;
        }
        self
    }

    pub fn with_platform_adapters(mut self, adapters: PlatformAdapters) -> Self {
        self.options.platform_adapters = Some(adapters);
        self
    }

    /// Inject a `PlatformAdapters` bundle built from the host's
    /// detected platform — the bundle the production Tauri shell
    /// wires against `~/.clipvault` at first launch. The method
    /// exists to opt into the host detection explicitly so the
    /// bootstrap cannot silently fall back to it after the
    /// `MissingPlatformAdapters` guard was added.
    ///
    /// Tests that point the database at a tempdir MUST NOT call
    /// this helper: doing so would resolve `data_dir` to
    /// `~/.clipvault` and reintroduce the cross-namespace asset
    /// deletion the guard prevents. Tests should build a synthetic
    /// [`PlatformAdapters`] via [`Self::with_platform_adapters`]
    /// (the canonical helper lives in
    /// [`crate::test_support`]).
    pub fn with_default_platform_adapters(mut self) -> Result<Self, BootstrapError> {
        let detected_platform = DefaultPlatform::detect()?;
        let detected_capabilities = clipvault_platform::detect_capabilities(&detected_platform);
        let adapters = PlatformAdapters::stub(&detected_platform, detected_capabilities);
        self.options.platform_adapters = Some(adapters);
        Ok(self)
    }

    /// Open the database at `path`, run the built-in migrations and build
    /// the [`AppContext`].
    pub fn bootstrap_at(self, path: impl AsRef<Path>) -> Result<AppContext, BootstrapError> {
        let path = path.as_ref().to_path_buf();
        let mut database = Database::open(&path)?;
        let migrations = builtin_migrations();
        database.run_migrations(&migrations)?;
        self.finish(database, path)
    }

    /// Open the default `~/.clipvault/clipvault.db` and build the
    /// [`AppContext`]. Used by the Tauri shell on first launch.
    pub fn bootstrap_default(self) -> Result<AppContext, BootstrapError> {
        let path = default_database_path()?;
        self.bootstrap_at(path)
    }

    /// Wrap an already-opened database (useful for tests that build their
    /// own migrations).
    pub fn bootstrap_with_database(
        self,
        database: Database,
        path: PathBuf,
    ) -> Result<AppContext, BootstrapError> {
        self.finish(database, path)
    }

    fn finish(self, mut database: Database, path: PathBuf) -> Result<AppContext, BootstrapError> {
        let _ = path; // future-proofing: tracked here so callers can introspect
                      // Hard guard: refuse to build a context without explicit
                      // `PlatformAdapters`. The previous behaviour auto-detected the
                      // host platform and resolved `data_dir` to `~/.clipvault`,
                      // which let a test pointing the database at a tempdir sweep
                      // real PNG assets through the collector. See
                      // `BootstrapError::MissingPlatformAdapters` for the full
                      // context. The shell MUST call `with_default_platform_adapters`
                      // (production) or `with_platform_adapters` (tests) before
                      // reaching this branch.
        let platform_adapters = self
            .options
            .platform_adapters
            .ok_or(BootstrapError::MissingPlatformAdapters)?;
        // Cache the provider handle so the history service can enrich
        // captures with metadata without holding a separate reference.
        let app_metadata_provider = platform_adapters.app_metadata();

        let platform = platform_adapters.info().clone();
        let capabilities = platform_adapters.capabilities();
        info!(
            os = %platform.os_family,
            display = %platform.display_server,
            data_dir = %platform.data_dir.display(),
            clipboard_read = capabilities.clipboard_read,
            clipboard_write = capabilities.clipboard_write,
            global_hotkey = capabilities.global_hotkey,
            synthetic_paste = capabilities.synthetic_paste,
            active_application = capabilities.active_application,
            tray = capabilities.tray,
            "ClipVault bootstrap complete"
        );

        // The `privacy-settings` capability owns the local blacklist of
        // ignored application identifiers. The default retention policy
        // is seeded by `LocalSettingsReader` only when the key is
        // absent; we MUST NOT overwrite an existing user selection at
        // boot — the regression that motivated this comment shipped
        // a bootstrap that always wrote `DEFAULT_RETENTION` and
        // silently reverted the user's choice on every restart.
        let ignored_snapshot = {
            let conn = database.connection_mut();
            let ignored = IgnoredAppRepository::new(conn)
                .list()
                .map(|rows| rows.into_iter().map(|row| row.id).collect::<Vec<String>>())
                .unwrap_or_default();
            // Seed the default only when the key is missing so the
            // very first launch returns a deterministic policy and
            // any subsequent selection survives a restart.
            let now = self.options.clock.now();
            let already_persisted = AppSettingsRepository::new(conn)
                .get(RETENTION_SETTING_KEY)
                .ok()
                .flatten()
                .is_some();
            if !already_persisted {
                if let Err(error) = AppSettingsRepository::new(conn).set(
                    RETENTION_SETTING_KEY,
                    DEFAULT_RETENTION.as_setting_value(),
                    now,
                ) {
                    tracing::warn!(
                        error = %error,
                        "failed to seed default retention policy"
                    );
                }
            }
            ignored
        };

        // Wrap the platform probe in a thread-safe cache so the
        // background watcher can resolve the active application
        // identifier without ever invoking `NSWorkspace` off the main
        // thread on macOS. The shell is responsible for refreshing
        // the cache periodically on the platform thread (see
        // `AppContext::refresh_active_application`).
        let cached_probe = CachedActiveApplication::new(platform_adapters.active_app());
        let probe: Arc<dyn ActiveApplicationProbe> = Arc::new(cached_probe.clone());
        let normalised: Vec<String> = ignored_snapshot
            .iter()
            .map(|id| crate::privacy::normalize(id))
            .filter(|id| !id.is_empty())
            .collect();
        let matcher = CoreBlacklistMatcher::with_ignored(probe, normalised);
        let gate = PrivacyGate::new(matcher);
        let settings_service = SettingsService::new(Arc::clone(&self.options.clock), gate.clone());
        let ignored_apps_service =
            IgnoredAppsService::new(Arc::clone(&self.options.clock), gate.clone());
        let active_app_diagnostics = ActiveAppDiagnosticsState::new(
            capabilities.active_application,
            active_app_backend_kind(&platform, capabilities.active_application),
            Some(cached_probe.clone()),
        );

        let started_at = self.options.clock.now();
        // The asset stores are derived from the platform data
        // directory the adapters reported, so tests that inject a
        // synthetic `PlatformInfo` (a tempdir) get isolated
        // namespaces and never touch the developer's real
        // `~/.clipvault/assets`.
        let asset_store = ClipboardAssetStore::new(platform.data_dir.clone());
        let rich_asset_store = RichTextAssetStore::new(platform.data_dir.clone());
        let history = TextHistoryService::new(
            Arc::clone(&self.options.clipboard),
            Arc::clone(&self.options.clock),
            app_metadata_provider,
        )
        .with_privacy_gate(gate.clone())
        .with_asset_store(asset_store.clone())
        .with_rich_asset_store(rich_asset_store.clone());

        let management = HistoryManagementService::new(Arc::clone(&self.options.clock))
            .with_asset_store(asset_store)
            .with_rich_asset_store(rich_asset_store);

        let organization = OrganizationService::new(Arc::clone(&self.options.clock));

        // The suppression registry is built before the paste service
        // so the service can attach the same handle the watcher
        // consults. Cloning is cheap (the state lives behind an
        // `Arc`), and every consumer keeps a strong reference for
        // the lifetime of the context.
        let paste_suppression = PasteSuppression::new();
        let paste = PasteService::new(
            Arc::clone(&platform_adapters.clipboard()),
            Arc::clone(&platform_adapters.paste()),
        )
        .with_asset_store(ClipboardAssetStore::new(platform.data_dir.clone()))
        .with_rich_asset_store(RichTextAssetStore::new(platform.data_dir.clone()))
        .with_paste_suppression(paste_suppression.clone());

        let search = SearchService::new();

        Ok(AppContext {
            database: Arc::new(Mutex::new(database)),
            platform,
            capabilities: Arc::new(Mutex::new(capabilities)),
            clock: self.options.clock,
            clipboard: self.options.clipboard,
            platform_adapters,
            history,
            management,
            organization,
            paste,
            search,
            settings: settings_service,
            ignored_apps: ignored_apps_service,
            started_at,
            version: env!("CARGO_PKG_VERSION"),
            cached_active_app: cached_probe,
            active_app_diagnostics,
            paste_suppression,
        })
    }
}

impl Default for AppBootstrap {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper used by the Tauri shell. Mirrors
/// [`clipvault_db::default_database_path`] so the core owns the
/// resolution rule.
pub fn default_database_path() -> Result<PathBuf, BootstrapError> {
    Ok(clipvault_db::default_database_path()?)
}

/// Stable identifier for the active-app adapter the shell currently
/// uses. Mirrors the platform crate's [`ActiveAppBackendKind`] so the
/// diagnostics endpoint exposes a recognisable string instead of the
/// raw capability flag.
///
/// On Linux the value depends on the `display_server` the platform
/// layer detected: a plain X11 session reports `x11_ewmh`, a Wayland
/// session with a live `$DISPLAY` reports `xwayland_ewmh` (XWayland
/// only ever reports applications that publish a real X11 window)
/// and anything else collapses to `unavailable`. Splitting the two
/// surfaces preserves the historical `x11_ewmh` identifier while
/// giving the diagnostics card a metadata-only knob to distinguish a
/// native Wayland application from a real X11/XWayland one.
pub fn active_app_backend_kind(
    info: &clipvault_platform::PlatformInfo,
    available: bool,
) -> clipvault_platform::ActiveAppBackendKind {
    use clipvault_platform::{ActiveAppBackendKind, DisplayServer, OsFamily};
    if !available {
        return ActiveAppBackendKind::Unavailable;
    }
    match info.os_family {
        OsFamily::Macos => ActiveAppBackendKind::MacOsWorkspace,
        OsFamily::Linux => match info.display_server {
            DisplayServer::X11 => ActiveAppBackendKind::X11Ewmh,
            DisplayServer::Wayland => ActiveAppBackendKind::XWaylandEwmh,
            DisplayServer::Unknown => ActiveAppBackendKind::Unavailable,
        },
        _ => ActiveAppBackendKind::Unavailable,
    }
}

/// Re-exported list of migrations for callers that want to run them on
/// a pre-existing connection (tests mainly).
pub fn available_migrations() -> Vec<Migration> {
    builtin_migrations()
}

// Convenience accessor used by tests to silence the unused-import
// warnings while keeping the imports tight at the call site.
#[allow(dead_code)]
fn _ensure_traits_linked(
    _: &dyn ActiveApplicationProbe,
    _: &dyn HotkeyManager,
    _: &dyn PasteController,
    _: &dyn TrayController,
) {
}

#[cfg(test)]
mod tests {
    use super::*;
    use clipvault_platform::{ActiveAppBackendKind, DisplayServer, OsFamily, PlatformInfo};
    use std::path::PathBuf;

    fn info(os: OsFamily, display: DisplayServer) -> PlatformInfo {
        PlatformInfo {
            home_dir: PathBuf::from("/tmp"),
            data_dir: PathBuf::from("/tmp/.clipvault"),
            os_family: os,
            display_server: display,
        }
    }

    /// `x11_ewmh` on a plain X11 session, `xwayland_ewmh` on a
    /// Wayland session (the diagnostics surface distinguishes the
    /// two), and `unavailable` when the host is Linux without a
    /// recognised display server. Mirrors the contract the
    /// `desktop-platform-integration` spec pins for the
    /// `linux-source-app-metadata` change.
    #[test]
    fn active_app_backend_kind_distinguishes_x11_and_xwayland() {
        let info_x11 = info(OsFamily::Linux, DisplayServer::X11);
        let info_wayland = info(OsFamily::Linux, DisplayServer::Wayland);
        let info_unknown = info(OsFamily::Linux, DisplayServer::Unknown);
        let info_macos = info(OsFamily::Macos, DisplayServer::Unknown);
        assert_eq!(
            active_app_backend_kind(&info_x11, true),
            ActiveAppBackendKind::X11Ewmh
        );
        assert_eq!(
            active_app_backend_kind(&info_wayland, true),
            ActiveAppBackendKind::XWaylandEwmh
        );
        assert_eq!(
            active_app_backend_kind(&info_unknown, true),
            ActiveAppBackendKind::Unavailable
        );
        assert_eq!(
            active_app_backend_kind(&info_macos, true),
            ActiveAppBackendKind::MacOsWorkspace
        );
        // Unavailable overrides everything when the capability is
        // off.
        assert_eq!(
            active_app_backend_kind(&info_x11, false),
            ActiveAppBackendKind::Unavailable
        );
        assert_eq!(
            active_app_backend_kind(&info_wayland, false),
            ActiveAppBackendKind::Unavailable
        );
    }
}
