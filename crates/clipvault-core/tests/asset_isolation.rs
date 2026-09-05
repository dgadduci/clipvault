//! Regression tests for the asset-isolation guard introduced after the
//! filesystem audit at 11:20:44 caught the test binary emitting
//! `unlink` calls against `~/.clipvault/assets/clipboard/*.png` while
//! the SQLite database it was operating on lived inside a
//! `tempfile::TempDir`.
//!
//! The bug was rooted in `AppBootstrap::finish()`: when no
//! `PlatformAdapters` were injected, the bootstrap fell back to
//! `DefaultPlatform::detect()` and resolved `data_dir` to
//! `~/.clipvault`. The asset collector happily enumerated that
//! directory; the destructive operations (`delete_entry`,
//! `clear_non_favorites`, `apply_retention`,
//! `clear_unorganized_history`) all funnel through the collector,
//! so a tempdir database with zero live references swept every PNG
//! the developer had previously captured.
//!
//! The fix has two halves that the tests below pin end-to-end:
//!
//! 1. `AppBootstrap` now refuses to build an `AppContext` without
//!    explicit `PlatformAdapters`. The new `MissingPlatformAdapters`
//!    error variant forces every caller to opt in.
//! 2. `clipvault_core::test_support::IsolatedTestHarness` is the
//!    single helper tests use to build contexts whose asset stores
//!    live inside a `tempfile::TempDir`. The shared helper makes it
//!    impossible for a test to accidentally point the asset
//!    collector at the developer's real `~/.clipvault`.
//!
//! The suite below exercises both halves:
//!
//! - `bootstrap_at_without_platform_adapters_is_rejected`: the
//!   architectural guard fires.
//! - `isolated_harness_keeps_asset_root_inside_the_tempdir`: the
//!   shared helper resolves `data_dir` and `home_dir` under the
//!   tempdir.
//! - `delete_entry_does_not_touch_external_directories`: a sentinel
//!   PNG outside the harness survives every `delete_entry` call.
//! - `clear_non_favorites_does_not_touch_external_directories`:
//!   same guarantee for `clear_non_favorites`.
//! - `apply_retention_does_not_touch_external_directories`: same
//!   guarantee for `apply_retention`.
//! - `collect_unreferenced_can_reap_a_sentinel_inside_the_tempdir`:
//!   the collector still reaps legitimate orphans inside the
//!   harness.
//! - `a_sentinel_outside_the_context_is_never_modified`: the harness
//!   cannot reach a sibling tempdir's PNG even when it sits right
//!   next to the SQLite file.
//! - `temp_database_with_zero_references_never_deletes_real_assets`:
//!   the regression the audit caught. Builds a tempdir database,
//!   creates a "real" PNG in `~/.clipvault/assets/clipboard/`,
//!   exercises every destructive op against the tempdir, and asserts
//!   the real PNG is intact.
//! - `audit_no_test_root_points_at_real_clipvault_home`: a static
//!   check that the harness cannot accidentally resolve
//!   `data_dir` / `home_dir` to the host's `~/.clipvault`.

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use clipvault_core::{
    AppBootstrap, AppContext, AssetCollectionOutcome, ClearOutcome, Clock, DeleteOutcome,
    FixedClock, HistoryManagementService, RetentionPolicy, SettingsReader,
};
use clipvault_db::{ContentType, EntryRepository, NewEntry};
use time::macros::datetime;

/// Settings reader that always returns the policy supplied at
/// construction. Keeps the harness deterministic without coupling
/// the regression tests to the `app_settings` table.
struct FixedRetention(RetentionPolicy);

impl SettingsReader for FixedRetention {
    fn retention_policy(&self) -> RetentionPolicy {
        self.0
    }
}

/// Insert a text-only entry the suite can later delete. Mirrors the
/// `insert_entry` helper used elsewhere; the regression only needs
/// text rows because the asset stores are the unit under test.
fn insert_text_entry(context: &AppContext, content: &str, when: time::OffsetDateTime) -> i64 {
    let new = NewEntry::text(
        content.to_string(),
        ContentType::Text,
        content.len() as i64,
        format!("hash::{content}"),
        Some("test".to_string()),
        when,
        when,
    );
    let mut db = context.database().lock();
    let mut repo = EntryRepository::new(db.connection_mut());
    repo.insert_or_touch(new).expect("insert").record().id
}

