//! Linux-backed [`ApplicationMetadataProvider`].
//!
//! Resolves a stable X11 / XWayland `WM_CLASS` identifier against the
//! freedesktop `.desktop` files installed under the XDG-compliant
//! data roots, persists the resolved icon under the existing
//! `<data_dir>/assets/application-icons/` namespace and exposes the
//! user-visible display name the card rail already renders.
//!
//! The provider is intentionally narrow: it never executes a desktop
//! entry, never launches an external helper and never touches the
//! network. It only reads the `[Desktop Entry]` group, looks up the
//! `Icon=` value against a small set of XDG-compliant local roots
//! and writes a validated PNG through the existing
//! `application-icons/` namespace. The blacklist and capture
//! pipeline therefore keep the same `source_app`-based privacy
//! contract the macOS provider already honours.
//!
//! ## Design choices
//!
//! - The matching priority (`StartupWMClass` → `X-GNOME-WMClass` →
//!   filename) is the same priority documented in `design.md`.
//!   Comparison is case-insensitive ASCII and ties are resolved
//!   lexicographically so the result stays deterministic across
//!   runs.
//! - `NoDisplay=true` is **not** treated as a skip; the spec only
//!   excludes `Hidden=true` and non-`Application` types so the
//!   provider can answer for installed apps that hide themselves in
//!   the launcher.
//! - Icons resolve locally only: absolute paths and theme names are
//!   walked against an XDG icon root list compiled from a single
//!   shared `data_roots()` helper so the `.desktop` scanner and the
//!   icon scanner never disagree on which roots count. Each root
//!   gives rise to three directories — `applications/`, `icons/` and
//!   `pixmaps/` — and nothing else. The legacy `<root>/icons/icons/...`
//!   layout that the previous collector produced when it appended
//!   `/icons` twice is gone: `data_roots()` returns base roots, every
//!   helper joins each derivative exactly once.
//! - The icon writer is atomic: it builds a temporary file inside the
//!   destination directory, validates the PNG (signature + size +
//!   dimension) and renames the file over the destination. The
//!   temporary is cleaned up on every error path.
//! - An existing valid icon is **never** overwritten with an empty
//!   payload: when the lookup succeeds without an icon the provider
//!   returns the display name and leaves the previous reference
//!   untouched so the card rail keeps rendering the asset the
//!   previous capture persisted.
//! - When the resolver finds an SVG icon the provider rasterizes it
//!   to PNG through the `linux-svg-raster` helper and reuses the
//!   same atomic write path so the rest of the pipeline stays
//!   PNG-only. The rasterizer disables every external resource
//!   resolver, caps the source and target dimensions and never
//!   invokes a helper process.

use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use parking_lot::Mutex;

use crate::app_assets::APPLICATION_ICONS_DIR;
use crate::app_metadata::{
    icon_ref_for, ApplicationMetadata, ApplicationMetadataError, ApplicationMetadataProvider,
    IconDiagnostics, IconFailureKind, IconSourceKind, MatchStrategy,
};

#[cfg(feature = "linux-svg-raster")]
use crate::runtime::linux_svg_raster;

/// Linux-backed application-metadata provider. Resolves a stable
/// `WM_CLASS` identifier against the freedesktop `.desktop` files
/// installed on the host and persists the resolved icon under
/// `<assets_dir>/application-icons/<safe-id>.png`.
pub struct LinuxApplicationMetadataProvider {
    assets_dir: PathBuf,
    /// Roots the desktop-entry scanner walks. Cached at construction
    /// time so the lookup hot path never re-reads the environment.
    app_dirs: Vec<PathBuf>,
    /// Per-theme `apps/` directories the resolver probes for theme
    /// icons (e.g. `Icon=firefox`). Cached for the same reason.
    icon_apps_dirs: Vec<PathBuf>,
    /// `pixmaps/` directories the resolver probes for legacy layout
    /// icons (e.g. `Icon=firefox` with no theme). Cached alongside
    /// the theme directories so the lookup stays deterministic.
    pixmap_dirs: Vec<PathBuf>,
    /// Canonical parent roots for every entry in `icon_apps_dirs`
    /// and `pixmap_dirs`. The resolver refuses to follow any
    /// candidate whose canonical path escapes one of these roots,
    /// which keeps symlink traversal honest regardless of which
    /// theme the host installed the file under.
    icon_roots: Vec<PathBuf>,
    /// Filesystem abstraction the provider uses. Tests inject a fake
    /// so the unit suite can drive the parser without standing up a
    /// real `.desktop` installation.
    fs: std::sync::Arc<dyn DesktopFilesystem>,
    /// Strategy the most recent [`Self::lookup`] used to pick the
    /// candidate entry. Exposed through
    /// [`ApplicationMetadataProvider::last_match_strategy`] so the
    /// `linux-source-app-metadata` capture diagnostic can confirm
    /// whether the resolver hit [`MatchStrategy::StartupWmClass`],
    /// [`MatchStrategy::XGnomeWmClass`] or the file-name fallback.
    last_strategy: Mutex<MatchStrategy>,
    /// Snapshot the icon writer reported on the most recent
    /// successful lookup. Exposed through
    /// [`ApplicationMetadataProvider::last_icon_diagnostics`] so the
    /// capture diagnostic can confirm the persistence step ran.
    last_icon: Mutex<IconDiagnostics>,
}

impl LinuxApplicationMetadataProvider {
    /// Build a provider rooted at `<assets_dir>/application-icons/`.
    /// The constructor caches the XDG paths so the lookup hot path
    /// stays deterministic and free of environment re-reads.
    pub fn new(assets_dir: impl Into<PathBuf>) -> Self {
        Self::with_filesystem(assets_dir, std::sync::Arc::new(HostFilesystem))
    }

    /// Test-only constructor that injects a filesystem
    /// abstraction. Kept public so the integration suite can drive
    /// the parser against a memory-backed filesystem without going
    /// through the host filesystem. Production builds always
    /// construct the provider through [`Self::new`].
    pub fn with_filesystem(
        assets_dir: impl Into<PathBuf>,
        fs: std::sync::Arc<dyn DesktopFilesystem>,
    ) -> Self {
        let roots = data_roots(fs.as_ref());
        let (app_dirs, icon_apps_dirs, pixmap_dirs, icon_roots) =
            collect_directories(fs.as_ref(), &roots);
        Self {
            assets_dir: assets_dir.into(),
            app_dirs,
            icon_apps_dirs,
            pixmap_dirs,
            icon_roots,
            fs,
            last_strategy: Mutex::new(MatchStrategy::None),
            last_icon: Mutex::new(IconDiagnostics::default()),
        }
    }
}

impl ApplicationMetadataProvider for LinuxApplicationMetadataProvider {
    fn lookup(
        &self,
        identifier: &str,
    ) -> Result<Option<ApplicationMetadata>, ApplicationMetadataError> {
        let trimmed = identifier.trim();
        if trimmed.is_empty() {
            self.record_strategy(MatchStrategy::None);
            self.record_icon(IconDiagnostics::default());
            return Ok(None);
        }
        let needle = trimmed.to_ascii_lowercase();
        let candidate = match self.find_entry_with_strategy(&needle) {
            Some((priority, entry)) => {
                self.record_strategy(match_priority_to_strategy(priority));
                entry
            }
            None => {
                self.record_strategy(MatchStrategy::None);
                self.record_icon(IconDiagnostics::default());
                return Ok(None);
            }
        };
        let display_name = match resolve_display_name(&candidate, &current_locale_candidates()) {
            Some(name) if !name.is_empty() => name,
            _ => {
                self.record_icon(IconDiagnostics::default());
                return Ok(None);
            }
        };
        let icon_ref = self.persist_icon(&candidate, trimmed);
        Ok(Some(ApplicationMetadata {
            display_name,
            icon_ref,
        }))
    }

    fn name(&self) -> &'static str {
        "linux_app_metadata"
    }

    fn last_match_strategy(&self) -> MatchStrategy {
        *self.last_strategy.lock()
    }

    fn last_icon_diagnostics(&self) -> IconDiagnostics {
        *self.last_icon.lock()
    }
}

impl LinuxApplicationMetadataProvider {
    fn record_strategy(&self, strategy: MatchStrategy) {
        *self.last_strategy.lock() = strategy;
    }

    fn record_icon(&self, snapshot: IconDiagnostics) {
        *self.last_icon.lock() = snapshot;
    }
}

fn match_priority_to_strategy(priority: MatchPriority) -> MatchStrategy {
    match priority {
        MatchPriority::StartupWmClass => MatchStrategy::StartupWmClass,
        MatchPriority::GnomeWmClass => MatchStrategy::XGnomeWmClass,
        MatchPriority::Filename => MatchStrategy::DesktopFilename,
    }
}

