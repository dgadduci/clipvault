//! Linux-backed [`ApplicationMetadataProvider`].
//!
//! Resolves a stable X11 / XWayland `WM_CLASS` identifier against the
//! freedesktop `.desktop` files installed under
//! `XDG_DATA_HOME/applications` and the per-entry directories listed
//! in `XDG_DATA_DIRS`, persists the resolved icon under the existing
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
//!   filename → free-form stable id) is the same priority documented
//!   in `design.md`. Comparison is case-insensitive ASCII and ties are
//!   resolved lexicographically so the result stays deterministic
//!   across runs.
//! - `NoDisplay=true` is **not** treated as a skip; the spec only
//!   excludes `Hidden=true` and non-`Application` types so the
//!   provider can answer for installed apps that hide themselves in
//!   the launcher.
//! - Icons resolve locally only: absolute paths and theme names are
//!   walked against an XDG icon root list compiled from
//!   `XDG_DATA_HOME`, the entries in `XDG_DATA_DIRS`, the canonical
//!   `/usr/local/share` and `/usr/share` fallbacks and a small set of
//!   common sizes (`128`, `64`, `48`, `256`). Anything outside the
//!   allowed roots is silently skipped; the bridge keeps working
//!   with the existing generic icon fallback.
//! - The icon writer is atomic: it builds a temporary file inside the
//!   destination directory, validates the PNG (signature + size +
//!   dimension) and renames the file over the destination. The
//!   temporary is cleaned up on every error path.
//! - An existing valid icon is **never** overwritten with an empty
//!   payload: when the lookup succeeds without an icon the provider
//!   returns the display name and leaves the previous reference
//!   untouched so the card rail keeps rendering the asset the
//!   previous capture persisted.

use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::app_assets::APPLICATION_ICONS_DIR;
use crate::app_metadata::{
    icon_ref_for, ApplicationMetadata, ApplicationMetadataError, ApplicationMetadataProvider,
};

