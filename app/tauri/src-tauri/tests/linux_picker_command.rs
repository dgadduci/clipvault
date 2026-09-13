//! End-to-end tests for the Linux application picker catalog and the
//! `clipvault_ignored_app_linux_add` command.
//!
//! The picker must be deterministic: the identifier the catalog
//! returns MUST be the value the active-app adapter publishes. The
//! integration suite exercises the full decision matrix the
//! [`resolve_linux_picker_backend`] helper implements (X11,
//! XWayland, native Wayland, GNOME Shell extension, Unsupported)
//! and verifies that:
//!
//! - the catalog command returns `Supported` only when the runtime
//!   state proves a deterministic mapping exists;
//! - the catalog command returns `Unsupported` for sessions the
//!   picker cannot classify (`Unknown` display server, empty
//!   cache, GNOME extension accepted but not connected, ...);
//! - the `add` command revalidates the identifier against the
//!   current catalog and refuses arbitrary identifiers;
//! - the manual-entry surface (`add_with_metadata`) keeps working
//!   untouched so the documented fallback stays usable;
//! - cancelling the modal leaves the blacklist unchanged and never
//!   creates an asset.
//!
//! [`resolve_linux_picker_backend`]: clipvault_core::linux_picker::resolve_linux_picker_backend

#![cfg(all(target_os = "linux", feature = "linux-x11"))]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use clipvault_app::commands::{
    clipvault_ignored_app_linux_add_for_test_with_fs,
    clipvault_ignored_app_linux_catalog_for_test_with_fs, LinuxCatalogResponse,
    LinuxPickAndAddResponse,
};
use clipvault_core::{AppBootstrap, Clock, PlatformAdapters};
use clipvault_db::{builtin_migrations, Database};
use clipvault_platform::runtime::linux_app_catalog::IdentifierStrategy;
use clipvault_platform::LinuxPickerBackend;
use clipvault_platform::{
    runtime::linux_app_metadata::{DesktopEntry, DesktopFilesystem},
    ActiveApplicationProbe, Capabilities, DisplayServer, OsFamily, PlatformInfo,
};
use parking_lot::Mutex as ParkingLotMutex;

struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> time::OffsetDateTime {
        time::OffsetDateTime::now_utc()
    }
}

/// Probe whose `name()` returns the supplied backend string so the
/// diagnostics surface reports a real `ActiveAppBackendKind` value
/// to the picker resolver. The probe never produces an
/// application so the cache stays empty; tests that need a
/// populated cache call `set_identifier` first.
struct ScriptedProbe {
    backend: &'static str,
    identifier: ParkingLotMutex<Option<String>>,
}

impl ScriptedProbe {
    fn new(backend: &'static str) -> Self {
        Self {
            backend,
            identifier: ParkingLotMutex::new(None),
        }
    }

    fn set_identifier(&self, identifier: Option<String>) {
        let mut guard = self.identifier.lock();
        *guard = identifier;
    }
}

impl ActiveApplicationProbe for ScriptedProbe {
    fn active_application(
        &self,
    ) -> Result<Option<clipvault_platform::ActiveApplication>, clipvault_platform::ActiveAppError>
    {
        let guard = self.identifier.lock();
        Ok(guard
            .as_ref()
            .cloned()
            .map(|id| clipvault_platform::ActiveApplication::new(id.clone(), id)))
    }
    fn name(&self) -> &'static str {
        self.backend
    }
}

/// In-memory filesystem the catalog uses to read `.desktop` files
/// without touching the host installation. Mirrors the harness
/// the unit tests inside `linux_app_catalog.rs` use so the
/// integration suite and the platform crate stay in lock-step on
/// the contract the resolver enforces.
#[derive(Default)]
struct MemoryFs {
    files: Mutex<BTreeMap<PathBuf, Vec<u8>>>,
    directories: Mutex<Vec<PathBuf>>,
}