impl LinuxApplicationMetadataProvider {
    /// Persist the resolved icon under the existing
    /// `application-icons/` namespace. Returns `None` when no icon
    /// could be resolved or the write path failed; the metadata
    /// contract keeps the display name even when the icon path
    /// fails, so a non-`Some` return never converts a valid capture
    /// into a `Failed` outcome.
    ///
    /// The writer refuses to clobber a previously-persisted icon
    /// with an empty payload: when the entry's `Icon=` key is
    /// missing, unparseable, points outside an allowed root or
    /// points at a path that cannot be read, the function returns
    /// `None` without touching the destination file.
    ///
    /// The function records every step of the icon resolution onto
    /// the provider's `IconDiagnostics` slot so the
    /// `linux-source-app-metadata` capture diagnostic can confirm
    /// whether a missing icon is a missing entry, a missing file, a
    /// malformed PNG, a malformed SVG, an oversized SVG or a writer
    /// failure.
    fn persist_icon(&self, entry: &DesktopEntry, identifier: &str) -> Option<String> {
        let icon_value = entry.icon.as_deref()?;
        let resolved = match self.resolve_icon(icon_value) {
            Some(resolved) => resolved,
            None => {
                self.record_icon(IconDiagnostics {
                    declared: true,
                    kind: classify_icon_value(icon_value),
                    resolved: false,
                    rasterization_attempted: false,
                    rasterization_succeeded: false,
                    png_validated: false,
                    persisted: false,
                    bytes: None,
                    dimensions: None,
                    failure_kind: IconFailureKind::NotFound,
                });
                return None;
            }
        };
        let png_bytes = match self.png_payload_from_resolved(&resolved) {
            Ok(payload) => payload,
            Err(failure) => {
                self.record_icon(IconDiagnostics {
                    declared: true,
                    kind: resolved.kind,
                    resolved: true,
                    rasterization_attempted: resolved.kind == IconSourceKind::Svg,
                    rasterization_succeeded: false,
                    png_validated: false,
                    persisted: false,
                    bytes: None,
                    dimensions: None,
                    failure_kind: failure,
                });
                return None;
            }
        };
        let dimensions = png_header_dimensions(&png_bytes);
        let target_dir = self.assets_dir.join(APPLICATION_ICONS_DIR);
        match write_icon_atomic(&target_dir, identifier, &png_bytes) {
            Ok(icon_ref) => {
                self.record_icon(IconDiagnostics {
                    declared: true,
                    kind: resolved.kind,
                    resolved: true,
                    rasterization_attempted: resolved.kind == IconSourceKind::Svg,
                    rasterization_succeeded: resolved.kind == IconSourceKind::Svg,
                    png_validated: true,
                    persisted: true,
                    bytes: Some(png_bytes.len()),
                    dimensions,
                    failure_kind: IconFailureKind::None,
                });
                Some(icon_ref)
            }
            Err(_) => {
                self.record_icon(IconDiagnostics {
                    declared: true,
                    kind: resolved.kind,
                    resolved: true,
                    rasterization_attempted: resolved.kind == IconSourceKind::Svg,
                    rasterization_succeeded: resolved.kind == IconSourceKind::Svg,
                    png_validated: true,
                    persisted: false,
                    bytes: Some(png_bytes.len()),
                    dimensions,
                    failure_kind: IconFailureKind::WriteError,
                });
                None
            }
        }
    }

    /// Convert a resolved icon (PNG bytes on disk, SVG bytes on disk
    /// or a rasterized PNG payload) into the PNG bytes the writer
    /// persists. Every failure path returns the typed
    /// [`IconFailureKind`] the diagnostic sink consumes.
    fn png_payload_from_resolved(
        &self,
        resolved: &ResolvedIcon,
    ) -> Result<Vec<u8>, IconFailureKind> {
        match resolved.kind {
            IconSourceKind::Png | IconSourceKind::Pixmap => {
                let bytes = self.fs.read(&resolved.path).ok().flatten();
                match bytes {
                    Some(bytes) if looks_like_png(&bytes) => Ok(bytes),
                    Some(_) => Err(IconFailureKind::InvalidPng),
                    None => Err(IconFailureKind::NotFound),
                }
            }
            IconSourceKind::Svg => {
                #[cfg(feature = "linux-svg-raster")]
                {
                    let bytes = match self.fs.read(&resolved.path) {
                        Ok(Some(bytes)) => bytes,
                        Ok(None) => return Err(IconFailureKind::NotFound),
                        Err(_) => return Err(IconFailureKind::NotFound),
                    };
                    let rasterized = linux_svg_raster::rasterize_svg_to_png(&bytes);
                    match rasterized {
                        Ok(payload) => Ok(payload.png_bytes),
                        Err(error) => Err(map_svg_error(error)),
                    }
                }
                #[cfg(not(feature = "linux-svg-raster"))]
                {
                    let _ = resolved;
                    Err(IconFailureKind::SvgRejected)
                }
            }
            IconSourceKind::Unknown => Err(IconFailureKind::InvalidSvg),
            IconSourceKind::None => Err(IconFailureKind::NotFound),
        }
    }

    /// Walk the configured XDG roots and return the best-matching
    /// `DesktopEntry` for `identifier` together with the
    /// [`MatchPriority`] of the winning candidate. Returns `None`
    /// when no unambiguous match is found.
    fn find_entry_with_strategy(&self, identifier: &str) -> Option<(MatchPriority, DesktopEntry)> {
        let needle = identifier.to_ascii_lowercase();
        let mut best: Option<(MatchPriority, DesktopEntry)> = None;
        for dir in &self.app_dirs {
            let entries = match self.fs.read_dir_sorted(dir) {
                Ok(entries) => entries,
                Err(_) => continue,
            };
            for entry in entries {
                if entry.extension().and_then(|ext| ext.to_str()) != Some("desktop") {
                    continue;
                }
                let path = dir.join(&entry);
                let parsed = match self.fs.read_entry(&path) {
                    Ok(parsed) => parsed,
                    Err(_) => continue,
                };
                if !parsed.is_application() {
                    continue;
                }
                let priority = match classify(&parsed, &needle) {
                    Some(priority) => priority,
                    None => continue,
                };
                best = match best.take() {
                    None => Some((priority, parsed)),
                    Some((existing, existing_entry))
                        if priority < existing
                            || (priority == existing
                                && entry_path_str(&parsed.path)
                                    < entry_path_str(&existing_entry.path)) =>
                    {
                        Some((priority, parsed))
                    }
                    Some(existing) => Some(existing),
                };
            }
        }
        best
    }

    /// Resolve the `Icon=` value declared by an entry against the
    /// allowed XDG icon roots. The helper accepts:
    ///
    /// - absolute file paths under one of the allowed roots;
    /// - bare icon names looked up in the cached icon directories
    ///   (`<theme>/<size>x<size>/apps/`, `<theme>/scalable/apps/`,
    ///   `<theme>/<size>x<size>/apps/` legacy layout or
    ///   `<root>/pixmaps/`). The directories are walked in the
    ///   order `collect_directories` produced so the first hit
    ///   wins, with PNG preferred over SVG and absolute paths
    ///   rejected when they escape the configured icon roots.
    ///
    /// Returns `None` for any input that escapes the allowed roots
    /// or fails the format checks. The validator never logs the
    /// offending path or filename to keep the diagnostic surface
    /// metadata-only.
    fn resolve_icon(&self, raw: &str) -> Option<ResolvedIcon> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return None;
        }
        let candidate = Path::new(trimmed);
        if candidate.is_absolute() {
            let path = self.fs.canonicalize_if_safe(candidate).ok()?;
            if !self.icon_roots.iter().any(|root| path.starts_with(root)) {
                return None;
            }
            if !self.fs.is_file(&path) {
                return None;
            }
            let kind = classify_path_extension(&path);
            return Some(ResolvedIcon { path, kind });
        }
        // Theme-name lookup: walk each cached `apps/` directory in
        // the deterministic order `collect_directories` produced.
        let stem = trimmed.trim_end_matches(".png").trim_end_matches(".svg");
        let png_filename = format!("{stem}.png");
        let svg_filename = format!("{stem}.svg");
        for apps_dir in &self.icon_apps_dirs {
            if let Some(png) = self.find_candidate(apps_dir, &png_filename, IconSourceKind::Png) {
                return Some(png);
            }
        }
        for apps_dir in &self.icon_apps_dirs {
            if let Some(svg) = self.find_candidate(apps_dir, &svg_filename, IconSourceKind::Svg) {
                return Some(svg);
            }
        }
        for pixmap_dir in &self.pixmap_dirs {
            if let Some(png) =
                self.find_candidate(pixmap_dir, &png_filename, IconSourceKind::Pixmap)
            {
                return Some(png);
            }
        }
        for pixmap_dir in &self.pixmap_dirs {
            if let Some(svg) = self.find_candidate(pixmap_dir, &svg_filename, IconSourceKind::Svg) {
                return Some(svg);
            }
        }
        None
    }

    /// Look up `filename` under `apps_dir` and validate that the
    /// canonical path lives under one of the configured icon
    /// roots. Returns the [`ResolvedIcon`] on success, `None`
    /// otherwise.
    fn find_candidate(
        &self,
        apps_dir: &Path,
        filename: &str,
        kind: IconSourceKind,
    ) -> Option<ResolvedIcon> {
        let candidate = apps_dir.join(filename);
        if !self.fs.is_file(&candidate) {
            return None;
        }
        let canonical = self.fs.canonicalize_if_safe(&candidate).ok()?;
        if !self
            .icon_roots
            .iter()
            .any(|root| canonical.starts_with(root))
        {
            return None;
        }
        Some(ResolvedIcon {
            path: canonical,
            kind,
        })
    }
}

#[cfg(feature = "linux-svg-raster")]
fn map_svg_error(error: linux_svg_raster::SvgRasterError) -> IconFailureKind {
    use linux_svg_raster::SvgRasterError;
    match error {
        SvgRasterError::TooLarge
        | SvgRasterError::SourceTooLarge
        | SvgRasterError::TargetTooLarge => IconFailureKind::SvgRejected,
        SvgRasterError::InvalidSvg => IconFailureKind::InvalidSvg,
        SvgRasterError::EncodeFailed => IconFailureKind::RasterizationFailed,
    }
}

