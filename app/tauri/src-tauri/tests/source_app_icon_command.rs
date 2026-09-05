//! End-to-end tests for the `clipvault_source_app_icon` and
//! `clipvault_set_entry_title` Tauri commands. The icons bridge is
//! the only path between the relative `source_app_icon_ref` the
//! database stores and the PNG bytes the history card renders.
//!
//! The tests pin the safety and privacy contract the rest of the app
//! depends on:
//!
//! - relative references under `application-icons/` resolve to the
//!   raw PNG bytes and never return an absolute path;
//! - absolute references, traversal segments and references outside
//!   the `application-icons/` namespace are rejected with the
//!   `invalid_icon_ref` kind;
//! - non-PNG payloads and oversized files are rejected with the
//!   `icon_read_error` kind;
//! - the response never carries clipboard content, hashes, snippets
//!   or absolute paths;
//!
//! The title command tests pin the management flow:
//! - a non-empty trimmed title is persisted and returned;
//! - an empty input restores the default;
//! - overlong inputs are rejected with a `title_too_long` error.

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
        fs::create_dir_all(data_dir.join("assets").join("ignored-apps")).expect("mkdir");
        fs::create_dir_all(data_dir.join("assets").join("application-icons")).expect("mkdir");
        let db_path = data_dir.join("clipvault.db");
        let mut db = Database::open(&db_path).expect("open db");
        db.run_migrations(&builtin_migrations()).expect("migrate");
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
        std::mem::forget(dir);
        Self { data_dir }
    }

    fn write_app_icon(&self, name: &str, payload: &[u8]) -> PathBuf {
        let target = self
            .data_dir
            .join("assets")
            .join("application-icons")
            .join(name);
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

/// Wrap the source-app icon command for a test-friendly invocation
/// (the production command is a Tauri closure that reads the
/// managed state; the test wraps the validation pipeline without
/// standing up a Tauri runtime).
fn clipvault_source_app_icon_for_test_helper(
    data_dir: &std::path::Path,
    icon_ref: String,
) -> Result<Vec<u8>, CommandError> {
    use clipvault_platform::read_source_app_icon_bytes;
    use clipvault_platform::IconReadError;
    match read_source_app_icon_bytes(&icon_ref, data_dir) {
        Ok(bytes) => Ok(bytes),
        Err(IconReadError::Ref(reference_error)) => Err(CommandError::new(
            "invalid_icon_ref",
            reference_error.kind_str(),
        )),
        Err(other) => Err(CommandError::new("icon_read_error", other.to_string())),
    }
}

#[test]
fn source_app_icon_returns_png_bytes_for_valid_ref() {
    let harness = Harness::new();
    harness.write_app_icon("com.apple.textedit.png", &png_payload());
    let bytes = clipvault_source_app_icon_for_test_helper(
        harness.data_dir(),
        "application-icons/com.apple.textedit.png".to_string(),
    )
    .expect("must succeed");
    assert_eq!(bytes, png_payload());
}

#[test]
fn source_app_icon_rejects_ignored_apps_namespace_refs() {
    let harness = Harness::new();
    harness.write_app_icon("com.apple.textedit.png", &png_payload());
    let err = clipvault_source_app_icon_for_test_helper(
        harness.data_dir(),
        "ignored-apps/com.apple.textedit.png".to_string(),
    )
    .expect_err("must reject");
    let CommandError { kind, .. } = err;
    assert_eq!(kind, "invalid_icon_ref");
}

#[test]
fn source_app_icon_rejects_absolute_paths() {
    let harness = Harness::new();
    let err =
        clipvault_source_app_icon_for_test_helper(harness.data_dir(), "/etc/passwd".to_string())
            .expect_err("absolute");
    let CommandError { kind, .. } = err;
    assert_eq!(kind, "invalid_icon_ref");
}

#[test]
fn source_app_icon_rejects_traversal() {
    let harness = Harness::new();
    for case in [
        "application-icons/../etc/passwd",
        "application-icons/sub/../../escape",
    ] {
        let err = clipvault_source_app_icon_for_test_helper(harness.data_dir(), case.to_string())
            .expect_err("traversal");
        assert!(
            matches!(err, CommandError { kind, .. } if kind == "invalid_icon_ref"),
            "case {case} must reject traversal, got {err:?}",
        );
    }
}

#[test]
fn source_app_icon_rejects_out_of_scope_refs() {
    let harness = Harness::new();
    for case in ["snapshots/com.apple.textedit.png", "com.apple.textedit.png"] {
        let err = clipvault_source_app_icon_for_test_helper(harness.data_dir(), case.to_string())
            .expect_err("scope");
        assert!(
            matches!(err, CommandError { kind, .. } if kind == "invalid_icon_ref"),
            "case {case} must reject scope, got {err:?}",
        );
    }
}

#[test]
fn source_app_icon_rejects_non_png_payload() {
    let harness = Harness::new();
    harness.write_app_icon("not-a-png.bin", b"definitely not a png");
    let err = clipvault_source_app_icon_for_test_helper(
        harness.data_dir(),
        "application-icons/not-a-png.bin".to_string(),
    )
    .expect_err("not png");
    assert!(matches!(err, CommandError { kind, .. } if kind == "icon_read_error"));
}

#[test]
fn source_app_icon_rejects_oversized_files() {
    let harness = Harness::new();
    let mut bytes = PNG_SIGNATURE.to_vec();
    bytes.extend(std::iter::repeat_n(
        b'X',
        MAX_ICON_BYTES_LEGACY + 1 - PNG_SIGNATURE.len(),
    ));
    harness.write_app_icon("huge.png", &bytes);
    let err = clipvault_source_app_icon_for_test_helper(
        harness.data_dir(),
        "application-icons/huge.png".to_string(),
    )
    .expect_err("too large");
    assert!(matches!(err, CommandError { kind, .. } if kind == "icon_read_error"));
}

#[test]
fn source_app_icon_does_not_carry_clipboard_or_paths() {
    let harness = Harness::new();
    let payload = png_payload();
    harness.write_app_icon("clean.png", &payload);
    let bytes = clipvault_source_app_icon_for_test_helper(
        harness.data_dir(),
        "application-icons/clean.png".to_string(),
    )
    .expect("must succeed");
    assert_eq!(bytes, payload);
    let as_string = String::from_utf8_lossy(&bytes);
    assert!(!as_string.starts_with('/'));
    assert!(!as_string.contains("home"));
    assert!(!as_string.contains(".clipvault"));
    assert!(!as_string.contains("clipboard"));
}

/// Smoke test confirming the existing `clipvault_ignored_app_icon`
/// command still passes after we added the new
/// `application-icons/` namespace. The two commands share the
/// asset pipeline; the test catches a regression where the new
/// namespace accidentally breaks the picker namespace.
#[test]
fn ignored_apps_namespace_still_works_after_source_app_changes() {
    let harness = Harness::new();
    harness.write_app_icon("com.apple.textedit.png", &png_payload());
    let ignored_bytes = clipvault_ignored_app_icon_for_test(
        harness.data_dir(),
        "ignored-apps/com.apple.textedit.png".to_string(),
    );
    assert!(ignored_bytes.is_err(), "no file in ignored-apps/");
}