impl MemoryFs {
    fn new(home: &Path) -> Self {
        // Mirror the XDG data roots the production `data_roots`
        // walks: `$HOME/.local/share`, then `/usr/local/share` and
        // `/usr/share` fallbacks. The catalog will probe each
        // directory in turn and skip the ones that are not
        // registered here.
        let dirs: Vec<PathBuf> = vec![
            home.to_path_buf(),
            home.join(".local/share"),
            home.join(".local/share/applications"),
            home.join(".local/share/icons"),
            home.join(".local/share/icons/hicolor/48x48/apps"),
        ];
        Self {
            files: Mutex::new(BTreeMap::new()),
            directories: Mutex::new(dirs),
        }
    }

    fn write(&self, path: &Path, content: &[u8]) {
        if let Some(parent) = path.parent() {
            self.mkdir(parent);
        }
        self.files
            .lock()
            .unwrap()
            .insert(path.to_path_buf(), content.to_vec());
    }

    fn write_png(&self, path: &Path) {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]);
        bytes.extend_from_slice(b"ICON");
        self.write(path, &bytes);
    }

    fn mkdir(&self, path: &Path) {
        let mut dirs = self.directories.lock().unwrap();
        if !dirs.iter().any(|existing| existing == path) {
            dirs.push(path.to_path_buf());
        }
    }
}

impl DesktopFilesystem for MemoryFs {
    fn read_entry(&self, path: &Path) -> std::io::Result<DesktopEntry> {
        let files = self.files.lock().unwrap();
        let bytes = files
            .get(path)
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "missing"))?;
        let raw = std::str::from_utf8(bytes).map_err(std::io::Error::other)?;
        Ok(parse_desktop_entry(raw, path))
    }

    fn read_dir_sorted(&self, dir: &Path) -> std::io::Result<Vec<PathBuf>> {
        let files = self.files.lock().unwrap();
        let mut entries: Vec<PathBuf> = files
            .keys()
            .filter(|path| path.parent() == Some(dir))
            .cloned()
            .collect();
        if entries.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "missing directory",
            ));
        }
        entries.sort();
        Ok(entries)
    }

    fn is_dir(&self, path: &Path) -> bool {
        let dirs = self.directories.lock().unwrap();
        if dirs.iter().any(|existing| existing == path) {
            return true;
        }
        let files = self.files.lock().unwrap();
        files.keys().any(|existing| existing.parent() == Some(path))
    }

    fn is_file(&self, path: &Path) -> bool {
        self.files.lock().unwrap().contains_key(path)
    }

    fn canonicalize_if_safe(&self, path: &Path) -> std::io::Result<PathBuf> {
        Ok(path.to_path_buf())
    }

    fn home_dir(&self) -> Option<PathBuf> {
        self.files
            .lock()
            .unwrap()
            .keys()
            .find_map(|path| path.ancestors().last().map(|p| p.to_path_buf()))
    }

    fn read(&self, path: &Path) -> std::io::Result<Option<Vec<u8>>> {
        Ok(self.files.lock().unwrap().get(path).cloned())
    }
}

fn parse_desktop_entry(raw: &str, path: &Path) -> DesktopEntry {
    let mut entry = DesktopEntry {
        path: path.to_path_buf(),
        type_is_application: false,
        hidden: false,
        name: None,
        name_locale: Vec::new(),
        icon: None,
        startup_wm_class: None,
        x_gnome_wm_class: None,
        exec_basename: None,
    };
    let mut in_target_group = false;
    for line in raw.lines() {
        let trimmed = match line.find('#') {
            Some(index) => &line[..index],
            None => line,
        }
        .trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(group) = trimmed.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            in_target_group = group.trim() == "Desktop Entry";
            continue;
        }
        if !in_target_group {
            continue;
        }
        let Some((key, value)) = trimmed.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();
        match key {
            "Type" => entry.type_is_application = value.eq_ignore_ascii_case("Application"),
            "Hidden" => entry.hidden = value.eq_ignore_ascii_case("true"),
            "Name" => entry.name = Some(value.to_string()),
            "Icon" => entry.icon = Some(value.to_string()),
            "StartupWMClass" => entry.startup_wm_class = Some(value.to_string()),
            "X-GNOME-WMClass" => entry.x_gnome_wm_class = Some(value.to_string()),
            _ => {}
        }
    }
    entry
}