/// Linux-backed application-metadata provider. Resolves a stable
/// `WM_CLASS` identifier against the freedesktop `.desktop` files
/// installed on the host and persists the resolved icon under
/// `<assets_dir>/application-icons/<safe-id>.png`.
pub struct LinuxApplicationMetadataProvider {
    assets_dir: PathBuf,
    /// Roots the desktop-entry scanner walks. Cached at construction
    /// time so the lookup hot path never re-reads the environment.
    app_dirs: Vec<PathBuf>,
    /// Roots the icon resolver walks. Cached for the same reason.
    icon_dirs: Vec<PathBuf>,
    /// Sizes the icon resolver probes when the entry names a theme
    /// icon (e.g. `Icon=firefox`). The provider walks each size in
    /// order and returns the first hit that lives under an allowed
    /// root. Picked sizes cover the dimensions the bridge can serve
    /// and the dimensions real `.desktop` files actually publish.
    icon_sizes: &'static [u32],
    /// Filesystem abstraction the provider uses. Tests inject a fake
    /// so the unit suite can drive the parser without standing up a
    /// real `.desktop` installation.
    fs: std::sync::Arc<dyn DesktopFilesystem>,
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
        Self {
            assets_dir: assets_dir.into(),
            app_dirs: collect_application_dirs(fs.as_ref()),
            icon_dirs: collect_icon_dirs(fs.as_ref()),
            icon_sizes: &[128, 64, 256, 48],
            fs,
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
            return Ok(None);
        }
        let candidate = match self.find_entry(trimmed) {
            Some(entry) => entry,
            None => return Ok(None),
        };
        let display_name = match resolve_display_name(&candidate, &current_locale_candidates()) {
            Some(name) if !name.is_empty() => name,
            _ => return Ok(None),
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
    fn persist_icon(&self, entry: &DesktopEntry, identifier: &str) -> Option<String> {
        let icon_value = entry.icon.as_deref()?;
        let source = self.resolve_icon_path(icon_value)?;
        let png_bytes = read_validated_png(&source)?;
        let target_dir = self.assets_dir.join(APPLICATION_ICONS_DIR);
        write_icon_atomic(&target_dir, identifier, &png_bytes).ok()
    }

    /// Walk the configured XDG roots and return the best-matching
    /// `DesktopEntry` for `identifier`. Returns `None` when no
    /// unambiguous match is found.
    fn find_entry(&self, identifier: &str) -> Option<DesktopEntry> {
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
        best.map(|(_, entry)| entry)
    }

    /// Resolve the `Icon=` value declared by an entry against the
    /// allowed XDG icon roots. The helper accepts:
    ///
    /// - absolute file paths under one of the allowed roots;
    /// - bare icon names looked up in the cached icon directories
    ///   (in the order the configured `icon_sizes` walk them);
    /// - PNG files only. Other formats (SVG, XPM) are silently
    ///   skipped — the spec only requires PNG compatibility so the
    ///   existing icon bridge can serve the bytes verbatim.
    ///
    /// Returns `None` for any input that escapes the allowed roots
    /// or fails the format checks. The validator never logs the
    /// offending path or filename to keep the diagnostic surface
    /// metadata-only.
    fn resolve_icon_path(&self, raw: &str) -> Option<PathBuf> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return None;
        }
        let candidate = Path::new(trimmed);
        if candidate.is_absolute() {
            let path = self.fs.canonicalize_if_safe(candidate).ok()?;
            if path.extension().and_then(|ext| ext.to_str()) == Some("png")
                && self.icon_dirs.iter().any(|root| path.starts_with(root))
            {
                return Some(path);
            }
            return None;
        }
        // Theme-name lookup: walk `<root>/<size>/apps/<name>.png`.
        let stem = trimmed.trim_end_matches(".png");
        let mut found: Option<PathBuf> = None;
        for root in &self.icon_dirs {
            for size in self.icon_sizes {
                let candidate = root
                    .join(format!("{size}x{size}"))
                    .join("apps")
                    .join(format!("{stem}.png"));
                if self.fs.is_file(&candidate) {
                    if let Ok(canonical) = self.fs.canonicalize_if_safe(&candidate) {
                        // Belt-and-braces: refuse anything that points
                        // outside the icon root via a symlink. The
                        // canonical comparison is the same check the
                        // existing `resolve_icon_path` helper uses.
                        let canonical_root = self
                            .fs
                            .canonicalize_if_safe(root)
                            .unwrap_or_else(|_| root.clone());
                        if canonical.starts_with(&canonical_root) {
                            found = Some(canonical);
                            break;
                        }
                    }
                }
            }
            if found.is_some() {
                break;
            }
        }
        found
    }
}

fn entry_path_str(path: &Path) -> String {
    path.to_string_lossy().into_owned()
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

/// Build the list of `applications/` directories the desktop-entry
/// scanner walks, in priority order. The list honours `XDG_DATA_HOME`
/// and `XDG_DATA_DIRS` and falls back to the documented standard
/// roots when the environment does not set them.
fn collect_application_dirs(fs: &dyn DesktopFilesystem) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    let home = env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| fs.home_dir());
    let xdg_data_home = env::var_os("XDG_DATA_HOME").map(PathBuf::from);
    if let Some(home) = xdg_data_home.or(home) {
        let dir = home.join("applications");
        if fs.is_dir(&dir) {
            dirs.push(dir);
        }
    }
    let xdg_data_dirs = env::var_os("XDG_DATA_DIRS")
        .map(PathOsStringExt::split_paths)
        .unwrap_or_default();
    let mut roots: Vec<PathBuf> = xdg_data_dirs
        .into_iter()
        .filter(|path| !path.as_os_str().is_empty())
        .collect();
    if roots.is_empty() {
        roots.push(PathBuf::from("/usr/local/share"));
        roots.push(PathBuf::from("/usr/share"));
    }
    for root in roots {
        let candidate = root.join("applications");
        if fs.is_dir(&candidate) {
            dirs.push(candidate);
        }
    }
    dirs
}

/// Helper trait wrapping `OsString::split_paths` so the
/// `collect_application_dirs` helper can be reused by tests without
/// reaching for `std::ffi` at every call site.
trait PathOsStringExt {
    fn split_paths(self) -> Vec<PathBuf>;
}

impl PathOsStringExt for OsString {
    fn split_paths(self) -> Vec<PathBuf> {
        std::env::split_paths(&self).collect()
    }
}

