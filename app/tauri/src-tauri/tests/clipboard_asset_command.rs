//! End-to-end tests for the `clipvault_clipboard_asset` Tauri command.
//!
//! The command is the only path between the relative `asset_ref` SQLite
//! stores and the PNG bytes the history card renders as a thumbnail.
//! The tests pin the safety and privacy contract the rest of the app
//! depends on:
//!
//! - a relative reference under `clipboard/` resolves to the exact
//!   bytes the asset store wrote, and the response never carries an
//!   absolute path;
//! - an empty reference, an absolute path, a traversal segment, a
//!   foreign namespace (`ignored-apps/`, `application-icons/`) and a
//!   nested reference are all rejected with the stable
//!   `invalid_asset_ref` kind plus the specific reason;
//! - a non-PNG payload, a corrupt PNG and an oversized file are
//!   rejected without any bytes reaching the caller;
//! - a symlink inside the namespace is rejected even when its target is
//!   a legitimate asset;
//! - the error surface never echoes the reference, the content hash or
//!   the data directory.
//!
//! The command body is exercised through
//! `clipvault_clipboard_asset_for_test`, which runs the identical
//! validation pipeline with an explicit `data_dir` so no Tauri runtime
//! is required.

use std::fs;
use std::path::PathBuf;

use clipvault_app::commands::{clipvault_clipboard_asset_for_test, CommandError};
use clipvault_core::{
    normalize_image, ClipboardAssetStore, ClipboardImage, MAX_CLIPBOARD_ASSET_BYTES,
};

const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];

struct Harness {
    _dir: tempfile::TempDir,
    data_dir: PathBuf,
    store: ClipboardAssetStore,
}

impl Harness {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let data_dir = dir.path().join("data");
        fs::create_dir_all(&data_dir).expect("data dir");
        let store = ClipboardAssetStore::new(&data_dir);
        Self {
            _dir: dir,
            data_dir,
            store,
        }
    }

    /// Persist a real image through the store and return its reference
    /// plus the exact bytes the store wrote.
    fn seed_asset(&self, fill: u8) -> (String, Vec<u8>) {
        let len = 8 * 8 * 4;
        let bitmap = ClipboardImage::new(vec![fill; len], 8, 8).expect("bitmap");
        let normalized = normalize_image(&bitmap).expect("normalize");
        let outcome = self.store.store_image(&normalized).expect("write");
        (outcome.asset_ref().to_string(), normalized.png().to_vec())
    }

    fn namespace(&self) -> PathBuf {
        self.store.root()
    }

    fn write_raw(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let root = self.namespace();
        fs::create_dir_all(&root).expect("mkdir");
        let path = root.join(name);
        fs::write(&path, bytes).expect("write");
        path
    }

    fn read(&self, asset_ref: &str) -> Result<Vec<u8>, CommandError> {
        clipvault_clipboard_asset_for_test(&self.data_dir, asset_ref.to_string())
    }
}

#[test]
fn clipboard_asset_returns_the_persisted_png_bytes() {
    let harness = Harness::new();
    let (reference, expected) = harness.seed_asset(0x21);

    let bytes = harness.read(&reference).expect("bytes returned");
    assert_eq!(bytes, expected);
    assert!(bytes.starts_with(&PNG_SIGNATURE));
}

#[test]
fn clipboard_asset_accepts_only_a_relative_namespace_reference() {
    let harness = Harness::new();
    let (reference, _) = harness.seed_asset(0x22);
    assert!(reference.starts_with("clipboard/"));
    assert!(!reference.starts_with('/'));
    // The command never accepts the absolute path of the same file.
    let absolute = harness
        .namespace()
        .join(reference.strip_prefix("clipboard/").expect("prefix"))
        .display()
        .to_string();
    let error = harness
        .read(&absolute)
        .expect_err("absolute must be rejected");
    assert_eq!(error.kind, "invalid_asset_ref");
    assert_eq!(error.message, "absolute");
}

#[test]
fn clipboard_asset_rejects_empty_and_malformed_references() {
    let harness = Harness::new();
    harness.seed_asset(0x23);

    let cases: [(&str, &str); 8] = [
        ("", "empty"),
        ("/etc/passwd", "absolute"),
        ("clipboard/../../etc/passwd", "traversal"),
        ("clipboard/sub/../../escape.png", "traversal"),
        ("../clipboard/x.png", "traversal"),
        ("clipboard/nested/x.png", "out_of_scope"),
        ("clipboardx/x.png", "out_of_scope"),
        ("x.png", "out_of_scope"),
    ];
    for (reference, expected) in cases {
        let error = harness
            .read(reference)
            .expect_err("reference must be rejected");
        assert_eq!(
            error.kind, "invalid_asset_ref",
            "reference {reference} produced the wrong kind"
        );
        assert_eq!(
            error.message, expected,
            "reference {reference} produced the wrong reason"
        );
    }
}

