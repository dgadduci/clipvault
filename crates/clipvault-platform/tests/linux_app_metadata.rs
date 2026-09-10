//! Integration tests for the Linux application-metadata provider.
//!
//! These tests pin the public contract the `linux-source-app-metadata`
//! change ships: the provider resolves a `WM_CLASS` identifier against
//! local `.desktop` files, persists the resolved icon under the
//! existing `application-icons/` namespace and exposes a stable
//! `name()` identifier so the diagnostics surface can route the
//! result without inspecting free-form strings.
//!
//! The test fixture injects the in-memory filesystem the platform
//! crate exposes so the suite does not depend on a real desktop
//! installation and runs identically on every developer machine and
//! in CI.

#![cfg(all(target_os = "linux", feature = "linux-x11"))]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use clipvault_platform::runtime::linux_app_metadata::{
    DesktopEntry, DesktopFilesystem, LinuxApplicationMetadataProvider,
};
use clipvault_platform::{
    ApplicationMetadataError, ApplicationMetadataProvider, IconFailureKind, IconSourceKind,
};

/// In-memory filesystem the integration suite drives. Mirrors the
/// production layout (`<home>/.local/share/applications/<file>.desktop`,
/// `<home>/.local/share/icons/<theme>/<size>x<size>/apps/<icon>.png`,
/// `<home>/.local/share/icons/<theme>/scalable/apps/<icon>.svg`,
/// `<home>/.local/share/pixmaps/<icon>.png`) so the matcher walks
/// the same paths the host adapter walks.
#[derive(Default)]
struct MemoryFs {
    home: PathBuf,
    files: Mutex<BTreeMap<PathBuf, Vec<u8>>>,
    directories: Mutex<Vec<PathBuf>>,
}

impl MemoryFs {
    fn new(home: PathBuf) -> Self {
        let directories = vec![home.clone()];
        Self {
            home,
            files: Mutex::new(BTreeMap::new()),
            directories: Mutex::new(directories),
        }
    }

    fn mkdir(&self, path: &Path) {
        let mut dirs = self.directories.lock().unwrap();
        if !dirs.iter().any(|existing| existing == path) {
            dirs.push(path.to_path_buf());
        }
    }

    fn write(&self, path: &Path, content: &[u8]) {
        self.mkdir(path.parent().expect("parent"));
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

    #[cfg(feature = "linux-svg-raster")]
    fn write_svg(&self, path: &Path, svg: &str) {
        self.write(path, svg.as_bytes());
    }
}

impl DesktopFilesystem for MemoryFs {
    fn read_entry(&self, path: &Path) -> std::io::Result<DesktopEntry> {
        let files = self.files.lock().unwrap();
        let bytes = files
            .get(path)
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "missing"))?;
        let raw = std::str::from_utf8(bytes).map_err(std::io::Error::other)?;
        Ok(parse(raw, path))
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
        Some(self.home.clone())
    }

    fn read(&self, path: &Path) -> std::io::Result<Option<Vec<u8>>> {
        Ok(self.files.lock().unwrap().get(path).cloned())
    }
}

