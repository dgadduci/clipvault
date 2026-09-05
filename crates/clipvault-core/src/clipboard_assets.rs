//! Local asset store for non-textual clipboard payloads.
//!
//! The store is the only component allowed to turn an in-memory
//! [`ClipboardImage`] into bytes on disk, and the only component
//! allowed to turn a persisted `asset_ref` back into bytes. It lives in
//! the core (not in the shell, not in the platform crate) so it can be
//! exercised without Tauri, Svelte, a real clipboard or a graphical
//! session: every entry point takes an explicit `data_dir`.
//!
//! ## Layout and reference shape
//!
//! ```text
//! <data_dir>/assets/clipboard/<lowercase-sha256>.png
//! ```
//!
//! The value persisted in SQLite is only the relative reference:
//!
//! ```text
//! clipboard/<lowercase-sha256>.png
//! ```
//!
//! An absolute path is never stored and never returned across the
//! Tauri boundary. The `clipboard/` namespace is deliberately
//! independent from `ignored-apps/` (blacklist picker icons) and
//! `application-icons/` (source-app icons): a reference from one
//! namespace can never resolve inside another.
//!
//! ## Safety guarantees on read
//!
//! [`ClipboardAssetStore::read_bytes`] mirrors the validator the
//! `application-icons` bridge uses and adds the image-specific bounds:
//!
//! - the reference must be non-empty, relative and free of any `..`
//!   component;
//! - it must start with the `clipboard/` prefix;
//! - the canonicalised path must stay under
//!   `<data_dir>/assets/clipboard/`, which rejects a symlink pointing
//!   outside the allowed root;
//! - the file must be a decodable PNG whose byte length stays under
//!   [`MAX_CLIPBOARD_ASSET_BYTES`] and whose dimensions stay under
//!   [`clipvault_platform::MAX_CLIPBOARD_IMAGE_DIM`].
//!
//! ## Atomicity
//!
//! [`ClipboardAssetStore::store_image`] writes to a temporary file in
//! the *same* directory and then `rename`s it into place, so a crash or
//! a failed write can never leave a partially valid asset that a later
//! read would serve. When the target already exists and passes
//! validation the bytes are reused instead of rewritten.
//!
//! ## Privacy
//!
//! No function in this module logs, returns or embeds pixels, the
//! captured text, the content hash or an absolute path. Errors carry a
//! stable snake_case kind plus non-sensitive metadata only.

use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use clipvault_platform::{ClipboardImage, MAX_CLIPBOARD_IMAGE_DIM};
use sha2::{Digest, Sha256};

/// Sub-directory under `<data_dir>/assets` that holds persisted
/// clipboard payloads. Independent from the application-icon
/// namespaces so the three feature areas can evolve separately.
pub const CLIPBOARD_ASSETS_DIR: &str = "clipboard";

/// File extension and the only asset format in this phase.
pub const CLIPBOARD_ASSET_EXTENSION: &str = "png";

/// Upper bound on the byte length of a persisted PNG asset (16 MiB).
///
/// A losslessly encoded screenshot of a 5K display stays well under
/// this bound; the cap exists so a tampered assets directory cannot
/// hand the webview an arbitrarily large buffer.
pub const MAX_CLIPBOARD_ASSET_BYTES: usize = 16 * 1024 * 1024;

/// PNG signature every conforming file starts with.
const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];

/// Errors surfaced while validating a reference or reading an asset.
///
/// Every variant is metadata-only and maps to a stable snake_case
/// identifier so the frontend can pick the matching fallback copy
/// without parsing the free-form message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetError {
    /// The reference was an empty string.
    Empty,
    /// The reference was an absolute path.
    Absolute,
    /// The reference contains a `..` component.
    Traversal,
    /// The reference does not start with the `clipboard/` prefix, or it
    /// nests a sub-directory the store never writes.
    OutOfScope,
    /// The reference is well-formed but the file (or the namespace
    /// directory) does not exist.
    NotFound,
    /// The canonical path escapes the allowed root — typically a
    /// symlink pointing outside `assets/clipboard/`.
    Escaped,
    /// The file exists but cannot be read or written.
    Io { reason: String },
    /// The payload exceeds [`MAX_CLIPBOARD_ASSET_BYTES`].
    TooLarge { size: usize },
    /// The payload is not a decodable PNG.
    NotPng,
    /// The PNG decodes but its dimensions are outside the accepted
    /// bounds.
    InvalidDimensions { width: u32, height: u32 },
    /// The bitmap could not be encoded to PNG.
    Encode { reason: String },
}

impl AssetError {
    pub fn kind_str(&self) -> &'static str {
        match self {
            AssetError::Empty => "empty",
            AssetError::Absolute => "absolute",
            AssetError::Traversal => "traversal",
            AssetError::OutOfScope => "out_of_scope",
            AssetError::NotFound => "not_found",
            AssetError::Escaped => "escaped",
            AssetError::Io { .. } => "io",
            AssetError::TooLarge { .. } => "too_large",
            AssetError::NotPng => "not_png",
            AssetError::InvalidDimensions { .. } => "invalid_dimensions",
            AssetError::Encode { .. } => "encode",
        }
    }
}