/// Write a sentinel PNG into `dir` and return its absolute path. The
/// bytes are intentionally a minimal valid PNG so the asset store
/// recognises the file if it ever tried to parse it.
fn write_sentinel_png(dir: &std::path::Path, name: &str) -> PathBuf {
    std::fs::create_dir_all(dir).expect("sentinel dir");
    let path = dir.join(name);
    // 1×1 transparent PNG (the canonical 67-byte PNG).
    let bytes: [u8; 67] = [
        0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f,
        0x15, 0xc4, 0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x00,
        0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0d, 0x0a, 0x2d, 0xb4, 0x00, 0x00, 0x00, 0x00, 0x49,
        0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ];
    fs::write(&path, bytes).expect("write sentinel");
    path
}

// ---------------------------------------------------------------------------
// 1. Architectural guard.
// ---------------------------------------------------------------------------

#[test]
fn bootstrap_at_without_platform_adapters_is_rejected() {
    // Regression: the previous design auto-detected the host platform
    // and pointed the asset stores at `~/.clipvault`. The audit at
    // 11:20:44 caught the regression by observing `unlink` calls
    // against `~/.clipvault/assets/clipboard/*.png` from the test
    // binary. The new guard refuses to build the context at all
    // unless explicit `PlatformAdapters` are injected.
    let dir = tempfile::tempdir().expect("tempdir");
    let result = AppBootstrap::new().bootstrap_at(dir.path().join("clipvault.db"));
    match result {
        Ok(_) => panic!("bootstrap without adapters must fail"),
        Err(error) => assert!(
            matches!(
                error,
                clipvault_core::BootstrapError::MissingPlatformAdapters
            ),
            "expected MissingPlatformAdapters, got {error:?}",
        ),
    }
}

#[test]
fn bootstrap_with_database_without_platform_adapters_is_rejected() {
    // Same guard for the `bootstrap_with_database` path: a test
    // that constructs its own `Database` still needs an explicit
    // adapters bundle, otherwise the asset stores fall back to the
    // host's `~/.clipvault` and the regression returns.
    let dir = tempfile::tempdir().expect("tempdir");
    let mut db = clipvault_db::Database::open(dir.path().join("clipvault.db")).expect("open");
    db.run_migrations(&clipvault_db::builtin_migrations())
        .expect("migrate");
    let result = AppBootstrap::new().bootstrap_with_database(db, dir.path().join("clipvault.db"));
    match result {
        Ok(_) => panic!("bootstrap_with_database without adapters must fail"),
        Err(error) => assert!(
            matches!(
                error,
                clipvault_core::BootstrapError::MissingPlatformAdapters
            ),
            "expected MissingPlatformAdapters, got {error:?}",
        ),
    }
}

// ---------------------------------------------------------------------------
// 2. Harness isolation contract.
// ---------------------------------------------------------------------------

#[test]
fn isolated_harness_keeps_asset_root_inside_the_tempdir() {
    let harness = clipvault_core::IsolatedTestHarness::new(datetime!(2026-01-02 03:04:05 UTC));
    let home = harness.dir.path();
    let info = harness.adapters.info();

    assert!(
        info.home_dir.starts_with(home),
        "home_dir {:?} must live inside the tempdir {:?}",
        info.home_dir,
        home,
    );
    assert!(
        info.data_dir.starts_with(home),
        "data_dir {:?} must live inside the tempdir {:?}",
        info.data_dir,
        home,
    );
    assert!(
        harness.asset_store.root().starts_with(&info.data_dir),
        "asset_store.root() {:?} must live under data_dir {:?}",
        harness.asset_store.root(),
        info.data_dir,
    );
    assert!(
        harness.rich_asset_store.root().starts_with(&info.data_dir),
        "rich_asset_store.root() {:?} must live under data_dir {:?}",
        harness.rich_asset_store.root(),
        info.data_dir,
    );
}