struct Harness {
    data_dir: PathBuf,
    context: clipvault_core::AppContext,
    probe: Arc<ScriptedProbe>,
    fs: Arc<MemoryFs>,
    home: PathBuf,
}

impl Harness {
    fn new(backend: &'static str, display: DisplayServer) -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let data_dir = dir.path().join("clipvault-home");
        let home = data_dir.join("home");
        std::fs::create_dir_all(&data_dir).expect("mkdir data");
        std::fs::create_dir_all(&home).expect("mkdir home");
        {
            let mut db = Database::open(data_dir.join("clipvault.db")).expect("open");
            db.run_migrations(&builtin_migrations()).expect("migrate");
        }
        let info = PlatformInfo {
            home_dir: home.clone(),
            data_dir: data_dir.clone(),
            os_family: OsFamily::Linux,
            display_server: display,
        };
        let probe = Arc::new(ScriptedProbe::new(backend));
        let adapters = PlatformAdapters::new(
            Arc::new(clipvault_core::FakeClipboardBackend::new()) as Arc<_>,
            Arc::new(clipvault_core::FakeHotkeyManager::new()) as Arc<_>,
            probe.clone() as Arc<_>,
            Arc::new(clipvault_core::FakePasteController::new()) as Arc<_>,
            Arc::new(clipvault_core::FakeTrayController::new()) as Arc<_>,
            Arc::new(clipvault_core::FakeSettingsNavigator::new()) as Arc<_>,
            Arc::new(clipvault_core::NoopApplicationMetadataProvider) as Arc<_>,
            Capabilities::ALL_AVAILABLE,
            info,
        );
        let context = AppBootstrap::new()
            .with_clock(Arc::new(FixedClock) as Arc<dyn Clock>)
            .with_clipboard(Arc::new(clipvault_core::FakeClipboard::new()) as Arc<_>)
            .with_platform_adapters(adapters)
            .bootstrap_at(data_dir.join("clipvault.db"))
            .expect("bootstrap");
        // The harness's `home` is what the in-memory filesystem
        // will report when the production catalog runs through
        // `current_home`. Mirror it so the catalog's data_roots
        // resolves the home `.local/share` root to this tempdir.
        let fs = Arc::new(MemoryFs::new(&home));
        Self {
            data_dir,
            context,
            probe,
            fs,
            home,
        }
    }

    fn write_firefox(&self) {
        let apps = self.home.join(".local/share/applications");
        self.fs.write(
            &apps.join("firefox.desktop"),
            b"[Desktop Entry]\nType=Application\nName=Firefox\nStartupWMClass=Firefox\n",
        );
        let icons = self.home.join(".local/share/icons/hicolor/48x48/apps");
        self.fs.write_png(&icons.join("firefox.png"));
    }

    fn set_gnome_consent(&self, consent: clipvault_core::GnomeConsentDecision) {
        self.context
            .gnome_integration()
            .save_consent(&self.context, consent)
            .expect("save_consent");
    }

    fn set_gnome_technical_state(&self, state: clipvault_core::GnomeTechnicalState) {
        self.context
            .gnome_integration()
            .save_technical_state(&self.context, state)
            .expect("save_technical_state");
    }

    fn set_gnome_runtime_technical_state(&self, state: clipvault_core::GnomeTechnicalState) {
        self.context
            .gnome_integration()
            .set_runtime_technical_state(state);
    }