impl fmt::Display for AssetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AssetError::Empty => f.write_str("clipboard asset reference is empty"),
            AssetError::Absolute => {
                f.write_str("clipboard asset reference must be a relative path")
            }
            AssetError::Traversal => {
                f.write_str("clipboard asset reference must not contain '..' components")
            }
            AssetError::OutOfScope => f.write_str(
                "clipboard asset reference must be 'clipboard/<name>.png' inside the assets directory",
            ),
            AssetError::NotFound => f.write_str("clipboard asset is missing on disk"),
            AssetError::Escaped => {
                f.write_str("clipboard asset reference resolves outside the assets directory")
            }
            AssetError::Io { reason } => write!(f, "clipboard asset io failed: {reason}"),
            AssetError::TooLarge { size } => write!(
                f,
                "clipboard asset exceeds the {MAX_CLIPBOARD_ASSET_BYTES} byte cap (got {size})"
            ),
            AssetError::NotPng => f.write_str("clipboard asset is not a valid PNG"),
            AssetError::InvalidDimensions { width, height } => write!(
                f,
                "clipboard asset dimensions {width}x{height} are outside the accepted bounds"
            ),
            AssetError::Encode { reason } => {
                write!(f, "clipboard asset encoding failed: {reason}")
            }
        }
    }
}

impl std::error::Error for AssetError {}

fn io_error(error: io::Error) -> AssetError {
    // `io::Error`'s Display can include a path on some platforms, so we
    // only keep the kind — never the path, never the payload.
    AssetError::Io {
        reason: error.kind().to_string(),
    }
}

/// A bitmap normalised to a canonical PNG, ready to be persisted.
///
/// `hash` is the lowercase hex SHA-256 of `png` — the same value the
/// capture pipeline uses as `content_hash`, so dedupe and the asset
/// name are derived from exactly the same bytes.
#[derive(Clone, PartialEq, Eq)]
pub struct NormalizedImage {
    png: Vec<u8>,
    hash: String,
    width: u32,
    height: u32,
}

impl NormalizedImage {
    /// Canonical PNG bytes.
    pub fn png(&self) -> &[u8] {
        &self.png
    }

    /// Lowercase hex SHA-256 of [`Self::png`].
    pub fn hash(&self) -> &str {
        &self.hash
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// Byte length of the encoded PNG. This is the value persisted as
    /// `content_size`.
    pub fn byte_len(&self) -> usize {
        self.png.len()
    }

    /// Relative reference this asset will be persisted under.
    pub fn asset_ref(&self) -> String {
        asset_ref_for_hash(&self.hash)
    }
}

impl fmt::Debug for NormalizedImage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Metadata only: neither the PNG bytes nor the hash (which is
        // derived from the payload) may reach a log line.
        f.debug_struct("NormalizedImage")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("png_len", &self.png.len())
            .finish()
    }
}

/// Relative reference for a normalised-PNG hash.
pub fn asset_ref_for_hash(hash: &str) -> String {
    format!("{CLIPBOARD_ASSETS_DIR}/{hash}.{CLIPBOARD_ASSET_EXTENSION}")
}

/// Encode a validated bitmap to a canonical PNG and hash the result.
///
/// The bitmap is already bounded by [`ClipboardImage`]'s invariants, so
/// this function only has to guarantee determinism: the same pixels
/// always produce byte-identical PNG output, which is what makes the
/// hash a usable dedupe key across restarts.
pub fn normalize_image(image: &ClipboardImage) -> Result<NormalizedImage, AssetError> {
    let width = image.width();
    let height = image.height();
    if width == 0
        || height == 0
        || width > MAX_CLIPBOARD_IMAGE_DIM
        || height > MAX_CLIPBOARD_IMAGE_DIM
    {
        return Err(AssetError::InvalidDimensions { width, height });
    }

    let mut png = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        // Fixed compression settings keep the output deterministic:
        // the same pixels must always hash to the same value.
        encoder.set_compression(png::Compression::Default);
        let mut writer = encoder.write_header().map_err(|error| AssetError::Encode {
            reason: error.to_string(),
        })?;
        writer
            .write_image_data(image.rgba())
            .map_err(|error| AssetError::Encode {
                reason: error.to_string(),
            })?;
        writer.finish().map_err(|error| AssetError::Encode {
            reason: error.to_string(),
        })?;
    }

    if png.len() > MAX_CLIPBOARD_ASSET_BYTES {
        return Err(AssetError::TooLarge { size: png.len() });
    }

    let hash = sha256_hex(&png);
    Ok(NormalizedImage {
        png,
        hash,
        width,
        height,
    })
}