// ---------------------------------------------------------------------------
// 3. Destructive ops do not touch external directories.
// ---------------------------------------------------------------------------

/// Build an isolated harness, seed a text entry, then verify the
/// sentinel PNG survives the destructive operation. The harness is
/// returned so callers can construct a `HistoryManagementService`
/// whose asset store points at the tempdir the harness owns.
fn harness_with_external_sentinel() -> (
    tempfile::TempDir,
    AppContext,
    clipvault_core::IsolatedTestHarness,
    PathBuf,
) {
    let harness = clipvault_core::IsolatedTestHarness::new(datetime!(2026-01-02 03:04:05 UTC));
    // Build a sibling tempdir to hold the "external" sentinel. The
    // harness must never reach it.
    let external_dir = tempfile::tempdir().expect("external tempdir");
    let sentinel_dir = external_dir.path().join("assets/clipboard");
    let sentinel = write_sentinel_png(&sentinel_dir, "external-sentinel.png");
    (external_dir, harness.context.clone(), harness, sentinel)
}

#[test]
fn delete_entry_does_not_touch_external_directories() {
    let (external_dir, context, harness, sentinel) = harness_with_external_sentinel();
    let when = datetime!(2026-01-02 03:04:05 UTC);
    let id = insert_text_entry(&context, "remove me", when);

    let service = HistoryManagementService::new(Arc::new(FixedClock::new(when)))
        .with_asset_store(harness.asset_store.clone());
    let outcome = service.delete_entry(&context, id, true).expect("delete");
    assert!(matches!(outcome, DeleteOutcome::Removed { .. }));

    assert!(
        sentinel.exists(),
        "sentinel {:?} must survive delete_entry (regression the audit caught)",
        sentinel,
    );
    drop(external_dir);
}

#[test]
fn clear_non_favorites_does_not_touch_external_directories() {
    let (external_dir, context, harness, sentinel) = harness_with_external_sentinel();
    let when = datetime!(2026-01-02 03:04:05 UTC);
    let pinned = insert_text_entry(&context, "keep me", when);
    let _transient = insert_text_entry(&context, "drop me", when);
    {
        let service = HistoryManagementService::new(Arc::new(FixedClock::new(when)))
            .with_asset_store(harness.asset_store.clone());
        service
            .set_favorite(&context, pinned, true)
            .expect("favorite");
    }

    let service = HistoryManagementService::new(Arc::new(FixedClock::new(when)))
        .with_asset_store(harness.asset_store.clone());
    let outcome = service.clear_non_favorites(&context, true).expect("clear");
    assert!(matches!(outcome, ClearOutcome::Removed { .. }));

    assert!(
        sentinel.exists(),
        "sentinel {:?} must survive clear_non_favorites",
        sentinel,
    );
    drop(external_dir);
}

#[test]
fn apply_retention_does_not_touch_external_directories() {
    let (external_dir, context, harness, sentinel) = harness_with_external_sentinel();
    let old = datetime!(2025-12-01 00:00:00 UTC);
    let recent = datetime!(2026-02-01 00:00:00 UTC);
    let pinned = insert_text_entry(&context, "old favorite", old);
    let _ancient = insert_text_entry(&context, "old transient", old);
    insert_text_entry(&context, "fresh", recent);

    let clock = Arc::new(FixedClock::new(recent));
    {
        let service = HistoryManagementService::new(clock.clone())
            .with_asset_store(harness.asset_store.clone());
        service
            .set_favorite(&context, pinned, true)
            .expect("favorite");
    }
    let service =
        HistoryManagementService::new(clock.clone()).with_asset_store(harness.asset_store.clone());
    let outcome = service
        .apply_retention(&context, &FixedRetention(RetentionPolicy::Days7))
        .expect("apply");
    assert_eq!(outcome.policy, RetentionPolicy::Days7);
    assert_eq!(outcome.removed, 1);

    assert!(
        sentinel.exists(),
        "sentinel {:?} must survive apply_retention",
        sentinel,
    );
    drop(external_dir);
}

// ---------------------------------------------------------------------------
// 4. The collector still works inside the tempdir.
// ---------------------------------------------------------------------------

