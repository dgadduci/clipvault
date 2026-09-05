use std::sync::Arc;

use clipvault_core::{
    AppBootstrap, AppContext, ClearOutcome, Clock, DeleteOutcome, HistoryManagementService,
    LocalSettingsReader, RetentionPolicy, SetFavoriteResult, SettingsReader,
};
use clipvault_db::{ContentType, EntryRepository, NewEntry};
use tempfile::TempDir;
use time::macros::datetime;

#[derive(Debug, Clone)]
struct FixedClock {
    instant: time::OffsetDateTime,
}

impl Clock for FixedClock {
    fn now(&self) -> time::OffsetDateTime {
        self.instant
    }
}

fn tempdir() -> TempDir {
    tempfile::tempdir().expect("tempdir")
}

/// Build an [`AppContext`] whose `PlatformAdapters` bundle points
/// `home_dir` / `data_dir` inside the tempdir. Required since the
/// destructive operations exercised below (`delete_entry`,
/// `clear_non_favorites`, `apply_retention`) all funnel through the
/// asset collector; without the isolated harness the collector
/// would sweep `~/.clipvault/assets/clipboard/*.png` because the
/// host-detected platform would point the asset stores at the real
/// user directory.
fn bootstrap_with_clock(clock: Arc<dyn Clock>) -> (TempDir, AppContext) {
    let dir = tempdir();
    let adapters = clipvault_core::build_isolated_adapters(dir.path(), &dir.path().join("data"));
    let context = AppBootstrap::new()
        .with_clock(clock)
        .with_clipboard(Arc::new(clipvault_core::FakeClipboard::new()))
        .with_platform_adapters(adapters)
        .bootstrap_at(dir.path().join("clipvault.db"))
        .expect("bootstrap");
    (dir, context)
}