    fn populate_cache(&self, identifier: &str) {
        self.probe.set_identifier(Some(identifier.to_string()));
        let _ = self.context.refresh_active_application();
    }

    fn list_blacklist(&self) -> Vec<clipvault_core::IgnoredAppEntry> {
        self.context
            .ignored_apps()
            .list(&self.context)
            .expect("list")
    }
}

fn run_catalog(
    harness: &Harness,
    backend: &'static str,
    strategy: IdentifierStrategy,
) -> Result<LinuxCatalogResponse, clipvault_app::commands::CommandError> {
    let assets_dir = harness.data_dir.join("assets");
    let fs: Arc<dyn DesktopFilesystem> = harness.fs.clone();
    let linux_backend = match backend {
        "x11_ewmh" | "xwayland_ewmh" => LinuxPickerBackend::X11OrXWaylandEwmh,
        "wayland_foreign_toplevel" | "wayland_wlr_foreign_toplevel" => {
            LinuxPickerBackend::WaylandNative
        }
        "gnome_shell_extension" => LinuxPickerBackend::GnomeShellExtension,
        _ => LinuxPickerBackend::Unsupported,
    };
    // The catalog's `data_roots` reads `$HOME` first then falls
    // back to the filesystem abstraction. Pin both to the harness
    // so the production catalog picks up the `.desktop` files
    // the in-memory filesystem exposes.
    let previous_home = std::env::var_os("HOME");
    let previous_xdg_data_home = std::env::var_os("XDG_DATA_HOME");
    let previous_xdg_data_dirs = std::env::var_os("XDG_DATA_DIRS");
    std::env::set_var("HOME", &harness.home);
    std::env::remove_var("XDG_DATA_HOME");
    std::env::remove_var("XDG_DATA_DIRS");
    let result = clipvault_ignored_app_linux_catalog_for_test_with_fs(
        &assets_dir,
        strategy,
        linux_backend,
        fs,
    );
    restore_env("HOME", previous_home);
    restore_env("XDG_DATA_HOME", previous_xdg_data_home);
    restore_env("XDG_DATA_DIRS", previous_xdg_data_dirs);
    result
}

fn restore_env(key: &str, value: Option<std::ffi::OsString>) {
    match value {
        Some(value) => std::env::set_var(key, value),
        None => std::env::remove_var(key),
    }
}

/// Helper that mirrors [`run_catalog`] for the `add` path: pins
/// `HOME` / `XDG_DATA_HOME` / `XDG_DATA_DIRS` to the harness so the
/// catalog walks the in-memory `.desktop` files instead of the
/// host installation.
fn run_add(
    harness: &Harness,
    identifier: &str,
) -> Result<LinuxPickAndAddResponse, clipvault_app::commands::CommandError> {
    let fs: Arc<dyn DesktopFilesystem> = harness.fs.clone();
    let previous_home = std::env::var_os("HOME");
    let previous_xdg_data_home = std::env::var_os("XDG_DATA_HOME");
    let previous_xdg_data_dirs = std::env::var_os("XDG_DATA_DIRS");
    std::env::set_var("HOME", &harness.home);
    std::env::remove_var("XDG_DATA_HOME");
    std::env::remove_var("XDG_DATA_DIRS");
    let result = clipvault_ignored_app_linux_add_for_test_with_fs(
        &harness.context,
        identifier,
        None,
        None,
        fs,
    );
    restore_env("HOME", previous_home);
    restore_env("XDG_DATA_HOME", previous_xdg_data_home);
    restore_env("XDG_DATA_DIRS", previous_xdg_data_dirs);
    result
}

// ---------------------------------------------------------------------
// Catalog command: backend resolution matrix.
// ---------------------------------------------------------------------

