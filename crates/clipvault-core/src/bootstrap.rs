//! Application bootstrap: assembles the database, platform info and
//! clock behind a stable [`AppContext`] that the shell (Tauri or future
//! CLI) can consume.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
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
use crate::capture_diagnostic::CaptureDebugSinkHandle;
use crate::clipboard::{Clipboard, FakeClipboard};
use crate::clipboard_assets::ClipboardAssetStore;
use crate::clock::{Clock, SystemClock};
use crate::code_language_service::CodeLanguageService;
use crate::gnome_integration::GnomeIntegrationService;
use crate::history::TextHistoryService;
use crate::ignored_apps_service::IgnoredAppsService;
use crate::management::{HistoryManagementService, DEFAULT_RETENTION, RETENTION_SETTING_KEY};
use crate::organization::OrganizationService;
use crate::paste::PasteService;
use crate::paste_suppression::PasteSuppression;
use crate::peer_discovery::PeerDiscoveryRuntime;
use crate::peer_identity::{LocalPeerIdentity, PeerIdentityService, PeerIdentityStore};
use crate::platform_adapters::PlatformAdapters;
use crate::privacy::{CoreBlacklistMatcher, PrivacyGate};
use crate::rich_text::RichTextAssetStore;
use crate::search::SearchService;
use crate::settings_service::SettingsService;
use crate::watcher::CaptureWatcher;

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
    /// Optional capture-debug sink the bootstrap installs. When
    /// `None`, the bootstrap reads `CLIPVAULT_DEBUG_CAPTURE` and
    /// either installs a tracing sink or leaves the diagnostics
    /// inert. Tests inject a recording sink through
    /// [`crate::bootstrap::AppBootstrap::with_capture_debug_sink`].
    pub capture_debug_sink: Option<CaptureDebugSinkHandle>,
    /// Optional peer identity store the bootstrap installs. When
    /// `None`, the bootstrap builds an
    /// [`crate::peer_identity::InMemoryPeerIdentityStore::always_unavailable`]
    /// so callers that never wired a platform keychain get a typed
    /// `Unavailable` outcome instead of an in-memory fake identity
    /// that would not survive a restart. Production shells inject
    /// the platform `KeychainPeerIdentityStore` through
    /// [`AppBootstrap::with_peer_identity_store`]; tests inject the
    /// regular `InMemoryPeerIdentityStore::new()` or a seeded
    /// variant to exercise the happy path without linking the
    /// keychain backend.
    pub peer_identity_store: Option<Arc<dyn PeerIdentityStore>>,
    /// Optional local peer discovery adapter the bootstrap
    /// installs. When `None`, the bootstrap builds a
    /// [`clipvault_platform::NoopPeerDiscoveryAdapter`] so the
    /// shell can drive the runtime idempotently without a real
    /// `mdns-sd` backend: the noop reports
    /// [`crate::peer_discovery::AdapterError::MulticastUnavailable`]
    /// on `start` and the runtime surfaces a typed
    /// `runtime_stopped` reason to the UI. Production shells that
    /// wire the real adapter from the future `clipvault-network`
    /// crate inject it through
    /// [`AppBootstrap::with_peer_discovery_adapter`].
    pub peer_discovery_adapter: Option<Arc<dyn crate::peer_discovery::PeerDiscoveryAdapter>>,
    /// Optional concrete mDNS adapter the bootstrap keeps so the
    /// pairing toggle can wire it into the productive pairing
    /// advertisement. The shell passes the same handle it used
    /// for [`Self::peer_discovery_adapter`] cast to its concrete
    /// type; the adapter is what `MdnsPairingAdvertisementSink`
    /// accepts. When `None` the pairing toggle surfaces the
    /// typed `Unavailable` outcome instead of attempting to
    /// downcast through `Any` — a coercion that the trait object
    /// does not allow without a back-channel like this one.
    #[cfg(all(
        feature = "local-peer-discovery-mdns",
        feature = "local-peer-pairing-tls"
    ))]
    pub peer_discovery_concrete: Option<Arc<clipvault_platform::MdnsPeerDiscoveryAdapter>>,
    /// Optional productive pairing transport the bootstrap
    /// installs. When `None`, the bootstrap resolves
    /// [`clipvault_platform::default_peer_transport`] which links
    /// the TLS-backed adapter on `local-peer-pairing-tls` builds
    /// and the noop stub otherwise. Tests inject a custom
    /// [`clipvault_platform::PeerTransport`] through
    /// [`AppBootstrap::with_pairing_transport`] so they can
    /// verify the shell shutdown order without standing up a real
    /// TLS listener. The production shell always leaves the slot
    /// empty so the bootstrap reaches the documented default.
    #[cfg(feature = "local-peer-pairing-tls")]
    pub pairing_transport: Option<Arc<dyn crate::peer_pairing::PeerTransport>>,
}