/// Icon the resolver found on disk. The `path` is canonical and
/// has been verified to live under one of the configured icon
/// roots; the `kind` records the source format so the diagnostic
/// surface can distinguish PNG, SVG and pixmap-sourced payloads.
#[derive(Debug, Clone)]
struct ResolvedIcon {
    path: PathBuf,
    kind: IconSourceKind,
}

fn entry_path_str(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn classify_path_extension(path: &Path) -> IconSourceKind {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("png") => IconSourceKind::Png,
        Some("svg") => IconSourceKind::Svg,
        Some("xpm") | Some("ico") => IconSourceKind::Unknown,
        _ => IconSourceKind::Unknown,
    }
}

fn classify_icon_value(value: &str) -> IconSourceKind {
    let lower = value.to_ascii_lowercase();
    if lower.ends_with(".svg") {
        IconSourceKind::Svg
    } else if lower.ends_with(".png") {
        IconSourceKind::Png
    } else if lower.contains("pixmaps/") || lower.starts_with("pixmaps/") {
        IconSourceKind::Pixmap
    } else {
        IconSourceKind::Unknown
    }
}

/// Lower-is-better priority the matcher assigns to a candidate
/// `.desktop` entry. Lower values mean the entry should win.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum MatchPriority {
    /// Highest confidence: the `StartupWMClass` field matches the
    /// `WM_CLASS` class segment exactly.
    StartupWmClass = 0,
    /// Second highest: the `X-GNOME-WMClass` field matches.
    GnomeWmClass = 1,
    /// The basename of the file (without the `.desktop` extension)
    /// matches the identifier.
    Filename = 2,
}

fn classify(entry: &DesktopEntry, identifier: &str) -> Option<MatchPriority> {
    if entry
        .startup_wm_class
        .as_deref()
        .map(|value| value.to_ascii_lowercase() == identifier)
        .unwrap_or(false)
    {
        return Some(MatchPriority::StartupWmClass);
    }
    if entry
        .x_gnome_wm_class
        .as_deref()
        .map(|value| value.to_ascii_lowercase() == identifier)
        .unwrap_or(false)
    {
        return Some(MatchPriority::GnomeWmClass);
    }
    let basename = entry
        .path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .map(|stem| stem.to_ascii_lowercase())?;
    if basename == identifier {
        return Some(MatchPriority::Filename);
    }
    None
}

/// In-memory representation of the relevant `[Desktop Entry]`
/// fields. The provider only needs the keys the spec pins; the
/// parser refuses every other key so a malformed file cannot smuggle
/// an unexpected field into the matcher.
///
/// The struct is `pub` so the integration suite can drive the
/// matcher through the public filesystem abstraction; the fields
/// stay visible so test fixtures can construct deterministic
/// entries without going through the production parser.
#[derive(Debug, Clone)]
pub struct DesktopEntry {
    pub path: PathBuf,
    pub type_is_application: bool,
    pub hidden: bool,
    pub name: Option<String>,
    pub name_locale: Vec<(String, String)>,
    pub icon: Option<String>,
    pub startup_wm_class: Option<String>,
    pub x_gnome_wm_class: Option<String>,
}

impl DesktopEntry {
    fn is_application(&self) -> bool {
        self.type_is_application && !self.hidden
    }
}

/// Read a `.desktop` file and extract the keys the spec pins. The
/// parser is deliberately narrow: it ignores comments, jumps over
/// any group other than `[Desktop Entry]`, only honours the keys
/// documented in `design.md` and rejects every value that includes
/// an unsupported escape. The result is the minimum the provider
/// needs to do its job; nothing else crosses this boundary.
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
        let value = unescape_desktop_value(value.trim());
        match key {
            "Type" => entry.type_is_application = value.eq_ignore_ascii_case("Application"),
            "Hidden" => entry.hidden = value.eq_ignore_ascii_case("true"),
            "NoDisplay" => {
                // `NoDisplay=true` is intentionally not a skip —
                // the spec allows it so installed apps that hide
                // themselves in the launcher still surface metadata
                // to the card rail.
            }
            "Name" => entry.name = Some(value),
            "Icon" => entry.icon = Some(value),
            "StartupWMClass" => entry.startup_wm_class = Some(value),
            "X-GNOME-WMClass" => entry.x_gnome_wm_class = Some(value),
            _ => {
                if let Some(locale) = key.strip_prefix("Name[") {
                    if let Some(stripped) = locale.strip_suffix(']') {
                        entry.name_locale.push((stripped.to_string(), value));
                    }
                }
            }
        }
    }
    entry
}

/// Reverse the single-line escape rules the freedesktop spec defines
/// for `.desktop` values. The provider only needs the `\\s`, `\\n`,
/// `\\t`, `\\r` and `\\\\` substitutions the spec documents; any
/// other escape is left untouched so a malformed value surfaces as a
/// non-match rather than as a silently truncated string.
fn unescape_desktop_value(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('s') => out.push(' '),
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// Resolve the locale-preferred display name. The lookup walks the
/// locale candidates in order and returns the first non-empty value
/// that matches the entry's `Name[locale]` table; if none match, it
/// falls back to `Name` and finally to `None` when the entry has no
/// display name at all.
fn resolve_display_name(entry: &DesktopEntry, locales: &[String]) -> Option<String> {
    for locale in locales {
        for (key, value) in &entry.name_locale {
            if key.eq_ignore_ascii_case(locale) && !value.is_empty() {
                return Some(value.clone());
            }
        }
    }
    entry
        .name
        .as_deref()
        .filter(|name| !name.is_empty())
        .map(str::to_string)
}

/// Compute the locale candidates the lookup prefers. Mirrors the
/// `$LANGUAGE` / `$LC_ALL` / `$LANG` chain the freedesktop spec
/// documents without pulling in any locale parsing crate. Returns
/// at least the empty-locale fallback so the provider always tries
/// the canonical `Name` field.
fn current_locale_candidates() -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for var in ["LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(value) = env::var(var) {
            for token in value.split(':') {
                let token = token.trim();
                if token.is_empty() {
                    continue;
                }
                if !out.iter().any(|existing| existing == token) {
                    out.push(token.to_string());
                }
            }
        }
    }
    out.push(String::new());
    out
}

/// Build the list of base data roots the XDG spec considers
/// authoritative. The list is the single source of truth for every
/// downstream collector (`applications/`, `icons/` and `pixmaps/`):
/// it honours `XDG_DATA_HOME`, falls back to `$HOME/.local/share`,
/// then walks `XDG_DATA_DIRS` and finally appends the documented
/// `/usr/local/share` and `/usr/share` fallbacks when the
/// environment does not advertise any directories. The result is
/// deduplicated, sorted lexicographically and stripped of trailing
/// separators so the same root never appears under two distinct
/// `PathBuf` representations.
///
/// The function never fails when a root is missing on disk — the
/// downstream collectors call `is_dir` to filter out absent paths,
/// so a guest account with no `$HOME/.local/share` is fine.
fn data_roots(fs: &dyn DesktopFilesystem) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();

    let home = env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| fs.home_dir());

    let xdg_data_home = env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .and_then(|path| normalize_root(&path));

    let primary_root = xdg_data_home
        .or_else(|| home.as_ref().map(|home| home.join(".local").join("share")))
        .and_then(|path| normalize_root(&path));
    if let Some(root) = primary_root {
        roots.push(root);
    }

    let xdg_data_dirs = env::var_os("XDG_DATA_DIRS")
        .map(PathOsStringExt::split_paths)
        .unwrap_or_default();
    let configured: Vec<PathBuf> = xdg_data_dirs
        .into_iter()
        .filter(|path| !path.as_os_str().is_empty())
        .filter_map(|path| normalize_root(&path))
        .collect();
    if configured.is_empty() {
        push_unique(&mut roots, &PathBuf::from("/usr/local/share"));
        push_unique(&mut roots, &PathBuf::from("/usr/share"));
    } else {
        for root in configured {
            push_unique(&mut roots, &root);
        }
    }

    // Optional fallbacks for hosts that ship applications/icons
    // outside the XDG data directories (Flatpak, Snap, NixOS).
    // Each candidate is included only when it exists on disk so
    // the list never silently drops the documented XDG_DATA_DIRS
    // entries when an optional namespace is absent.
    for candidate in optional_export_roots(home.as_ref()) {
        push_unique(&mut roots, &candidate);
    }

    roots
}

/// Optional XDG-export fallbacks that some package managers
/// (Flatpak, Snap, NixOS) install alongside the standard
/// `/usr/share` tree. The list intentionally stays small and only
/// adds roots that already exist on disk so the provider never
/// fabricates paths the host cannot read.
fn optional_export_roots(home: Option<&PathBuf>) -> Vec<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(home) = home {
        candidates.push(
            home.join(".local")
                .join("share")
                .join("flatpak")
                .join("exports")
                .join("share"),
        );
    }
    candidates.push(PathBuf::from("/var/lib/flatpak/exports/share"));
    candidates.push(PathBuf::from("/var/lib/snapd/desktop"));
    candidates.push(PathBuf::from("/run/current-system/sw/share"));
    candidates
        .into_iter()
        .filter_map(|path| normalize_root(&path))
        .collect()
}