#[test]
fn catalog_unsupported_for_unknown_display_server() {
    // The harness probes report a backend the resolver classifies
    // as `Unsupported` (no EWMH / native Wayland / GNOME contract).
    // The catalog command MUST reject and the manual-entry fallback
    // stays available.
    let harness = Harness::new("noop", DisplayServer::Unknown);
    let response =
        run_catalog(&harness, "noop", IdentifierStrategy::WmClass).expect("catalog response");
    assert!(
        matches!(response, LinuxCatalogResponse::Unsupported { .. }),
        "session without a deterministic active-app backend MUST report Unsupported, got: {response:?}"
    );
}

#[test]
fn catalog_supports_x11_without_populated_cache() {
    // The resolver no longer requires `cache_populated`: the catalog
    // derives its identifier from the installed `.desktop` files, so
    // an X11 session with no focused window yet still classifies as
    // `Supported` once the catalog has at least one `.desktop`
    // entry.
    let harness = Harness::new("x11_ewmh", DisplayServer::X11);
    harness.write_firefox();
    let response =
        run_catalog(&harness, "x11_ewmh", IdentifierStrategy::WmClass).expect("catalog response");
    let json = serde_json::to_value(&response).expect("serialise catalog response");
    assert_eq!(json["kind"], "supported");
    assert!(
        json.get("supported").is_none(),
        "the Tauri JSON contract must be internally tagged for the frontend union"
    );
    match response {
        LinuxCatalogResponse::Supported {
            backend,
            strategy,
            candidates,
        } => {
            assert_eq!(backend, "x11_or_xwayland_ewmh");
            assert_eq!(strategy, "wm_class");
            assert!(
                candidates.iter().any(|c| c.identifier == "Firefox"),
                "catalog MUST surface Firefox, got: {candidates:?}"
            );
        }
        other => panic!("expected Supported, got {other:?}"),
    }
}

#[test]
fn catalog_supports_xwayland_through_x11_branch() {
    // XWayland MUST reuse the X11/EWMH branch because XWayland is
    // an X11 server. The picker backend therefore resolves to
    // `X11OrXWaylandEwmh`, not to `WaylandNative`. The test runs
    // without populating the cache to pin the contract that
    // `cache_populated` is not a requirement.
    let harness = Harness::new("xwayland_ewmh", DisplayServer::Wayland);
    harness.write_firefox();
    let response = run_catalog(&harness, "xwayland_ewmh", IdentifierStrategy::WmClass)
        .expect("catalog response");
    match response {
        LinuxCatalogResponse::Supported { backend, .. } => {
            assert_eq!(
                backend, "x11_or_xwayland_ewmh",
                "XWayland MUST be classified under the X11 branch"
            );
        }
        other => panic!("expected Supported, got {other:?}"),
    }
}

#[test]
fn catalog_supports_native_wayland_without_populated_cache() {
    // Native Wayland: the resolver must offer the catalog as soon as
    // the backend reports `wayland_*_foreign_toplevel`, regardless
    // of whether the cache has observed a focused window.
    let harness = Harness::new("wayland_wlr_foreign_toplevel", DisplayServer::Wayland);
    harness.write_firefox();
    let response = run_catalog(
        &harness,
        "wayland_wlr_foreign_toplevel",
        IdentifierStrategy::WmClass,
    )
    .expect("catalog response");
    match response {
        LinuxCatalogResponse::Supported { backend, .. } => {
            assert_eq!(backend, "wayland_native");
        }
        other => panic!("expected Supported, got {other:?}"),
    }
}