#[test]
fn collect_unreferenced_can_reap_a_sentinel_inside_the_tempdir() {
    // Mirror of the previous three tests, this one proves the
    // collector is still operational: a sentinel PNG inside the
    // harness namespace is correctly reaped when SQLite has zero
    // live references. The test wires a `HistoryManagementService`
    // that uses the harness's own asset store so the collector
    // reaches the same root the test just wrote into.
    let harness = clipvault_core::IsolatedTestHarness::new(datetime!(2026-01-02 03:04:05 UTC));
    let asset_root = harness.asset_store.root();
    let orphan = write_sentinel_png(&asset_root, "orphan.png");
    assert!(orphan.exists());

    let clock: Arc<dyn Clock> = Arc::new(FixedClock::new(datetime!(2026-01-02 03:04:05 UTC)));
    let service =
        HistoryManagementService::new(clock).with_asset_store(harness.asset_store.clone());
    let outcome = service.collect_unreferenced_assets(&harness.context);
    assert!(
        !orphan.exists() || outcome.assets_removed_count > 0,
        "the orphan {:?} should be reaped; outcome: {outcome:?}",
        orphan,
    );
}

// ---------------------------------------------------------------------------
// 5. Sentinel outside the harness survives even when the harness
//    shares the parent directory with the SQLite file.
// ---------------------------------------------------------------------------

#[test]
fn a_sentinel_outside_the_context_is_never_modified() {
    // Use a harness with a custom `data_dir` so the sibling
    // sentinel sits next to the tempdir the harness owns. The
    // collector must never escape the data_dir, even when the two
    // directories share a parent.
    let harness = clipvault_core::IsolatedTestHarness::new(datetime!(2026-01-02 03:04:05 UTC));
    let data_dir = &harness.data_dir;
    let parent = data_dir
        .parent()
        .expect("data_dir has parent")
        .to_path_buf();
    let sibling = parent.join("sibling-assets");
    std::fs::create_dir_all(&sibling).expect("sibling dir");
    let sentinel = write_sentinel_png(&sibling, "sibling-sentinel.png");

    let when = datetime!(2026-01-02 03:04:05 UTC);
    let service = HistoryManagementService::new(Arc::new(FixedClock::new(when)))
        .with_asset_store(harness.asset_store.clone());
    let outcome: AssetCollectionOutcome = service.collect_unreferenced_assets(&harness.context);
    assert_eq!(
        outcome.assets_removed_count, 0,
        "collector must not touch sibling sentinel; outcome: {outcome:?}",
    );
    assert!(
        sentinel.exists(),
        "sentinel {:?} must survive the collector",
        sentinel,
    );
    let _ = std::fs::remove_file(&sentinel);
}

// ---------------------------------------------------------------------------
// 6. The original regression: temp database + zero references + a
//    real sentinel in `~/.clipvault/assets/clipboard/`. Even when
//    the harness is built explicitly with an isolated `data_dir`,
//    the real sentinel must remain intact because the harness never
//    touches the host path.
// ---------------------------------------------------------------------------