/// Decode a PNG asset back into a platform-neutral bitmap.
///
/// Used by the paste pipeline so a persisted image can be written back
/// to the clipboard without ever being converted to text. The decoder
/// enforces the same dimension and size bounds as the read validator.
pub fn decode_png(bytes: &[u8]) -> Result<ClipboardImage, AssetError> {
    if bytes.len() > MAX_CLIPBOARD_ASSET_BYTES {
        return Err(AssetError::TooLarge { size: bytes.len() });
    }
    if !looks_like_png(bytes) {
        return Err(AssetError::NotPng);
    }
    let decoder = png::Decoder::new(io::Cursor::new(bytes));
    let mut reader = decoder.read_info().map_err(|_| AssetError::NotPng)?;
    let info = reader.info();
    let (width, height) = (info.width, info.height);
    if width == 0
        || height == 0
        || width > MAX_CLIPBOARD_IMAGE_DIM
        || height > MAX_CLIPBOARD_IMAGE_DIM
    {
        return Err(AssetError::InvalidDimensions { width, height });
    }
    let mut buffer = vec![0; reader.output_buffer_size()];
    let frame = reader
        .next_frame(&mut buffer)
        .map_err(|_| AssetError::NotPng)?;
    buffer.truncate(frame.buffer_size());
    let rgba = match (frame.color_type, frame.bit_depth) {
        (png::ColorType::Rgba, png::BitDepth::Eight) => buffer,
        (png::ColorType::Rgb, png::BitDepth::Eight) => {
            let mut expanded = Vec::with_capacity(buffer.len() / 3 * 4);
            for chunk in buffer.chunks_exact(3) {
                expanded.extend_from_slice(chunk);
                expanded.push(0xFF);
            }
            expanded
        }
        // The store only ever writes 8-bit RGBA, so anything else is a
        // foreign file in the namespace: reject instead of guessing.
        _ => return Err(AssetError::NotPng),
    };
    ClipboardImage::new(rgba, width, height)
        .map_err(|_| AssetError::InvalidDimensions { width, height })
}

/// Lowercase hex SHA-256 of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use fmt::Write;
        // `write!` into a `String` cannot fail.
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// Verify the PNG magic header. A full parse happens in
/// [`decode_png`]; this cheap check short-circuits obvious garbage
/// before the decoder allocates.
fn looks_like_png(bytes: &[u8]) -> bool {
    bytes.len() >= PNG_SIGNATURE.len() && bytes[..PNG_SIGNATURE.len()] == PNG_SIGNATURE
}

/// Outcome of persisting a normalised image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreOutcome {
    /// A new asset file was created.
    Written { asset_ref: String },
    /// A valid asset with the same hash already existed and was
    /// reused; no second file was created.
    Reused { asset_ref: String },
}

impl StoreOutcome {
    pub fn asset_ref(&self) -> &str {
        match self {
            StoreOutcome::Written { asset_ref } | StoreOutcome::Reused { asset_ref } => asset_ref,
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            StoreOutcome::Written { .. } => "written",
            StoreOutcome::Reused { .. } => "reused",
        }
    }
}

/// Filesystem-backed store for clipboard payload assets.
///
/// Cheap to clone: it only holds the resolved data directory.
#[derive(Debug, Clone)]
pub struct ClipboardAssetStore {
    data_dir: PathBuf,
}