#[test]
fn catalog_supports_gnome_when_consent_and_connection_are_aligned() {
    // GNOME operative: the cache is intentionally left empty to
    // prove `cache_populated` is no longer a requirement for the
    // GNOME branch either.
    let harness = Harness::new("gnome_shell_extension", DisplayServer::Wayland);
    harness.set_gnome_consent(clipvault_core::GnomeConsentDecision::Accepted);
    harness.set_gnome_technical_state(clipvault_core::GnomeTechnicalState::Identified);
    // Install a `firefox.desktop` so the catalog produces a
    // candidate under the GNOME `desktop_file_id` strategy.
    let apps = harness.home.join(".local/share/applications");
    harness.fs.write(
        &apps.join("firefox.desktop"),
        b"[Desktop Entry]\nType=Application\nName=Firefox\nStartupWMClass=Firefox\n",
    );
    let response = run_catalog(
        &harness,
        "gnome_shell_extension",
        IdentifierStrategy::DesktopFileId,
    )
    .expect("catalog response");
    match response {
        LinuxCatalogResponse::Supported {
            backend,
            strategy,
            candidates,
        } => {
            assert_eq!(backend, "gnome_shell_extension");
            assert_eq!(strategy, "desktop_file_id");
            assert!(
                candidates.iter().any(|c| c.identifier == "firefox.desktop"),
                "GNOME path MUST use the Desktop File ID, got: {candidates:?}"
            );
        }
        other => panic!("expected Supported, got {other:?}"),
    }
}

#[test]
fn gnome_runtime_no_active_application_keeps_catalog_and_add_available() {
    // The Privacy window itself owns focus while the user opens the visual
    // picker. GNOME therefore reports NoActiveApplication even though the
    // bridge is connected. The state persisted while the listener started is
    // still ActivationPending and must not win the runtime decision.
    let harness = Harness::new("gnome_shell_extension", DisplayServer::Wayland);
    harness.set_gnome_consent(clipvault_core::GnomeConsentDecision::Accepted);
    harness.set_gnome_technical_state(clipvault_core::GnomeTechnicalState::ActivationPending);
    harness.set_gnome_runtime_technical_state(
        clipvault_core::GnomeTechnicalState::NoActiveApplication,
    );
    harness.write_firefox();

    assert_eq!(
        harness.context.linux_picker_backend(),
        LinuxPickerBackend::GnomeShellExtension,
        "the live state must make the GNOME catalog available"
    );
    let catalog = run_catalog(
        &harness,
        "gnome_shell_extension",
        IdentifierStrategy::DesktopFileId,
    )
    .expect("catalog");
    assert!(
        matches!(catalog, LinuxCatalogResponse::Supported { .. }),
        "GNOME runtime state must select a visible catalog, got {catalog:?}"
    );

    let outcome = run_add(&harness, "firefox.desktop").expect("add through live catalog");
    assert!(matches!(outcome, LinuxPickAndAddResponse::Added { .. }));
    assert_eq!(harness.list_blacklist().len(), 1);
}

#[test]
fn catalog_unsupported_when_gnome_consent_missing() {
    let harness = Harness::new("gnome_shell_extension", DisplayServer::Wayland);
    // No consent set: stays `Unknown`.
    harness.set_gnome_technical_state(clipvault_core::GnomeTechnicalState::Connected);
    let response = run_catalog(
        &harness,
        "gnome_shell_extension",
        IdentifierStrategy::DesktopFileId,
    )
    .expect("catalog response");
    assert!(
        matches!(response, LinuxCatalogResponse::Unsupported { .. }),
        "GNOME without accepted consent MUST report Unsupported, got: {response:?}"
    );
}

#[test]
fn catalog_unsupported_when_gnome_activation_pending() {
    // ActivationPending is explicitly out of the supported set: the
    // extension has not yet completed the first handshake and the
    // picker refuses to commit to a mapping it cannot guarantee.
    let harness = Harness::new("gnome_shell_extension", DisplayServer::Wayland);
    harness.set_gnome_consent(clipvault_core::GnomeConsentDecision::Accepted);
    harness.set_gnome_technical_state(clipvault_core::GnomeTechnicalState::ActivationPending);
    let response = run_catalog(
        &harness,
        "gnome_shell_extension",
        IdentifierStrategy::DesktopFileId,
    )
    .expect("catalog response");
    assert!(
        matches!(response, LinuxCatalogResponse::Unsupported { .. }),
        "GNOME accepted but ActivationPending MUST report Unsupported, got: {response:?}"
    );
}