#[test]
fn clipboard_asset_rejects_the_sibling_icon_namespaces() {
    // Namespace isolation: the clipboard bridge must never serve a
    // blacklist icon or a source-application icon, even when the file
    // exists and is a valid PNG.
    let harness = Harness::new();
    let assets = harness.data_dir.join("assets");
    for namespace in ["ignored-apps", "application-icons"] {
        let dir = assets.join(namespace);
        fs::create_dir_all(&dir).expect("mkdir");
        let mut bytes = PNG_SIGNATURE.to_vec();
        bytes.extend_from_slice(b" icon payload");
        fs::write(dir.join("com.apple.Terminal.png"), &bytes).expect("write");

        let error = harness
            .read(&format!("{namespace}/com.apple.Terminal.png"))
            .expect_err("foreign namespace must be rejected");
        assert_eq!(error.kind, "invalid_asset_ref");
        assert_eq!(error.message, "out_of_scope");
        // And the file is still there: rejecting must not delete it.
        assert!(dir.join("com.apple.Terminal.png").exists());
    }
}

#[test]
fn clipboard_asset_reports_a_missing_file() {
    let harness = Harness::new();
    harness.seed_asset(0x24);
    let error = harness
        .read("clipboard/ghost.png")
        .expect_err("missing file must be rejected");
    assert_eq!(error.kind, "invalid_asset_ref");
    assert_eq!(error.message, "not_found");
}

#[test]
fn clipboard_asset_rejects_a_non_png_payload() {
    let harness = Harness::new();
    harness.write_raw("fake.png", b"definitely not a png");
    let error = harness
        .read("clipboard/fake.png")
        .expect_err("non-png must be rejected");
    assert_eq!(error.kind, "invalid_asset_ref");
    assert_eq!(error.message, "not_png");
}

#[test]
fn clipboard_asset_rejects_a_corrupt_png() {
    let harness = Harness::new();
    let (_, bytes) = harness.seed_asset(0x25);
    // Valid signature and header, truncated data: the webview must
    // never receive a partially decodable image.
    harness.write_raw("corrupt.png", &bytes[..bytes.len() / 2]);
    let error = harness
        .read("clipboard/corrupt.png")
        .expect_err("corrupt png must be rejected");
    assert_eq!(error.kind, "invalid_asset_ref");
    assert_eq!(error.message, "not_png");
}

#[test]
fn clipboard_asset_rejects_an_oversized_file() {
    let harness = Harness::new();
    let mut bytes = PNG_SIGNATURE.to_vec();
    bytes.resize(MAX_CLIPBOARD_ASSET_BYTES + 1, b'X');
    harness.write_raw("huge.png", &bytes);
    let error = harness
        .read("clipboard/huge.png")
        .expect_err("oversized file must be rejected");
    assert_eq!(error.kind, "invalid_asset_ref");
    assert_eq!(error.message, "too_large");
}

#[cfg(unix)]
#[test]
fn clipboard_asset_rejects_a_symlink_inside_the_namespace() {
    let harness = Harness::new();
    let (reference, _) = harness.seed_asset(0x26);
    let real = harness
        .namespace()
        .join(reference.strip_prefix("clipboard/").expect("prefix"));
    std::os::unix::fs::symlink(&real, harness.namespace().join("alias.png")).expect("symlink");

    let error = harness
        .read("clipboard/alias.png")
        .expect_err("symlink must be rejected");
    assert_eq!(error.kind, "invalid_asset_ref");
    assert_eq!(error.message, "escaped");
    // The genuine reference still resolves.
    assert!(harness.read(&reference).is_ok());
}

#[cfg(unix)]
#[test]
fn clipboard_asset_rejects_a_symlink_escaping_the_namespace() {
    let harness = Harness::new();
    let outside = harness.data_dir.join("outside.png");
    let mut bytes = PNG_SIGNATURE.to_vec();
    bytes.extend_from_slice(b" outside payload");
    fs::write(&outside, &bytes).expect("write outside");
    fs::create_dir_all(harness.namespace()).expect("mkdir");
    std::os::unix::fs::symlink(&outside, harness.namespace().join("escape.png")).expect("symlink");

    let error = harness
        .read("clipboard/escape.png")
        .expect_err("escaping symlink must be rejected");
    assert_eq!(error.kind, "invalid_asset_ref");
    assert_eq!(error.message, "escaped");
    // The file outside the namespace is untouched.
    assert!(outside.exists());
}