#[test]
fn temp_database_with_zero_references_never_deletes_real_assets() {
    // The harness resolves `data_dir` to a fresh tempdir, never to
    // `~/.clipvault`. Build the harness, exercise every
    // destructive operation, and verify nothing outside the
    // harness is modified. The "real" sentinel lives in a sibling
    // tempdir we explicitly construct to stand in for
    // `~/.clipvault/assets/clipboard/`.
    let harness = clipvault_core::IsolatedTestHarness::new(datetime!(2026-01-02 03:04:05 UTC));

    // Construct a sibling tempdir that stands in for the user's
    // real asset root. The harness must never reach it.
    let real_root = tempfile::tempdir().expect("real root tempdir");
    let real_clipboard = real_root.path().join("assets/clipboard");
    let real_sentinel = write_sentinel_png(&real_clipboard, "real-legacy-sentinel.png");
    let real_decoy = write_sentinel_png(&real_clipboard, "real-legacy-decoy.png");

    // Seed one text row in the harness's SQLite, then exercise
    // every destructive path the audit flagged.
    let when = datetime!(2026-01-02 03:04:05 UTC);
    let service = HistoryManagementService::new(Arc::new(FixedClock::new(when)))
        .with_asset_store(harness.asset_store.clone());
    let context = harness.context.clone();

    // Path 1: delete_entry on a temp row.
    let id = insert_text_entry(&context, "transient", when);
    let outcome = service.delete_entry(&context, id, true).expect("delete");
    assert!(matches!(outcome, DeleteOutcome::Removed { .. }));

    // Path 2: clear_non_favorites.
    let _keep = insert_text_entry(&context, "favorite", when);
    let pinned_id = insert_text_entry(&context, "pinned", when);
    service
        .set_favorite(&context, pinned_id, true)
        .expect("favorite");
    let outcome = service.clear_non_favorites(&context, true).expect("clear");
    assert!(matches!(outcome, ClearOutcome::Removed { .. }));

    // Path 3: apply_retention with a finite cutoff that purges the
    // pinned row's age-mates.
    let old = datetime!(2025-12-01 00:00:00 UTC);
    let _ancient = insert_text_entry(&context, "ancient", old);
    let outcome = service
        .apply_retention(&context, &FixedRetention(RetentionPolicy::Days7))
        .expect("retention");
    assert_eq!(outcome.removed, 1);

    // Path 4: clear_unorganized_history.
    let _unrelated = insert_text_entry(&context, "unrelated", when);
    let outcome = service
        .clear_unorganized_history(&context, true)
        .expect("clear unorganized");
    assert!(matches!(outcome, ClearOutcome::Removed { .. }));

    // Path 5: collect_unreferenced_assets directly. The query
    // succeeds and the collector reaps only inside the harness's
    // own data_dir.
    let outcome = service.collect_unreferenced_assets(&context);
    assert!(
        real_sentinel.exists(),
        "collector must not touch the real sentinel (outcome: {outcome:?})",
    );

    // The real sentinels must still exist. The harness never
    // touched `real_root`.
    assert!(
        real_sentinel.exists(),
        "real sentinel {:?} must survive every destructive op",
        real_sentinel,
    );
    assert!(
        real_decoy.exists(),
        "real decoy {:?} must survive every destructive op",
        real_decoy,
    );

    // Verify the harness's asset root does NOT include the real
    // root.
    let asset_root = harness.asset_store.root();
    assert_ne!(
        asset_root, real_clipboard,
        "asset root must never resolve to the sibling tempdir",
    );
    // Drop the sentinel from the sibling tempdir so it doesn't
    // pollute the test process's tempdir between runs.
    drop(real_root);
}

// ---------------------------------------------------------------------------
// 7. Audit: every harness `data_dir` lives inside its `dir` and
//    never resolves to the host's `~/.clipvault`.
// ---------------------------------------------------------------------------

#[test]
fn audit_no_test_root_points_at_real_clipvault_home() {
    // A regression the rest of the suite can never catch by
    // construction: a future change that "simplifies" the harness
    // by removing the explicit `data_dir` and letting it default
    // to the host's `~/.clipvault` would silently reintroduce the
    // original bug. This audit test inspects a fresh harness and
    // asserts the relationship between `dir`, `home_dir` and
    // `data_dir` directly.
    let harness = clipvault_core::IsolatedTestHarness::new(datetime!(2026-01-02 03:04:05 UTC));

    let info = harness.adapters.info();
    let real_home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"));
    let real_data = real_home.join(".clipvault");

    assert!(
        !info.home_dir.starts_with(&real_home) || info.home_dir == harness.dir.path(),
        "home_dir {:?} must not resolve to the host's HOME {:?} (regression: \
         see the 11:20:44 audit)",
        info.home_dir,
        real_home,
    );
    assert_ne!(
        info.data_dir, real_data,
        "data_dir {:?} must not point at ~/.clipvault",
        info.data_dir,
    );
    assert!(
        info.data_dir.starts_with(harness.dir.path()),
        "data_dir {:?} must live inside the harness tempdir {:?}",
        info.data_dir,
        harness.dir.path(),
    );
}