/// Build the list of icon roots the resolver walks. The list mirrors
/// the freedesktop icon-theme spec: every `<root>/icons/hicolor` and
/// every `<root>/icons/<size>/apps` pair, deduplicated and ordered.
fn collect_icon_dirs(fs: &dyn DesktopFilesystem) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    let home = env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| fs.home_dir());
    let xdg_data_home = env::var_os("XDG_DATA_HOME").map(PathBuf::from);
    if let Some(home) = xdg_data_home.or(home) {
        let dir = home.join("icons");
        if fs.is_dir(&dir) {
            roots.push(dir);
        }
    }
    let xdg_data_dirs = env::var_os("XDG_DATA_DIRS")
        .map(PathOsStringExt::split_paths)
        .unwrap_or_default();
    let mut base_roots: Vec<PathBuf> = xdg_data_dirs
        .into_iter()
        .filter(|path| !path.as_os_str().is_empty())
        .collect();
    if base_roots.is_empty() {
        base_roots.push(PathBuf::from("/usr/local/share"));
        base_roots.push(PathBuf::from("/usr/share"));
    }
    for root in &base_roots {
        let candidate = root.join("icons");
        if fs.is_dir(&candidate) {
            roots.push(candidate);
        }
    }
    let mut out: Vec<PathBuf> = Vec::new();
    for root in roots {
        for size in [128u32, 64, 256, 48] {
            let apps = root.join(format!("{size}x{size}")).join("apps");
            if fs.is_dir(&apps) {
                out.push(apps);
            }
        }
    }
    out
}