impl Default for BootstrapOptions {
    fn default() -> Self {
        Self {
            clock: Arc::new(SystemClock),
            clipboard: Arc::new(FakeClipboard::new()),
            platform_adapters: None,
            capture_debug_sink: None,
            peer_identity_store: None,
            peer_discovery_adapter: None,
            #[cfg(all(
                feature = "local-peer-discovery-mdns",
                feature = "local-peer-pairing-tls"
            ))]
            peer_discovery_concrete: None,
            #[cfg(feature = "local-peer-pairing-tls")]
            pairing_transport: None,
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
    gnome_integration: GnomeIntegrationService,
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
    /// Optional, opt-in capture-debug instrumentation the
    /// `linux-source-app-metadata` change ships. Wired by the
    /// production bootstrap when `CLIPVAULT_DEBUG_CAPTURE=1`; tests
    /// inject a recording sink directly. The handle is cheap to
    /// clone and the watcher / history services route every event
    /// through it without further plumbing.
    capture_debug: CaptureDebugSinkHandle,
    /// One-shot flag the watcher uses to ensure the environment
    /// snapshot is emitted at most once per process. Resetting the
    /// flag is intentionally unsupported: the environment does not
    /// change during a single boot, so a second emission would only
    /// duplicate the same metadata.
    environment_snapshot_emitted: Arc<AtomicBool>,
    /// Local peer discovery runtime. The shell drives
    /// [`PeerDiscoveryRuntime::start`] / [`PeerDiscoveryRuntime::stop`]
    /// idempotently whenever the `local_peer_sharing_enabled`
    /// toggle flips, and reads [`PeerDiscoveryRuntime::snapshot`] to
    /// render the metadata-only `Equipos` view. The runtime owns
    /// the platform adapter and the presence table; the bootstrap
    /// only injects the runtime so the shell sees a single
    /// metadata-only surface.
    peer_discovery: PeerDiscoveryRuntime,
    /// Local peer mutual pairing runtime. The shell starts a
    /// session through [`PairingRuntime::start_outbound`],
    /// accepts inbound pairing envelopes through
    /// [`PairingRuntime::observe_pairing`], and reads
    /// [`PairingRuntime::snapshot`] to render the metadata-only
    /// pairing modal. The runtime owns the platform transport
    /// and the in-memory session table; the bootstrap wires the
    /// persistence adapter that delegates to
    /// [`clipvault_db::KnownPeerRepository`].
    peer_pairing: crate::peer_pairing::PairingRuntime,
    /// Metadata-only browser for the transferable text history
    /// of a trusted, active peer. The shell drives
    /// [`PeerTextHistoryService::browse`] from the
    /// `peer-text-history-browser` Tauri command and reads
    /// [`PeerTextHistoryService::record_peer_state`] to keep the
    /// in-memory trust / active cache aligned with the
    /// discovery + pairing runtimes. The service is metadata-only
    /// by construction: it never mutates SQLite in response to a
    /// browsing call and never emits a `history-updated` event.
    peer_text_history: crate::peer_text_history::PeerTextHistoryService,
    /// Explicit-import façade the shell drives when the user
    /// activates `Importar` for a row of a trusted active peer.
    /// The service is metadata-only by construction: it never
    /// mutates SQLite outside the documented
    /// `peer-text-import` change, never writes to the clipboard,
    /// never invokes PrivacyGate / paste, and never carries the
    /// imported body outside the authenticated fetch + commit
    /// window.
    peer_text_import: crate::peer_text_import::PeerImportService,
    /// Metadata-only browser for the transferable image history
    /// of a trusted, active peer. The shell drives
    /// [`PeerImageHistoryService::browse`] from the
    /// `peer-image-import` Tauri command and reads
    /// [`PeerImageHistoryService::record_peer_state`] to keep the
    /// in-memory trust / active cache aligned with the discovery
    /// + pairing runtimes. The service is metadata-only by
    /// construction: it never mutates SQLite in response to a
    /// browsing call and never emits a `history-updated` event.
    peer_image_history: crate::peer_image_history::PeerImageHistoryService,
    /// Explicit-import façade the shell drives when the user
    /// activates `Importar` for an image row of a trusted active
    /// peer. The service is metadata-only by construction: it
    /// never writes to the clipboard, never invokes PrivacyGate
    /// / paste, and never carries the imported bytes outside the
    /// authenticated fetch + commit window.
    peer_image_import: crate::peer_image_import::PeerImageImportService,
    /// Bounded PNG thumbnail façade the
    /// `peer-image-preview-thumbnails` change ships. The shell
    /// drives [`PeerImageThumbnailService::fetch_thumbnail`]
    /// from the new Tauri command when a remote image card
    /// intersects the visible remote-history viewport; the
    /// service collapses every failure mode into a typed
    /// [`crate::peer_image_thumbnail::PeerImageThumbnailOutcome`]
    /// so the renderer can keep the static placeholder without
    /// surfacing a global rail error.
    peer_image_thumbnail: crate::peer_image_thumbnail::PeerImageThumbnailService,
    /// Bounded, on-demand source-app presentation facade. Its
    /// client requests require the selected peer's trusted/active
    /// cache; the host handler is installed on the authenticated TLS
    /// listener and revalidates peer capability and row metadata.
    peer_source_app_presentation:
        crate::peer_source_app_presentation_service::PeerSourceAppPresentationService,
    /// Concrete mDNS adapter the bootstrap installed for
    /// discovery. The toggle command wires this handle into the
    /// `PairingAdvertisement` the productive pairing transport
    /// publishes through. `None` on hosts / builds without the
    /// `local-peer-discovery-mdns` feature — the toggle then
    /// surfaces `transport_unavailable` instead of a half-broken
    /// listener that registered the discovery-only placeholder
    /// port (`DISCOVERY_ONLY_PORT = 0`).
    #[cfg(all(
        feature = "local-peer-discovery-mdns",
        feature = "local-peer-pairing-tls"
    ))]
    peer_discovery_concrete: Option<Arc<clipvault_platform::MdnsPeerDiscoveryAdapter>>,
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

    pub fn gnome_integration(&self) -> &GnomeIntegrationService {
        &self.gnome_integration
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

    /// Atomically swap the wrapped probe. The GNOME Shell
    /// integration uses the helper to wire the GNOME-connected
    /// probe ahead of the X11 / Wayland chain once the user accepts
    /// the consent prompt. The capture loop observes the new
    /// probe on the very next tick.
    pub fn swap_active_app_probe(
        &self,
        new_probe: Arc<dyn clipvault_platform::ActiveApplicationProbe>,
    ) {
        self.cached_active_app.swap_probe(new_probe);
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

    /// Snapshot the runtime state the Linux visual blacklist picker
    /// needs to decide whether the current session supports the
    /// picker. The helper reads the backend from the
    /// [`clipvault_platform::CachedActiveApplication`] wrapper —
    /// *not* from the [`crate::ActiveAppDiagnosticsState`] snapshot —
    /// so the resolution reflects the probe the capture loop is
    /// using *right now*, including the value
    /// [`Self::swap_active_app_probe`] just swapped in (for example
    /// the GNOME-backed probe the integration installs once the user
    /// accepts the consent prompt). The diagnostics snapshot keeps
    /// the backend it was constructed with, which lags behind the
    /// probe swap and would otherwise keep reporting the original
    /// `x11_ewmh` / `xwayland_ewmh` / `unavailable` hint after the
    /// integration took over.
    ///
    /// The helper also reads the GNOME integration consent and live
    /// technical state in one place so the picker command and the
    /// catalog-add command always agree on the resolution. Before a
    /// listener exists, the runtime state is initialised from the
    /// durable fallback; the shell updates it from the live snapshot
    /// before either picker command. Linux hosts that have not
    /// installed the GNOME integration pass `None` for both GNOME
    /// fields and the resolver falls back to the EWMH / native
    /// Wayland branch. The helper is a pure read; calling it does not
    /// mutate the diagnostics or the integration state.
    #[cfg(target_os = "linux")]
    pub fn linux_picker_session_state(&self) -> crate::linux_picker::LinuxPickerSessionState<'_> {
        let (consent_decision, technical_state) =
            if self.platform().os_family == clipvault_platform::OsFamily::Linux {
                (
                    Some(self.gnome_integration.load_consent_from_cache()),
                    Some(self.gnome_integration.runtime_technical_state()),
                )
            } else {
                (None, None)
            };
        // The probe wrapper is the source of truth for the backend
        // the capture loop is using. `swap_active_app_probe` rewrites
        // the wrapper but leaves the diagnostics backend untouched,
        // so reading `diagnostics.backend` here would keep reporting
        // the initial `DisplayServer`-derived hint forever. The
        // wrapper exposes the current probe through `inner().name()`
        // and `CachedActiveApplication::name` is a thin forwarding
        // accessor, so we use it directly to avoid the extra lock.
        let backend = Some(self.cached_active_app.name());
        crate::linux_picker::LinuxPickerSessionState {
            backend,
            cache_populated: false,
            gnome_consent: consent_decision
                .map(crate::linux_picker::GnomeConsentDecision::from_core),
            gnome_technical_state: technical_state
                .map(crate::linux_picker::GnomeTechnicalState::from_core),
        }
    }

    /// Resolve the Linux picker backend from the runtime state. Thin
    /// wrapper around [`crate::linux_picker::resolve_linux_picker_backend`]
    /// that builds the [`LinuxPickerSessionState`] from the
    /// diagnostics and GNOME integration state.
    #[cfg(target_os = "linux")]
    pub fn linux_picker_backend(&self) -> clipvault_platform::LinuxPickerBackend {
        crate::linux_picker::resolve_linux_picker_backend(self.linux_picker_session_state())
    }

    /// Optional, opt-in capture-debug sink the `linux-source-app-metadata`
    /// instrumentation installs. The bootstrap wires this from the
    /// `CLIPVAULT_DEBUG_CAPTURE` environment variable; tests inject
    /// a recording sink through
    /// [`crate::bootstrap::AppBootstrap::with_capture_debug_sink`].
    pub fn capture_debug(&self) -> &CaptureDebugSinkHandle {
        &self.capture_debug
    }

    /// Whether the watcher has already emitted the one-shot
    /// environment snapshot through the configured sink. The flag is
    /// sticky; the watcher only emits the snapshot once per
    /// process so the diagnostic stream does not duplicate the same
    /// metadata on every iteration.
    pub fn environment_snapshot_emitted(&self) -> bool {
        self.environment_snapshot_emitted.load(Ordering::Acquire)
    }

    /// Mark the environment snapshot as emitted so subsequent ticks
    /// skip the duplicate emission.
    pub fn mark_environment_snapshot_emitted(&self) {
        self.environment_snapshot_emitted
            .store(true, Ordering::Release);
    }

    /// Handle to the local peer discovery runtime. The shell uses
    /// it to drive `start` / `stop` whenever the toggle flips and
    /// to render the metadata-only `Equipos` snapshot. The runtime
    /// outlives the shell: dropping the handle is enough to tear it
    /// down deterministically.
    pub fn peer_discovery(&self) -> PeerDiscoveryRuntime {
        self.peer_discovery.clone()
    }

    /// Handle to the local peer pairing runtime. The shell uses
    /// it to start / accept / cancel pairing sessions and to
    /// surface the metadata-only session snapshot the pairing
    /// modal renders.
    pub fn peer_pairing(&self) -> crate::peer_pairing::PairingRuntime {
        self.peer_pairing.clone()
    }

    /// Accessor the shell drives to render the metadata-only
    /// transferable text history of a trusted, active peer. The
    /// service is cheap to clone (every field is `Arc`-shared).
    pub fn peer_text_history(&self) -> &crate::peer_text_history::PeerTextHistoryService {
        &self.peer_text_history
    }

    /// Accessor the shell drives to import the canonical text
    /// of a remote entry through the explicit `Importar` action.
    /// The service is cheap to clone (every field is
    /// `Arc`-shared).
    pub fn peer_text_import(&self) -> &crate::peer_text_import::PeerImportService {
        &self.peer_text_import
    }

    /// Accessor the shell drives to render the metadata-only
    /// transferable image history of a trusted, active peer.
    pub fn peer_image_history(&self) -> &crate::peer_image_history::PeerImageHistoryService {
        &self.peer_image_history
    }

    /// Accessor the shell drives to import the canonical PNG
    /// payload of a remote image entry through the explicit
    /// `Importar` action.
    pub fn peer_image_import(&self) -> &crate::peer_image_import::PeerImageImportService {
        &self.peer_image_import
    }

    /// Bounded PNG thumbnail façade the
    /// `peer-image-preview-thumbnails` change ships. The shell
    /// drives the service from the new Tauri command when a
    /// remote image card intersects the visible remote-history
    /// viewport. The façade is metadata-only by construction:
    /// it never persists the generated PNG and never substitutes
    /// the snapshot for the original `Importar` payload.
    pub fn peer_image_thumbnail(&self) -> &crate::peer_image_thumbnail::PeerImageThumbnailService {
        &self.peer_image_thumbnail
    }

    pub fn peer_source_app_presentation(
        &self,
    ) -> &crate::peer_source_app_presentation_service::PeerSourceAppPresentationService {
        &self.peer_source_app_presentation
    }

    /// Best-effort wire of the local peer identity the runtime
    /// uses for self-filtering, plus the validated display name
    /// the runtime publishes in its own TXT record. The shell
    /// calls this after the secure store reports `Ok`; the
    /// runtime ignores the call when the secure store is
    /// unavailable so a temporarily missing keychain does not
    /// block the user from editing the toggle (the runtime
    /// reports `identity_unavailable` in that case).
    ///
    /// `display_name` is the trimmed, validated value the settings
    /// service exposes; the runtime rejects an empty string so
    /// the adapter cannot accidentally publish a record without a
    /// visible name. When the secure store is unreachable the
    /// function ignores `identity` and clears the snapshot.
    pub fn refresh_peer_discovery_local_identity(
        &self,
        identity: Option<&LocalPeerIdentity>,
        display_name: Option<&str>,
    ) {
        let snapshot = identity.and_then(|id| {
            let name = display_name.unwrap_or("").trim();
            if name.is_empty() {
                None
            } else {
                Some(crate::peer_discovery::LocalPeerIdentitySnapshot::new(
                    id.peer_id.clone(),
                    id.fingerprint.clone(),
                    name.to_string(),
                ))
            }
        });
        self.peer_discovery.set_local_identity(snapshot);
    }

    /// Install the [`crate::peer_pairing::MaterialLoader`] the
    /// production pairing transport uses to bind a real listener.
    /// The bootstrap leaves the slot empty by default; the shell
    /// wires a loader that delegates to the platform keychain so
    /// `install_pairing_transport` can mint the cert from the
    /// secure store. The pairing runtime never sees the seed —
    /// only the typed [`clipvault_platform::LocalIdentityMaterial`]
    /// the loader returns.
    #[cfg(feature = "local-peer-pairing-tls")]
    pub fn install_pairing_material_loader(
        &self,
        loader: Arc<dyn crate::peer_pairing::MaterialLoader>,
    ) {
        self.peer_pairing.set_material_loader(loader);
    }

    /// Install a default material loader backed by the
    /// platform keychain store the bootstrap was wired with.
    /// The helper is the canonical way for the production shell
    /// to register a productive material loader without
    /// importing the platform crate directly; tests that exercise
    /// the runtime can leave the slot empty.
    #[cfg(all(
        feature = "local-peer-pairing-tls",
        feature = "local-peer-identity-keychain"
    ))]
    pub fn install_default_pairing_material_loader(
        &self,
        keychain: Arc<clipvault_platform::KeychainPeerIdentityStore>,
    ) {
        let loader = KeychainPairingMaterialLoader::new(keychain);
        self.peer_pairing
            .set_material_loader(Arc::new(loader) as Arc<dyn crate::peer_pairing::MaterialLoader>);
    }

    /// Forward the typed outcome the pairing transport returns
    /// when the shell asks it to bind a real listener. The
    /// bootstrap keeps the call site thin so the shell stays a
    /// typed adapter that never inspects free-form strings.
    #[cfg(feature = "local-peer-pairing-tls")]
    pub fn install_pairing_transport(
        &self,
        sink: Arc<dyn crate::peer_pairing::TransportSink>,
        advertisement: Arc<dyn crate::peer_pairing::PairingAdvertisement>,
        display_name: &str,
    ) -> Result<u16, crate::peer_pairing::TransportOutcome> {
        self.peer_pairing
            .install_pairing_transport(sink, advertisement, display_name)
    }

    /// Forward the typed outcome with an explicit
    /// [`RemotePeerResolver`]. The shell uses this variant when
    /// the mDNS adapter is wired so the productive pairing
    /// transport can resolve a `peer_id` to a `SocketAddr`
    /// without exposing the address to the bridge.
    #[cfg(feature = "local-peer-pairing-tls")]
    pub fn install_pairing_transport_with_resolver(
        &self,
        sink: Arc<dyn crate::peer_pairing::TransportSink>,
        advertisement: Arc<dyn crate::peer_pairing::PairingAdvertisement>,
        resolver: Arc<dyn clipvault_platform::peer_transport::RemotePeerResolver>,
        display_name: &str,
    ) -> Result<u16, crate::peer_pairing::TransportOutcome> {
        self.peer_pairing.install_pairing_transport_with_resolver(
            sink,
            advertisement,
            resolver,
            display_name,
        )
    }

    /// Install the local peer identity the pairing runtime uses
    /// to compute the SAS and sign the local approval. The
    /// bootstrap caches the value through
    /// [`crate::peer_pairing::PairingRuntime::set_local_identity`]
    /// so the runtime never has to reach into the platform crate
    /// to resolve it. The shell calls this every time the secure
    /// store hands out (or rotates) the local identity.
    #[cfg(feature = "local-peer-pairing-tls")]
    pub fn install_pairing_local_identity(&self, identity: Option<&LocalPeerIdentity>) {
        self.peer_pairing.set_local_identity(identity.cloned());
    }

    /// Concrete mDNS adapter the bootstrap installed. The toggle
    /// command wires this handle into the
    /// [`crate::peer_pairing::PairingAdvertisement`] the productive
    /// pairing transport publishes through; `None` on hosts /
    /// builds without the productive feature pair, where the
    /// toggle must surface the typed `Unavailable` outcome.
    #[cfg(all(
        feature = "local-peer-discovery-mdns",
        feature = "local-peer-pairing-tls"
    ))]
    pub fn peer_discovery_mdns_adapter(
        &self,
    ) -> Option<Arc<clipvault_platform::MdnsPeerDiscoveryAdapter>> {
        self.peer_discovery_concrete.clone()
    }

    /// Stop the production pairing transport. The platform
    /// layer withdraws the mDNS advertisement and closes the
    /// listener before returning; the runtime surfaces the typed
    /// outcome so the shell can render the off state without
    /// inspecting free-form strings.
    pub fn stop_pairing_transport(&self) -> Result<(), crate::peer_pairing::TransportOutcome> {
        self.peer_pairing.stop_pairing_transport()
    }

    /// Whether the production pairing transport is currently
    /// bound to a real listener. The shell consults this before
    /// calling [`Self::install_pairing_transport`] so a redundant
    /// toggle is a no-op.
    pub fn pairing_transport_is_running(&self) -> bool {
        self.peer_pairing.pairing_transport_is_running()
    }

    /// Return the ephemeral port the pairing transport bound to,
    /// or `None` while the listener is stopped. The toggle helper
    /// uses this value to confirm the productive install actually
    /// succeeded — a `None` while the toggle is on means the
    /// listener never came up and the toggle response must
    /// collapse to `RuntimeStopped` instead of `active`.
    pub fn pairing_bound_port(&self) -> Option<u16> {
        self.peer_pairing.pairing_bound_port()
    }

    /// Wire the shared [`CaptureWatcher`] the destructive operations
    /// rebaseline after a successful row removal. The Tauri shell
    /// calls this once at startup, right after the context and the
    /// watcher are built, so the management layer can forward
    /// baseline requests to the same watcher instance the background
    /// capture loop and the manual `Tick capture` command already
    /// share. The accessor only forwards to the management service;
    /// the shell stays a thin adapter and never inspects the
    /// watcher's dedupe state directly.
    pub fn attach_capture_watcher(&self, watcher: Arc<CaptureWatcher>) {
        self.management.set_capture_watcher_invalidator(
            watcher
                as Arc<
                    dyn crate::management::CaptureWatcherInvalidator<
                        Baseline = crate::watcher::BaselineOutcome,
                    >,
                >,
        );
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

    /// Inject a capture-debug sink the bootstrap installs instead of
    /// the default `CLIPVAULT_DEBUG_CAPTURE`-driven wiring. Tests use
    /// this to attach a [`crate::capture_diagnostic::RecordingCaptureDebugSink`]
    /// without touching the global environment; production shells
    /// leave the field empty so the bootstrap reads the env var
    /// exactly once at startup.
    pub fn with_capture_debug_sink(mut self, sink: CaptureDebugSinkHandle) -> Self {
        self.options.capture_debug_sink = Some(sink);
        self
    }

    /// Inject the [`PeerIdentityStore`] the bootstrap hands to the
    /// settings service. Production shells pass the platform
    /// keychain-backed implementation; tests pass the regular
    /// `InMemoryPeerIdentityStore::new()` (or a seeded variant).
    /// When omitted the bootstrap installs the
    /// [`InMemoryPeerIdentityStore::always_unavailable`] fake so
    /// callers that never wired a platform keychain see a typed
    /// `Unavailable` outcome instead of an in-memory identity that
    /// would not survive a restart.
    pub fn with_peer_identity_store(mut self, store: Arc<dyn PeerIdentityStore>) -> Self {
        self.options.peer_identity_store = Some(store);
        self
    }

    /// Inject the [`PeerDiscoveryAdapter`] the bootstrap hands to
    /// the runtime. Production shells that wire the future
    /// `clipvault-network` adapter pass it here; tests pass a fake
    /// implementation to exercise the start / stop / drain contract
    /// without standing up mDNS. When omitted the bootstrap
    /// installs [`clipvault_platform::NoopPeerDiscoveryAdapter`]
    /// so the runtime reports a typed `runtime_stopped` reason
    /// without ever touching the network.
    pub fn with_peer_discovery_adapter(
        mut self,
        adapter: Arc<dyn crate::peer_discovery::PeerDiscoveryAdapter>,
    ) -> Self {
        self.options.peer_discovery_adapter = Some(adapter);
        self
    }

    /// Inject the concrete [`MdnsPeerDiscoveryAdapter`] the
    /// pairing toggle wires into the productive pairing
    /// advertisement. The shell MUST pass the same handle it
    /// used for [`Self::with_peer_discovery_adapter`] so the
    /// discovery and pairing halves agree on the underlying
    /// `mdns-sd` daemon. When omitted the toggle surfaces the
    /// typed `Unavailable` outcome instead of attempting to
    /// downcast through `Any`.
    #[cfg(all(
        feature = "local-peer-discovery-mdns",
        feature = "local-peer-pairing-tls"
    ))]
    pub fn with_peer_discovery_concrete(
        mut self,
        adapter: Arc<clipvault_platform::MdnsPeerDiscoveryAdapter>,
    ) -> Self {
        self.options.peer_discovery_concrete = Some(adapter);
        self
    }

    /// Inject the productive pairing transport the bootstrap
    /// installs in place of [`clipvault_platform::default_peer_transport`].
    /// Tests use this hook to wire a scriptable
    /// [`clipvault_platform::PeerTransport`] so the shell's
    /// shutdown order can be asserted without standing up a real
    /// TLS listener. The production shell MUST leave the slot
    /// empty so the bootstrap reaches the documented TLS-backed
    /// default.
    #[cfg(feature = "local-peer-pairing-tls")]
    pub fn with_pairing_transport(
        mut self,
        transport: Arc<dyn crate::peer_pairing::PeerTransport>,
    ) -> Self {
        self.options.pairing_transport = Some(transport);
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
        let peer_identity_store: Arc<dyn PeerIdentityStore> =
            self.options.peer_identity_store.clone().unwrap_or_else(|| {
                // Production default: never silently mint an
                // in-memory identity that would not survive a
                // restart. The platform keychain must be wired
                // explicitly through
                // [`AppBootstrap::with_peer_identity_store`] so the
                // shell surfaces a typed `Unavailable` outcome
                // until then.
                Arc::new(crate::peer_identity::InMemoryPeerIdentityStore::always_unavailable())
            });
        let peer_identity_service = PeerIdentityService::new(peer_identity_store);
        let settings_service = SettingsService::new(Arc::clone(&self.options.clock), gate.clone())
            .with_peer_identity_service(peer_identity_service.clone());
        let ignored_apps_service =
            IgnoredAppsService::new(Arc::clone(&self.options.clock), gate.clone());
        let gnome_integration_service =
            GnomeIntegrationService::new(Arc::clone(&self.options.clock));
        // Wrap the database once so the GNOME integration service can
        // prime its in-memory cache without touching `AppContext` and
        // every later consumer sees the same `Arc<Mutex<Database>>`.
        let database_handle = Arc::new(Mutex::new(database));
        let _ = gnome_integration_service.prime_from_database(&database_handle);
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
        // Share the same in-process asset mutation lock with the
        // capture pipeline. Peer-import rollback must not unlink an
        // asset between local image capture's store/reuse and SQLite
        // insert.
        let peer_image_import_asset_store = asset_store.clone();
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

        // Build the local peer discovery runtime. The shell drives
        // start / stop whenever the opt-in toggle flips. Production
        // hosts on macOS / Linux install the `mdns-sd`-backed
        // adapter from `clipvault-platform`; cross-compiles and
        // platforms that do not enable `local-peer-discovery-mdns`
        // fall back to the noop stub that reports a typed
        // `runtime_stopped` reason.
        let peer_discovery_adapter: Arc<dyn crate::peer_discovery::PeerDiscoveryAdapter> = self
            .options
            .peer_discovery_adapter
            .clone()
            .unwrap_or_else(|| Arc::new(default_peer_discovery_adapter()));
        let peer_discovery = PeerDiscoveryRuntime::new(peer_discovery_adapter);
        // The runtime drains events on a worker thread; install the
        // persistence closure the bootstrap owns. The closure is
        // the only place where the runtime touches
        // `known_peers`; keeping the call site in one place makes
        // the worker lifecycle easy to audit.
        let persist_closure: Arc<
            dyn Fn(&clipvault_db::PeerObservation) -> clipvault_db::UpsertObservationOutcome
                + Send
                + Sync,
        > = {
            let database_handle_for_closure = Arc::clone(&database_handle);
            Arc::new(move |observation: &clipvault_db::PeerObservation| {
                let mut db = database_handle_for_closure.lock();
                let conn = db.connection_mut();
                let mut repo = clipvault_db::KnownPeerRepository::new(conn);
                // Persistence failures (sqlite errors) collapse into a
                // `Conflict` outcome so the worker logs the rejection
                // without aborting the drain loop. The runtime surfaces
                // the conflict in the diagnostics endpoint; the user
                // never sees a raw sqlite message.
                match repo.upsert_observation(observation) {
                    Ok(outcome) => outcome,
                    Err(error) => {
                        tracing::warn!(
                            error = %error,
                            "known_peers upsert failed; treating as conflict"
                        );
                        // Return the last known row as a synthetic
                        // conflict so the worker keeps draining.
                        clipvault_db::UpsertObservationOutcome::Conflict(clipvault_db::KnownPeer {
                            peer_id: observation.peer_id.clone(),
                            public_key_fingerprint: observation.public_key_fingerprint.clone(),
                            full_public_key_fingerprint: observation
                                .full_public_key_fingerprint
                                .clone()
                                .unwrap_or_default(),
                            display_name: observation.display_name.clone(),
                            protocol_major: observation.protocol_major,
                            capability: observation.capability.clone(),
                            caps_extra: observation.caps_extra.clone(),
                            caps_extra_v2: observation.caps_extra_v2.clone(),
                            first_seen_at: String::new(),
                            last_discovered_at: String::new(),
                            updated_at: String::new(),
                            trust_state: clipvault_db::TrustState::Unverified,
                            tls_cert_fingerprint: String::new(),
                            paired_at: String::new(),
                            paired_protocol_major: 0,
                            cursor_secret: String::new(),
                        })
                    }
                }
            })
        };
        peer_discovery.set_persistence(persist_closure);

        // Build the local peer pairing runtime. The runtime owns
        // the platform transport + the in-memory session table
        // and delegates every trust-state transition to the
        // repository the bootstrap owns. The persistence
        // adapter is the only place where the runtime touches
        // `known_peers`, mirroring the discovery worker.
        //
        // The bootstrap installs the production TLS-backed
        // transport (not the noop stub) so the
        // `local-peer-pairing-tls` feature gates the only
        // productive install path. Cross-compiles and
        // unsupported targets fall back to the noop transport
        // through `default_peer_transport` so the runtime
        // surfaces a typed `Unavailable` reason instead of
        // silently spawning a half-broken listener.
        let pairing_persistence: Arc<dyn crate::peer_pairing::PairingPersistence + Send + Sync> = {
            let database_handle_for_closure = Arc::clone(&database_handle);
            Arc::new(KnownPeerPairingPersistence::new(
                database_handle_for_closure,
            ))
        };
        let pairing_transport: Arc<dyn crate::peer_pairing::PeerTransport> =
            resolve_pairing_transport(
                #[cfg(feature = "local-peer-pairing-tls")]
                self.options.pairing_transport.clone(),
                #[cfg(not(feature = "local-peer-pairing-tls"))]
                None,
            );
        // Borrow the pairing transport again (the `pairing_runtime`
        // call above already cloned it into the runtime) so the
        // `peer_text_history` service can piggy-back on the same
        // mTLS dial loop. Cloning the `Arc` keeps both surfaces
        // pointed at the same listener the bootstrap installs.
        let peer_text_history_transport: Arc<dyn crate::peer_text_history::PeerHistoryTransport> =
            peer_text_history_transport_for(
                #[cfg(feature = "local-peer-pairing-tls")]
                Arc::clone(&pairing_transport),
            );
        let peer_text_history =
            crate::peer_text_history::PeerTextHistoryService::new(peer_text_history_transport);
        let peer_pairing = crate::peer_pairing::PairingRuntime::new(
            pairing_transport.clone(),
            pairing_persistence,
        );
        #[cfg(feature = "local-peer-pairing-tls")]
        let source_app_name_capability_resolver: Arc<dyn Fn(&str) -> bool + Send + Sync> =
            Arc::new({
                let database_for_resolver = Arc::clone(&database_handle);
                move |peer_id: &str| {
                    let mut db = database_for_resolver.lock();
                    let repo = clipvault_db::KnownPeerRepository::new(db.connection_mut());
                    repo.get(peer_id).ok().flatten().is_some_and(|row| {
                        row.trust_state == clipvault_db::TrustState::Trusted
                            && crate::peer_discovery::decode_capabilities(&row.caps_extra_v2)
                                .iter()
                                .any(|token| {
                                    token
                                        == crate::peer_discovery::SOURCE_APP_PRESENTATION_CAPABILITY
                                })
                    })
                }
            });
        // Build the productive host-side history handler the
        // listener drives when an authenticated peer asks for
        // `list_recent_text`. The bootstrap installs the adapter
        // *before* the very first inbound connection so the wire
        // contract stays stable across rebuilds: a productive host
        // always serves the first page, a build without the
        // productive feature pair collapses to the documented
        // `not_available` reason without ever exposing a half-
        // configured listener. The source borrows the same shared
        // database handle the runtime already holds, so SQLite
        // never reaches across the platform boundary.
        #[cfg(feature = "local-peer-pairing-tls")]
        {
            let host_source: Arc<dyn crate::peer_text_history::HostHistorySource> = Arc::new(
                crate::peer_text_history::EntryRepositoryHostHistorySource::new(Arc::clone(
                    &database_handle,
                )),
            );
            let handler = crate::peer_pairing::PeerTextHistoryHostHandlerAdapter::new(
                peer_text_history.clone(),
                Arc::clone(&host_source),
            )
            .with_source_app_name_capability_resolver(Arc::clone(
                &source_app_name_capability_resolver,
            ));
            // The productive install path persists the handler so a
            // follow-up `start_with_material_and_resolver` already
            // has the wiring in place. The `install_history_handler`
            // API is idempotent and refuses to bind to a transport
            // that is not running, so the call collapses to a
            // logged warning instead of breaking the bootstrap.
            if let Err(error) = peer_pairing.install_history_handler_inner(Arc::new(handler)) {
                tracing::warn!(
                    ?error,
                    "productive history handler install failed; bootstrap continues with no host history"
                );
            }
        }
        // Build the productive host-side fetch handler the
        // listener drives when an authenticated peer asks for
        // `fetch_text`. The bootstrap installs the adapter
        // *before* the very first inbound connection so the wire
        // contract stays stable across rebuilds: a productive host
        // always serves the body, a build without the productive
        // feature pair collapses to the documented `not_available`
        // reason without ever exposing a half-configured
        // listener. The persistence adapter borrows the same
        // shared database handle the runtime already holds, so
        // SQLite never reaches across the platform boundary.
        #[cfg(feature = "local-peer-pairing-tls")]
        {
            let fetch_persistence: Arc<dyn crate::peer_text_import::PeerImportPersistence> =
                Arc::new(crate::peer_import_sqlite::SqliteImportPersistence::new(
                    Arc::clone(&database_handle),
                ));
            let fetch_handler: Arc<dyn clipvault_platform::peer_transport::FetchTextHostHandler> =
                Arc::new(
                    crate::peer_text_import::PeerTextImportHostHandlerAdapter::new(
                        fetch_persistence,
                    ),
                );
            if let Err(error) = peer_pairing.install_fetch_handler_inner(fetch_handler) {
                tracing::warn!(
                    ?error,
                    "productive fetch handler install failed; bootstrap continues with no host import"
                );
            }
        }
        // Build the explicit-import facade the shell drives when
        // the user activates `Importar` for a row of a trusted,
        // active peer. The service piggy-backs on the same
        // pairing transport the productive install wired so the
        // mTLS dial loop is shared with the history browse; the
        // persistence adapter borrows the same shared database
        // handle the runtime already holds. Cross-compiles and
        // unsupported targets fall back to the noop transport so
        // the runtime surfaces a typed `TransportUnavailable`
        // reason instead of silently spawning a half-broken
        // import.
        let peer_text_import_transport: Arc<dyn crate::peer_text_import::PeerFetchTransport> =
            peer_text_import_transport_for(
                #[cfg(feature = "local-peer-pairing-tls")]
                Arc::clone(&pairing_transport),
            );
        let peer_text_import_persistence: Arc<dyn crate::peer_text_import::PeerImportPersistence> =
            Arc::new(crate::peer_import_sqlite::SqliteImportPersistence::new(
                Arc::clone(&database_handle),
            ));
        let peer_text_import = crate::peer_text_import::PeerImportService::new(
            peer_text_import_transport,
            peer_text_import_persistence,
            Arc::new(crate::peer_text_import::SystemImportClock),
        );
        // ----------------------------------------------------------------
        // `peer-image-import` wiring.
        //
        // Build the metadata-only image-history browser the shell
        // drives when the user opens a trusted, active peer. The
        // transport piggy-backs on the productive pairing transport
        // the bootstrap already wired so the mTLS dial loop is
        // shared with the text history / import routes. Cross-
        // compiles and unsupported targets fall back to the noop
        // transport.
        let peer_image_history_transport: Arc<
            dyn crate::peer_image_history::PeerImageHistoryTransport,
        > = peer_image_history_transport_for(
            #[cfg(feature = "local-peer-pairing-tls")]
            Arc::clone(&pairing_transport),
        );
        // SQLite-backed resolver the image routes consult before
        // serving or accepting any image request. A peer that did
        // not advertise the `image_import` capability (an old
        // build, a legacy `pairing` only record, …) collapses to
        // `false` so the host never serves image metadata to a
        // caller that did not opt into the contract. The
        // resolver is the same closure the client-side façade
        // uses, so the two surfaces stay in lockstep regardless
        // of which endpoint reaches the host first.
        let image_capability_resolver: crate::peer_image_history::PeerCapabilityResolver = Arc::new(
            {
                let database_for_resolver = Arc::clone(&database_handle);
                move |peer_id: &str| -> bool {
                    let mut db = database_for_resolver.lock();
                    let conn = db.connection_mut();
                    let repo = clipvault_db::KnownPeerRepository::new(conn);
                    match repo.get(peer_id) {
                        Ok(Some(row)) => {
                            // The legacy `capability` column stays at
                            // the canonical token; the additive
                            // `image_import` capability travels
                            // through the dedicated `caps_extra`
                            // column. The bootstrap combines the two
                            // so a host that ships
                            // `capability=pairing` plus
                            // `caps_extra=image_import` resolves to
                            // the productive image surface.
                            crate::peer_discovery::decode_capabilities(&row.caps_extra)
                                .iter()
                                .any(|token| {
                                    token == crate::peer_discovery::IMAGE_IMPORT_CAPABILITY
                                })
                        }
                        Ok(None) => false,
                        Err(error) => {
                            tracing::warn!(
                                error = %error,
                                "image capability resolver: known_peers lookup failed; defaulting to capability absent"
                            );
                            false
                        }
                    }
                }
            },
        );
        let peer_image_import_capability_resolver:
            crate::peer_image_import::PeerImageCapabilityResolver = Arc::new({
                let database_for_resolver = Arc::clone(&database_handle);
                move |peer_id: &str| -> bool {
                    let mut db = database_for_resolver.lock();
                    let conn = db.connection_mut();
                    let repo = clipvault_db::KnownPeerRepository::new(conn);
                    match repo.get(peer_id) {
                        Ok(Some(row)) => crate::peer_discovery::decode_capabilities(
                            &row.caps_extra,
                        )
                        .iter()
                        .any(|token| token == crate::peer_discovery::IMAGE_IMPORT_CAPABILITY),
                        Ok(None) => false,
                        Err(error) => {
                            tracing::warn!(
                                error = %error,
                                "image capability resolver: known_peers lookup failed; defaulting to capability absent"
                            );
                            false
                        }
                    }
                }
            });
        let peer_image_history =
            crate::peer_image_history::PeerImageHistoryService::new(peer_image_history_transport)
                .with_capability_resolver(Arc::clone(&image_capability_resolver));
        // Pairing persists one per-peer cursor secret. Keep both
        // history services' independent runtime caches synchronized
        // with that row so trust promotion, revocation and restart
        // have identical authorization semantics for text and image
        // browsing.
        #[cfg(feature = "local-peer-pairing-tls")]
        peer_pairing.install_cursor_secret_cache(Arc::new(
            crate::peer_text_history::PeerHistoryCursorSecretCache::new(
                peer_text_history.clone(),
                peer_image_history.clone(),
            ),
        ));
        // Build the explicit-image-import facade the shell drives
        // when the user activates `Importar` for an image row of a
        // trusted, active peer. The service piggy-backs on the same
        // pairing transport the productive install wired; the
        // persistence adapter borrows the same shared database
        // handle the runtime already holds.
        let peer_image_import_transport: Arc<
            dyn crate::peer_image_import::PeerFetchImageTransport,
        > = peer_image_import_transport_for(
            #[cfg(feature = "local-peer-pairing-tls")]
            Arc::clone(&pairing_transport),
        );
        let peer_image_import_persistence: Arc<
            dyn crate::peer_image_import::PeerImageImportPersistence,
        > = Arc::new(crate::peer_image_sqlite::SqliteImageImportPersistence::new(
            Arc::clone(&database_handle),
            peer_image_import_asset_store.clone(),
        ));
        let peer_image_import = crate::peer_image_import::PeerImageImportService::new(
            peer_image_import_transport,
            peer_image_import_persistence,
            Arc::new(crate::peer_image_import::SystemImageImportClock),
        )
        .with_capability_resolver(Arc::clone(&peer_image_import_capability_resolver));
        // Install the host-side image history + image fetch
        // handlers the listener drives when a peer asks for
        // `list_recent_images` or `fetch_image`. The bootstrap
        // installs the adapters before the very first inbound
        // connection so the wire contract stays stable across
        // rebuilds: a productive host always serves the request,
        // a build without the productive feature pair collapses to
        // the documented `not_available` reason.
        #[cfg(feature = "local-peer-pairing-tls")]
        {
            // The asset validator the host source uses to drop
            // rows whose backing PNG is missing or invalid. The
            // closure runs against the `ClipboardAssetStore` the
            // local capture pipeline owns so an entry that
            // points at a missing / out-of-namespace PNG is
            // removed from the wire projection without ever
            // asking the peer to import a broken asset. The
            // validator reuses the asset store's full validation
            // pipeline (path / namespace / size / PNG signature /
            // decode / dimensions) so corrupt, oversized or
            // out-of-namespace references never reach the wire.
            // The helper reads the file locally to validate it
            // and never returns the bytes — the wire contract
            // stays metadata-only and the asset store enforces a
            // single source of truth for the validation rules.
            let asset_validator_store = peer_image_import_asset_store.clone();
            let image_host_source: Arc<dyn crate::peer_image_history::HostImageHistorySource> =
                Arc::new(
                    crate::peer_image_history::EntryRepositoryHostImageHistorySource::with_asset_validator(
                        Arc::clone(&database_handle),
                        move |asset_ref: &str| asset_validator_store.validate(asset_ref).is_ok(),
                    ),
                );
            let image_history_handler: Arc<
                dyn clipvault_platform::peer_transport::ImageHistoryHostHandler,
            > = Arc::new(
                crate::peer_pairing::PeerImageHistoryHostHandlerAdapter::new(
                    peer_image_history.clone(),
                    Arc::clone(&image_host_source),
                )
                .with_source_app_name_capability_resolver(Arc::clone(
                    &source_app_name_capability_resolver,
                )),
            );
            if let Err(error) =
                peer_pairing.install_image_history_handler_inner(image_history_handler)
            {
                tracing::warn!(
                    ?error,
                    "productive image history handler install failed; bootstrap continues with no host image history"
                );
            }
            let image_fetch_persistence: Arc<
                dyn crate::peer_image_import::PeerImageImportPersistence,
            > = Arc::new(crate::peer_image_sqlite::SqliteImageImportPersistence::new(
                Arc::clone(&database_handle),
                peer_image_import_asset_store.clone(),
            ));
            let image_fetch_handler: Arc<
                dyn clipvault_platform::peer_transport::FetchImageHostHandler,
            > = Arc::new(
                crate::peer_image_import::PeerImageImportHostHandlerAdapter::new(
                    image_fetch_persistence,
                )
                .with_capability_resolver(Arc::clone(&peer_image_import_capability_resolver)),
            );
            if let Err(error) = peer_pairing.install_image_fetch_handler_inner(image_fetch_handler)
            {
                tracing::warn!(
                    ?error,
                    "productive image fetch handler install failed; bootstrap continues with no host image import"
                );
            }

            // Install the host-side image-thumbnail handler the
            // `peer-image-preview-thumbnails` change ships. The
            // handler borrows the same shared database handle +
            // asset store the runtime already holds so the wire
            // contract stays consistent with the image-fetch
            // path. The bootstrap wires both capability
            // resolvers (the thumbnail route additionally
            // requires `image_import`) so a peer that only ships
            // one of the two collapses to the typed
            // `capability_missing` reason without persisting or
            // serving the derivative.
            let peer_image_thumbnail_capability_resolver:
                crate::peer_image_thumbnail::PeerImageThumbnailCapabilityResolver = Arc::new({
                let database_for_resolver = Arc::clone(&database_handle);
                move |peer_id: &str| -> bool {
                    let mut db = database_for_resolver.lock();
                    let conn = db.connection_mut();
                    let repo = clipvault_db::KnownPeerRepository::new(conn);
                    match repo.get(peer_id) {
                        Ok(Some(row)) => crate::peer_discovery::decode_capabilities(
                            &row.caps_extra,
                        )
                        .iter()
                        .any(|token| {
                            token
                                == crate::peer_discovery::IMAGE_PREVIEW_THUMBNAIL_CAPABILITY
                        }),
                        Ok(None) => false,
                        Err(error) => {
                            tracing::warn!(
                                error = %error,
                                "image thumbnail capability resolver: known_peers lookup failed; defaulting to capability absent"
                            );
                            false
                        }
                    }
                }
            });
            // Host-side persistence adapter the listener drives
            // when an authenticated peer asks for the bounded
            // thumbnail. The adapter borrows the same shared
            // SQLite handle + asset store the productive
            // image-fetch adapter already uses, so the wire
            // contract stays consistent with the rest of the
            // peer-image pipeline.
            let image_thumbnail_persistence: Arc<
                dyn crate::peer_image_thumbnail::PeerImageThumbnailHostPersistence,
            > = Arc::new(
                crate::peer_image_thumbnail::SqliteImageThumbnailHostPersistence::new(
                    Arc::clone(&database_handle),
                    peer_image_import_asset_store.clone(),
                ),
            );
            let image_thumbnail_gate =
                Arc::new(crate::peer_image_thumbnail::PeerImageThumbnailHostGate::new());
            // Host-side capability resolvers the handler
            // consults before serving the thumbnail. The
            // thumbnail route requires BOTH `image_import` and
            // `image_preview_thumbnail` so a peer that ships
            // only one collapses to the typed
            // `capability_missing` reason.
            let thumbnail_cap_resolver:
                crate::peer_image_thumbnail::PeerImageThumbnailHostCapabilityResolver =
                Arc::clone(&peer_image_thumbnail_capability_resolver);
            let image_import_cap_resolver:
                crate::peer_image_thumbnail::PeerImageThumbnailHostCapabilityResolver =
                Arc::clone(&peer_image_import_capability_resolver);
            let image_thumbnail_handler: Arc<
                dyn clipvault_platform::peer_transport::FetchImageThumbnailHostHandler,
            > = Arc::new(
                crate::peer_image_thumbnail::PeerImageThumbnailHostHandler::new(
                    image_thumbnail_persistence,
                    image_thumbnail_gate,
                )
                .with_capability_resolvers(thumbnail_cap_resolver, image_import_cap_resolver),
            );
            if let Err(error) =
                peer_pairing.install_image_thumbnail_handler_inner(image_thumbnail_handler)
            {
                tracing::warn!(
                    ?error,
                    "productive image thumbnail handler install failed; bootstrap continues with no host image thumbnails"
                );
            }

            let source_app_presentation_authorization:
                crate::peer_source_app_presentation_service::PeerSourceAppPresentationHostAuthorizationResolver =
                Arc::new({
                    let database_for_resolver = Arc::clone(&database_handle);
                    move |peer_id: &str| {
                        let mut db = database_for_resolver.lock();
                        let repo = clipvault_db::KnownPeerRepository::new(db.connection_mut());
                        match repo.get(peer_id) {
                            Ok(Some(row))
                                if row.trust_state == clipvault_db::TrustState::Trusted =>
                            {
                                if crate::peer_discovery::decode_capabilities(&row.caps_extra_v2)
                                    .iter()
                                    .any(|token| {
                                        token
                                            == crate::peer_discovery::SOURCE_APP_PRESENTATION_CAPABILITY
                                    })
                                {
                                    crate::peer_source_app_presentation_service::PeerSourceAppPresentationHostAuthorization::Authorized
                                } else {
                                    crate::peer_source_app_presentation_service::PeerSourceAppPresentationHostAuthorization::CapabilityMissing
                                }
                            }
                            _ => crate::peer_source_app_presentation_service::PeerSourceAppPresentationHostAuthorization::NotTrusted,
                        }
                    }
                });
            let source_app_presentation_persistence: Arc<
                dyn crate::peer_source_app_presentation_service::PeerSourceAppPresentationHostPersistence,
            > = Arc::new(
                crate::peer_source_app_presentation_service::SqlitePeerSourceAppPresentationHostPersistence::new(
                    Arc::clone(&database_handle),
                    peer_image_import_asset_store.clone(),
                ),
            );
            let source_app_presentation_handler: Arc<
                dyn clipvault_platform::peer_transport::FetchSourceAppPresentationHostHandler,
            > = Arc::new(
                crate::peer_source_app_presentation_service::PeerSourceAppPresentationHostHandlerAdapter::new(
                    source_app_presentation_persistence,
                    source_app_presentation_authorization,
                ),
            );
            if let Err(error) = pairing_transport
                .install_source_app_presentation_handler(source_app_presentation_handler)
            {
                tracing::warn!(
                    ?error,
                    "productive source-app presentation handler install failed; bootstrap continues with no host source-app presentation"
                );
            }
        }
        #[cfg(feature = "local-peer-pairing-tls")]
        let _pairing_transport_arc = Arc::clone(&pairing_transport);
        #[cfg(not(feature = "local-peer-pairing-tls"))]
        let _pairing_transport_arc = ();
        // Build the client-side thumbnail façade the
        // `peer-image-preview-thumbnails` change ships. The
        // service borrows the same productive transport the
        // bridge hands to the image-fetch / history façades so
        // the mTLS dial loop is shared with the productive
        // pairing routes; on builds without the TLS feature
        // pair the service falls back to the noop transport the
        // shell already uses for the image import bridge. The
        // service is metadata-only by construction: it never
        // persists the generated PNG and never substitutes the
        // snapshot for the original `Importar` payload.
        let peer_image_thumbnail_transport_for_context: Arc<
            dyn crate::peer_image_thumbnail::PeerFetchImageThumbnailTransport,
        > = {
            #[cfg(feature = "local-peer-pairing-tls")]
            {
                Arc::new(
                    crate::peer_image_thumbnail::PeerPairingFetchImageThumbnailTransportAdapter::new(
                        Arc::clone(&_pairing_transport_arc),
                    ),
                )
            }
            #[cfg(not(feature = "local-peer-pairing-tls"))]
            {
                Arc::new(crate::peer_image_thumbnail::NoopPeerFetchImageThumbnailTransport)
            }
        };
        let peer_image_thumbnail_capability_resolver_for_context:
            crate::peer_image_thumbnail::PeerImageThumbnailCapabilityResolver = Arc::new({
            let database_for_resolver = Arc::clone(&database_handle);
            move |peer_id: &str| -> bool {
                let mut db = database_for_resolver.lock();
                let conn = db.connection_mut();
                let repo = clipvault_db::KnownPeerRepository::new(conn);
                match repo.get(peer_id) {
                    Ok(Some(row)) => crate::peer_discovery::decode_capabilities(
                        &row.caps_extra,
                    )
                    .iter()
                    .any(|token| {
                        token == crate::peer_discovery::IMAGE_PREVIEW_THUMBNAIL_CAPABILITY
                    }),
                    Ok(None) => false,
                    Err(error) => {
                        tracing::warn!(
                            error = %error,
                            "image thumbnail capability resolver: known_peers lookup failed; defaulting to capability absent"
                        );
                        false
                    }
                }
            }
        });
        let peer_image_thumbnail = crate::peer_image_thumbnail::PeerImageThumbnailService::new(
            peer_image_thumbnail_transport_for_context,
        )
        .with_capability_resolver(Arc::clone(
            &peer_image_thumbnail_capability_resolver_for_context,
        ));
        let peer_source_app_presentation_transport: Arc<
            dyn crate::peer_source_app_presentation_service::PeerSourceAppPresentationTransport,
        > = {
            #[cfg(feature = "local-peer-pairing-tls")]
            {
                Arc::new(
                    crate::peer_source_app_presentation_service::PairingFetchSourceAppPresentationAdapter::new(
                        Arc::clone(&_pairing_transport_arc),
                    ),
                )
            }
            #[cfg(not(feature = "local-peer-pairing-tls"))]
            {
                Arc::new(
                    crate::peer_source_app_presentation_service::NoopPeerSourceAppPresentationTransport,
                )
            }
        };
        let source_app_presentation_capability_resolver:
            crate::peer_source_app_presentation_service::PeerSourceAppPresentationCapabilityResolver =
            Arc::new({
                let database_for_resolver = Arc::clone(&database_handle);
                move |peer_id: &str| {
                    let mut db = database_for_resolver.lock();
                    let repo = clipvault_db::KnownPeerRepository::new(db.connection_mut());
                    repo.get(peer_id)
                        .ok()
                        .flatten()
                        .is_some_and(|row| {
                            crate::peer_discovery::decode_capabilities(&row.caps_extra_v2)
                                .iter()
                                .any(|token| {
                                    token
                                        == crate::peer_discovery::SOURCE_APP_PRESENTATION_CAPABILITY
                                })
                        })
                }
            });
        let peer_source_app_presentation =
            crate::peer_source_app_presentation_service::PeerSourceAppPresentationService::new(
                peer_source_app_presentation_transport,
            )
            .with_capability_resolver(source_app_presentation_capability_resolver);
        // Pre-populate both per-peer HMAC secret caches from the
        // persisted `known_peers.cursor_secret` rows. The services
        // mirror this persisted secret so a restart never invalidates
        // cursors the peer already holds. The preload is best-effort and
        // also performs the `trusted` backfill the
        // `peer-text-history-browser` change requires: rows that
        // were `trusted` before the column landed carry an empty
        // `cursor_secret`, and without an explicit mint the host
        // would reject the first dial with the typed
        // `not_trusted` reason. The bootstrap therefore mints a
        // fresh CSPRNG secret for every `trusted` row whose column
        // is empty or malformed, persists it through
        // [`clipvault_db::KnownPeerRepository::set_cursor_secret`],
        // and only then installs the value in the in-memory cache
        // so a restart can never observe a secret that was lost
        // on the next boot. Non-`trusted` rows are left untouched
        // so the cache stays scoped to peers the runtime can sign
        // cursors for. Failures are logged with a fixed phrase —
        // no `peer_id`, secret, hostname, IP, port, certificate or
        // content ever crosses the log boundary.
        {
            let mut db = database_handle.lock();
            if let Err(error) =
                preload_cursor_secrets(&mut db, &peer_text_history, &peer_image_history)
            {
                tracing::warn!(
                    ?error,
                    "known_peers preload failed; cursor secrets will mint on next trust promotion"
                );
            }
        }
        // Restore the mTLS pins persisted by earlier successful
        // pairing sessions before the shell starts the listener.
        // `TlsPeerTransport` deliberately keeps its verifier map
        // in memory, so without this preload a trusted peer would
        // become `UnknownPeer` after every application restart even
        // though its certificate fingerprint remains in SQLite.
        #[cfg(feature = "local-peer-pairing-tls")]
        {
            let mut db = database_handle.lock();
            if preload_trusted_peer_pins(&mut db, &peer_pairing).is_err() {
                tracing::warn!(
                    "known_peers pin preload failed; trusted peer history remains unavailable until pairing is renewed"
                );
            }
        }
        // Build the metadata-only transferable-text browser. The
        // service borrows the shared database handle the bootstrap
        // already holds and is therefore read-only by construction:
        // every browsing call goes through the SQL projection and
        // The bootstrap wires the productive mTLS-backed
        // [`PeerHistoryTransport`] the productive pairing transport
        // already installed (see the borrow above for the
        // `peer_text_history_transport_for` helper). The
        // client-side facade dials the remote listener over mTLS
        // through the runtime and forwards the typed outcome the
        // transport returns; the host-side projection lives behind
        // the [`crate::peer_text_history::HostHistorySource`] trait
        // the listener drives when an authenticated peer asks for
        // `list_recent_text`. The shell drives the per-peer
        // `trusted` / `active` cache through
        // [`PeerTextHistoryService::record_peer_state`] on every
        // snapshot / health probe so a stale cache cannot outlive
        // the runtime transition that should invalidate it.
        // Keep the concrete mDNS adapter the bootstrap installed
        // so the toggle command can wire it into the
        // [`crate::peer_pairing::PairingAdvertisement`] the
        // productive pairing transport publishes through. Storing
        // the concrete type (not the trait object the discovery
        // runtime owns) is required because the
        // `MdnsPairingAdvertisementSink` constructor lives in
        // `clipvault-platform` and expects the typed handle. We
        // downcast the trait object via `Any`; a `None` outcome
        // collapses into the typed `Unavailable` path the toggle
        // already documents.
        #[cfg(all(
            feature = "local-peer-discovery-mdns",
            feature = "local-peer-pairing-tls"
        ))]
        let peer_discovery_concrete = self.options.peer_discovery_concrete.clone();
        #[cfg(not(all(
            feature = "local-peer-discovery-mdns",
            feature = "local-peer-pairing-tls"
        )))]
        let _peer_discovery_concrete: Option<
            Arc<clipvault_platform::NoopPeerDiscoveryAdapter>,
        > = {
            let _ = self;
            None
        };

        Ok(AppContext {
            database: database_handle,
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
            gnome_integration: gnome_integration_service,
            started_at,
            version: env!("CARGO_PKG_VERSION"),
            cached_active_app: cached_probe,
            active_app_diagnostics,
            paste_suppression,
            peer_discovery,
            peer_pairing,
            peer_text_history,
            peer_text_import,
            peer_image_history,
            peer_image_import,
            peer_image_thumbnail,
            peer_source_app_presentation,
            // The capture-debug sink is either the caller-supplied
            // handle (tests) or the production wiring that consults
            // `CLIPVAULT_DEBUG_CAPTURE` exactly once at startup. When
            // the env var is unset the handle resolves to a
            // [`NullCaptureDebugSink`] and the capture pipeline never
            // emits an event.
            capture_debug: self
                .options
                .capture_debug_sink
                .unwrap_or_else(CaptureDebugSinkHandle::enabled),
            environment_snapshot_emitted: Arc::new(AtomicBool::new(false)),
            #[cfg(all(
                feature = "local-peer-discovery-mdns",
                feature = "local-peer-pairing-tls"
            ))]
            peer_discovery_concrete,
        })
    }
}