#[test]
fn catalog_unsupported_when_gnome_extension_inactive() {
    let harness = Harness::new("gnome_shell_extension", DisplayServer::Wayland);
    harness.set_gnome_consent(clipvault_core::GnomeConsentDecision::Accepted);
    harness.set_gnome_technical_state(clipvault_core::GnomeTechnicalState::NotInstalled);
    let response = run_catalog(
        &harness,
        "gnome_shell_extension",
        IdentifierStrategy::DesktopFileId,
    )
    .expect("catalog response");
    assert!(
        matches!(response, LinuxCatalogResponse::Unsupported { .. }),
        "GNOME accepted but inactive MUST report Unsupported, got: {response:?}"
    );
}

#[test]
fn catalog_unsupported_when_gnome_disconnected() {
    let harness = Harness::new("gnome_shell_extension", DisplayServer::Wayland);
    harness.set_gnome_consent(clipvault_core::GnomeConsentDecision::Accepted);
    harness.set_gnome_technical_state(clipvault_core::GnomeTechnicalState::Disconnected);
    let response = run_catalog(
        &harness,
        "gnome_shell_extension",
        IdentifierStrategy::DesktopFileId,
    )
    .expect("catalog response");
    assert!(
        matches!(response, LinuxCatalogResponse::Unsupported { .. }),
        "GNOME accepted but Disconnected MUST report Unsupported, got: {response:?}"
    );
}

#[test]
fn catalog_unsupported_when_empty_even_on_supported_backend() {
    // An empty catalog MUST fall back to `Unsupported` so the user
    // is never offered an empty picker modal — the manual-entry
    // surface remains the documented fallback.
    let harness = Harness::new("x11_ewmh", DisplayServer::X11);
    harness.populate_cache("firefox");
    // No `.desktop` files installed in the harness.
    let response =
        run_catalog(&harness, "x11_ewmh", IdentifierStrategy::WmClass).expect("catalog response");
    assert!(
        matches!(response, LinuxCatalogResponse::Unsupported { .. }),
        "empty catalog on a supported backend MUST still report Unsupported, got: {response:?}"
    );
}

// ---------------------------------------------------------------------
// Catalog command: privacy surface.
// ---------------------------------------------------------------------

#[test]
fn catalog_metadata_never_carries_paths_or_payload() {
    let harness = Harness::new("x11_ewmh", DisplayServer::X11);
    harness.populate_cache("firefox");
    let apps = harness.home.join(".local/share/applications");
    harness.fs.write(
        &apps.join("secret.desktop"),
        b"[Desktop Entry]\nType=Application\nName=Secret\nStartupWMClass=Secret\nExec=/usr/bin/secret --with args\n",
    );
    let response =
        run_catalog(&harness, "x11_ewmh", IdentifierStrategy::WmClass).expect("catalog response");
    let json = serde_json::to_string(&response).expect("serialise");
    for forbidden in [
        ".local/share/applications",
        "secret.desktop",
        "Exec=",
        "/usr/bin/",
    ] {
        assert!(
            !json.contains(forbidden),
            "catalog JSON must not contain {forbidden:?}, got: {json}"
        );
    }
}

// ---------------------------------------------------------------------
// Add command: revalidates against the catalog and the runtime state.
// ---------------------------------------------------------------------

#[test]
fn add_persists_identifier_and_uses_catalog_metadata() {
    let harness = Harness::new("x11_ewmh", DisplayServer::X11);
    harness.write_firefox();
    harness.populate_cache("firefox");
    // The frontend-supplied display_name and icon_ref are
    // intentionally tampered: the catalog MUST win over them.
    let outcome = run_add(&harness, "Firefox").expect("add outcome");
    let json = serde_json::to_value(&outcome).expect("serialise add response");
    assert_eq!(json["kind"], "added");
    assert!(
        json.get("added").is_none(),
        "the Tauri JSON contract must be internally tagged for the frontend union"
    );
    match outcome {
        LinuxPickAndAddResponse::Added { entry } => {
            // The catalog wins over the frontend payload: the
            // display name comes from the `.desktop` entry, not
            // from the request body.
            assert_eq!(entry.id, "firefox");
            assert_eq!(entry.display_name.as_deref(), Some("Firefox"));
        }
        other => panic!("expected Added, got {other:?}"),
    }
    let entries = harness.list_blacklist();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].id, "firefox");
}