impl ClipboardAssetStore {
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
        }
    }

    /// `<data_dir>/assets/clipboard`.
    pub fn root(&self) -> PathBuf {
        self.data_dir
            .join(clipvault_platform::ASSETS_DIR)
            .join(CLIPBOARD_ASSETS_DIR)
    }

    /// Persist `image` and return the relative reference.
    ///
    /// The write is atomic (temp file + `rename` inside the same
    /// directory) and idempotent: a second call with the same bytes
    /// reuses the existing file and reports [`StoreOutcome::Reused`].
    pub fn store_image(&self, image: &NormalizedImage) -> Result<StoreOutcome, AssetError> {
        let root = self.root();
        fs::create_dir_all(&root).map_err(io_error)?;
        let file_name = format!("{}.{CLIPBOARD_ASSET_EXTENSION}", image.hash());
        let target = root.join(&file_name);
        let asset_ref = image.asset_ref();

        // Reuse an existing, valid asset: the hash already proves the
        // bytes match, so re-encoding would only churn the disk.
        if target.exists() && read_validated_png(&target).is_ok() {
            return Ok(StoreOutcome::Reused { asset_ref });
        }

        // Temp file in the *same* directory so the rename stays on one
        // filesystem and is therefore atomic.
        let temp = root.join(format!(".{}.tmp", image.hash()));
        // A leftover temp file from an interrupted run must not make
        // this attempt fail.
        let _ = fs::remove_file(&temp);
        if let Err(error) = fs::write(&temp, image.png()) {
            let _ = fs::remove_file(&temp);
            return Err(io_error(error));
        }
        if let Err(error) = fs::rename(&temp, &target) {
            // Never leave a partial file behind.
            let _ = fs::remove_file(&temp);
            return Err(io_error(error));
        }
        Ok(StoreOutcome::Written { asset_ref })
    }

    /// Resolve a relative `asset_ref` to its canonical absolute path
    /// inside the clipboard asset namespace.
    ///
    /// The absolute path never leaves the backend: callers use it to
    /// read bytes and serve them through a command.
    pub fn resolve(&self, asset_ref: &str) -> Result<PathBuf, AssetError> {
        if asset_ref.is_empty() {
            return Err(AssetError::Empty);
        }
        let candidate = Path::new(asset_ref);
        if candidate.is_absolute() {
            return Err(AssetError::Absolute);
        }
        if candidate
            .components()
            .any(|component| matches!(component, Component::ParentDir))
        {
            return Err(AssetError::Traversal);
        }
        let prefix = format!("{CLIPBOARD_ASSETS_DIR}/");
        if !asset_ref.starts_with(&prefix) {
            return Err(AssetError::OutOfScope);
        }
        let relative = candidate
            .strip_prefix(CLIPBOARD_ASSETS_DIR)
            .map_err(|_| AssetError::OutOfScope)?;
        // The store writes flat file names only; a nested reference
        // would mean the caller invented a shape we never produce.
        if relative.components().count() != 1 {
            return Err(AssetError::OutOfScope);
        }

        let allowed_root = self.root();
        let canonical_root = fs::canonicalize(&allowed_root).map_err(|_| AssetError::NotFound)?;
        let full_path = allowed_root.join(relative);

        // Reject a symlink *before* canonicalising. The store only ever
        // creates regular files, so a link inside the namespace is a
        // tampering signal even when its target happens to be a
        // legitimate asset — and canonicalisation would otherwise hide
        // it by resolving to the real file.
        let metadata = fs::symlink_metadata(&full_path).map_err(|_| AssetError::NotFound)?;
        if metadata.file_type().is_symlink() {
            return Err(AssetError::Escaped);
        }
        if !metadata.is_file() {
            return Err(AssetError::NotFound);
        }

        let canonical = fs::canonicalize(&full_path).map_err(|_| AssetError::NotFound)?;
        if !canonical.starts_with(&canonical_root) {
            return Err(AssetError::Escaped);
        }
        Ok(canonical)
    }

    /// Read the validated PNG bytes behind `asset_ref`.
    ///
    /// This is the single entry point the Tauri asset command uses.
    /// Every rejection path returns a typed error and **no** bytes.
    pub fn read_bytes(&self, asset_ref: &str) -> Result<Vec<u8>, AssetError> {
        let path = self.resolve(asset_ref)?;
        read_validated_png(&path)
    }

    /// Delete every file in the clipboard namespace whose name is not
    /// referenced by `referenced`.
    ///
    /// The caller is responsible for computing `referenced` from
    /// SQLite *after* the data mutation has committed, so an asset that
    /// is still referenced by another row is never a candidate.
    ///
    /// The collector is idempotent, never recurses out of the
    /// namespace and never touches the `ignored-apps` or
    /// `application-icons` directories. A file it does not recognise
    /// (a directory, a leftover `.tmp`, a foreign extension) is left
    /// alone rather than deleted, except for stale temporaries which
    /// are safe to reclaim because the store only creates them inside
    /// a single `store_image` call.
    ///
    /// Returns the number of assets removed.
    pub fn collect_unreferenced(&self, referenced: &BTreeSet<String>) -> Result<usize, AssetError> {
        let root = self.root();
        let entries = match fs::read_dir(&root) {
            Ok(entries) => entries,
            // No namespace directory yet: nothing to collect. This is a
            // success, not an error, so retention can always run.
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(0),
            Err(error) => return Err(io_error(error)),
        };

        let mut removed = 0usize;
        for entry in entries {
            let entry = entry.map_err(io_error)?;
            // `file_type` does not follow symlinks, so a symlink placed
            // in the namespace is not treated as a regular file and is
            // therefore never followed nor deleted.
            let file_type = entry.file_type().map_err(io_error)?;
            if !file_type.is_file() {
                continue;
            }
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            // Reclaim our own stale temporaries; leave anything else.
            if name.starts_with('.') && name.ends_with(".tmp") {
                if fs::remove_file(entry.path()).is_ok() {
                    removed += 1;
                }
                continue;
            }
            if !name.ends_with(&format!(".{CLIPBOARD_ASSET_EXTENSION}")) {
                continue;
            }
            let candidate_ref = format!("{CLIPBOARD_ASSETS_DIR}/{name}");
            if referenced.contains(&candidate_ref) {
                continue;
            }
            if fs::remove_file(entry.path()).is_ok() {
                removed += 1;
            }
        }
        Ok(removed)
    }
}