impl Default for AppBootstrap {
    fn default() -> Self {
        Self::new()
    }
}

/// Resolve the [`crate::peer_pairing::PeerTransport`] the
/// bootstrap installs. Production shells always leave the slot
/// empty so the helper falls through to
/// [`crate::peer_pairing::default_peer_transport`]; tests inject
/// a scriptable transport through
/// [`AppBootstrap::with_pairing_transport`] so they can verify the
/// shell shutdown order without standing up a real TLS listener.
fn resolve_pairing_transport(
    injected: Option<Arc<dyn crate::peer_pairing::PeerTransport>>,
) -> Arc<dyn crate::peer_pairing::PeerTransport> {
    if let Some(transport) = injected {
        return transport;
    }
    crate::peer_pairing::default_peer_transport()
}

/// Resolve the [`crate::peer_text_history::PeerHistoryTransport`]
/// the bootstrap installs behind the client-side facade. The
/// productive install path piggy-backs on the productive pairing
/// transport the bootstrap already wired; cross-compiles and
/// unsupported targets fall back to a noop transport that surfaces
/// [`crate::peer_text_history::PeerHistoryTransportError::Unavailable`]
/// so the runtime never reaches for half-broken transport state.
fn peer_text_history_transport_for(
    #[cfg(feature = "local-peer-pairing-tls")] pairing_transport: Arc<
        dyn crate::peer_pairing::PeerTransport,
    >,
) -> Arc<dyn crate::peer_text_history::PeerHistoryTransport> {
    #[cfg(feature = "local-peer-pairing-tls")]
    {
        Arc::new(
            crate::peer_text_history::PeerPairingHistoryTransportAdapter::new(pairing_transport),
        )
    }
    #[cfg(not(feature = "local-peer-pairing-tls"))]
    {
        let _ = ();
        Arc::new(crate::peer_text_history::NoopPeerHistoryTransport)
    }
}

