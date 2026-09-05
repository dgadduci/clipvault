//! End-to-end tests for the `clipvault_ignored_app_icon` Tauri
//! command. The command is the single bridge between the relative
//! `icon_ref` the database stores and the PNG bytes the settings
//! panel renders. The tests below pin the privacy and safety
//! guarantees the rest of the app depends on:
//!
//! - a relative reference under `ignored-apps/` resolves to the raw
//!   PNG bytes and never returns an absolute path;
//! - absolute references, traversal segments, references outside
//!   `ignored-apps/` and references whose canonical path escapes the
//!   assets directory are rejected with a typed
//!   [`CommandError`] whose `kind` is `invalid_icon_ref`;
//! - legacy files that exceed the write-side cap (older 1024×1024
//!   PNGs from Chrome/Affinity) stay readable until the user
//!   re-selects the application and the picker overwrites them;
//! - non-PNG payloads and oversized files above the legacy cap are
//!   rejected with the `icon_read_error` kind so the frontend can
//!   fall back to the letter render;
//! - the response never carries clipboard content, hashes, snippets,
//!   absolute paths or any other metadata outside the PNG payload.

use std::fs;
use std::path::PathBuf;

use clipvault_app::commands::{clipvault_ignored_app_icon_for_test, CommandError};
use clipvault_core::{AppBootstrap, Clock, PlatformAdapters};
use clipvault_db::{builtin_migrations, Database};
use clipvault_platform::{
    Capabilities, DisplayServer, OsFamily, PlatformInfo, MAX_ICON_BYTES_LEGACY,
};

const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];

struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> time::OffsetDateTime {
        time::OffsetDateTime::now_utc()
    }
}

struct Harness {
    data_dir: PathBuf,
}

impl Harness {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let data_dir = dir.path().join("clipvault-home");
        fs::create_dir_all(data_dir.join("assets").join("ignored-apps")).expect("mkdir assets");
        let db_path = data_dir.join("clipvault.db");
        let mut db = Database::open(&db_path).expect("open db");
        db.run_migrations(&builtin_migrations()).expect("migrate");
        // Build a context through the canonical bootstrap so the
        // platform info and capabilities match what the Tauri shell
        // exposes in production.
        let info = PlatformInfo {
            home_dir: dir.path().to_path_buf(),
            data_dir: data_dir.clone(),
            os_family: OsFamily::Macos,
            display_server: DisplayServer::Unknown,
        };
        let adapters = PlatformAdapters::stub(&info, Capabilities::default());
        let _ = AppBootstrap::new()
            .with_clock(std::sync::Arc::new(FixedClock) as std::sync::Arc<dyn Clock>)
            .with_clipboard(std::sync::Arc::new(clipvault_core::FakeClipboard::new())
                as std::sync::Arc<dyn clipvault_core::Clipboard>)
            .with_platform_adapters(adapters)
            .bootstrap_at(&db_path)
            .expect("bootstrap");
        // `dir` is kept alive through the `Drop` semantics of the
        // harness: dropping the harness removes the temp directory,
        // so the test holds it implicitly via `data_dir`. The
        // explicit `dir` field is dropped last when the harness goes
        // out of scope, after every test has finished.
        std::mem::forget(dir);
        Self { data_dir }
    }

    fn write_icon(&self, name: &str, payload: &[u8]) -> PathBuf {
        let target = self.data_dir.join("assets").join("ignored-apps").join(name);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).expect("mkdir parent");
        }
        fs::write(&target, payload).expect("write");
        target
    }

    fn data_dir(&self) -> &std::path::Path {
        &self.data_dir
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        if let Some(parent) = self.data_dir.parent() {
            let _ = fs::remove_dir_all(parent);
        }
    }
}

fn png_payload() -> Vec<u8> {
    let mut bytes = PNG_SIGNATURE.to_vec();
    bytes.extend_from_slice(b"fixture");
    bytes
}

#[test]
fn clipvault_ignored_app_icon_returns_png_bytes_for_valid_ref() {
    let harness = Harness::new();
    harness.write_icon("com.apple.textedit.png", &png_payload());
    let bytes = clipvault_ignored_app_icon_for_test(
        harness.data_dir(),
        "ignored-apps/com.apple.textedit.png".to_string(),
    )
    .expect("must succeed");
    assert_eq!(bytes, png_payload());
}

#[test]
fn clipvault_ignored_app_icon_rejects_absolute_paths() {
    let harness = Harness::new();
    let err = clipvault_ignored_app_icon_for_test(harness.data_dir(), "/etc/passwd".to_string())
        .expect_err("must reject absolute");
    let CommandError { kind, .. } = err;
    assert_eq!(kind, "invalid_icon_ref");
}

#[test]
fn clipvault_ignored_app_icon_rejects_traversal() {
    let harness = Harness::new();
    for case in [
        "ignored-apps/../etc/passwd",
        "ignored-apps/sub/../../escape",
    ] {
        let err = clipvault_ignored_app_icon_for_test(harness.data_dir(), case.to_string())
            .expect_err("must reject traversal");
        assert!(
            matches!(err, CommandError { kind, .. } if kind == "invalid_icon_ref"),
            "case {case} must reject traversal, got {err:?}",
        );
    }
}