fn read_validated_png(path: &Path) -> Result<Vec<u8>, AssetError> {
    // `symlink_metadata` does not follow links: a symlink inside the
    // namespace is rejected before it is ever opened. The canonical
    // comparison in `resolve` already rejects links that escape the
    // root; this check additionally refuses links that stay inside it,
    // because the store never creates one.
    let metadata = fs::symlink_metadata(path).map_err(|_| AssetError::NotFound)?;
    if metadata.file_type().is_symlink() {
        return Err(AssetError::Escaped);
    }
    if !metadata.is_file() {
        return Err(AssetError::NotFound);
    }
    let size = usize::try_from(metadata.len()).unwrap_or(usize::MAX);
    if size > MAX_CLIPBOARD_ASSET_BYTES {
        // Reject on metadata so an oversized file is never read into
        // memory in the first place.
        return Err(AssetError::TooLarge { size });
    }
    let bytes = fs::read(path).map_err(io_error)?;
    if bytes.len() > MAX_CLIPBOARD_ASSET_BYTES {
        return Err(AssetError::TooLarge { size: bytes.len() });
    }
    if !looks_like_png(&bytes) {
        return Err(AssetError::NotPng);
    }
    // Full decode: a truncated or corrupt PNG must not reach the
    // webview, and the dimensions have to stay inside the bounds.
    decode_png(&bytes)?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bitmap(width: u32, height: u32, fill: u8) -> ClipboardImage {
        let len = (width as usize) * (height as usize) * 4;
        ClipboardImage::new(vec![fill; len], width, height).expect("valid bitmap")
    }

    fn store() -> (tempfile::TempDir, ClipboardAssetStore) {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = ClipboardAssetStore::new(dir.path());
        (dir, store)
    }

    #[test]
    fn normalize_produces_a_canonical_png_with_a_sha256_name() {
        let normalized = normalize_image(&bitmap(4, 3, 0x40)).expect("normalize");
        assert!(looks_like_png(normalized.png()));
        assert_eq!(normalized.width(), 4);
        assert_eq!(normalized.height(), 3);
        assert_eq!(normalized.hash().len(), 64, "sha256 hex is 64 chars");
        assert!(
            normalized
                .hash()
                .chars()
                .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)),
            "hash must be lowercase hex"
        );
        assert_eq!(
            normalized.asset_ref(),
            format!("clipboard/{}.png", normalized.hash())
        );
        assert_eq!(normalized.byte_len(), normalized.png().len());
    }

    #[test]
    fn normalize_is_deterministic_for_identical_pixels() {
        // This is what makes the hash a usable dedupe key: the same
        // bitmap must always produce byte-identical PNG output, across
        // calls and across restarts.
        let first = normalize_image(&bitmap(8, 8, 0x11)).expect("first");
        let second = normalize_image(&bitmap(8, 8, 0x11)).expect("second");
        assert_eq!(first.png(), second.png());
        assert_eq!(first.hash(), second.hash());
    }

    #[test]
    fn normalize_distinguishes_different_pixels_and_geometry() {
        let base = normalize_image(&bitmap(8, 8, 0x11)).expect("base");
        let other_pixels = normalize_image(&bitmap(8, 8, 0x22)).expect("pixels");
        let other_geometry = normalize_image(&bitmap(4, 16, 0x11)).expect("geometry");
        assert_ne!(base.hash(), other_pixels.hash());
        assert_ne!(base.hash(), other_geometry.hash());
    }

    #[test]
    fn normalized_debug_never_leaks_bytes_or_hash() {
        let normalized = normalize_image(&bitmap(2, 2, 0xAB)).expect("normalize");
        let rendered = format!("{normalized:?}");
        assert!(rendered.contains("width"));
        assert!(rendered.contains("png_len"));
        assert!(
            !rendered.contains(normalized.hash()),
            "the hash must not appear in Debug output"
        );
    }

    #[test]
    fn round_trip_encode_then_decode_preserves_pixels() {
        let original = bitmap(5, 7, 0x7F);
        let normalized = normalize_image(&original).expect("normalize");
        let decoded = decode_png(normalized.png()).expect("decode");
        assert_eq!(decoded.width(), original.width());
        assert_eq!(decoded.height(), original.height());
        assert_eq!(decoded.rgba(), original.rgba());
    }

    #[test]
    fn decode_rejects_non_png_and_corrupt_payloads() {
        assert_eq!(decode_png(b"definitely not a png"), Err(AssetError::NotPng));
        // Valid signature, truncated body: the decoder must refuse
        // instead of handing partial pixels to the webview.
        let mut truncated = PNG_SIGNATURE.to_vec();
        truncated.extend_from_slice(b"\x00\x00\x00\rIHDR-broken");
        assert_eq!(decode_png(&truncated), Err(AssetError::NotPng));
    }

    #[test]
    fn store_writes_then_reuses_the_same_asset() {
        let (_dir, store) = store();
        let normalized = normalize_image(&bitmap(6, 6, 0x33)).expect("normalize");

        let first = store.store_image(&normalized).expect("write");
        assert_eq!(first.kind(), "written");
        assert_eq!(first.asset_ref(), normalized.asset_ref());

        let second = store.store_image(&normalized).expect("reuse");
        assert_eq!(second.kind(), "reused");
        assert_eq!(second.asset_ref(), first.asset_ref());

        // Exactly one file on disk.
        let files: Vec<_> = fs::read_dir(store.root())
            .expect("read_dir")
            .filter_map(Result::ok)
            .collect();
        assert_eq!(files.len(), 1, "reuse must not create a second file");
    }

    #[test]
    fn store_leaves_no_temporary_behind_on_success() {
        let (_dir, store) = store();
        let normalized = normalize_image(&bitmap(3, 3, 0x55)).expect("normalize");
        store.store_image(&normalized).expect("write");
        let temporaries: Vec<_> = fs::read_dir(store.root())
            .expect("read_dir")
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(temporaries.is_empty(), "atomic write must clean up");
    }

    #[test]
    fn store_recovers_from_a_leftover_temporary() {
        // Simulate a crash between `write` and `rename`: the stale temp
        // file must not block the next attempt.
        let (_dir, store) = store();
        let normalized = normalize_image(&bitmap(3, 3, 0x66)).expect("normalize");
        fs::create_dir_all(store.root()).expect("mkdir");
        let stale = store.root().join(format!(".{}.tmp", normalized.hash()));
        fs::write(&stale, b"partial garbage").expect("stale temp");

        let outcome = store.store_image(&normalized).expect("write");
        assert_eq!(outcome.kind(), "written");
        assert!(!stale.exists(), "stale temp must be replaced");
        let bytes = store.read_bytes(outcome.asset_ref()).expect("read back");
        assert_eq!(bytes, normalized.png());
    }

    #[test]
    fn store_replaces_a_corrupt_existing_asset_instead_of_serving_it() {
        let (_dir, store) = store();
        let normalized = normalize_image(&bitmap(4, 4, 0x77)).expect("normalize");
        fs::create_dir_all(store.root()).expect("mkdir");
        let target = store.root().join(format!("{}.png", normalized.hash()));
        fs::write(&target, b"not a png at all").expect("corrupt file");

        let outcome = store.store_image(&normalized).expect("rewrite");
        assert_eq!(
            outcome.kind(),
            "written",
            "a corrupt asset must be rewritten, not reused"
        );
        assert_eq!(
            store.read_bytes(outcome.asset_ref()).expect("read"),
            normalized.png()
        );
    }

    #[test]
    fn read_bytes_round_trips_a_persisted_asset() {
        let (_dir, store) = store();
        let normalized = normalize_image(&bitmap(9, 2, 0x21)).expect("normalize");
        let outcome = store.store_image(&normalized).expect("write");
        let bytes = store.read_bytes(outcome.asset_ref()).expect("read");
        assert_eq!(bytes, normalized.png());
    }

    #[test]
    fn read_bytes_rejects_an_empty_reference() {
        let (_dir, store) = store();
        assert_eq!(store.read_bytes("").unwrap_err(), AssetError::Empty);
    }

    #[test]
    fn read_bytes_rejects_absolute_paths() {
        let (_dir, store) = store();
        for reference in ["/etc/passwd", "/tmp/clipboard/x.png"] {
            assert_eq!(
                store.read_bytes(reference).unwrap_err(),
                AssetError::Absolute,
                "{reference} must be rejected as absolute"
            );
        }
    }

    #[test]
    fn read_bytes_rejects_traversal() {
        let (_dir, store) = store();
        for reference in [
            "clipboard/../../etc/passwd",
            "clipboard/sub/../../escape.png",
            "../clipboard/x.png",
        ] {
            assert_eq!(
                store.read_bytes(reference).unwrap_err(),
                AssetError::Traversal,
                "{reference} must be rejected as traversal"
            );
        }
    }

    #[test]
    fn read_bytes_rejects_foreign_namespaces() {
        // Namespace isolation: a blacklist-icon or source-app-icon
        // reference must never resolve through the clipboard bridge.
        let (_dir, store) = store();
        for reference in [
            "ignored-apps/com.apple.textedit.png",
            "application-icons/com.apple.Terminal.png",
            "x.png",
            "clipboardx/y.png",
            "clipboard/nested/y.png",
        ] {
            assert_eq!(
                store.read_bytes(reference).unwrap_err(),
                AssetError::OutOfScope,
                "{reference} must be rejected as out of scope"
            );
        }
    }

    #[test]
    fn read_bytes_reports_a_missing_asset() {
        let (_dir, store) = store();
        fs::create_dir_all(store.root()).expect("mkdir");
        assert_eq!(
            store.read_bytes("clipboard/ghost.png").unwrap_err(),
            AssetError::NotFound
        );
    }

    #[test]
    fn read_bytes_reports_not_found_when_the_namespace_is_absent() {
        let (_dir, store) = store();
        assert_eq!(
            store.read_bytes("clipboard/ghost.png").unwrap_err(),
            AssetError::NotFound
        );
    }

    #[cfg(unix)]
    #[test]
    fn read_bytes_rejects_a_symlink_escaping_the_namespace() {
        let (dir, store) = store();
        fs::create_dir_all(store.root()).expect("mkdir");
        let outside = dir.path().join("outside.png");
        let normalized = normalize_image(&bitmap(2, 2, 0x01)).expect("normalize");
        fs::write(&outside, normalized.png()).expect("write outside");
        std::os::unix::fs::symlink(&outside, store.root().join("escape.png")).expect("symlink");
        let error = store.read_bytes("clipboard/escape.png").unwrap_err();
        assert!(
            matches!(error, AssetError::Escaped),
            "expected Escaped, got {error:?}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn read_bytes_rejects_a_symlink_that_stays_inside_the_namespace() {
        // The store never creates a symlink, so one inside the
        // namespace is a tampering signal even when its target is a
        // legitimate asset.
        let (_dir, store) = store();
        let normalized = normalize_image(&bitmap(2, 2, 0x02)).expect("normalize");
        let outcome = store.store_image(&normalized).expect("write");
        let real = store.root().join(format!("{}.png", normalized.hash()));
        std::os::unix::fs::symlink(&real, store.root().join("alias.png")).expect("symlink");
        assert_eq!(
            store.read_bytes("clipboard/alias.png").unwrap_err(),
            AssetError::Escaped
        );
        // The genuine reference still works.
        assert!(store.read_bytes(outcome.asset_ref()).is_ok());
    }

    #[test]
    fn read_bytes_rejects_a_non_png_payload_in_the_namespace() {
        let (_dir, store) = store();
        fs::create_dir_all(store.root()).expect("mkdir");
        fs::write(store.root().join("fake.png"), b"not a png").expect("write");
        assert_eq!(
            store.read_bytes("clipboard/fake.png").unwrap_err(),
            AssetError::NotPng
        );
    }

    #[test]
    fn read_bytes_rejects_a_corrupt_png_before_serving_bytes() {
        let (_dir, store) = store();
        fs::create_dir_all(store.root()).expect("mkdir");
        let normalized = normalize_image(&bitmap(4, 4, 0x03)).expect("normalize");
        // Valid signature and header, truncated data chunk.
        let truncated = &normalized.png()[..normalized.png().len() / 2];
        fs::write(store.root().join("corrupt.png"), truncated).expect("write");
        assert_eq!(
            store.read_bytes("clipboard/corrupt.png").unwrap_err(),
            AssetError::NotPng
        );
    }

    #[test]
    fn read_bytes_enforces_the_size_cap_without_reading_the_file() {
        let (_dir, store) = store();
        fs::create_dir_all(store.root()).expect("mkdir");
        let path = store.root().join("huge.png");
        let mut bytes = PNG_SIGNATURE.to_vec();
        bytes.resize(MAX_CLIPBOARD_ASSET_BYTES + 1, b'X');
        fs::write(&path, &bytes).expect("write");
        match store.read_bytes("clipboard/huge.png").unwrap_err() {
            AssetError::TooLarge { size } => assert_eq!(size, bytes.len()),
            other => panic!("expected TooLarge, got {other:?}"),
        }
    }

    #[test]
    fn collect_removes_only_unreferenced_assets() {
        let (_dir, store) = store();
        let kept = normalize_image(&bitmap(4, 4, 0x0A)).expect("kept");
        let dropped = normalize_image(&bitmap(4, 4, 0x0B)).expect("dropped");
        let kept_ref = store
            .store_image(&kept)
            .expect("write kept")
            .asset_ref()
            .to_string();
        let dropped_ref = store
            .store_image(&dropped)
            .expect("write dropped")
            .asset_ref()
            .to_string();

        let mut referenced = BTreeSet::new();
        referenced.insert(kept_ref.clone());

        let removed = store.collect_unreferenced(&referenced).expect("collect");
        assert_eq!(removed, 1);
        assert!(store.read_bytes(&kept_ref).is_ok(), "referenced asset kept");
        assert_eq!(
            store.read_bytes(&dropped_ref).unwrap_err(),
            AssetError::NotFound
        );
    }

    #[test]
    fn collect_keeps_an_asset_shared_by_two_references() {
        // A single file referenced under the same name by two rows must
        // survive as long as the reference is in the live set.
        let (_dir, store) = store();
        let shared = normalize_image(&bitmap(4, 4, 0x0C)).expect("shared");
        let shared_ref = store
            .store_image(&shared)
            .expect("write")
            .asset_ref()
            .to_string();
        let mut referenced = BTreeSet::new();
        referenced.insert(shared_ref.clone());
        assert_eq!(store.collect_unreferenced(&referenced).expect("collect"), 0);
        assert!(store.read_bytes(&shared_ref).is_ok());
    }

    #[test]
    fn collect_is_idempotent_and_safe_on_an_empty_namespace() {
        let (_dir, store) = store();
        let referenced = BTreeSet::new();
        // No directory yet: not an error.
        assert_eq!(store.collect_unreferenced(&referenced).expect("first"), 0);

        let orphan = normalize_image(&bitmap(2, 2, 0x0D)).expect("orphan");
        store.store_image(&orphan).expect("write");
        assert_eq!(store.collect_unreferenced(&referenced).expect("second"), 1);
        // Running again removes nothing and still succeeds.
        assert_eq!(store.collect_unreferenced(&referenced).expect("third"), 0);
    }

    #[test]
    fn collect_reclaims_stale_temporaries_and_leaves_foreign_files_alone() {
        let (_dir, store) = store();
        fs::create_dir_all(store.root()).expect("mkdir");
        fs::write(store.root().join(".abc.tmp"), b"partial").expect("temp");
        fs::write(store.root().join("notes.txt"), b"foreign").expect("foreign");

        let removed = store
            .collect_unreferenced(&BTreeSet::new())
            .expect("collect");
        assert_eq!(removed, 1, "only the stale temporary is reclaimed");
        assert!(
            store.root().join("notes.txt").exists(),
            "an unrecognised file must be left alone"
        );
    }

    #[test]
    fn collect_never_touches_the_sibling_asset_namespaces() {
        // `ignored-apps/` and `application-icons/` belong to other
        // capabilities; the clipboard collector must not walk into them.
        let (dir, store) = store();
        let assets = dir.path().join(clipvault_platform::ASSETS_DIR);
        for namespace in ["ignored-apps", "application-icons"] {
            let path = assets.join(namespace);
            fs::create_dir_all(&path).expect("mkdir");
            fs::write(path.join("keep.png"), b"\x89PNG\r\n\x1a\n icon").expect("write");
        }
        let orphan = normalize_image(&bitmap(2, 2, 0x0E)).expect("orphan");
        store.store_image(&orphan).expect("write");

        assert_eq!(
            store
                .collect_unreferenced(&BTreeSet::new())
                .expect("collect"),
            1
        );
        for namespace in ["ignored-apps", "application-icons"] {
            assert!(
                assets.join(namespace).join("keep.png").exists(),
                "{namespace} must be untouched"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn collect_does_not_follow_or_delete_symlinks() {
        let (dir, store) = store();
        fs::create_dir_all(store.root()).expect("mkdir");
        let outside = dir.path().join("precious.png");
        fs::write(&outside, b"\x89PNG\r\n\x1a\n precious").expect("write");
        std::os::unix::fs::symlink(&outside, store.root().join("link.png")).expect("symlink");

        store
            .collect_unreferenced(&BTreeSet::new())
            .expect("collect");
        assert!(outside.exists(), "the symlink target must survive");
    }

    #[test]
    fn error_kind_strings_are_stable() {
        let cases: [(AssetError, &str); 11] = [
            (AssetError::Empty, "empty"),
            (AssetError::Absolute, "absolute"),
            (AssetError::Traversal, "traversal"),
            (AssetError::OutOfScope, "out_of_scope"),
            (AssetError::NotFound, "not_found"),
            (AssetError::Escaped, "escaped"),
            (AssetError::Io { reason: "x".into() }, "io"),
            (AssetError::TooLarge { size: 1 }, "too_large"),
            (AssetError::NotPng, "not_png"),
            (
                AssetError::InvalidDimensions {
                    width: 0,
                    height: 0,
                },
                "invalid_dimensions",
            ),
            (AssetError::Encode { reason: "x".into() }, "encode"),
        ];
        for (error, expected) in cases {
            assert_eq!(error.kind_str(), expected);
        }
    }

    #[test]
    fn error_messages_never_include_an_absolute_path() {
        // Privacy contract: an error surfaced to the user or a log must
        // not disclose where the data directory lives.
        let (_dir, store) = store();
        let errors = [
            store.read_bytes("clipboard/ghost.png").unwrap_err(),
            store.read_bytes("/etc/passwd").unwrap_err(),
            store.read_bytes("clipboard/../x.png").unwrap_err(),
        ];
        for error in errors {
            let rendered = error.to_string();
            assert!(!rendered.contains('/') || !rendered.contains("/Users"));
            assert!(!rendered.contains(&store.root().display().to_string()));
        }
    }

    #[test]
    fn sha256_hex_matches_the_known_digest_of_the_empty_input() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn namespace_constants_are_stable_and_distinct() {
        assert_eq!(CLIPBOARD_ASSETS_DIR, "clipboard");
        assert_eq!(CLIPBOARD_ASSET_EXTENSION, "png");
        assert_ne!(CLIPBOARD_ASSETS_DIR, clipvault_platform::IGNORED_APPS_DIR);
        assert_ne!(
            CLIPBOARD_ASSETS_DIR,
            clipvault_platform::APPLICATION_ICONS_DIR
        );
    }
}