/// Resolve the [`crate::peer_text_import::PeerFetchTransport`]
/// the bootstrap installs behind the client-side facade. The
/// productive install path piggy-backs on the productive pairing
/// transport the bootstrap already wired; cross-compiles and
/// unsupported targets fall back to a noop transport that surfaces
/// [`crate::peer_text_import::PeerFetchTransportError::Unavailable`]
/// so the runtime never reaches for half-broken transport state.
fn peer_text_import_transport_for(
    #[cfg(feature = "local-peer-pairing-tls")] pairing_transport: Arc<
        dyn crate::peer_pairing::PeerTransport,
    >,
) -> Arc<dyn crate::peer_text_import::PeerFetchTransport> {
    #[cfg(feature = "local-peer-pairing-tls")]
    {
        Arc::new(crate::peer_text_import::PeerPairingFetchTransportAdapter::new(pairing_transport))
    }
    #[cfg(not(feature = "local-peer-pairing-tls"))]
    {
        let _ = ();
        Arc::new(crate::peer_text_import::NoopPeerFetchTransport)
    }
}

/// Resolve the [`crate::peer_image_history::PeerImageHistoryTransport`]
/// the bootstrap installs behind the client-side facade. Mirrors
/// [`peer_text_history_transport_for`].
fn peer_image_history_transport_for(
    #[cfg(feature = "local-peer-pairing-tls")] pairing_transport: Arc<
        dyn crate::peer_pairing::PeerTransport,
    >,
) -> Arc<dyn crate::peer_image_history::PeerImageHistoryTransport> {
    #[cfg(feature = "local-peer-pairing-tls")]
    {
        Arc::new(
            crate::peer_image_history::PeerPairingImageHistoryTransportAdapter::new(
                pairing_transport,
            ),
        )
    }
    #[cfg(not(feature = "local-peer-pairing-tls"))]
    {
        let _ = ();
        Arc::new(crate::peer_image_history::NoopPeerImageHistoryTransport)
    }
}