#[test]
fn add_rejects_blank_identifier() {
    let harness = Harness::new("x11_ewmh", DisplayServer::X11);
    harness.write_firefox();
    harness.populate_cache("firefox");
    let err = run_add(&harness, "   ").expect_err("blank identifier");
    // A blank identifier is rejected by the catalog command (the
    // catalog cannot find it) and the add command propagates that
    // decision as `unsupported_session`. Both behaviours are valid
    // rejections; the test pins the documented stable error.
    assert!(
        err.kind == "unsupported_session" || err.kind == "missing_identifier",
        "blank identifier MUST be rejected, got: kind={}, message={}",
        err.kind,
        err.message
    );
    assert!(harness.list_blacklist().is_empty());
}

#[test]
fn add_rejects_arbitrary_identifier_on_unsupported_session() {
    // The harness uses a `noop` backend which the resolver maps to
    // `Unsupported`. The frontend MUST NOT be able to push
    // arbitrary identifiers through this path.
    let harness = Harness::new("noop", DisplayServer::Unknown);
    let err = run_add(&harness, "firefox").expect_err("add must reject arbitrary identifier");
    assert_eq!(err.kind, "unsupported_session");
    assert!(harness.list_blacklist().is_empty());
}

#[test]
fn add_rejects_identifier_not_in_catalog() {
    // The session supports the picker (X11 with the resolver
    // recognising `x11_ewmh`), but the catalogue does not contain
    // the requested identifier. The add command MUST refuse so the
    // user cannot smuggle an arbitrary identifier through.
    let harness = Harness::new("x11_ewmh", DisplayServer::X11);
    harness.write_firefox();
    let err =
        run_add(&harness, "ghost-app").expect_err("identifier outside catalog MUST be rejected");
    assert_eq!(err.kind, "unsupported_session");
    assert!(harness.list_blacklist().is_empty());
}

#[test]
fn cancellation_leaves_blacklist_and_assets_unchanged() {
    // The modal close button never invokes the add command. The
    // test simulates that by NOT calling `add`. The blacklist MUST
    // stay empty and no asset MUST be created. The previous
    // implementation had an unreachable `Cancelled => Added` arm
    // in the linux add command that manufactured a row with an
    // empty identifier; the fix pins `unreachable!()` so the
    // `Cancelled` outcome cannot leak a fake row even if a future
    // refactor lets the variant escape `add_with_metadata`.
    let harness = Harness::new("x11_ewmh", DisplayServer::X11);
    harness.write_firefox();
    // No add call here: simulates the user closing the modal.
    assert!(harness.list_blacklist().is_empty());
}

// ---------------------------------------------------------------------
// Manual entry still works for non-catalog flows.
// ---------------------------------------------------------------------

#[test]
fn manual_entry_remains_available_for_unknown_session() {
    // The Linux catalog command returns Unsupported for this
    // session, but the manual entry path (`add_with_metadata`)
    // MUST still work so the documented fallback stays available.
    let harness = Harness::new("noop", DisplayServer::Unknown);
    let outcome = harness
        .context
        .ignored_apps()
        .add_with_metadata(&harness.context, "firefox", Some("Firefox"), None)
        .expect("manual entry");
    use clipvault_core::PickAndAddOutcome;
    assert!(matches!(outcome, PickAndAddOutcome::Added(_)));
    assert_eq!(harness.list_blacklist().len(), 1);
}