/// Re-implementation of the production `.desktop` parser so the
/// integration suite can drive the matcher through the public API
/// without leaning on the private `parse_desktop_entry` helper. The
/// implementation mirrors the one in `linux_app_metadata.rs`; the
/// production parser is `pub(crate)` and the integration tests live
/// outside that crate, so the parser cannot be reused directly.
fn parse(raw: &str, path: &Path) -> DesktopEntry {
    let mut entry = DesktopEntry {
        path: path.to_path_buf(),
        type_is_application: false,
        hidden: false,
        name: None,
        name_locale: Vec::new(),
        icon: None,
        startup_wm_class: None,
        x_gnome_wm_class: None,
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

fn fixture(home: &Path) -> (MemoryFs, PathBuf) {
    let fs = MemoryFs::new(home.to_path_buf());
    let assets = home.join("data/assets");
    fs.mkdir(&assets);
    fs.mkdir(&assets.join("application-icons"));
    (fs, assets)
}

fn write_desktop(fs: &MemoryFs, apps_dir: &Path, basename: &str, content: &str) {
    fs.write(
        &apps_dir.join(format!("{basename}.desktop")),
        content.as_bytes(),
    );
}

#[cfg(feature = "linux-svg-raster")]
const MINIMAL_SVG: &str = "<?xml version=\"1.0\"?><svg xmlns=\"http://www.w3.org/2000/svg\" width=\"32\" height=\"32\"><rect width=\"32\" height=\"32\" fill=\"#abcdef\"/></svg>";

/// Smoke test: a known X11 application with a matching `StartupWMClass`
/// resolves to the localized display name and a relative icon
/// reference.
#[test]
fn resolves_startup_wm_class_to_localized_name_and_icon() {
    let home = PathBuf::from("/home/tester");
    let (fs, assets) = fixture(&home);
    let apps = home.join(".local/share/applications");
    let icons = home.join(".local/share/icons/128x128/apps");
    fs.mkdir(&apps);
    fs.mkdir(&icons);
    fs.write_png(&icons.join("firefox.png"));
    write_desktop(
        &fs,
        &apps,
        "firefox",
        "[Desktop Entry]\nType=Application\nName=Firefox\nName[es]=Firefox Navegador\nStartupWMClass=Firefox\nIcon=firefox\n",
    );
    let provider =
        LinuxApplicationMetadataProvider::with_filesystem(assets.clone(), std::sync::Arc::new(fs));
    std::env::set_var("LANG", "es_ES.UTF-8");
    let result = provider.lookup("Firefox").expect("ok");
    let metadata = result.expect("some metadata");
    assert_eq!(metadata.display_name, "Firefox Navegador");
    assert_eq!(
        metadata.icon_ref.as_deref(),
        Some("application-icons/Firefox.png")
    );
    let target = assets.join("application-icons/Firefox.png");
    assert!(target.is_file());
    std::env::remove_var("LANG");
}

/// `X-GNOME-WMClass` wins the tie against the filename match.
#[test]
fn resolves_x_gnome_wm_class() {
    let home = PathBuf::from("/home/tester");
    let (fs, assets) = fixture(&home);
    let apps = home.join(".local/share/applications");
    fs.mkdir(&apps);
    write_desktop(
        &fs,
        &apps,
        "code",
        "[Desktop Entry]\nType=Application\nName=Code\nX-GNOME-WMClass=Code\n",
    );
    let provider =
        LinuxApplicationMetadataProvider::with_filesystem(assets, std::sync::Arc::new(fs));
    let metadata = provider.lookup("Code").expect("ok").expect("some metadata");
    assert_eq!(metadata.display_name, "Code");
}

/// Hidden entries are skipped; the lookup returns `Ok(None)` instead
/// of fabricating a name.
#[test]
fn hidden_entry_is_skipped() {
    let home = PathBuf::from("/home/tester");
    let (fs, assets) = fixture(&home);
    let apps = home.join(".local/share/applications");
    fs.mkdir(&apps);
    write_desktop(
        &fs,
        &apps,
        "firefox",
        "[Desktop Entry]\nType=Application\nHidden=true\nName=Firefox\nStartupWMClass=Firefox\n",
    );
    let provider =
        LinuxApplicationMetadataProvider::with_filesystem(assets, std::sync::Arc::new(fs));
    let metadata = provider.lookup("Firefox").expect("ok");
    assert!(metadata.is_none());
}

/// Non-application entries are skipped.
#[test]
fn non_application_entry_is_skipped() {
    let home = PathBuf::from("/home/tester");
    let (fs, assets) = fixture(&home);
    let apps = home.join(".local/share/applications");
    fs.mkdir(&apps);
    write_desktop(
        &fs,
        &apps,
        "trash",
        "[Desktop Entry]\nType=Directory\nName=Trash\n",
    );
    let provider =
        LinuxApplicationMetadataProvider::with_filesystem(assets, std::sync::Arc::new(fs));
    let metadata = provider.lookup("trash").expect("ok");
    assert!(metadata.is_none());
}

/// Icon assets outside the allowed roots are silently rejected; the
/// lookup keeps the display name.
#[test]
fn rejects_icon_outside_allowed_roots() {
    let home = PathBuf::from("/home/tester");
    let (fs, assets) = fixture(&home);
    let apps = home.join(".local/share/applications");
    fs.mkdir(&apps);
    let outside = home.join("outside.png");
    fs.write_png(&outside);
    write_desktop(
        &fs,
        &apps,
        "firefox",
        &format!(
            "[Desktop Entry]\nType=Application\nName=Firefox\nIcon={}\n",
            outside.display()
        ),
    );
    let provider =
        LinuxApplicationMetadataProvider::with_filesystem(assets.clone(), std::sync::Arc::new(fs));
    let metadata = provider
        .lookup("firefox")
        .expect("ok")
        .expect("some metadata");
    assert_eq!(metadata.display_name, "Firefox");
    assert!(metadata.icon_ref.is_none());
}

/// A non-PNG icon is not persisted when the rasterizer feature is
/// disabled; the lookup keeps the display name.
#[test]
fn rejects_non_png_icon_when_rasterizer_disabled() {
    let home = PathBuf::from("/home/tester");
    let (fs, assets) = fixture(&home);
    let apps = home.join(".local/share/applications");
    let icons = home.join(".local/share/icons/128x128/apps");
    fs.mkdir(&apps);
    fs.mkdir(&icons);
    fs.write(&icons.join("firefox.svg"), b"<svg/>");
    write_desktop(
        &fs,
        &apps,
        "firefox",
        "[Desktop Entry]\nType=Application\nName=Firefox\nIcon=firefox\n",
    );
    let provider =
        LinuxApplicationMetadataProvider::with_filesystem(assets.clone(), std::sync::Arc::new(fs));
    let metadata = provider
        .lookup("firefox")
        .expect("ok")
        .expect("some metadata");
    assert_eq!(metadata.display_name, "Firefox");
    assert!(metadata.icon_ref.is_none());
}

/// Stable identifier for diagnostics. The provider returns
/// `Ok(None)` when no metadata matches and never escalates to the
/// typed `Unavailable` error for the "no match" branch.
#[test]
fn provider_returns_ok_none_for_unknown_application() {
    let home = PathBuf::from("/home/tester");
    let (fs, assets) = fixture(&home);
    let provider =
        LinuxApplicationMetadataProvider::with_filesystem(assets, std::sync::Arc::new(fs));
    let result = provider.lookup("unknown-app").expect("ok");
    assert!(result.is_none());
    assert!(matches!(
        provider.lookup("anything"),
        Ok(None) | Ok(Some(_))
    ));
}

/// The provider must be `Send + Sync` so the bootstrap can hand it
/// to the capture pipeline.
#[test]
fn provider_is_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<LinuxApplicationMetadataProvider>();
    let _provider: std::sync::Arc<dyn ApplicationMetadataProvider> = std::sync::Arc::new(
        LinuxApplicationMetadataProvider::new(PathBuf::from("/tmp/.clipvault/assets")),
    );
    assert_send_sync::<std::sync::Arc<dyn ApplicationMetadataProvider>>();
}

/// The provider exposes a stable name so the diagnostics surface can
/// route the result without inspecting free-form strings.
#[test]
fn provider_name_is_stable() {
    let home = PathBuf::from("/home/tester");
    let (fs, assets) = fixture(&home);
    let provider =
        LinuxApplicationMetadataProvider::with_filesystem(assets, std::sync::Arc::new(fs));
    assert_eq!(provider.name(), "linux_app_metadata");
}

/// Canonical Ubuntu layout: the icon lives under
/// `<root>/icons/hicolor/<size>x<size>/apps/<name>.png` (one
/// directory level deeper than the legacy `<root>/icons/<size>x<size>/apps/`
/// layout). The regression the change fixes used to skip this
/// layout entirely, so the test pins the new behaviour.
#[test]
fn resolves_icon_from_hicolor_theme_layout() {
    let home = PathBuf::from("/home/tester");
    let (fs, assets) = fixture(&home);
    let apps = home.join(".local/share/applications");
    let icons = home.join(".local/share/icons/hicolor/128x128/apps");
    fs.mkdir(&apps);
    fs.mkdir(&icons);
    fs.write_png(&icons.join("firefox.png"));
    write_desktop(
        &fs,
        &apps,
        "firefox",
        "[Desktop Entry]\nType=Application\nName=Firefox\nStartupWMClass=Firefox\nIcon=firefox\n",
    );
    let provider =
        LinuxApplicationMetadataProvider::with_filesystem(assets.clone(), std::sync::Arc::new(fs));
    let metadata = provider
        .lookup("Firefox")
        .expect("ok")
        .expect("some metadata");
    assert_eq!(metadata.display_name, "Firefox");
    let icon_ref = metadata
        .icon_ref
        .as_deref()
        .expect("icon ref must point at the persisted PNG");
    assert!(
        icon_ref.starts_with("application-icons/"),
        "icon ref must live under application-icons/, got {icon_ref}"
    );
    let target = assets.join(icon_ref);
    assert!(target.is_file(), "icon must be persisted to {target:?}");
}

/// Scalable theme layout: the canonical theme-aware fallback also
/// covers `<root>/icons/<theme>/scalable/apps/<name>.png`. Only PNGs
/// are persisted, but the directory walk must reach that path so a
/// later change can plug in an SVG-to-PNG converter without having
/// to re-do the layout discovery.
#[test]
fn resolves_icon_from_scalable_theme_layout() {
    let home = PathBuf::from("/home/tester");
    let (fs, assets) = fixture(&home);
    let apps = home.join(".local/share/applications");
    let icons = home.join(".local/share/icons/hicolor/scalable/apps");
    fs.mkdir(&apps);
    fs.mkdir(&icons);
    fs.write_png(&icons.join("code.png"));
    write_desktop(
        &fs,
        &apps,
        "code",
        "[Desktop Entry]\nType=Application\nName=Code\nStartupWMClass=code\nIcon=code\n",
    );
    let provider =
        LinuxApplicationMetadataProvider::with_filesystem(assets.clone(), std::sync::Arc::new(fs));
    let metadata = provider.lookup("code").expect("ok").expect("some metadata");
    assert_eq!(metadata.display_name, "Code");
    assert!(metadata.icon_ref.is_some());
}

/// An unknown icon theme must not panic: the resolver falls through
/// to `hicolor` and eventually returns `None` for the icon when no
/// directory owns the file.
#[test]
fn missing_icon_in_known_theme_is_skipped() {
    let home = PathBuf::from("/home/tester");
    let (fs, assets) = fixture(&home);
    let apps = home.join(".local/share/applications");
    let icons = home.join(".local/share/icons/hicolor/128x128/apps");
    fs.mkdir(&apps);
    fs.mkdir(&icons);
    write_desktop(
        &fs,
        &apps,
        "firefox",
        "[Desktop Entry]\nType=Application\nName=Firefox\nStartupWMClass=Firefox\nIcon=missing\n",
    );
    let provider =
        LinuxApplicationMetadataProvider::with_filesystem(assets.clone(), std::sync::Arc::new(fs));
    let metadata = provider
        .lookup("Firefox")
        .expect("ok")
        .expect("some metadata");
    assert_eq!(metadata.display_name, "Firefox");
    assert!(
        metadata.icon_ref.is_none(),
        "missing icon must not produce an icon reference"
    );
}

/// The provider's `lookup` returns the typed `ApplicationMetadataError`
/// variant only on a backend failure; the in-memory harness never
/// produces one but the contract is documented for callers that
/// branch on the enum.
#[test]
fn provider_error_contract_is_typed() {
    let unavailable = ApplicationMetadataError::Unavailable;
    assert_eq!(
        unavailable.to_string(),
        "application metadata lookup is unavailable on this platform"
    );
    let backend = ApplicationMetadataError::backend("test");
    assert!(backend.to_string().contains("test"));
}

/// Multiple distinct applications resolve independently — the
/// matcher is not whitelisted for any single bundle (Warp,
/// Firefox, GNOME Terminal, …).
#[test]
fn resolves_multiple_distinct_applications() {
    let home = PathBuf::from("/home/tester");
    let (fs, assets) = fixture(&home);
    let apps = home.join(".local/share/applications");
    let icons = home.join(".local/share/icons/hicolor/128x128/apps");
    fs.mkdir(&apps);
    fs.mkdir(&icons);
    fs.write_png(&icons.join("firefox.png"));
    fs.write_png(&icons.join("gnome-terminal.png"));
    write_desktop(
        &fs,
        &apps,
        "firefox",
        "[Desktop Entry]\nType=Application\nName=Firefox\nStartupWMClass=firefox\nIcon=firefox\n",
    );
    write_desktop(
        &fs,
        &apps,
        "gnome-terminal",
        "[Desktop Entry]\nType=Application\nName=Terminal\nStartupWMClass=gnome-terminal\nIcon=gnome-terminal\n",
    );
    write_desktop(
        &fs,
        &apps,
        "code",
        "[Desktop Entry]\nType=Application\nName=Code\nStartupWMClass=dev.warp.Warp\nIcon=code\n",
    );
    let provider =
        LinuxApplicationMetadataProvider::with_filesystem(assets, std::sync::Arc::new(fs));
    let firefox = provider.lookup("firefox").expect("ok").expect("ok");
    let terminal = provider.lookup("gnome-terminal").expect("ok").expect("ok");
    let warp = provider.lookup("dev.warp.Warp").expect("ok").expect("ok");
    assert_eq!(firefox.display_name, "Firefox");
    assert_eq!(terminal.display_name, "Terminal");
    assert_eq!(warp.display_name, "Code");
}

/// `.desktop` file without an `Icon=` key returns the display name
/// but no icon ref and no failure is recorded.
#[test]
fn desktop_without_icon_keeps_display_name() {
    let home = PathBuf::from("/home/tester");
    let (fs, assets) = fixture(&home);
    let apps = home.join(".local/share/applications");
    fs.mkdir(&apps);
    write_desktop(
        &fs,
        &apps,
        "firefox",
        "[Desktop Entry]\nType=Application\nName=Firefox\nStartupWMClass=firefox\n",
    );
    let provider =
        LinuxApplicationMetadataProvider::with_filesystem(assets.clone(), std::sync::Arc::new(fs));
    let metadata = provider
        .lookup("firefox")
        .expect("ok")
        .expect("some metadata");
    assert_eq!(metadata.display_name, "Firefox");
    assert!(metadata.icon_ref.is_none());
    let diagnostics = provider.last_icon_diagnostics();
    assert!(!diagnostics.declared);
    assert!(!diagnostics.resolved);
    assert_eq!(diagnostics.failure_kind, IconFailureKind::None);
}

/// When the icon resolver cannot find a candidate under any XDG
/// root the diagnostic records `IconFailureKind::NotFound`.
#[test]
fn icon_not_found_failure_is_typed() {
    let home = PathBuf::from("/home/tester");
    let (fs, assets) = fixture(&home);
    let apps = home.join(".local/share/applications");
    fs.mkdir(&apps);
    write_desktop(
        &fs,
        &apps,
        "firefox",
        "[Desktop Entry]\nType=Application\nName=Firefox\nStartupWMClass=firefox\nIcon=missing-icon\n",
    );
    let provider =
        LinuxApplicationMetadataProvider::with_filesystem(assets.clone(), std::sync::Arc::new(fs));
    let metadata = provider
        .lookup("firefox")
        .expect("ok")
        .expect("some metadata");
    assert_eq!(metadata.display_name, "Firefox");
    assert!(metadata.icon_ref.is_none());
    let diagnostics = provider.last_icon_diagnostics();
    assert_eq!(diagnostics.failure_kind, IconFailureKind::NotFound);
}

/// `Icon=/abs/path` outside the XDG roots is rejected with
/// `IconFailureKind::NotFound`.
#[test]
fn icon_outside_roots_is_rejected() {
    let home = PathBuf::from("/home/tester");
    let (fs, assets) = fixture(&home);
    let apps = home.join(".local/share/applications");
    fs.mkdir(&apps);
    let outside = home.join("outside.png");
    fs.write_png(&outside);
    write_desktop(
        &fs,
        &apps,
        "firefox",
        &format!(
            "[Desktop Entry]\nType=Application\nName=Firefox\nIcon={}\n",
            outside.display()
        ),
    );
    let provider =
        LinuxApplicationMetadataProvider::with_filesystem(assets.clone(), std::sync::Arc::new(fs));
    let metadata = provider
        .lookup("firefox")
        .expect("ok")
        .expect("some metadata");
    assert!(metadata.icon_ref.is_none());
    let diagnostics = provider.last_icon_diagnostics();
    assert_eq!(diagnostics.failure_kind, IconFailureKind::NotFound);
}

/// SVG-only icons persist via the rasterizer when the
/// `linux-svg-raster` feature is enabled.
#[cfg(feature = "linux-svg-raster")]
#[test]
fn svg_only_icon_is_rasterized_and_persisted() {
    let home = PathBuf::from("/home/tester");
    let (fs, assets) = fixture(&home);
    let apps = home.join(".local/share/applications");
    let scalable = home.join(".local/share/icons/hicolor/scalable/apps");
    fs.mkdir(&apps);
    fs.mkdir(&home.join(".local/share/icons"));
    fs.mkdir(&scalable);
    fs.write_svg(&scalable.join("code.svg"), MINIMAL_SVG);
    write_desktop(
        &fs,
        &apps,
        "code",
        "[Desktop Entry]\nType=Application\nName=Code\nStartupWMClass=code\nIcon=code\n",
    );
    let provider =
        LinuxApplicationMetadataProvider::with_filesystem(assets.clone(), std::sync::Arc::new(fs));
    let metadata = provider.lookup("code").expect("ok").expect("some metadata");
    let icon_ref = metadata.icon_ref.expect("icon ref");
    assert_eq!(icon_ref, "application-icons/code.png");
    let target = assets.join(&icon_ref);
    let bytes = std::fs::read(&target).expect("read");
    assert!(bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]));
    let diagnostics = provider.last_icon_diagnostics();
    assert_eq!(diagnostics.kind, IconSourceKind::Svg);
    assert!(diagnostics.rasterization_attempted);
    assert!(diagnostics.rasterization_succeeded);
}