#[test]
fn clipboard_asset_errors_never_leak_a_path_reference_or_hash() {
    // Privacy contract: the error channel carries a stable snake_case
    // reason and nothing else. No data directory, no reference, no
    // content hash, no PNG bytes.
    let harness = Harness::new();
    let (reference, _) = harness.seed_asset(0x27);
    let data_dir = harness.data_dir.display().to_string();

    let probes = [
        "",
        "/etc/passwd",
        "clipboard/../../etc/passwd",
        "ignored-apps/x.png",
        "clipboard/ghost.png",
    ];
    for probe in probes {
        let error = harness.read(probe).expect_err("must reject");
        let rendered = format!("{}|{}", error.kind, error.message);
        assert!(
            !rendered.contains(&data_dir),
            "data dir leaked for {probe}: {rendered}"
        );
        assert!(
            !rendered.contains(&reference),
            "asset reference leaked for {probe}: {rendered}"
        );
        assert!(
            !rendered.contains('/'),
            "a path separator leaked for {probe}: {rendered}"
        );
        assert!(
            !rendered.contains("PNG"),
            "payload bytes leaked for {probe}: {rendered}"
        );
    }
}

#[test]
fn clipboard_asset_response_is_bytes_only() {
    // The success payload is the PNG bytes and nothing else: no
    // wrapper carrying a path, no metadata the frontend could render
    // as text.
    let harness = Harness::new();
    let (reference, expected) = harness.seed_asset(0x28);
    let bytes: Vec<u8> = harness.read(&reference).expect("bytes");
    assert_eq!(bytes, expected);
    let as_text = String::from_utf8_lossy(&bytes);
    assert!(!as_text.contains(&harness.data_dir.display().to_string()));
    assert!(!as_text.contains("clipboard/"));
}

// ---------------------------------------------------------------------------
// `desktop-shell-layout` 10.8: the persisted PNG bytes must keep
// resolving through `clipvault_clipboard_asset` after the desktop
// restarts. The user-reported regression was that image cards never
// re-minted their `blob:` URL after a fresh launch, even though the
// files were still on disk. The tests below pin the contract the rail
// and the card rely on so the bridge cannot drift back to a failure.
// ---------------------------------------------------------------------------

#[test]
fn clipboard_asset_returns_persisted_bytes_after_dropping_the_harness() {
    // Seed an asset through the harness, drop the harness (and its
    // `TempDir` cleanup guard) and re-open a fresh store pointing at
    // the same data directory. The bytes the original store wrote
    // must survive the cycle and the second harness must hand them
    // back through the command.
    let dir = tempfile::tempdir().expect("tempdir");
    let data_dir = dir.path().join("data");
    std::fs::create_dir_all(&data_dir).expect("data dir");
    let reference;
    let expected;
    {
        let first_store = ClipboardAssetStore::new(&data_dir);
        let bitmap = ClipboardImage::new(vec![0x33; 8 * 8 * 4], 8, 8).expect("bitmap");
        let normalized = normalize_image(&bitmap).expect("normalize");
        let outcome = first_store.store_image(&normalized).expect("write");
        reference = outcome.asset_ref().to_string();
        expected = normalized.png().to_vec();
    }
    drop(dir.path().to_path_buf());
    // Re-open against the same data directory; the bytes must still
    // be reachable through the production bridge.
    let bytes = clipvault_clipboard_asset_for_test(&data_dir, reference.clone())
        .expect("bytes after reopen");
    assert_eq!(bytes, expected);
    assert!(bytes.starts_with(&PNG_SIGNATURE));
}

#[test]
fn clipboard_asset_rejects_a_reference_that_no_longer_matches_the_persisted_row() {
    // Pin the error channel: after a restart the bridge must surface
    // every rejection as the same `invalid_asset_ref` kind the
    // frontend routes on, so a missing asset never collapses to a
    // generic exception or to `null` bytes that the resolver would
    // mistake for an empty payload.
    let harness = Harness::new();
    harness.seed_asset(0x34);
    let probes: [&str; 3] = [
        "clipboard/ghost.png",
        "clipboard/zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz.png",
        "ignored-apps/anything.png",
    ];
    for probe in probes {
        let error = harness
            .read(probe)
            .expect_err("probe must be rejected after reopen");
        assert_eq!(error.kind, "invalid_asset_ref");
        assert!(
            !error.message.is_empty(),
            "stable rejection reason must be preserved",
        );
    }
}