/// Normalize a candidate root by stripping trailing separators and
/// returning `None` when the path is empty after the strip. The
/// helper keeps `data_roots()` deterministic across hosts that
/// quote or double-quote the XDG variables.
fn normalize_root(path: &Path) -> Option<PathBuf> {
    let trimmed = path.components().collect::<Vec<_>>();
    if trimmed.is_empty() {
        return None;
    }
    let normalized: PathBuf = trimmed.iter().collect();
    if normalized.as_os_str().is_empty() {
        None
    } else {
        Some(normalized)
    }
}

fn push_unique(roots: &mut Vec<PathBuf>, candidate: &Path) {
    let normalized = match normalize_root(candidate) {
        Some(path) => path,
        None => return,
    };
    if !roots.iter().any(|existing| existing == &normalized) {
        roots.push(normalized);
    }
}

/// Helper trait wrapping `OsString::split_paths` so the
/// `data_roots` helper can be reused by tests without reaching for
/// `std::ffi` at every call site.
trait PathOsStringExt {
    fn split_paths(self) -> Vec<PathBuf>;
}

impl PathOsStringExt for OsString {
    fn split_paths(self) -> Vec<PathBuf> {
        std::env::split_paths(&self).collect()
    }
}

/// Build the `.desktop`, icon-theme and pixmap directories the
/// resolver walks. Every directory is rooted at one of the base
/// roots [`data_roots`] returns and joined exactly once with the
/// trailing `applications/`, `icons/<theme>/<size>x<size>/apps/`,
/// `icons/<theme>/scalable/apps/` or `pixmaps/` segment. The legacy
/// `<root>/icons/<size>x<size>/apps/` layout that some distributions
/// still ship is included alongside the theme-aware layout so the
/// provider finds icons regardless of how the host packaged them.
///
/// The function does **not** append `icons` twice: every helper
/// joins each segment exactly once, which removes the
/// `<root>/icons/icons/...` regression the previous collector
/// produced.
fn collect_directories(
    fs: &dyn DesktopFilesystem,
    roots: &[PathBuf],
) -> (Vec<PathBuf>, Vec<PathBuf>, Vec<PathBuf>, Vec<PathBuf>) {
    let mut app_dirs: Vec<PathBuf> = Vec::new();
    let mut icon_apps_dirs: Vec<PathBuf> = Vec::new();
    let mut pixmap_dirs: Vec<PathBuf> = Vec::new();
    let mut icon_roots: Vec<PathBuf> = Vec::new();

    for root in roots {
        let applications = root.join("applications");
        if fs.is_dir(&applications) {
            app_dirs.push(applications);
        }

        let icons = root.join("icons");
        if fs.is_dir(&icons) {
            // Canonicalize once so every theme-derived directory
            // shares the same root comparison; the canonical form
            // strips trailing-separator ambiguity that some
            // distributions ship with.
            let canonical_root = fs
                .canonicalize_if_safe(&icons)
                .unwrap_or_else(|_| icons.clone());
            if !icon_roots
                .iter()
                .any(|existing| existing == &canonical_root)
            {
                icon_roots.push(canonical_root);
            }
            let themes = discover_icon_themes(fs, &icons);
            for theme in themes {
                for size in ICON_SIZES {
                    let apps = icons
                        .join(&theme)
                        .join(format!("{size}x{size}"))
                        .join("apps");
                    if fs.is_dir(&apps) {
                        icon_apps_dirs.push(apps);
                    }
                }
                let scalable = icons.join(&theme).join("scalable").join("apps");
                if fs.is_dir(&scalable) {
                    icon_apps_dirs.push(scalable);
                }
            }
            // Legacy layout: `<root>/icons/<size>x<size>/apps`. The
            // walk happens at most once per size, so the resolver
            // never produces the duplicated `<root>/icons/icons/...`
            // path that the previous collector emitted.
            for size in ICON_SIZES {
                let apps = icons.join(format!("{size}x{size}")).join("apps");
                if fs.is_dir(&apps) {
                    icon_apps_dirs.push(apps);
                }
            }
        }

        let pixmaps = root.join("pixmaps");
        if fs.is_dir(&pixmaps) {
            pixmap_dirs.push(pixmaps);
        }
    }

    (app_dirs, icon_apps_dirs, pixmap_dirs, icon_roots)
}

/// Icon sizes the resolver walks when the entry names a theme
/// icon. The list covers the dimensions the freedesktop spec and
/// every documented distribution actually publish; the order is
/// deterministic so the resolver picks the same file across runs.
pub(crate) const ICON_SIZES: &[u32] = &[16, 22, 24, 32, 48, 64, 96, 128, 256];

/// Discover the icon themes installed under `<icons_root>`. The
/// walker is conservative: it inspects every direct child of the
/// icons root that looks like a theme directory and includes
/// `hicolor` last so it acts as a deterministic fallback for any
/// theme that failed to register a custom directory. Returns the
/// list in the order it should be probed.
fn discover_icon_themes(fs: &dyn DesktopFilesystem, icons_root: &Path) -> Vec<String> {
    let Ok(entries) = fs.read_dir_sorted(icons_root) else {
        return vec!["hicolor".to_string()];
    };
    let mut themes: Vec<String> = Vec::new();
    let mut has_hicolor = false;
    for entry in entries {
        let Some(name) = entry.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !fs.is_dir(&entry) {
            continue;
        }
        if name == "hicolor" {
            has_hicolor = true;
            continue;
        }
        themes.push(name.to_string());
    }
    themes.sort();
    if has_hicolor {
        themes.push("hicolor".to_string());
    } else if themes.is_empty() {
        themes.push("hicolor".to_string());
    }
    themes
}

/// Decode the IHDR width / height from the bytes the icon writer
/// just validated. The helper honours the PNG big-endian wire
/// format and returns `None` when the buffer is too short to
/// carry the IHDR chunk.
fn png_header_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    const HEADER_OFFSET: usize = 8 + 4 + 4;
    if bytes.len() < HEADER_OFFSET + 8 {
        return None;
    }
    let width = u32::from_be_bytes([
        bytes[HEADER_OFFSET],
        bytes[HEADER_OFFSET + 1],
        bytes[HEADER_OFFSET + 2],
        bytes[HEADER_OFFSET + 3],
    ]);
    let height = u32::from_be_bytes([
        bytes[HEADER_OFFSET + 4],
        bytes[HEADER_OFFSET + 5],
        bytes[HEADER_OFFSET + 6],
        bytes[HEADER_OFFSET + 7],
    ]);
    Some((width, height))
}

/// Persist `bytes` to `<dir>/<safe-id>.png` atomically: create a
/// temporary file inside the destination directory, write the bytes,
/// flush, sync, validate the file and rename over the destination.
/// Any error path cleans up the temporary so the namespace stays
/// tidy across crashes and failed lookups.
fn write_icon_atomic(dir: &Path, identifier: &str, bytes: &[u8]) -> Result<String, std::io::Error> {
    fs::create_dir_all(dir)?;
    let safe_id = sanitize_identifier(identifier);
    let target = dir.join(format!("{safe_id}.png"));
    if !looks_like_png(bytes) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "icon bytes are not a valid PNG",
        ));
    }
    let temp_name = format!(".{safe_id}.{}.png.tmp", std::process::id());
    let temp_path = dir.join(&temp_name);
    {
        let mut file = fs::File::create(&temp_path)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    if let Err(error) = fs::rename(&temp_path, &target) {
        let _ = fs::remove_file(&temp_path);
        return Err(error);
    }
    Ok(icon_ref_for(identifier))
}

/// Sanitize an identifier so the icon filename can never escape the
/// `application-icons/` namespace. Mirrors the rule the
/// `app_metadata` module already exposes via `icon_ref_for` so the
/// writer and the read-side validator stay in lock-step.
fn sanitize_identifier(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        out.push_str("app");
    }
    out
}

/// Mirrors the magic-header check in `app_assets` so the icon
/// bridge can serve the bytes without re-validating the file. Kept
/// local to the provider so a future change to the bridge validator
/// can evolve independently as long as both sides agree on the
/// signature.
fn looks_like_png(bytes: &[u8]) -> bool {
    const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    bytes.len() >= SIGNATURE.len() && bytes[..SIGNATURE.len()] == SIGNATURE
}

/// Filesystem abstraction the provider uses. The trait is the seam
/// tests hook to drive the parser without standing up a host
/// `.desktop` installation. Exposed as `pub` so the integration
/// suite can ship a memory-backed implementation; the production
/// host adapter stays an opaque detail of the provider.
pub trait DesktopFilesystem: Send + Sync {
    fn read_entry(&self, path: &Path) -> std::io::Result<DesktopEntry>;
    fn read_dir_sorted(&self, dir: &Path) -> std::io::Result<Vec<PathBuf>>;
    fn is_dir(&self, path: &Path) -> bool;
    fn is_file(&self, path: &Path) -> bool;
    fn canonicalize_if_safe(&self, path: &Path) -> std::io::Result<PathBuf>;
    fn home_dir(&self) -> Option<PathBuf>;
    fn read(&self, path: &Path) -> std::io::Result<Option<Vec<u8>>>;
}

/// Host-backed filesystem implementation.
struct HostFilesystem;

impl DesktopFilesystem for HostFilesystem {
    fn read_entry(&self, path: &Path) -> std::io::Result<DesktopEntry> {
        let raw = fs::read_to_string(path)?;
        Ok(parse_desktop_entry(&raw, path))
    }

    fn read_dir_sorted(&self, dir: &Path) -> std::io::Result<Vec<PathBuf>> {
        let mut entries: Vec<PathBuf> = fs::read_dir(dir)?
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .collect();
        entries.sort();
        Ok(entries)
    }