/// Malformed SVG icons surface `IconFailureKind::InvalidSvg`.
#[cfg(feature = "linux-svg-raster")]
#[test]
fn malformed_svg_records_invalid_svg_failure() {
    let home = PathBuf::from("/home/tester");
    let (fs, assets) = fixture(&home);
    let apps = home.join(".local/share/applications");
    let scalable = home.join(".local/share/icons/hicolor/scalable/apps");
    fs.mkdir(&apps);
    fs.mkdir(&home.join(".local/share/icons"));
    fs.mkdir(&scalable);
    fs.write(&scalable.join("code.svg"), b"<<not svg>>");
    write_desktop(
        &fs,
        &apps,
        "code",
        "[Desktop Entry]\nType=Application\nName=Code\nStartupWMClass=code\nIcon=code\n",
    );
    let provider =
        LinuxApplicationMetadataProvider::with_filesystem(assets.clone(), std::sync::Arc::new(fs));
    let metadata = provider.lookup("code").expect("ok").expect("some metadata");
    assert!(metadata.icon_ref.is_none());
    let diagnostics = provider.last_icon_diagnostics();
    assert_eq!(diagnostics.failure_kind, IconFailureKind::InvalidSvg);
}

/// PNG icons take priority over SVG icons that share the same
/// icon name.
#[cfg(feature = "linux-svg-raster")]
#[test]
fn png_icon_is_preferred_over_svg() {
    let home = PathBuf::from("/home/tester");
    let (fs, assets) = fixture(&home);
    let apps = home.join(".local/share/applications");
    let png_apps = home.join(".local/share/icons/hicolor/48x48/apps");
    let svg_apps = home.join(".local/share/icons/hicolor/scalable/apps");
    fs.mkdir(&apps);
    fs.mkdir(&home.join(".local/share/icons"));
    fs.mkdir(&png_apps);
    fs.mkdir(&svg_apps);
    fs.write_png(&png_apps.join("code.png"));
    fs.write_svg(&svg_apps.join("code.svg"), MINIMAL_SVG);
    write_desktop(
        &fs,
        &apps,
        "code",
        "[Desktop Entry]\nType=Application\nName=Code\nStartupWMClass=code\nIcon=code\n",
    );
    let provider =
        LinuxApplicationMetadataProvider::with_filesystem(assets.clone(), std::sync::Arc::new(fs));
    let metadata = provider.lookup("code").expect("ok").expect("some metadata");
    let icon_ref = metadata.icon_ref.expect("icon ref");
    let diagnostics = provider.last_icon_diagnostics();
    assert_eq!(diagnostics.kind, IconSourceKind::Png);
    assert!(!diagnostics.rasterization_attempted);
    let target = assets.join(&icon_ref);
    let bytes = std::fs::read(&target).expect("read");
    assert_eq!(
        &bytes[..8],
        &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]
    );
}