/// Resolve the [`crate::peer_image_import::PeerFetchImageTransport`]
/// the bootstrap installs behind the client-side facade. Mirrors
/// [`peer_text_import_transport_for`].
fn peer_image_import_transport_for(
    #[cfg(feature = "local-peer-pairing-tls")] pairing_transport: Arc<
        dyn crate::peer_pairing::PeerTransport,
    >,
) -> Arc<dyn crate::peer_image_import::PeerFetchImageTransport> {
    #[cfg(feature = "local-peer-pairing-tls")]
    {
        Arc::new(
            crate::peer_image_import::PeerPairingFetchImageTransportAdapter::new(pairing_transport),
        )
    }
    #[cfg(not(feature = "local-peer-pairing-tls"))]
    {
        let _ = ();
        Arc::new(crate::peer_image_import::NoopPeerFetchImageTransport)
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

/// Resolve the production [`PeerDiscoveryAdapter`] the bootstrap
/// installs when the caller did not inject one. Production builds
/// on macOS / Linux link the `mdns-sd`-backed adapter the
/// `local-peer-discovery` change ships; every other build falls
/// back to [`NoopPeerDiscoveryAdapter`] so the runtime still
/// surfaces a typed `runtime_stopped` reason instead of panicking
/// on a missing backend.
///
/// The function inspects the platform target through `cfg` so the
/// platform crate can keep `mdns-sd` behind an optional feature;
/// builds compiled without `local-peer-discovery-mdns` never link
/// the adapter regardless of host.
fn default_peer_discovery_adapter() -> impl clipvault_platform::PeerDiscoveryAdapter {
    #[cfg(all(
        feature = "local-peer-discovery-mdns",
        any(target_os = "macos", target_os = "linux")
    ))]
    {
        clipvault_platform::MdnsPeerDiscoveryAdapter::new()
    }
    #[cfg(not(all(
        feature = "local-peer-discovery-mdns",
        any(target_os = "macos", target_os = "linux")
    )))]
    {
        clipvault_platform::NoopPeerDiscoveryAdapter::new()
    }
}