    fn is_dir(&self, path: &Path) -> bool {
        path.is_dir()
    }

    fn is_file(&self, path: &Path) -> bool {
        path.is_file()
    }

    fn canonicalize_if_safe(&self, path: &Path) -> std::io::Result<PathBuf> {
        fs::canonicalize(path)
    }

    fn home_dir(&self) -> Option<PathBuf> {
        dirs::home_dir()
    }

    fn read(&self, path: &Path) -> std::io::Result<Option<Vec<u8>>> {
        match fs::read(path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::sync::Mutex;

    /// In-memory filesystem the unit tests drive. The structure
    /// mirrors the host layout (`<root>/applications/<file>.desktop`,
    /// `<root>/icons/<theme>/<size>x<size>/apps/<icon>.png`,
    /// `<root>/icons/<theme>/scalable/apps/<icon>.svg`,
    /// `<root>/pixmaps/<icon>.png`) so the test paths read the
    /// same way the production paths do.
    #[derive(Default)]
    struct MemoryFilesystem {
        home: PathBuf,
        files: Mutex<BTreeMap<PathBuf, Vec<u8>>>,
        directories: Mutex<Vec<PathBuf>>,
    }

    impl MemoryFilesystem {
        fn new(home: PathBuf) -> Self {
            let fs = Self {
                home,
                files: Mutex::new(BTreeMap::new()),
                directories: Mutex::new(Vec::new()),
            };
            fs.directories.lock().unwrap().push(fs.home.clone());
            fs
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
            bytes.extend_from_slice(b"\x00\x00\x00\x00ICONHRDR");
            self.write(path, &bytes);
        }

        fn write_svg(&self, path: &Path, svg: &str) {
            self.write(path, svg.as_bytes());
        }

        fn canonical(&self, path: &Path) -> PathBuf {
            path.to_path_buf()
        }
    }

    impl DesktopFilesystem for MemoryFilesystem {
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
            let files = self.files.lock().unwrap();
            files.contains_key(path)
        }

        fn canonicalize_if_safe(&self, path: &Path) -> std::io::Result<PathBuf> {
            Ok(self.canonical(path))
        }

        fn home_dir(&self) -> Option<PathBuf> {
            Some(self.home.clone())
        }

        fn read(&self, path: &Path) -> std::io::Result<Option<Vec<u8>>> {
            Ok(self.files.lock().unwrap().get(path).cloned())
        }
    }

    fn harness(home: &Path) -> (MemoryFilesystem, PathBuf) {
        let fs = MemoryFilesystem::new(home.to_path_buf());
        let assets = home.join("data/assets");
        fs.mkdir(&assets);
        fs.mkdir(&assets.join("application-icons"));
        (fs, assets)
    }

    #[test]
    fn parses_minimal_desktop_entry() {
        let raw = "[Desktop Entry]\nType=Application\nName=Firefox\nStartupWMClass=Firefox\nIcon=firefox\n";
        let entry = parse_desktop_entry(raw, Path::new("/usr/share/applications/firefox.desktop"));
        assert!(entry.is_application());
        assert_eq!(entry.name.as_deref(), Some("Firefox"));
        assert_eq!(entry.startup_wm_class.as_deref(), Some("Firefox"));
        assert_eq!(entry.icon.as_deref(), Some("firefox"));
    }

    #[test]
    fn skips_non_application_and_hidden_entries() {
        let raw = "[Desktop Entry]\nType=Directory\nName=Trash\n";
        let entry = parse_desktop_entry(raw, Path::new("/usr/share/applications/trash.desktop"));
        assert!(!entry.is_application());
        let raw = "[Desktop Entry]\nType=Application\nHidden=true\nName=Hidden\n";
        let entry = parse_desktop_entry(raw, Path::new("/usr/share/applications/hidden.desktop"));
        assert!(!entry.is_application());
    }

    #[test]
    fn keeps_nodisplay_entries() {
        let raw = "[Desktop Entry]\nType=Application\nNoDisplay=true\nName=Terminal\n";
        let entry = parse_desktop_entry(raw, Path::new("/usr/share/applications/terminal.desktop"));
        assert!(entry.is_application());
        assert_eq!(entry.name.as_deref(), Some("Terminal"));
    }

    #[test]
    fn parses_localized_names_and_escapes() {
        let raw = "[Desktop Entry]\nType=Application\nName=Firefox\nName[es]=Firefox\\sNavegador\nName[en_GB]=Firefox Browser\n";
        let entry = parse_desktop_entry(raw, Path::new("/usr/share/applications/firefox.desktop"));
        assert_eq!(entry.name.as_deref(), Some("Firefox"));
        let mut locales: Vec<(&str, &str)> = entry
            .name_locale
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        locales.sort_by(|a, b| a.0.cmp(b.0));
        assert_eq!(
            locales,
            vec![("en_GB", "Firefox Browser"), ("es", "Firefox Navegador")]
        );
    }

    #[test]
    fn ignores_groups_other_than_desktop_entry() {
        let raw = "[Desktop Action new-window]\nName=New Window\n[Desktop Entry]\nType=Application\nName=Firefox\nIcon=firefox\n";
        let entry = parse_desktop_entry(raw, Path::new("/usr/share/applications/firefox.desktop"));
        assert!(entry.is_application());
        assert_eq!(entry.name.as_deref(), Some("Firefox"));
        assert_eq!(entry.icon.as_deref(), Some("firefox"));
    }

    #[test]
    fn comments_and_blank_lines_are_ignored() {
        let raw = "# leading comment\n[Desktop Entry]\n\n# inline comment\nType=Application\nName=Terminal # trailing\n";
        let entry = parse_desktop_entry(raw, Path::new("/usr/share/applications/terminal.desktop"));
        assert!(entry.is_application());
        assert!(entry.name.unwrap().starts_with("Terminal"));
    }

    #[test]
    fn startup_wm_class_matches_case_insensitively() {
        let raw = "[Desktop Entry]\nType=Application\nName=Firefox\nStartupWMClass=firefox\n";
        let entry = parse_desktop_entry(raw, Path::new("/usr/share/applications/firefox.desktop"));
        assert_eq!(
            classify(&entry, "firefox"),
            Some(MatchPriority::StartupWmClass)
        );
        assert_eq!(
            classify(&entry, "FIREFOX"),
            Some(MatchPriority::StartupWmClass)
        );
    }

    #[test]
    fn gnome_wm_class_is_second_priority() {
        let raw = "[Desktop Entry]\nType=Application\nName=Code\nX-GNOME-WMClass=Code\n";
        let entry = parse_desktop_entry(raw, Path::new("/usr/share/applications/code.desktop"));
        assert_eq!(classify(&entry, "code"), Some(MatchPriority::GnomeWmClass));
        let raw = "[Desktop Entry]\nType=Application\nName=Code\nStartupWMClass=Code\nX-GNOME-WMClass=code-oss\n";
        let entry = parse_desktop_entry(raw, Path::new("/usr/share/applications/code.desktop"));
        assert_eq!(
            classify(&entry, "code"),
            Some(MatchPriority::StartupWmClass)
        );
    }

    #[test]
    fn filename_match_falls_through() {
        let raw = "[Desktop Entry]\nType=Application\nName=Terminal\n";
        let entry = parse_desktop_entry(raw, Path::new("/usr/share/applications/terminal.desktop"));
        assert_eq!(classify(&entry, "terminal"), Some(MatchPriority::Filename));
        assert_eq!(classify(&entry, "unknown"), None);
    }

    #[test]
    fn tie_break_is_lexicographic_on_path() {
        let home = Path::new("/home/tester");
        let (fs, assets) = harness(home);
        let apps = home.join(".local/share/applications");
        fs.mkdir(&apps);
        fs.write(
            &apps.join("firefox.desktop"),
            b"[Desktop Entry]\nType=Application\nName=Firefox\n",
        );
        fs.write(
            &apps.join("firefox-bin.desktop"),
            b"[Desktop Entry]\nType=Application\nName=Firefox Bin\n",
        );
        let provider =
            LinuxApplicationMetadataProvider::with_filesystem(assets, std::sync::Arc::new(fs));
        let entry = provider
            .find_entry_with_strategy("firefox")
            .expect("match")
            .1;
        assert!(entry.path.ends_with("firefox.desktop"));
    }

    #[test]
    fn unknown_identifier_returns_no_match() {
        let home = Path::new("/home/tester");
        let (fs, assets) = harness(home);
        let apps = home.join(".local/share/applications");
        fs.mkdir(&apps);
        fs.write(
            &apps.join("firefox.desktop"),
            b"[Desktop Entry]\nType=Application\nName=Firefox\n",
        );
        let provider =
            LinuxApplicationMetadataProvider::with_filesystem(assets, std::sync::Arc::new(fs));
        assert!(provider.find_entry_with_strategy("ghost-app").is_none());
    }

    #[test]
    fn lookup_returns_display_name_without_icon_when_icon_unavailable() {
        let home = Path::new("/home/tester");
        let (fs, assets) = harness(home);
        let apps = home.join(".local/share/applications");
        fs.mkdir(&apps);
        fs.write(
            &apps.join("firefox.desktop"),
            b"[Desktop Entry]\nType=Application\nName=Firefox\nStartupWMClass=firefox\nIcon=missing-icon\n",
        );
        let provider = LinuxApplicationMetadataProvider::with_filesystem(
            assets.clone(),
            std::sync::Arc::new(fs),
        );
        let metadata = provider
            .lookup("firefox")
            .expect("ok")
            .expect("some metadata");
        assert_eq!(metadata.display_name, "Firefox");
        assert!(
            metadata.icon_ref.is_none(),
            "missing icon must not produce an icon reference"
        );
        let icons_dir = assets.join("application-icons");
        assert!(std::fs::read_dir(&icons_dir).unwrap().next().is_none());
    }

    #[test]
    fn lookup_persists_absolute_icon_when_path_is_within_allowed_roots() {
        let home = Path::new("/home/tester");
        let (fs, assets) = harness(home);
        let apps = home.join(".local/share/applications");
        let icons = home.join(".local/share/icons/128x128/apps");
        fs.mkdir(&apps);
        fs.mkdir(&icons);
        let icon_path = icons.join("firefox.png");
        fs.write_png(&icon_path);
        fs.write(
            &apps.join("firefox.desktop"),
            format!(
                "[Desktop Entry]\nType=Application\nName=Firefox\nStartupWMClass=Firefox\nIcon={}\n",
                icon_path.display()
            )
            .as_bytes(),
        );
        let provider = LinuxApplicationMetadataProvider::with_filesystem(
            assets.clone(),
            std::sync::Arc::new(fs),
        );
        let metadata = provider
            .lookup("Firefox")
            .expect("ok")
            .expect("some metadata");
        assert_eq!(metadata.display_name, "Firefox");
        let icon_ref = metadata.icon_ref.expect("icon");
        assert_eq!(icon_ref, "application-icons/Firefox.png");
        let target = assets.join("application-icons").join("Firefox.png");
        assert!(target.is_file(), "icon must be persisted to {target:?}");
    }

    #[test]
    fn lookup_resolves_theme_name_through_xdg_icon_roots() {
        let home = Path::new("/home/tester");
        let (fs, assets) = harness(home);
        let apps = home.join(".local/share/applications");
        let icons = home.join(".local/share/icons/64x64/apps");
        fs.mkdir(&apps);
        fs.mkdir(&icons);
        fs.write_png(&icons.join("firefox.png"));
        fs.write(
            &apps.join("firefox.desktop"),
            b"[Desktop Entry]\nType=Application\nName=Firefox\nIcon=firefox\n",
        );
        let provider = LinuxApplicationMetadataProvider::with_filesystem(
            assets.clone(),
            std::sync::Arc::new(fs),
        );
        let metadata = provider
            .lookup("firefox")
            .expect("ok")
            .expect("some metadata");
        let icon_ref = metadata.icon_ref.expect("icon");
        assert_eq!(icon_ref, "application-icons/firefox.png");
    }

    #[test]
    fn lookup_rejects_absolute_icon_outside_allowed_roots() {
        let home = Path::new("/home/tester");
        let (fs, assets) = harness(home);
        let apps = home.join(".local/share/applications");
        fs.mkdir(&apps);
        let outside = home.join("outside.png");
        fs.write_png(&outside);
        fs.write(
            &apps.join("firefox.desktop"),
            format!(
                "[Desktop Entry]\nType=Application\nName=Firefox\nIcon={}\n",
                outside.display()
            )
            .as_bytes(),
        );
        let provider = LinuxApplicationMetadataProvider::with_filesystem(
            assets.clone(),
            std::sync::Arc::new(fs),
        );
        let metadata = provider
            .lookup("firefox")
            .expect("ok")
            .expect("some metadata");
        assert!(
            metadata.icon_ref.is_none(),
            "out-of-scope icon must not be copied"
        );
        let icons_dir = assets.join("application-icons");
        assert!(std::fs::read_dir(&icons_dir).unwrap().next().is_none());
    }

    #[test]
    #[cfg(not(feature = "linux-svg-raster"))]
    fn lookup_rejects_svg_icon_when_rasterizer_feature_disabled() {
        let home = Path::new("/home/tester");
        let (fs, assets) = harness(home);
        let apps = home.join(".local/share/applications");
        let icons = home.join(".local/share/icons/hicolor/scalable/apps");
        fs.mkdir(&apps);
        fs.mkdir(&icons);
        fs.write(&icons.join("firefox.svg"), b"<svg/>");
        fs.write(
            &apps.join("firefox.desktop"),
            b"[Desktop Entry]\nType=Application\nName=Firefox\nIcon=firefox\n",
        );
        let provider = LinuxApplicationMetadataProvider::with_filesystem(
            assets.clone(),
            std::sync::Arc::new(fs),
        );
        let metadata = provider
            .lookup("firefox")
            .expect("ok")
            .expect("some metadata");
        assert!(
            metadata.icon_ref.is_none(),
            "svg icon must not be persisted without the rasterizer feature"
        );
        let icons_dir = assets.join("application-icons");
        assert!(std::fs::read_dir(&icons_dir).unwrap().next().is_none());
    }

    #[test]
    fn lookup_replaces_existing_icon_only_with_a_new_valid_one() {
        let home = Path::new("/home/tester");
        let (fs, assets) = harness(home);
        let apps = home.join(".local/share/applications");
        let icons = home.join(".local/share/icons/128x128/apps");
        fs.mkdir(&apps);
        fs.mkdir(&icons);
        let icon_path = icons.join("firefox.png");
        fs.write_png(&icon_path);
        let existing_target = assets.join("application-icons/firefox.png");
        std::fs::write(&existing_target, b"PRESERVE_ME").unwrap();
        fs.write(
            &apps.join("firefox.desktop"),
            b"[Desktop Entry]\nType=Application\nName=Firefox\n",
        );
        let provider = LinuxApplicationMetadataProvider::with_filesystem(
            assets.clone(),
            std::sync::Arc::new(fs),
        );
        let metadata = provider
            .lookup("firefox")
            .expect("ok")
            .expect("some metadata");
        assert!(metadata.icon_ref.is_none());
        let bytes = std::fs::read(&existing_target).unwrap();
        assert_eq!(bytes, b"PRESERVE_ME");
    }

    #[test]
    fn lookup_uses_localized_name_when_locale_matches() {
        let home = Path::new("/home/tester");
        let (fs, assets) = harness(home);
        let apps = home.join(".local/share/applications");
        fs.mkdir(&apps);
        fs.write(
            &apps.join("firefox.desktop"),
            b"[Desktop Entry]\nType=Application\nName=Firefox\nName[es]=Firefox Navegador\nStartupWMClass=firefox\n",
        );
        let provider =
            LinuxApplicationMetadataProvider::with_filesystem(assets, std::sync::Arc::new(fs));
        std::env::set_var("LANG", "es_ES.UTF-8");
        std::env::set_var("LC_MESSAGES", "es_ES.UTF-8");
        let metadata = provider
            .lookup("firefox")
            .expect("ok")
            .expect("some metadata");
        assert_eq!(metadata.display_name, "Firefox Navegador");
        std::env::remove_var("LANG");
        std::env::remove_var("LC_MESSAGES");
    }

    #[test]
    fn lookup_falls_back_to_generic_name_when_no_locale_matches() {
        let home = Path::new("/home/tester");
        let (fs, assets) = harness(home);
        let apps = home.join(".local/share/applications");
        fs.mkdir(&apps);
        fs.write(
            &apps.join("firefox.desktop"),
            b"[Desktop Entry]\nType=Application\nName=Firefox\nName[fr]=Navigateur\nStartupWMClass=firefox\n",
        );
        let provider =
            LinuxApplicationMetadataProvider::with_filesystem(assets, std::sync::Arc::new(fs));
        std::env::set_var("LANG", "es_ES.UTF-8");
        let metadata = provider
            .lookup("firefox")
            .expect("ok")
            .expect("some metadata");
        assert_eq!(metadata.display_name, "Firefox");
        std::env::remove_var("LANG");
    }

    #[test]
    fn lookup_returns_none_for_empty_or_whitespace_identifier() {
        let home = Path::new("/home/tester");
        let (fs, assets) = harness(home);
        let provider =
            LinuxApplicationMetadataProvider::with_filesystem(assets, std::sync::Arc::new(fs));
        assert!(matches!(provider.lookup(""), Ok(None)));
        assert!(matches!(provider.lookup("   "), Ok(None)));
    }

    #[test]
    fn lookup_returns_none_when_entry_has_no_display_name() {
        let home = Path::new("/home/tester");
        let (fs, assets) = harness(home);
        let apps = home.join(".local/share/applications");
        fs.mkdir(&apps);
        fs.write(
            &apps.join("firefox.desktop"),
            b"[Desktop Entry]\nType=Application\nStartupWMClass=firefox\nIcon=missing\n",
        );
        let provider =
            LinuxApplicationMetadataProvider::with_filesystem(assets, std::sync::Arc::new(fs));
        let metadata = provider.lookup("firefox").expect("ok");
        assert!(metadata.is_none());
    }

    #[test]
    fn provider_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<LinuxApplicationMetadataProvider>();
    }