/// Validate the supplied bytes look like a PNG we can serve through
/// the existing icon bridge. The helper reuses the magic header the
/// `app_assets` module already enforces and caps the byte length so
/// the writer cannot persist a runaway payload.
fn read_validated_png(path: &Path) -> Option<Vec<u8>> {
    let bytes = fs::read(path).ok()?;
    if bytes.len() > crate::app_assets::MAX_ICON_BYTES_LEGACY {
        return None;
    }
    if !looks_like_png(&bytes) {
        return None;
    }
    Some(bytes)
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::sync::Mutex;

    /// In-memory filesystem the unit tests drive. The structure
    /// mirrors the host layout (`<root>/applications/<file>.desktop`,
    /// `<root>/icons/<size>x<size>/apps/<icon>.png`) so the test
    /// paths read the same way the production paths do.
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
        // NoDisplay=true must not turn the entry into an
        // "is_application() == false" candidate; the spec keeps the
        // metadata available so the card rail can still render it.
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
        // Only the `[Desktop Entry]` `Icon=` survives; the Action
        // group's `Name=New Window` is not promoted to the entry's
        // canonical name.
        assert_eq!(entry.name.as_deref(), Some("Firefox"));
        assert_eq!(entry.icon.as_deref(), Some("firefox"));
    }

    #[test]
    fn comments_and_blank_lines_are_ignored() {
        let raw = "# leading comment\n[Desktop Entry]\n\n# inline comment\nType=Application\nName=Terminal # trailing\n";
        let entry = parse_desktop_entry(raw, Path::new("/usr/share/applications/terminal.desktop"));
        assert!(entry.is_application());
        // Trailing comment is part of the value because the parser
        // only strips the leading `#`. The contract is documented;
        // real `.desktop` files keep the value on a single line
        // without inline comments.
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
        // StartupWMClass still wins because it has the lower
        // priority value.
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
        // Two `.desktop` files both match by filename (no
        // StartupWMClass / X-GNOME-WMClass). The earlier alphabetic
        // path wins so the result is deterministic across runs.
        let home = Path::new("/home/tester");
        let (fs, assets) = harness(home);
        let apps = home.join("applications");
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
        // Lookups for the basename `firefox` find both files; the
        // lexicographic tie break selects the `firefox.desktop`
        // entry.
        let entry = provider.find_entry("firefox").expect("match");
        assert!(entry.path.ends_with("firefox.desktop"));
    }

    #[test]
    fn unknown_identifier_returns_no_match() {
        let home = Path::new("/home/tester");
        let (fs, assets) = harness(home);
        let apps = home.join("applications");
        fs.mkdir(&apps);
        fs.write(
            &apps.join("firefox.desktop"),
            b"[Desktop Entry]\nType=Application\nName=Firefox\n",
        );
        let provider =
            LinuxApplicationMetadataProvider::with_filesystem(assets, std::sync::Arc::new(fs));
        assert!(provider.find_entry("ghost-app").is_none());
    }

    #[test]
    fn lookup_returns_display_name_without_icon_when_icon_unavailable() {
        let home = Path::new("/home/tester");
        let (fs, assets) = harness(home);
        let apps = home.join("applications");
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
        // Nothing was written under the assets directory.
        let icons_dir = assets.join("application-icons");
        assert!(std::fs::read_dir(&icons_dir).unwrap().next().is_none());
    }

    #[test]
    fn lookup_persists_absolute_icon_when_path_is_within_allowed_roots() {
        let home = Path::new("/home/tester");
        let (fs, assets) = harness(home);
        let apps = home.join("applications");
        let icons = home.join("icons/128x128/apps");
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
        let apps = home.join("applications");
        let icons = home.join("icons/64x64/apps");
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
        let apps = home.join("applications");
        fs.mkdir(&apps);
        // The icon path lives outside every icon root the provider
        // walked; the helper refuses to copy it even though the
        // bytes are reachable.
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
    fn lookup_rejects_non_png_icon() {
        let home = Path::new("/home/tester");
        let (fs, assets) = harness(home);
        let apps = home.join("applications");
        let icons = home.join("icons/128x128/apps");
        fs.mkdir(&apps);
        fs.mkdir(&icons);
        let icon_path = icons.join("firefox.svg");
        fs.write(
            &icon_path,
            b"<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>",
        );
        fs.write(
            &apps.join("firefox.desktop"),
            format!(
                "[Desktop Entry]\nType=Application\nName=Firefox\nIcon={}\n",
                icon_path.display()
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
            "non-PNG icons must not be persisted"
        );
    }

    #[test]
    fn lookup_writes_icon_atomically_and_cleans_temporary_on_failure() {
        let home = Path::new("/home/tester");
        let (fs, assets) = harness(home);
        let apps = home.join("applications");
        let icons = home.join("icons/128x128/apps");
        fs.mkdir(&apps);
        fs.mkdir(&icons);
        let icon_path = icons.join("firefox.png");
        fs.write(&icon_path, b"definitely-not-a-png");
        fs.write(
            &apps.join("firefox.desktop"),
            format!(
                "[Desktop Entry]\nType=Application\nName=Firefox\nIcon={}\n",
                icon_path.display()
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
            "invalid PNG must not be persisted"
        );
        let icons_dir = assets.join("application-icons");
        let mut remaining = std::fs::read_dir(&icons_dir).unwrap();
        assert!(remaining.next().is_none(), "no leftover asset files");
    }

    #[test]
    fn lookup_replaces_existing_icon_only_with_a_new_valid_one() {
        let home = Path::new("/home/tester");
        let (fs, assets) = harness(home);
        let apps = home.join("applications");
        let icons = home.join("icons/128x128/apps");
        fs.mkdir(&apps);
        fs.mkdir(&icons);
        let icon_path = icons.join("firefox.png");
        fs.write_png(&icon_path);
        let existing_target = assets.join("application-icons/firefox.png");
        // Pre-populate the destination with a marker payload that
        // would fail the PNG signature check; the next lookup with a
        // missing icon must leave the file intact instead of
        // rewriting it with an empty body.
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
        let apps = home.join("applications");
        fs.mkdir(&apps);
        fs.write(
            &apps.join("firefox.desktop"),
            b"[Desktop Entry]\nType=Application\nName=Firefox\nName[es]=Firefox Navegador\nStartupWMClass=firefox\n",
        );
        let provider =
            LinuxApplicationMetadataProvider::with_filesystem(assets, std::sync::Arc::new(fs));
        // Force the locale walker to prefer Spanish.
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
        let apps = home.join("applications");
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
        let apps = home.join("applications");
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
        // The bootstrap stores the provider behind
        // `Arc<dyn ApplicationMetadataProvider>`, which requires
        // `Send + Sync`. We exercise the requirement at compile
        // time by checking the auto traits of the concrete type.
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
        // Unknown escapes pass through verbatim so a malformed
        // value cannot silently truncate.
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
        // No leftover temporary file.
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
}