fn insert_entry(context: &AppContext, content: &str, when: time::OffsetDateTime) -> i64 {
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

fn set_retention(context: &AppContext, raw: &str, when: time::OffsetDateTime) {
    use clipvault_db::AppSettingsRepository;
    let mut db = context.database().lock();
    let mut repo = AppSettingsRepository::new(db.connection_mut());
    repo.set(clipvault_core::RETENTION_SETTING_KEY, raw, when)
        .expect("set");
}

#[test]
fn set_favorite_marks_and_unmarks_entries() {
    let when = datetime!(2026-01-02 03:04:05 UTC);
    let clock: Arc<dyn Clock> = Arc::new(FixedClock { instant: when });
    let (_dir, context) = bootstrap_with_clock(clock.clone());
    let id = insert_entry(&context, "alpha", when);
    let service = HistoryManagementService::new(clock);

    let SetFavoriteResult { entry } = service
        .set_favorite(&context, id, true)
        .expect("set_favorite true");
    let entry = entry.expect("present");
    assert!(entry.is_pinned);

    let SetFavoriteResult { entry } = service
        .set_favorite(&context, id, false)
        .expect("set_favorite false");
    let entry = entry.expect("present");
    assert!(!entry.is_pinned);
}

#[test]
fn set_favorite_for_missing_entry_returns_none() {
    let when = datetime!(2026-01-02 03:04:05 UTC);
    let clock: Arc<dyn Clock> = Arc::new(FixedClock { instant: when });
    let (_dir, context) = bootstrap_with_clock(clock.clone());
    let service = HistoryManagementService::new(clock);
    let outcome = service
        .set_favorite(&context, 9_999, true)
        .expect("set_favorite");
    assert!(outcome.entry.is_none());
}

#[test]
fn delete_entry_requires_confirmation() {
    let when = datetime!(2026-01-02 03:04:05 UTC);
    let clock: Arc<dyn Clock> = Arc::new(FixedClock { instant: when });
    let (_dir, context) = bootstrap_with_clock(clock.clone());
    let id = insert_entry(&context, "alpha", when);
    let service = HistoryManagementService::new(clock);

    let outcome = service
        .delete_entry(&context, id, false)
        .expect("delete without confirm");
    assert_eq!(outcome, DeleteOutcome::ConfirmationRequired);

    let mut db = context.database().lock();
    let repo = EntryRepository::new(db.connection_mut());
    assert_eq!(repo.count().unwrap(), 1, "no row should be removed");
}

#[test]
fn delete_entry_is_idempotent() {
    let when = datetime!(2026-01-02 03:04:05 UTC);
    let clock: Arc<dyn Clock> = Arc::new(FixedClock { instant: when });
    let (_dir, context) = bootstrap_with_clock(clock.clone());
    let id = insert_entry(&context, "alpha", when);
    let service = HistoryManagementService::new(clock);

    let outcome = service.delete_entry(&context, id, true).expect("delete");
    assert_eq!(outcome, DeleteOutcome::Removed { removed: 1 });
    let outcome = service
        .delete_entry(&context, id, true)
        .expect("delete again");
    assert_eq!(outcome, DeleteOutcome::NotFound);
}

#[test]
fn clear_history_requires_confirmation_and_preserves_favorites() {
    let old = datetime!(2025-12-01 00:00:00 UTC);
    let recent = datetime!(2026-02-01 00:00:00 UTC);
    let clock: Arc<dyn Clock> = Arc::new(FixedClock { instant: recent });
    let (_dir, context) = bootstrap_with_clock(clock.clone());
    let pinned = insert_entry(&context, "keep-me", old);
    insert_entry(&context, "drop-1", old);
    insert_entry(&context, "drop-2", old);
    let service = HistoryManagementService::new(clock.clone());
    service
        .set_favorite(&context, pinned, true)
        .expect("favorite")
        .entry
        .expect("entry");

    // Without confirmation: nothing changes.
    let outcome = service.clear_non_favorites(&context, false).expect("clear");
    assert_eq!(outcome, ClearOutcome::ConfirmationRequired);
    assert_eq!(
        EntryRepository::new(context.database().lock().connection_mut())
            .count()
            .unwrap(),
        3
    );

    // With confirmation: favorites stay.
    let outcome = service
        .clear_non_favorites(&context, true)
        .expect("clear confirmed");
    assert_eq!(outcome, ClearOutcome::Removed { removed: 2 });

    let mut db = context.database().lock();
    let repo = EntryRepository::new(db.connection_mut());
    assert_eq!(repo.count().unwrap(), 1);
    assert!(repo.find_by_id(pinned).unwrap().unwrap().is_pinned);
    let _ = recent;
}

#[test]
fn apply_retention_uses_local_setting_and_skips_favorites() {
    let old = datetime!(2025-12-01 00:00:00 UTC);
    let recent = datetime!(2026-02-01 00:00:00 UTC);
    let clock: Arc<dyn Clock> = Arc::new(FixedClock { instant: recent });
    let (_dir, context) = bootstrap_with_clock(clock.clone());

    set_retention(&context, "7d", recent);
    let pinned = insert_entry(&context, "favorite-old", old);
    insert_entry(&context, "non-favorite-old", old);
    insert_entry(&context, "fresh", recent);
    let service = HistoryManagementService::new(clock.clone());
    service
        .set_favorite(&context, pinned, true)
        .expect("favorite")
        .entry
        .expect("entry");

    let reader = LocalSettingsReader::new(context.clone(), clock);
    let outcome = service
        .apply_retention(&context, &reader)
        .expect("retention");
    assert_eq!(outcome.policy, RetentionPolicy::Days7);
    assert_eq!(
        outcome.removed, 1,
        "exactly one non-favorite should be purged"
    );
    assert_eq!(
        EntryRepository::new(context.database().lock().connection_mut())
            .count()
            .unwrap(),
        2
    );
    let mut db = context.database().lock();
    let repo = EntryRepository::new(db.connection_mut());
    assert!(repo.find_by_id(pinned).unwrap().is_some());
    drop(db);
}

#[test]
fn apply_retention_with_forever_is_a_noop() {
    let recent = datetime!(2026-02-01 00:00:00 UTC);
    let clock: Arc<dyn Clock> = Arc::new(FixedClock { instant: recent });
    let (_dir, context) = bootstrap_with_clock(clock.clone());
    set_retention(&context, "forever", recent);
    let _ = insert_entry(&context, "ancient", datetime!(2020-01-01 00:00:00 UTC));
    insert_entry(&context, "fresh", recent);

    let reader = LocalSettingsReader::new(context.clone(), clock.clone());
    let service = HistoryManagementService::new(clock);
    let outcome = service
        .apply_retention(&context, &reader)
        .expect("retention");
    assert_eq!(outcome.policy, RetentionPolicy::Forever);
    assert_eq!(outcome.removed, 0);
    assert_eq!(
        EntryRepository::new(context.database().lock().connection_mut())
            .count()
            .unwrap(),
        2
    );
}

#[test]
fn apply_retention_falls_back_to_default_when_setting_missing() {
    let recent = datetime!(2026-02-01 00:00:00 UTC);
    let clock: Arc<dyn Clock> = Arc::new(FixedClock { instant: recent });
    let (_dir, context) = bootstrap_with_clock(clock.clone());
    insert_entry(&context, "ancient", datetime!(2020-01-01 00:00:00 UTC));
    insert_entry(&context, "fresh", recent);

    let reader = LocalSettingsReader::new(context.clone(), clock.clone());
    let service = HistoryManagementService::new(clock);
    let outcome = service
        .apply_retention(&context, &reader)
        .expect("retention");
    assert_eq!(outcome.policy, clipvault_core::DEFAULT_RETENTION);
    // The default is 30 days: the "ancient" entry is purged, the
    // "fresh" one stays.
    assert_eq!(outcome.removed, 1);
}

#[test]
fn apply_retention_is_idempotent_across_repeated_invocations() {
    let recent = datetime!(2026-02-01 00:00:00 UTC);
    let clock: Arc<dyn Clock> = Arc::new(FixedClock { instant: recent });
    let (_dir, context) = bootstrap_with_clock(clock.clone());
    set_retention(&context, "30d", recent);
    insert_entry(&context, "ancient", datetime!(2020-01-01 00:00:00 UTC));

    let reader = LocalSettingsReader::new(context.clone(), clock.clone());
    let service = HistoryManagementService::new(clock);

    let first = service.apply_retention(&context, &reader).expect("first");
    let second = service.apply_retention(&context, &reader).expect("second");

    // The first call purges the only matching row; the second call
    // observes an empty matching set and reports zero removals.
    assert_eq!(first.removed, 1);
    assert_eq!(second.removed, 0);
    let remaining = {
        let mut db = context.database().lock();
        EntryRepository::new(db.connection_mut()).count().unwrap()
    };
    assert_eq!(remaining, 0);
}

#[test]
fn preview_retention_reports_would_remove_without_mutating() {
    let recent = datetime!(2026-02-01 00:00:00 UTC);
    let clock: Arc<dyn Clock> = Arc::new(FixedClock { instant: recent });
    let (_dir, context) = bootstrap_with_clock(clock.clone());
    set_retention(&context, "30d", recent);
    insert_entry(&context, "ancient", datetime!(2020-01-01 00:00:00 UTC));
    insert_entry(&context, "fresh", recent);

    let service = HistoryManagementService::new(clock.clone());
    let preview = service
        .preview_retention(&context, &LocalSettingsReader::new(context.clone(), clock))
        .expect("preview");
    assert_eq!(preview.policy, RetentionPolicy::Days30);
    assert_eq!(preview.would_remove, 1);
    let remaining = {
        let mut db = context.database().lock();
        EntryRepository::new(db.connection_mut()).count().unwrap()
    };
    assert_eq!(remaining, 2, "preview must not mutate the database");
}

// ---------------------------------------------------------------------------
// Bug 2 regression: bootstrap must NOT overwrite the user's retention choice.
// Every supported policy must survive a second bootstrap without being reset
// to `DEFAULT_RETENTION`.
// ---------------------------------------------------------------------------
#[test]
fn each_retention_policy_survives_a_second_bootstrap() {
    let recent = datetime!(2026-02-01 00:00:00 UTC);
    let clock: Arc<dyn Clock> = Arc::new(FixedClock { instant: recent });
    let dir = tempdir();
    let db_path = dir.path().join("clipvault.db");

    for (raw, expected) in [
        ("forever", RetentionPolicy::Forever),
        ("days_7", RetentionPolicy::Days7),
        ("days_30", RetentionPolicy::Days30),
        ("days_90", RetentionPolicy::Days90),
    ] {
        // First bootstrap seeds the default. The harness wires an
        // isolated `PlatformAdapters` bundle so the asset collector
        // cannot reach the developer's real `~/.clipvault`.
        let adapters =
            clipvault_core::build_isolated_adapters(dir.path(), &dir.path().join("data"));
        let ctx1 = AppBootstrap::new()
            .with_clock(clock.clone())
            .with_clipboard(Arc::new(clipvault_core::FakeClipboard::new()))
            .with_platform_adapters(adapters)
            .bootstrap_at(&db_path)
            .expect("first bootstrap");
        set_retention(&ctx1, raw, recent);
        // Drop the first context to release the lock before
        // re-opening the same database file.
        drop(ctx1);

        // Second bootstrap must observe the user choice intact.
        let adapters =
            clipvault_core::build_isolated_adapters(dir.path(), &dir.path().join("data"));
        let ctx2 = AppBootstrap::new()
            .with_clock(clock.clone())
            .with_clipboard(Arc::new(clipvault_core::FakeClipboard::new()))
            .with_platform_adapters(adapters)
            .bootstrap_at(&db_path)
            .expect("second bootstrap");
        let reader = LocalSettingsReader::new(ctx2.clone(), clock.clone());
        let resolved = reader.retention_policy();
        assert_eq!(
            resolved, expected,
            "policy `{raw}` must survive a restart (got {resolved:?})"
        );
    }
}

#[test]
fn bootstrap_seeds_default_only_when_key_missing() {
    let recent = datetime!(2026-02-01 00:00:00 UTC);
    let clock: Arc<dyn Clock> = Arc::new(FixedClock { instant: recent });
    let dir = tempdir();
    let db_path = dir.path().join("clipvault.db");

    // Empty database: the first bootstrap must seed the default.
    // The harness wires an isolated `PlatformAdapters` bundle so
    // the asset collector cannot reach the developer's real
    // `~/.clipvault`.
    let adapters = clipvault_core::build_isolated_adapters(dir.path(), &dir.path().join("data"));
    let ctx1 = AppBootstrap::new()
        .with_clock(clock.clone())
        .with_clipboard(Arc::new(clipvault_core::FakeClipboard::new()))
        .with_platform_adapters(adapters)
        .bootstrap_at(&db_path)
        .expect("first bootstrap");
    let reader = LocalSettingsReader::new(ctx1.clone(), clock.clone());
    assert_eq!(reader.retention_policy(), clipvault_core::DEFAULT_RETENTION);

    // Persist a different policy and drop the context so the next
    // bootstrap actually has to reopen the file.
    set_retention(&ctx1, "days_90", recent);
    drop(ctx1);

    // Second bootstrap must NOT touch the existing key.
    let adapters = clipvault_core::build_isolated_adapters(dir.path(), &dir.path().join("data"));
    let ctx2 = AppBootstrap::new()
        .with_clock(clock.clone())
        .with_clipboard(Arc::new(clipvault_core::FakeClipboard::new()))
        .with_platform_adapters(adapters)
        .bootstrap_at(&db_path)
        .expect("second bootstrap");
    let reader = LocalSettingsReader::new(ctx2, clock);
    assert_eq!(
        reader.retention_policy(),
        RetentionPolicy::Days90,
        "bootstrap must not overwrite an existing user choice"
    );
}

// ---------------------------------------------------------------------------
// Bug 4 regression: preview must read the same policy as apply, must count
// only non-favorites, and must never mutate the database.
// ---------------------------------------------------------------------------
#[test]
fn preview_count_matches_apply_for_old_entries_and_favorites() {
    let recent = datetime!(2026-02-01 00:00:00 UTC);
    let clock: Arc<dyn Clock> = Arc::new(FixedClock { instant: recent });
    let (_dir, context) = bootstrap_with_clock(clock.clone());
    set_retention(&context, "days_7", recent);

    let ancient = datetime!(2025-01-01 00:00:00 UTC);
    let pinned_old = insert_entry(&context, "favorite-ancient", ancient);
    insert_entry(&context, "non-favorite-ancient", ancient);
    insert_entry(&context, "fresh-1", recent);
    insert_entry(&context, "fresh-2", recent);

    let service = HistoryManagementService::new(clock.clone());
    service
        .set_favorite(&context, pinned_old, true)
        .expect("favorite")
        .entry
        .expect("entry");

    let reader = LocalSettingsReader::new(context.clone(), clock.clone());
    let preview = service
        .preview_retention(&context, &reader)
        .expect("preview");
    assert_eq!(preview.policy, RetentionPolicy::Days7);
    assert_eq!(
        preview.would_remove, 1,
        "preview must count only non-favorites older than the cutoff"
    );

    let before = EntryRepository::new(context.database().lock().connection_mut())
        .count()
        .unwrap();
    let outcome = service.apply_retention(&context, &reader).expect("apply");
    assert_eq!(outcome.removed, 1, "apply must purge the same set");
    assert_eq!(
        outcome.policy, preview.policy,
        "apply must read the same policy as preview"
    );
    let after = EntryRepository::new(context.database().lock().connection_mut())
        .count()
        .unwrap();
    assert_eq!(before, 4);
    assert_eq!(after, 3);
    // The favorite survives both passes.
    let pinned_row = EntryRepository::new(context.database().lock().connection_mut())
        .find_by_id(pinned_old)
        .unwrap()
        .expect("present");
    assert!(pinned_row.is_pinned);
}

#[test]
fn preview_with_forever_reports_zero_remove() {
    let recent = datetime!(2026-02-01 00:00:00 UTC);
    let clock: Arc<dyn Clock> = Arc::new(FixedClock { instant: recent });
    let (_dir, context) = bootstrap_with_clock(clock.clone());
    set_retention(&context, "forever", recent);
    insert_entry(&context, "ancient", datetime!(2020-01-01 00:00:00 UTC));

    let service = HistoryManagementService::new(clock.clone());
    let preview = service
        .preview_retention(&context, &LocalSettingsReader::new(context.clone(), clock))
        .expect("preview");
    assert_eq!(preview.policy, RetentionPolicy::Forever);
    assert_eq!(preview.would_remove, 0);
    assert_eq!(
        EntryRepository::new(context.database().lock().connection_mut())
            .count()
            .unwrap(),
        1,
        "preview must not delete rows even with a finite-looking payload"
    );
}