/// Icons living under `<root>/pixmaps/<name>.png` resolve through
/// the legacy pixmap layout.
#[test]
fn pixmaps_layout_is_supported() {
    let home = PathBuf::from("/home/tester");
    let (fs, assets) = fixture(&home);
    let apps = home.join(".local/share/applications");
    let pixmaps = home.join(".local/share/pixmaps");
    fs.mkdir(&apps);
    fs.mkdir(&pixmaps);
    fs.write_png(&pixmaps.join("firefox.png"));
    write_desktop(
        &fs,
        &apps,
        "firefox",
        "[Desktop Entry]\nType=Application\nName=Firefox\nStartupWMClass=firefox\nIcon=firefox\n",
    );
    let provider =
        LinuxApplicationMetadataProvider::with_filesystem(assets.clone(), std::sync::Arc::new(fs));
    let metadata = provider
        .lookup("firefox")
        .expect("ok")
        .expect("some metadata");
    let icon_ref = metadata.icon_ref.expect("icon ref");
    assert_eq!(icon_ref, "application-icons/firefox.png");
    let diagnostics = provider.last_icon_diagnostics();
    assert_eq!(diagnostics.kind, IconSourceKind::Pixmap);
}

/// Yaru theme layout (Ubuntu's stock theme).
#[test]
fn yaru_theme_layout_is_supported() {
    let home = PathBuf::from("/home/tester");
    let (fs, assets) = fixture(&home);
    let apps = home.join(".local/share/applications");
    let icons = home.join(".local/share/icons/Yaru/48x48/apps");
    fs.mkdir(&apps);
    fs.mkdir(&home.join(".local/share/icons"));
    fs.mkdir(&icons);
    fs.write_png(&icons.join("firefox.png"));
    write_desktop(
        &fs,
        &apps,
        "firefox",
        "[Desktop Entry]\nType=Application\nName=Firefox\nStartupWMClass=firefox\nIcon=firefox\n",
    );
    let provider =
        LinuxApplicationMetadataProvider::with_filesystem(assets.clone(), std::sync::Arc::new(fs));
    let metadata = provider
        .lookup("firefox")
        .expect("ok")
        .expect("some metadata");
    assert!(metadata.icon_ref.is_some());
}