/// Pre-populate the [`crate::peer_text_history::PeerTextHistoryService`]
/// cache with every persisted `known_peers.cursor_secret` row and
/// backfill any `trusted` row whose column is empty or malformed.
/// The helper is the single source of truth for the
/// "trusted-but-no-secret" recovery path the bootstrap installs.
/// Non-`trusted` rows are left untouched: the runtime only signs
/// cursors for trusted peers, so a stale secret on an unverified
/// row would only make future debugging noisier without any
/// functional benefit. A malformed hex column is rotated rather
/// than accepted so a corrupted row cannot downgrade the HMAC
/// pipeline to a deterministic key. The function consumes the
/// database handle so it can read the rows, mint a fresh secret
/// and persist it through the same repository the runtime
/// trusts. The initial row listing is the only failure that
/// propagates to the bootstrap caller; a per-row write failure is
/// logged locally with a fixed phrase (never the `peer_id`, the
/// secret, hostname, IP, port, certificate, content or SQL
/// detail), the affected row is skipped so the secret never
/// reaches the in-memory cache, and the loop keeps processing
/// the remaining rows so the bootstrap stays alive.
pub(super) fn preload_cursor_secrets(
    database: &mut clipvault_db::Database,
    text_service: &crate::peer_text_history::PeerTextHistoryService,
    image_service: &crate::peer_image_history::PeerImageHistoryService,
) -> Result<(), clipvault_db::KnownPeersError> {
    let conn = database.connection_mut();
    let repo = clipvault_db::KnownPeerRepository::new(conn);
    let rows = repo.list()?;
    for row in rows {
        if row.trust_state != clipvault_db::TrustState::Trusted {
            continue;
        }
        let hex_is_valid = row.cursor_secret.len() == 64
            && row.cursor_secret.chars().all(|c| c.is_ascii_hexdigit());
        if hex_is_valid {
            // The persisted column already carries the canonical
            // 32-byte secret. Install it verbatim; a follow-up
            // `set_cursor_secret` would invalidate every cursor
            // the peer already holds.
            let _ = text_service.install_cursor_secret_hex(&row.peer_id, &row.cursor_secret);
            let _ = image_service.install_cursor_secret_hex(&row.peer_id, &row.cursor_secret);
            continue;
        }
        // Backfill path: empty / short / non-hexadecimal column.
        // Mint a fresh secret, persist it, and only after the
        // write succeeds install it in the cache. If the write
        // fails, this helper logs a fixed phrase — never the
        // `peer_id`, the secret, hostname, IP, port, certificate,
        // content or SQL detail — leaves the cache empty for the
        // affected row, and continues with the remaining rows so
        // the bootstrap keeps running. The peer ends up served as
        // `not_trusted` until the next trust promotion re-mints
        // a fresh secret.
        let minted = crate::peer_text_history::PeerCursorSecret::generate();
        let minted_hex = minted.to_hex();
        let mut repo_mut = clipvault_db::KnownPeerRepository::new(conn);
        match repo_mut.set_cursor_secret(&row.peer_id, &minted_hex) {
            Ok(()) => {
                let _ = text_service.install_cursor_secret_hex(&row.peer_id, &minted_hex);
                let _ = image_service.install_cursor_secret_hex(&row.peer_id, &minted_hex);
            }
            Err(_) => {
                tracing::warn!(
                    "cursor secret backfill failed; the affected peer will be served as not_trusted until the next trust promotion"
                );
            }
        }
    }
    Ok(())
}