#[test]
fn clipvault_ignored_app_icon_rejects_out_of_scope_refs() {
    let harness = Harness::new();
    for case in ["snapshots/com.apple.textedit.png", "com.apple.textedit.png"] {
        let err = clipvault_ignored_app_icon_for_test(harness.data_dir(), case.to_string())
            .expect_err("must reject out_of_scope");
        assert!(
            matches!(err, CommandError { kind, .. } if kind == "invalid_icon_ref"),
            "case {case} must reject scope, got {err:?}",
        );
    }
}

#[test]
fn clipvault_ignored_app_icon_rejects_missing_files() {
    let harness = Harness::new();
    let err = clipvault_ignored_app_icon_for_test(
        harness.data_dir(),
        "ignored-apps/ghost.png".to_string(),
    )
    .expect_err("missing file");
    // A missing file is rejected with the `invalid_icon_ref` kind
    // because the validator surfaces a `NotFound` reference error
    // before any read attempt. The frontend treats both kinds as a
    // safe fallback, so the precise label is metadata-only.
    assert!(matches!(err, CommandError { kind, .. } if kind == "invalid_icon_ref"));
}

#[cfg(unix)]
#[test]
fn clipvault_ignored_app_icon_rejects_symlink_escape() {
    let harness = Harness::new();
    let icons = harness.data_dir.join("assets").join("ignored-apps");
    let outside = harness.data_dir.join("outside.png");
    fs::write(&outside, png_payload()).expect("write outside");
    std::os::unix::fs::symlink(&outside, icons.join("escape.png")).expect("symlink");
    let err = clipvault_ignored_app_icon_for_test(
        harness.data_dir(),
        "ignored-apps/escape.png".to_string(),
    )
    .expect_err("escape");
    assert!(matches!(err, CommandError { kind, .. } if kind == "invalid_icon_ref"));
}

#[test]
fn clipvault_ignored_app_icon_rejects_non_png_payload() {
    let harness = Harness::new();
    harness.write_icon("not-a-png.bin", b"definitely not a png");
    let err = clipvault_ignored_app_icon_for_test(
        harness.data_dir(),
        "ignored-apps/not-a-png.bin".to_string(),
    )
    .expect_err("not png");
    assert!(matches!(err, CommandError { kind, .. } if kind == "icon_read_error"));
}

#[test]
fn clipvault_ignored_app_icon_accepts_legacy_large_icons() {
    // Pre-existing 1024×1024 PNGs (Chrome, Affinity) used to exceed
    // `MAX_ICON_BYTES` but stay under `MAX_ICON_BYTES_LEGACY`. The
    // command must keep them readable so the row does not render as
    // a broken icon forever; re-selecting the application rewrites
    // the asset at the documented smaller size.
    let harness = Harness::new();
    let mut bytes = PNG_SIGNATURE.to_vec();
    bytes.extend(std::iter::repeat_n(
        b'X',
        600 * 1024, // above MAX_ICON_BYTES (512 KB), below MAX_ICON_BYTES_LEGACY (4 MB)
    ));
    harness.write_icon("legacy.png", &bytes);
    let read = clipvault_ignored_app_icon_for_test(
        harness.data_dir(),
        "ignored-apps/legacy.png".to_string(),
    )
    .expect("legacy icon must be readable");
    assert_eq!(read, bytes);
}

#[test]
fn clipvault_ignored_app_icon_rejects_oversized_files() {
    // Anything above `MAX_ICON_BYTES_LEGACY` is still rejected so a
    // tampered asset directory cannot exhaust the renderer memory.
    let harness = Harness::new();
    let mut bytes = PNG_SIGNATURE.to_vec();
    bytes.extend(std::iter::repeat_n(
        b'X',
        MAX_ICON_BYTES_LEGACY + 1 - PNG_SIGNATURE.len(),
    ));
    harness.write_icon("huge.png", &bytes);
    let err = clipvault_ignored_app_icon_for_test(
        harness.data_dir(),
        "ignored-apps/huge.png".to_string(),
    )
    .expect_err("too large");
    assert!(matches!(err, CommandError { kind, .. } if kind == "icon_read_error"));
}

#[test]
fn clipvault_ignored_app_icon_does_not_carry_clipboard_or_paths() {
    let harness = Harness::new();
    let payload = png_payload();
    harness.write_icon("clean.png", &payload);
    let bytes = clipvault_ignored_app_icon_for_test(
        harness.data_dir(),
        "ignored-apps/clean.png".to_string(),
    )
    .expect("must succeed");
    // The bytes must contain only the PNG payload — no clipboard
    // content, no absolute path, no metadata outside the PNG.
    assert_eq!(bytes, payload);
    // Reject explicit absolute-path signatures the backend could
    // accidentally leak.
    let as_string = String::from_utf8_lossy(&bytes);
    assert!(!as_string.starts_with('/'));
    assert!(!as_string.contains("home"));
    assert!(!as_string.contains(".clipvault"));
    assert!(!as_string.contains("clipboard"));
}