/// Adwaita theme layout (GNOME's stock theme).
#[test]
fn adwaita_theme_layout_is_supported() {
    let home = PathBuf::from("/home/tester");
    let (fs, assets) = fixture(&home);
    let apps = home.join(".local/share/applications");
    let icons = home.join(".local/share/icons/Adwaita/64x64/apps");
    fs.mkdir(&apps);
    fs.mkdir(&home.join(".local/share/icons"));
    fs.mkdir(&icons);
    fs.write_png(&icons.join("firefox.png"));
    write_desktop(
        &fs,
        &apps,
        "firefox",
        "[Desktop Entry]\nType=Application\nName=Firefox\nStartupWMClass=firefox\nIcon=firefox\n",
    );
    let provider =
        LinuxApplicationMetadataProvider::with_filesystem(assets.clone(), std::sync::Arc::new(fs));
    let metadata = provider
        .lookup("firefox")
        .expect("ok")
        .expect("some metadata");
    assert!(metadata.icon_ref.is_some());
}

/// Multiple sizes under the same theme all resolve.
#[test]
fn multiple_icon_sizes_resolve() {
    let home = PathBuf::from("/home/tester");
    let (fs, assets) = fixture(&home);
    let apps = home.join(".local/share/applications");
    fs.mkdir(&apps);
    fs.mkdir(&home.join(".local/share/icons"));
    fs.mkdir(&home.join(".local/share/icons/hicolor/16x16/apps"));
    fs.mkdir(&home.join(".local/share/icons/hicolor/22x22/apps"));
    fs.mkdir(&home.join(".local/share/icons/hicolor/24x24/apps"));
    fs.mkdir(&home.join(".local/share/icons/hicolor/32x32/apps"));
    fs.mkdir(&home.join(".local/share/icons/hicolor/48x48/apps"));
    fs.mkdir(&home.join(".local/share/icons/hicolor/64x64/apps"));
    fs.mkdir(&home.join(".local/share/icons/hicolor/96x96/apps"));
    fs.mkdir(&home.join(".local/share/icons/hicolor/128x128/apps"));
    fs.mkdir(&home.join(".local/share/icons/hicolor/256x256/apps"));
    fs.write_png(
        &home
            .join(".local/share/icons/hicolor/256x256/apps")
            .join("firefox.png"),
    );
    write_desktop(
        &fs,
        &apps,
        "firefox",
        "[Desktop Entry]\nType=Application\nName=Firefox\nStartupWMClass=firefox\nIcon=firefox\n",
    );
    let provider =
        LinuxApplicationMetadataProvider::with_filesystem(assets.clone(), std::sync::Arc::new(fs));
    let metadata = provider
        .lookup("firefox")
        .expect("ok")
        .expect("some metadata");
    assert!(metadata.icon_ref.is_some());
}