/// Restore persisted certificate pins for trusted peers before a new
/// productive TLS listener starts. Pairing persists the SHA-256
/// fingerprint across launches, whereas the transport intentionally
/// keeps its verifier map in memory. Restoring only canonical trusted
/// rows keeps a restart from downgrading an existing pairing into an
/// `UnknownPeer` history outcome without accepting malformed or
/// untrusted metadata.
///
/// A failure to list the rows propagates to the bootstrap, which logs
/// a fixed phrase and keeps starting the application. An individual
/// arm failure is likewise logged with a fixed phrase and does not
/// prevent other trusted peers from being restored. Neither path logs
/// peer identifiers, fingerprints, endpoints, SQL, or clipboard data.
#[cfg(feature = "local-peer-pairing-tls")]
pub(super) fn preload_trusted_peer_pins(
    database: &mut clipvault_db::Database,
    pairing: &crate::peer_pairing::PairingRuntime,
) -> Result<(), clipvault_db::KnownPeersError> {
    let repo = clipvault_db::KnownPeerRepository::new(database.connection_mut());
    let rows = repo.list()?;
    for row in rows {
        if row.trust_state != clipvault_db::TrustState::Trusted
            || !is_canonical_cert_fingerprint(&row.tls_cert_fingerprint)
        {
            continue;
        }
        if pairing
            .restore_trusted_pin(&row.peer_id, &row.tls_cert_fingerprint)
            .is_err()
        {
            tracing::warn!(
                "trusted peer pin restore failed; the affected peer remains unavailable until pairing is renewed"
            );
        }
    }
    Ok(())
}

/// A certificate fingerprint is a canonical lower-case SHA-256 hex
/// projection. The pairing handshake is the sole producer of this
/// value; rejecting anything else prevents a corrupt persisted row
/// from entering the mTLS verifier map during startup.
#[cfg(feature = "local-peer-pairing-tls")]
fn is_canonical_cert_fingerprint(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte.is_ascii_lowercase())
}

/// Adapter the bootstrap installs as the
/// [`crate::peer_pairing::PairingPersistence`] backend. The
/// adapter delegates every transition to
/// [`clipvault_db::KnownPeerRepository`] so the runtime never has
/// to write SQL; the runtime calls the typed transition methods
/// the repository exposes and surfaces the typed
/// [`clipvault_db::TrustTransitionOutcome`] back to the pairing
/// state machine. SQLite failures collapse into
/// [`crate::peer_pairing::PairingPersistenceError::Unavailable`]
/// so the runtime never sees a raw sqlite error.
struct KnownPeerPairingPersistence {
    database: Arc<Mutex<Database>>,
}

/// Adapter the bootstrap installs as the
/// [`crate::peer_pairing::MaterialLoader`] when the shell wires
/// the platform keychain. The loader is the single owner of the
/// seed bytes — the runtime only sees the typed
/// [`clipvault_platform::LocalIdentityMaterial`] the keychain
/// returns. Keychain failures collapse into
/// [`crate::peer_pairing::PairingPersistenceError::Unavailable`]
/// so the runtime surfaces a typed reason instead of leaking the
/// raw platform detail.
#[cfg(all(
    feature = "local-peer-pairing-tls",
    feature = "local-peer-identity-keychain"
))]
struct KeychainPairingMaterialLoader {
    keychain: Arc<clipvault_platform::KeychainPeerIdentityStore>,
}

#[cfg(all(
    feature = "local-peer-pairing-tls",
    feature = "local-peer-identity-keychain"
))]
impl KeychainPairingMaterialLoader {
    fn new(keychain: Arc<clipvault_platform::KeychainPeerIdentityStore>) -> Self {
        Self { keychain }
    }
}

#[cfg(all(
    feature = "local-peer-pairing-tls",
    feature = "local-peer-identity-keychain"
))]
impl crate::peer_pairing::MaterialLoader for KeychainPairingMaterialLoader {
    fn load(
        &self,
    ) -> Result<
        clipvault_platform::LocalIdentityMaterial,
        crate::peer_pairing::PairingPersistenceError,
    > {
        self.keychain
            .load_or_create_material()
            .map_err(|_error| crate::peer_pairing::PairingPersistenceError::Unavailable)
    }
}

impl KnownPeerPairingPersistence {
    fn new(database: Arc<Mutex<Database>>) -> Self {
        Self { database }
    }
}

impl crate::peer_pairing::PairingPersistence for KnownPeerPairingPersistence {
    fn mark_trusted(
        &self,
        peer_id: &str,
        tls_cert_fingerprint: &str,
        paired_at: time::OffsetDateTime,
        paired_protocol_major: i64,
    ) -> Result<clipvault_db::TrustTransitionOutcome, crate::peer_pairing::PairingPersistenceError>
    {
        let mut db = self.database.lock();
        let mut repo = clipvault_db::KnownPeerRepository::new(db.connection_mut());
        repo.mark_trusted(
            peer_id,
            tls_cert_fingerprint,
            paired_at,
            paired_protocol_major,
        )
        .map_err(|_| crate::peer_pairing::PairingPersistenceError::Unavailable)
    }

    fn mark_revoked(
        &self,
        peer_id: &str,
    ) -> Result<clipvault_db::TrustTransitionOutcome, crate::peer_pairing::PairingPersistenceError>
    {
        let mut db = self.database.lock();
        let mut repo = clipvault_db::KnownPeerRepository::new(db.connection_mut());
        repo.mark_revoked(peer_id)
            .map_err(|_| crate::peer_pairing::PairingPersistenceError::Unavailable)
    }

    fn mark_blocked(
        &self,
        peer_id: &str,
    ) -> Result<clipvault_db::TrustTransitionOutcome, crate::peer_pairing::PairingPersistenceError>
    {
        let mut db = self.database.lock();
        let mut repo = clipvault_db::KnownPeerRepository::new(db.connection_mut());
        repo.mark_blocked(peer_id)
            .map_err(|_| crate::peer_pairing::PairingPersistenceError::Unavailable)
    }

    fn unblock(
        &self,
        peer_id: &str,
    ) -> Result<clipvault_db::TrustTransitionOutcome, crate::peer_pairing::PairingPersistenceError>
    {
        let mut db = self.database.lock();
        let mut repo = clipvault_db::KnownPeerRepository::new(db.connection_mut());
        repo.unblock(peer_id)
            .map_err(|_| crate::peer_pairing::PairingPersistenceError::Unavailable)
    }

    fn load(
        &self,
        peer_id: &str,
    ) -> Result<Option<clipvault_db::KnownPeer>, crate::peer_pairing::PairingPersistenceError> {
        let mut db = self.database.lock();
        let repo = clipvault_db::KnownPeerRepository::new(db.connection_mut());
        repo.get(peer_id)
            .map_err(|_| crate::peer_pairing::PairingPersistenceError::Unavailable)
    }

    fn set_cursor_secret(
        &self,
        peer_id: &str,
        secret_hex: &str,
    ) -> Result<(), crate::peer_pairing::PairingPersistenceError> {
        let mut db = self.database.lock();
        let mut repo = clipvault_db::KnownPeerRepository::new(db.connection_mut());
        repo.set_cursor_secret(peer_id, secret_hex)
            .map_err(|_| crate::peer_pairing::PairingPersistenceError::Unavailable)
    }