    #[test]
    fn provider_name_is_stable() {
        let home = Path::new("/home/tester");
        let (fs, assets) = harness(home);
        let provider =
            LinuxApplicationMetadataProvider::with_filesystem(assets, std::sync::Arc::new(fs));
        assert_eq!(provider.name(), "linux_app_metadata");
    }

    #[test]
    fn unescape_handles_canonical_sequences() {
        assert_eq!(unescape_desktop_value("a\\sb"), "a b");
        assert_eq!(unescape_desktop_value("a\\nb"), "a\nb");
        assert_eq!(unescape_desktop_value("a\\\\b"), "a\\b");
        assert_eq!(unescape_desktop_value("a\\xb"), "a\\xb");
    }

    #[test]
    fn write_icon_atomic_persists_and_cleans_temporary() {
        let dir = tempfile::tempdir().expect("tempdir");
        let bytes = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, b'O', b'K'];
        let icon_ref = write_icon_atomic(dir.path(), "firefox", &bytes).expect("write");
        assert_eq!(icon_ref, "application-icons/firefox.png");
        let target = dir.path().join("firefox.png");
        assert_eq!(std::fs::read(&target).expect("read"), bytes);
        let mut remaining = std::fs::read_dir(dir.path()).unwrap();
        let leftover = remaining.find(|entry| {
            entry
                .as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains(".tmp")
        });
        assert!(leftover.is_none());
    }

    #[test]
    fn write_icon_atomic_rejects_invalid_png() {
        let dir = tempfile::tempdir().expect("tempdir");
        let err = write_icon_atomic(dir.path(), "firefox", b"definitely-not-a-png")
            .expect_err("must reject");
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        let mut remaining = std::fs::read_dir(dir.path()).unwrap();
        let leftover = remaining.find(|entry| {
            entry
                .as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains(".tmp")
        });
        assert!(leftover.is_none(), "temporary must be cleaned up");
    }

    #[test]
    fn sanitize_identifier_replaces_unsafe_characters() {
        assert_eq!(sanitize_identifier("firefox"), "firefox");
        assert_eq!(sanitize_identifier("a/b c"), "a_b_c");
        assert_eq!(sanitize_identifier(""), "app");
    }

    #[test]
    fn data_roots_falls_back_to_local_share_when_xdg_data_home_is_unset() {
        let fs = MemoryFilesystem::new(PathBuf::from("/home/tester"));
        let roots = data_roots(&fs);
        assert!(
            roots
                .iter()
                .any(|root| root == &PathBuf::from("/home/tester/.local/share")),
            "missing $HOME/.local/share fallback: {roots:?}"
        );
    }

    #[test]
    fn data_roots_respects_xdg_data_home_when_set() {
        let fs = MemoryFilesystem::new(PathBuf::from("/home/tester"));
        // SAFETY: tests are single-threaded for env mutations and
        // restore the previous value before returning.
        unsafe {
            std::env::set_var("XDG_DATA_HOME", "/custom/share");
        }
        let roots = data_roots(&fs);
        unsafe {
            std::env::remove_var("XDG_DATA_HOME");
        }
        assert!(roots
            .iter()
            .any(|root| root == &PathBuf::from("/custom/share")));
    }

    #[test]
    fn data_roots_deduplicates_entries() {
        let fs = MemoryFilesystem::new(PathBuf::from("/home/tester"));
        unsafe {
            std::env::set_var("XDG_DATA_DIRS", "/usr/share:/usr/share:/usr/local/share");
        }
        let roots = data_roots(&fs);
        unsafe {
            std::env::remove_var("XDG_DATA_DIRS");
        }
        let deduped: Vec<_> = roots
            .iter()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        assert_eq!(roots, deduped, "data_roots must deduplicate entries");
    }

    #[test]
    fn data_roots_appends_usr_share_fallback_when_xdg_data_dirs_empty() {
        let fs = MemoryFilesystem::new(PathBuf::from("/home/tester"));
        unsafe {
            std::env::set_var("XDG_DATA_DIRS", "");
        }
        let roots = data_roots(&fs);
        unsafe {
            std::env::remove_var("XDG_DATA_DIRS");
        }
        assert!(roots
            .iter()
            .any(|root| root == &PathBuf::from("/usr/local/share")));
        assert!(roots
            .iter()
            .any(|root| root == &PathBuf::from("/usr/share")));
    }

    #[test]
    fn collect_icon_apps_dirs_never_produces_duplicated_icons_segment() {
        // Pin the regression the previous collector emitted
        // (`<root>/icons/icons/...`). With the refactor every
        // directory in the cached list must contain at most one
        // `icons` segment.
        let home = PathBuf::from("/home/tester");
        let fs = MemoryFilesystem::new(home.clone());
        let icons = home.join(".local/share/icons/hicolor/48x48/apps");
        fs.mkdir(&home.join(".local/share/icons"));
        fs.mkdir(&icons);
        let assets = home.join("assets");
        fs.mkdir(&assets);
        let provider =
            LinuxApplicationMetadataProvider::with_filesystem(assets, std::sync::Arc::new(fs));
        for dir in &provider.icon_apps_dirs {
            let segments: Vec<_> = dir
                .components()
                .filter_map(|component| match component {
                    std::path::Component::Normal(os) => Some(os.to_string_lossy().into_owned()),
                    _ => None,
                })
                .collect();
            let occurrences = segments
                .iter()
                .filter(|segment| *segment == "icons")
                .count();
            assert!(
                occurrences <= 1,
                "path {dir:?} contains {occurrences} `icons` segments"
            );
        }
    }

    #[test]
    fn collect_icon_apps_dirs_walks_hicolor_theme() {
        let home = PathBuf::from("/home/tester");
        let fs = MemoryFilesystem::new(home.clone());
        let icons = home.join(".local/share/icons/hicolor/128x128/apps");
        fs.mkdir(&home.join(".local/share/icons"));
        fs.mkdir(&icons);
        fs.write_png(&icons.join("firefox.png"));
        let assets = home.join("assets");
        fs.mkdir(&assets);
        let provider =
            LinuxApplicationMetadataProvider::with_filesystem(assets, std::sync::Arc::new(fs));
        assert!(
            provider
                .icon_apps_dirs
                .iter()
                .any(|dir| dir.ends_with("hicolor/128x128/apps")),
            "missing hicolor theme path: {:?}",
            provider.icon_apps_dirs
        );
    }

    #[test]
    fn collect_icon_apps_dirs_walks_scalable_theme_layout() {
        let home = PathBuf::from("/home/tester");
        let fs = MemoryFilesystem::new(home.clone());
        let scalable = home.join(".local/share/icons/hicolor/scalable/apps");
        fs.mkdir(&home.join(".local/share/icons"));
        fs.mkdir(&scalable);
        fs.write_png(&scalable.join("firefox.png"));
        let assets = home.join("assets");
        fs.mkdir(&assets);
        let provider =
            LinuxApplicationMetadataProvider::with_filesystem(assets, std::sync::Arc::new(fs));
        assert!(
            provider
                .icon_apps_dirs
                .iter()
                .any(|dir| dir.ends_with("hicolor/scalable/apps")),
            "missing scalable theme path: {:?}",
            provider.icon_apps_dirs
        );
    }

    #[test]
    fn collect_icon_apps_dirs_walks_pixmaps_namespace() {
        let home = PathBuf::from("/home/tester");
        let fs = MemoryFilesystem::new(home.clone());
        let pixmaps = home.join(".local/share/pixmaps");
        fs.mkdir(&pixmaps);
        fs.write_png(&pixmaps.join("firefox.png"));
        let assets = home.join("assets");
        fs.mkdir(&assets);
        let provider =
            LinuxApplicationMetadataProvider::with_filesystem(assets, std::sync::Arc::new(fs));
        assert!(
            provider
                .pixmap_dirs
                .iter()
                .any(|dir| dir.ends_with("pixmaps")),
            "missing pixmaps namespace: {:?}",
            provider.pixmap_dirs
        );
    }

    #[test]
    fn resolve_icon_prefers_png_over_svg_for_the_same_icon() {
        let home = PathBuf::from("/home/tester");
        let (fs, assets) = harness(&home);
        let apps = home.join(".local/share/applications");
        let png_apps = home.join(".local/share/icons/hicolor/48x48/apps");
        let svg_apps = home.join(".local/share/icons/hicolor/scalable/apps");
        fs.mkdir(&apps);
        fs.mkdir(&home.join(".local/share/icons"));
        fs.mkdir(&png_apps);
        fs.mkdir(&svg_apps);
        fs.write_png(&png_apps.join("firefox.png"));
        fs.write_svg(
            &svg_apps.join("firefox.svg"),
            "<?xml version=\"1.0\"?><svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"16\"><rect width=\"16\" height=\"16\" fill=\"#abcdef\"/></svg>",
        );
        fs.write(
            &apps.join("firefox.desktop"),
            b"[Desktop Entry]\nType=Application\nName=Firefox\nIcon=firefox\n",
        );
        let provider = LinuxApplicationMetadataProvider::with_filesystem(
            assets.clone(),
            std::sync::Arc::new(fs),
        );
        let metadata = provider
            .lookup("firefox")
            .expect("ok")
            .expect("some metadata");
        let icon_ref = metadata.icon_ref.expect("icon");
        assert_eq!(icon_ref, "application-icons/firefox.png");
        let diagnostics = provider.last_icon_diagnostics();
        assert_eq!(diagnostics.kind, IconSourceKind::Png);
        assert!(!diagnostics.rasterization_attempted);
    }

    #[test]
    #[cfg(feature = "linux-svg-raster")]
    fn resolve_icon_falls_back_to_svg_when_only_svg_is_present() {
        let home = PathBuf::from("/home/tester");
        let (fs, assets) = harness(&home);
        let apps = home.join(".local/share/applications");
        let scalable = home.join(".local/share/icons/hicolor/scalable/apps");
        fs.mkdir(&apps);
        fs.mkdir(&home.join(".local/share/icons"));
        fs.mkdir(&scalable);
        fs.write_svg(
            &scalable.join("firefox.svg"),
            "<?xml version=\"1.0\"?><svg xmlns=\"http://www.w3.org/2000/svg\" width=\"32\" height=\"32\"><rect width=\"32\" height=\"32\" fill=\"#abcdef\"/></svg>",
        );
        fs.write(
            &apps.join("firefox.desktop"),
            b"[Desktop Entry]\nType=Application\nName=Firefox\nIcon=firefox\n",
        );
        let provider = LinuxApplicationMetadataProvider::with_filesystem(
            assets.clone(),
            std::sync::Arc::new(fs),
        );
        let metadata = provider
            .lookup("firefox")
            .expect("ok")
            .expect("some metadata");
        let icon_ref = metadata.icon_ref.expect("icon");
        assert_eq!(icon_ref, "application-icons/firefox.png");
        let target = assets.join(&icon_ref);
        let bytes = std::fs::read(&target).expect("read");
        assert!(looks_like_png(&bytes));
        let diagnostics = provider.last_icon_diagnostics();
        assert_eq!(diagnostics.kind, IconSourceKind::Svg);
        assert!(diagnostics.rasterization_attempted);
        assert!(diagnostics.rasterization_succeeded);
    }

    #[test]
    #[cfg(feature = "linux-svg-raster")]
    fn resolve_icon_reports_invalid_svg_failure() {
        let home = PathBuf::from("/home/tester");
        let (fs, assets) = harness(&home);
        let apps = home.join(".local/share/applications");
        let scalable = home.join(".local/share/icons/hicolor/scalable/apps");
        fs.mkdir(&apps);
        fs.mkdir(&home.join(".local/share/icons"));
        fs.mkdir(&scalable);
        fs.write(&scalable.join("firefox.svg"), b"<<not svg>>");
        fs.write(
            &apps.join("firefox.desktop"),
            b"[Desktop Entry]\nType=Application\nName=Firefox\nIcon=firefox\n",
        );
        let provider = LinuxApplicationMetadataProvider::with_filesystem(
            assets.clone(),
            std::sync::Arc::new(fs),
        );
        let metadata = provider
            .lookup("firefox")
            .expect("ok")
            .expect("some metadata");
        assert!(metadata.icon_ref.is_none());
        let diagnostics = provider.last_icon_diagnostics();
        assert_eq!(diagnostics.failure_kind, IconFailureKind::InvalidSvg);
    }

    #[test]
    fn lookup_falls_back_to_pixmaps_layout() {
        let home = PathBuf::from("/home/tester");
        let (fs, assets) = harness(&home);
        let apps = home.join(".local/share/applications");
        let pixmaps = home.join(".local/share/pixmaps");
        fs.mkdir(&apps);
        fs.mkdir(&pixmaps);
        fs.write_png(&pixmaps.join("firefox.png"));
        fs.write(
            &apps.join("firefox.desktop"),
            b"[Desktop Entry]\nType=Application\nName=Firefox\nIcon=firefox\n",
        );
        let provider = LinuxApplicationMetadataProvider::with_filesystem(
            assets.clone(),
            std::sync::Arc::new(fs),
        );
        let metadata = provider
            .lookup("firefox")
            .expect("ok")
            .expect("some metadata");
        let icon_ref = metadata.icon_ref.expect("icon");
        assert_eq!(icon_ref, "application-icons/firefox.png");
        let diagnostics = provider.last_icon_diagnostics();
        assert_eq!(diagnostics.kind, IconSourceKind::Pixmap);
    }

    #[test]
    fn lookup_with_no_icon_declared_keeps_display_name() {
        let home = PathBuf::from("/home/tester");
        let (fs, assets) = harness(&home);
        let apps = home.join(".local/share/applications");
        fs.mkdir(&apps);
        fs.write(
            &apps.join("ghost.desktop"),
            b"[Desktop Entry]\nType=Application\nName=Ghost\nStartupWMClass=ghost\n",
        );
        let provider = LinuxApplicationMetadataProvider::with_filesystem(
            assets.clone(),
            std::sync::Arc::new(fs),
        );
        let metadata = provider
            .lookup("ghost")
            .expect("ok")
            .expect("some metadata");
        assert_eq!(metadata.display_name, "Ghost");
        assert!(metadata.icon_ref.is_none());
        let diagnostics = provider.last_icon_diagnostics();
        assert!(!diagnostics.declared);
        assert!(!diagnostics.resolved);
    }

    #[test]
    fn lookup_with_wm_class_distinct_from_display_name_matches_via_startup_wm_class() {
        let home = PathBuf::from("/home/tester");
        let (fs, assets) = harness(&home);
        let apps = home.join(".local/share/applications");
        fs.mkdir(&apps);
        // `dev.warp.Warp` is the identifier the active-app probe
        // exposes for Warp; the visible name is "Warp Terminal".
        fs.write(
            &apps.join("warp.desktop"),
            b"[Desktop Entry]\nType=Application\nName=Warp Terminal\nStartupWMClass=dev.warp.Warp\nIcon=warp\n",
        );
        let provider =
            LinuxApplicationMetadataProvider::with_filesystem(assets, std::sync::Arc::new(fs));
        let metadata = provider
            .lookup("dev.warp.Warp")
            .expect("ok")
            .expect("some metadata");
        assert_eq!(metadata.display_name, "Warp Terminal");
        assert_eq!(
            provider.last_match_strategy(),
            MatchStrategy::StartupWmClass
        );
    }

    #[test]
    fn lookup_with_source_app_distinct_from_visible_name_resolves_metadata() {
        // Same as the test above but using `X-GNOME-WMClass` to
        // confirm the second-priority branch also resolves the
        // metadata.
        let home = PathBuf::from("/home/tester");
        let (fs, assets) = harness(&home);
        let apps = home.join(".local/share/applications");
        fs.mkdir(&apps);
        fs.write(
            &apps.join("gnome-terminal.desktop"),
            b"[Desktop Entry]\nType=Application\nName=Terminal\nX-GNOME-WMClass=gnome-terminal\n",
        );
        let provider =
            LinuxApplicationMetadataProvider::with_filesystem(assets, std::sync::Arc::new(fs));
        let metadata = provider
            .lookup("gnome-terminal")
            .expect("ok")
            .expect("some metadata");
        assert_eq!(metadata.display_name, "Terminal");
        assert_eq!(provider.last_match_strategy(), MatchStrategy::XGnomeWmClass);
    }

    #[test]
    fn lookup_with_arbitrary_application_resolves_name() {
        // Pin the "multiple distinct apps" scenario: the provider
        // does not whitelist any single application; the matcher
        // accepts every `.desktop` it walks.
        let home = PathBuf::from("/home/tester");
        let (fs, assets) = harness(&home);
        let apps = home.join(".local/share/applications");
        fs.mkdir(&apps);
        for (basename, name, wm_class) in [
            ("firefox", "Firefox", "firefox"),
            ("gnome-terminal", "Terminal", "gnome-terminal"),
            ("code", "Code", "code"),
            ("warp", "Warp Terminal", "dev.warp.Warp"),
        ] {
            let desktop = format!(
                "[Desktop Entry]\nType=Application\nName={name}\nStartupWMClass={wm_class}\nIcon={basename}\n"
            );
            fs.write(
                &apps.join(format!("{basename}.desktop")),
                desktop.as_bytes(),
            );
        }
        let provider = LinuxApplicationMetadataProvider::with_filesystem(
            assets.clone(),
            std::sync::Arc::new(fs),
        );
        for identifier in ["firefox", "gnome-terminal", "code", "dev.warp.Warp"] {
            let result = provider.lookup(identifier).expect("ok");
            assert!(result.is_some(), "identifier {identifier} must resolve");
        }
    }

    #[test]
    fn lookup_creates_application_icons_only_on_success() {
        let home = PathBuf::from("/home/tester");
        let (fs, assets) = harness(&home);
        let apps = home.join(".local/share/applications");
        fs.mkdir(&apps);
        fs.write(
            &apps.join("ghost.desktop"),
            b"[Desktop Entry]\nType=Application\nName=Ghost\nStartupWMClass=ghost\n",
        );
        let provider = LinuxApplicationMetadataProvider::with_filesystem(
            assets.clone(),
            std::sync::Arc::new(fs),
        );
        let _ = provider.lookup("ghost").expect("ok");
        let icons_dir = assets.join("application-icons");
        assert!(std::fs::read_dir(&icons_dir).unwrap().next().is_none());
    }

    #[test]
    fn lookup_reports_out_of_roots_failure_for_absolute_paths() {
        let home = PathBuf::from("/home/tester");
        let (fs, assets) = harness(&home);
        let apps = home.join(".local/share/applications");
        fs.mkdir(&apps);
        let outside = home.join("outside.png");
        fs.write_png(&outside);
        fs.write(
            &apps.join("firefox.desktop"),
            format!(
                "[Desktop Entry]\nType=Application\nName=Firefox\nIcon={}\n",
                outside.display()
            )
            .as_bytes(),
        );
        let provider = LinuxApplicationMetadataProvider::with_filesystem(
            assets.clone(),
            std::sync::Arc::new(fs),
        );
        let metadata = provider
            .lookup("firefox")
            .expect("ok")
            .expect("some metadata");
        assert!(metadata.icon_ref.is_none());
        let diagnostics = provider.last_icon_diagnostics();
        assert_eq!(diagnostics.failure_kind, IconFailureKind::NotFound);
    }
}