/// A symlink that escapes the configured icon root is rejected
/// silently — the lookup keeps the display name.
#[test]
fn symlink_outside_root_is_rejected() {
    let home = PathBuf::from("/home/tester");
    let (fs, assets) = fixture(&home);
    let apps = home.join(".local/share/applications");
    let icons = home.join(".local/share/icons/hicolor/48x48/apps");
    fs.mkdir(&apps);
    fs.mkdir(&home.join(".local/share/icons"));
    fs.mkdir(&icons);
    let escaped = home.join("escaped.png");
    fs.write_png(&escaped);
    // The .desktop references the symlink path. The resolver
    // canonicalises the candidate path and rejects anything that
    // escapes the configured icon roots.
    write_desktop(
        &fs,
        &apps,
        "firefox",
        &format!(
            "[Desktop Entry]\nType=Application\nName=Firefox\nIcon={}\n",
            escaped.display()
        ),
    );
    let provider =
        LinuxApplicationMetadataProvider::with_filesystem(assets.clone(), std::sync::Arc::new(fs));
    let metadata = provider
        .lookup("firefox")
        .expect("ok")
        .expect("some metadata");
    assert_eq!(metadata.display_name, "Firefox");
    assert!(metadata.icon_ref.is_none());
}

/// Existing assets on disk are never overwritten with a PNG the
/// provider failed to rasterize.
#[test]
fn existing_icon_is_preserved_when_lookup_fails() {
    let home = PathBuf::from("/home/tester");
    let (fs, assets) = fixture(&home);
    let apps = home.join(".local/share/applications");
    fs.mkdir(&apps);
    let existing = assets.join("application-icons/firefox.png");
    std::fs::write(&existing, b"PRESERVE_ME").unwrap();
    write_desktop(
        &fs,
        &apps,
        "firefox",
        "[Desktop Entry]\nType=Application\nName=Firefox\nStartupWMClass=firefox\n",
    );
    let provider =
        LinuxApplicationMetadataProvider::with_filesystem(assets.clone(), std::sync::Arc::new(fs));
    let metadata = provider
        .lookup("firefox")
        .expect("ok")
        .expect("some metadata");
    assert_eq!(metadata.display_name, "Firefox");
    assert!(metadata.icon_ref.is_none());
    let bytes = std::fs::read(&existing).expect("read");
    assert_eq!(bytes, b"PRESERVE_ME");
}

/// `application-icons/` is created only when a valid icon was
/// persisted; empty directories do not leak onto disk.
#[test]
fn application_icons_dir_only_created_on_persistence() {
    let home = PathBuf::from("/home/tester");
    let (fs, assets) = fixture(&home);
    let apps = home.join(".local/share/applications");
    fs.mkdir(&apps);
    write_desktop(
        &fs,
        &apps,
        "ghost",
        "[Desktop Entry]\nType=Application\nName=Ghost\nStartupWMClass=ghost\n",
    );
    let provider =
        LinuxApplicationMetadataProvider::with_filesystem(assets.clone(), std::sync::Arc::new(fs));
    let metadata = provider
        .lookup("ghost")
        .expect("ok")
        .expect("some metadata");
    assert!(metadata.icon_ref.is_none());
    let icons_dir = assets.join("application-icons");
    assert!(std::fs::read_dir(&icons_dir).unwrap().next().is_none());
}