    fn clear_cursor_secret(
        &self,
        peer_id: &str,
    ) -> Result<(), crate::peer_pairing::PairingPersistenceError> {
        let mut db = self.database.lock();
        let mut repo = clipvault_db::KnownPeerRepository::new(db.connection_mut());
        repo.clear_cursor_secret(peer_id)
            .map_err(|_| crate::peer_pairing::PairingPersistenceError::Unavailable)
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

    /// Bootstrap backfill: three rows in a temp `known_peers`
    /// (`trusted` with no secret, `trusted` with a valid secret,
    /// `unverified` with no secret) drive the helper end-to-end.
    /// The `trusted` row without a secret MUST come back with a
    /// freshly minted secret persisted in SQLite AND installed
    /// in the in-memory cache; the `trusted` row with a valid
    /// secret MUST stay byte-for-byte identical; the non-`trusted`
    /// row MUST remain empty (the runtime never signs cursors for
    /// it). The seed data lives on a tempdir the test owns so the
    /// workspace's `~/.clipvault` is never touched.
    #[test]
    fn preload_cursor_secrets_backfills_trusted_legacy_peers() {
        use clipvault_db::{builtin_migrations, Database, TrustState};
        use std::collections::HashMap;

        let dir = tempfile::tempdir().expect("tempdir");
        let mut db = Database::open(dir.path().join("clipvault.db")).expect("open");
        db.run_migrations(&builtin_migrations()).expect("migrate");

        // Seed three rows.
        let trusted_no_secret_id = "peer-trusted-no-secret";
        let trusted_with_secret_id = "peer-trusted-with-secret";
        let unverified_no_secret_id = "peer-unverified-no-secret";
        let valid_secret_hex = "1".repeat(64);

        {
            let conn = db.connection_mut();
            let mut repo = clipvault_db::KnownPeerRepository::new(conn);
            for (peer_id, secret) in [
                (trusted_no_secret_id, String::new()),
                (trusted_with_secret_id, valid_secret_hex.clone()),
                (unverified_no_secret_id, String::new()),
            ] {
                let outcome = repo
                    .upsert_observation(&clipvault_db::PeerObservation {
                        peer_id: peer_id.to_string(),
                        public_key_fingerprint: "f".repeat(16),
                        full_public_key_fingerprint: Some("f".repeat(64)),
                        display_name: "Studio".to_string(),
                        protocol_major: 1,
                        capability: "pairing".to_string(),
                        caps_extra: String::new(),
                        caps_extra_v2: String::new(),
                        observed_at: time::OffsetDateTime::UNIX_EPOCH,
                    })
                    .expect("seed observation");
                assert!(matches!(
                    outcome,
                    clipvault_db::UpsertObservationOutcome::Stored(_)
                ));
                if !secret.is_empty() {
                    repo.set_cursor_secret(peer_id, &secret)
                        .expect("seed secret");
                }
            }
            // Promote the two we want to `Trusted`; the third
            // stays `Unverified`.
            let outcome = repo
                .mark_trusted(
                    trusted_no_secret_id,
                    "fingerprint-aaaa",
                    time::OffsetDateTime::UNIX_EPOCH,
                    1,
                )
                .expect("mark_trusted a");
            assert!(matches!(
                outcome,
                clipvault_db::TrustTransitionOutcome::Stored(_)
            ));
            let outcome = repo
                .mark_trusted(
                    trusted_with_secret_id,
                    "fingerprint-bbbb",
                    time::OffsetDateTime::UNIX_EPOCH,
                    1,
                )
                .expect("mark_trusted b");
            assert!(matches!(
                outcome,
                clipvault_db::TrustTransitionOutcome::Stored(_)
            ));
        }

        let service = crate::peer_text_history::PeerTextHistoryService::new(Arc::new(
            crate::peer_text_history::NoopPeerHistoryTransport,
        ));
        let image_service = crate::peer_image_history::PeerImageHistoryService::new(Arc::new(
            crate::peer_image_history::NoopPeerImageHistoryTransport,
        ));
        preload_cursor_secrets(&mut db, &service, &image_service).expect("preload");

        // Reload the rows so the test never inspects the in-memory
        // cache directly: the contract the runtime consumes is the
        // persisted column, and the cache mirrors it via
        // `install_cursor_secret_hex` which the test already
        // exercised end-to-end through the helper.
        let conn = db.connection_mut();
        let repo = clipvault_db::KnownPeerRepository::new(conn);
        let rows: HashMap<String, clipvault_db::KnownPeer> = repo
            .list()
            .expect("list")
            .into_iter()
            .map(|row| (row.peer_id.clone(), row))
            .collect();

        let backfilled = &rows[trusted_no_secret_id];
        assert_eq!(backfilled.trust_state, TrustState::Trusted);
        assert_eq!(
            backfilled.cursor_secret.len(),
            64,
            "trusted-no-secret row must end up with a 64-hex persisted secret"
        );
        assert!(
            backfilled
                .cursor_secret
                .chars()
                .all(|c| c.is_ascii_hexdigit()),
            "backfilled secret must be a valid 64-hex string"
        );

        let preserved = &rows[trusted_with_secret_id];
        assert_eq!(preserved.trust_state, TrustState::Trusted);
        assert_eq!(
            preserved.cursor_secret, valid_secret_hex,
            "trusted-with-secret row must keep its persisted secret verbatim"
        );

        let left = &rows[unverified_no_secret_id];
        assert_eq!(left.trust_state, TrustState::Unverified);
        assert!(
            left.cursor_secret.is_empty(),
            "non-trusted row must stay empty so the runtime never leaks a secret outside its trust scope"
        );

        // The cache mirrors the persisted secrets: the backfilled
        // and preserved rows both have a known secret, the
        // unverified row does not.
        let backfilled_secret = service
            .cursor_secret_hex(trusted_no_secret_id)
            .expect("backfilled cache hit");
        assert_eq!(backfilled_secret, backfilled.cursor_secret);
        assert_eq!(
            image_service.cursor_secret_hex(trusted_no_secret_id),
            Some(backfilled.cursor_secret.clone()),
            "the image-history cache must mirror the backfilled trusted secret"
        );
        let preserved_secret = service
            .cursor_secret_hex(trusted_with_secret_id)
            .expect("preserved cache hit");
        assert_eq!(preserved_secret, valid_secret_hex);
        assert_eq!(
            image_service.cursor_secret_hex(trusted_with_secret_id),
            Some(valid_secret_hex.clone()),
            "the image-history cache must retain the persisted trusted secret"
        );
        let empty_image_source = crate::peer_image_history::InMemoryHostImageHistorySource::new();
        assert!(matches!(
            image_service.serve(
                trusted_with_secret_id,
                None,
                crate::peer_image_history::MAX_IMAGE_PAGE_ROWS as u32,
                &empty_image_source,
            ),
            crate::peer_image_history::HostImageHistoryResponse::Ok(..)
        ));
        assert!(
            service.cursor_secret_hex(unverified_no_secret_id).is_none(),
            "non-trusted peer must NOT be cached"
        );
        assert!(
            image_service
                .cursor_secret_hex(unverified_no_secret_id)
                .is_none(),
            "non-trusted peer must NOT be cached for image history"
        );
    }

    /// Bootstrap backfill against a row whose `cursor_secret`
    /// column carries a malformed (non-hexadecimal) value: the
    /// helper MUST treat the column as empty, mint a fresh
    /// secret, persist it and only then install it in the cache.
    /// The previous prototype would have kept the corrupted value
    /// and the host would have failed to verify every cursor the
    /// peer ever minted.
    #[test]
    fn preload_cursor_secrets_rotates_malformed_hex_for_trusted_legacy_peers() {
        use clipvault_db::{builtin_migrations, Database, TrustState};

        let dir = tempfile::tempdir().expect("tempdir");
        let mut db = Database::open(dir.path().join("clipvault.db")).expect("open");
        db.run_migrations(&builtin_migrations()).expect("migrate");
        let malformed_id = "peer-malformed";
        {
            let conn = db.connection_mut();
            let mut repo = clipvault_db::KnownPeerRepository::new(conn);
            repo.upsert_observation(&clipvault_db::PeerObservation {
                peer_id: malformed_id.to_string(),
                public_key_fingerprint: "f".repeat(16),
                full_public_key_fingerprint: Some("f".repeat(64)),
                display_name: "Studio".to_string(),
                protocol_major: 1,
                capability: "pairing".to_string(),
                caps_extra: String::new(),
                caps_extra_v2: String::new(),
                observed_at: time::OffsetDateTime::UNIX_EPOCH,
            })
            .expect("seed observation");
            repo.mark_trusted(
                malformed_id,
                "fingerprint-cccc",
                time::OffsetDateTime::UNIX_EPOCH,
                1,
            )
            .expect("mark_trusted");
            // Force a non-empty, non-hexadecimal value into the
            // column so the helper sees the malformed branch.
            let _ = conn.execute(
                "UPDATE known_peers SET cursor_secret = ?1 WHERE peer_id = ?2",
                rusqlite::params!["not-hex", malformed_id],
            );
        }
        let service = crate::peer_text_history::PeerTextHistoryService::new(Arc::new(
            crate::peer_text_history::NoopPeerHistoryTransport,
        ));
        let image_service = crate::peer_image_history::PeerImageHistoryService::new(Arc::new(
            crate::peer_image_history::NoopPeerImageHistoryTransport,
        ));
        preload_cursor_secrets(&mut db, &service, &image_service).expect("preload");

        let conn = db.connection_mut();
        let repo = clipvault_db::KnownPeerRepository::new(conn);
        let row = repo.get(malformed_id).expect("get").expect("present");
        assert_eq!(row.trust_state, TrustState::Trusted);
        assert_eq!(row.cursor_secret.len(), 64);
        assert!(
            row.cursor_secret.chars().all(|c| c.is_ascii_hexdigit()),
            "malformed column must have been rotated into a valid hex"
        );
        let cached = service.cursor_secret_hex(malformed_id).expect("cache hit");
        assert_eq!(cached, row.cursor_secret);
        assert_eq!(
            image_service.cursor_secret_hex(malformed_id),
            Some(row.cursor_secret),
            "malformed persisted secret must be rotated into both history caches"
        );
    }

    /// Certificate pins live in the productive TLS transport's
    /// in-memory verifier map, while the pairing runtime persists
    /// them in `known_peers`. A restart must restore every canonical
    /// trusted pin before the listener starts, and must leave
    /// untrusted or malformed rows absent from that map.
    #[cfg(feature = "local-peer-pairing-tls")]
    #[test]
    fn preload_trusted_peer_pins_restores_only_canonical_trusted_rows() {
        use clipvault_db::{builtin_migrations, Database};
        use clipvault_platform::peer_transport::PeerTransport as _;

        let dir = tempfile::tempdir().expect("tempdir");
        let mut db = Database::open(dir.path().join("clipvault.db")).expect("open");
        db.run_migrations(&builtin_migrations()).expect("migrate");
        let trusted_id = "peer-trusted-pin";
        let malformed_id = "peer-malformed-pin";
        let unverified_id = "peer-unverified-pin";
        let trusted_pin = "a".repeat(64);

        {
            let conn = db.connection_mut();
            let mut repo = clipvault_db::KnownPeerRepository::new(conn);
            for peer_id in [trusted_id, malformed_id, unverified_id] {
                repo.upsert_observation(&clipvault_db::PeerObservation {
                    peer_id: peer_id.to_string(),
                    public_key_fingerprint: "f".repeat(16),
                    full_public_key_fingerprint: Some("f".repeat(64)),
                    display_name: "Studio".to_string(),
                    protocol_major: 1,
                    capability: "pairing".to_string(),
                    caps_extra: String::new(),
                    caps_extra_v2: String::new(),
                    observed_at: time::OffsetDateTime::UNIX_EPOCH,
                })
                .expect("seed observation");
            }
            repo.mark_trusted(
                trusted_id,
                &trusted_pin,
                time::OffsetDateTime::UNIX_EPOCH,
                1,
            )
            .expect("mark trusted");
            repo.mark_trusted(
                malformed_id,
                "not-a-certificate-fingerprint",
                time::OffsetDateTime::UNIX_EPOCH,
                1,
            )
            .expect("mark malformed trusted");
        }

        let transport = Arc::new(clipvault_platform::peer_transport::TlsPeerTransport::new());
        let pairing = crate::peer_pairing::PairingRuntime::new(
            transport.clone(),
            Arc::new(crate::peer_pairing::InMemoryPairingPersistence::new()),
        );
        preload_trusted_peer_pins(&mut db, &pairing).expect("preload pins");

        assert!(
            transport.health_check(trusted_id, &trusted_pin).is_ok(),
            "a canonical trusted fingerprint must be restored into the verifier map"
        );
        assert!(
            matches!(
                transport.health_check(malformed_id, "b".repeat(64).as_str()),
                Err(clipvault_platform::peer_transport::TransportError::UnknownPeer)
            ),
            "a malformed trusted fingerprint must not enter the verifier map"
        );
        assert!(
            matches!(
                transport.health_check(unverified_id, "c".repeat(64).as_str()),
                Err(clipvault_platform::peer_transport::TransportError::UnknownPeer)
            ),
            "an untrusted peer must not enter the verifier map"
        );
    }
}
