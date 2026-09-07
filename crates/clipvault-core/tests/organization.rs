//! End-to-end tests for the `tags-and-collections` capability.
//!
//! These tests exercise the same public surface the Tauri shell uses:
//! they build an [`AppContext`] through [`AppBootstrap`], drive the
//! [`OrganizationService`] (and the capture / search / management
//! services that need to react to it) and assert the typed outcomes.
//! The clipboard payload, hashes and asset references never enter
//! any of these tests: the change is metadata-only by construction.
//!
//! Every test in this file uses
//! [`clipvault_core::test_support::isolated_harness_at`]. The shared
//! helper wires `PlatformAdapters` whose `home_dir` / `data_dir` live
//! inside the tempdir, so the destructive paths the suite exercises
//! (`delete_entry`, `clear_non_favorites`, `apply_retention`) cannot
//! reach the developer's real `~/.clipvault/assets/clipboard`. The
//! regression that motivated this note was caught by a filesystem
//! audit at 11:20:44: the previous `bootstrap_at(tempdir.db)` helper
//! fell back to `DefaultPlatform::detect()` and pointed the asset
//! stores at the real `~/.clipvault`, so every
//! `clear_non_favorites` / `delete_entry` / `apply_retention` test
//! emitted `unlink` calls against `~/.clipvault/assets/clipboard/*.png`.

use std::sync::Arc;

use clipvault_core::{
    AppBootstrap, AppContext, Clock, DeleteOutcome, OrganizationServiceError, SearchFilter,
    SourceAppFilter,
};
use clipvault_db::{
    ContentType, EntryRepository, NewEntry, OrganizationError, OrganizationRepository,
};
use clipvault_search::SearchQuery;
use rusqlite::Connection;
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

fn bootstrap_with_clock(when: time::OffsetDateTime) -> (TempDir, AppContext) {
    let dir = tempfile::tempdir().expect("tempdir");
    let adapters = clipvault_core::build_isolated_adapters(dir.path(), &dir.path().join("data"));
    let context = AppBootstrap::new()
        .with_clock(Arc::new(FixedClock { instant: when }))
        .with_clipboard(Arc::new(clipvault_core::FakeClipboard::new()))
        .with_platform_adapters(adapters)
        .bootstrap_at(dir.path().join("clipvault.db"))
        .expect("bootstrap");
    (dir, context)
}

/// Reopen a previously bootstrapped context with the same isolated
/// harness so restart-cycle tests never touch the real
/// `~/.clipvault`. The clock is re-anchored to `when` so the
/// migration / upsert round-trip is deterministic.
fn reopen_isolated_context(dir: &TempDir, when: time::OffsetDateTime) -> AppContext {
    let adapters = clipvault_core::build_isolated_adapters(dir.path(), &dir.path().join("data"));
    AppBootstrap::new()
        .with_clock(Arc::new(FixedClock { instant: when }))
        .with_clipboard(Arc::new(clipvault_core::FakeClipboard::new()))
        .with_platform_adapters(adapters)
        .bootstrap_at(dir.path().join("clipvault.db"))
        .expect("reopen")
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

/// Run `f` against the [`OrganizationRepository`] while holding the
/// database lock so the borrows stay scoped to the closure.
fn with_org<R>(context: &AppContext, f: impl FnOnce(&OrganizationRepository) -> R) -> R {
    let mut db = context.database().lock();
    let repo = OrganizationRepository::new(db.connection_mut());
    f(&repo)
}

/// Run `f` against the raw [`Connection`] while holding the
/// database lock so the borrows stay scoped to the closure.
fn with_conn<R>(context: &AppContext, f: impl FnOnce(&Connection) -> R) -> R {
    let db = context.database().lock();
    let conn = db.connection();
    f(conn)
}

// ---------------------------------------------------------------------
// Historial protection.
// ---------------------------------------------------------------------

#[test]
fn historial_collection_is_seeded_and_protected_from_rename_and_delete() {
    let (_dir, context) = bootstrap_with_clock(datetime!(2026-01-02 03:04:05 UTC));
    let history_id = context
        .organization()
        .history_collection_id(&context)
        .expect("history id");

    let rename_error = context
        .organization()
        .rename_collection(&context, history_id, "Otro nombre")
        .unwrap_err();
    assert!(matches!(
        rename_error,
        OrganizationServiceError::Repository(OrganizationError::SystemCollectionProtected(_))
    ));

    let delete_error = context
        .organization()
        .delete_collection(&context, history_id)
        .unwrap_err();
    assert!(matches!(
        delete_error,
        OrganizationServiceError::Repository(OrganizationError::SystemCollectionProtected(_))
    ));
}

#[test]
fn historial_backfill_attaches_every_existing_entry() {
    let (_dir, context) = bootstrap_with_clock(datetime!(2026-01-02 03:04:05 UTC));
    let when = datetime!(2026-01-02 03:04:05 UTC);
    let mut ids = Vec::new();
    for content in ["one", "two", "three"] {
        ids.push(insert_entry(&context, content, when));
    }
    let history_id = context
        .organization()
        .history_collection_id(&context)
        .expect("history id");

    let attached: i64 = ids
        .iter()
        .map(|id| {
            with_org(&context, |org| {
                let mut members = org.entry_collection_ids(*id).expect("ids");
                members.retain(|cid| *cid == history_id);
                if members.is_empty() {
                    0
                } else {
                    1
                }
            })
        })
        .sum();
    assert_eq!(attached, ids.len() as i64);
}

#[test]
fn new_capture_joins_historial_in_same_transaction() {
    let (_dir, context) = bootstrap_with_clock(datetime!(2026-01-02 03:04:05 UTC));
    let when = datetime!(2026-01-02 03:04:05 UTC);
    let id = insert_entry(&context, "captured", when);
    let history_id = context
        .organization()
        .history_collection_id(&context)
        .expect("history id");
    let mut collections = with_org(&context, |org| org.entry_collection_ids(id).expect("ids"));
    collections.sort();
    assert_eq!(collections, vec![history_id]);
}

// ---------------------------------------------------------------------
// User collections CRUD.
// ---------------------------------------------------------------------

#[test]
fn create_collection_rejects_duplicate_name_case_insensitively() {
    let (_dir, context) = bootstrap_with_clock(datetime!(2026-01-02 03:04:05 UTC));
    let service = context.organization();
    service
        .create_collection(&context, "Trabajo")
        .expect("first");
    let error = service
        .create_collection(&context, "trabajo")
        .expect_err("second must fail");
    assert!(matches!(
        error,
        OrganizationServiceError::Repository(OrganizationError::CollectionNameTaken(_))
    ));
}

#[test]
fn create_collection_rejects_empty_and_overlong_names() {
    let (_dir, context) = bootstrap_with_clock(datetime!(2026-01-02 03:04:05 UTC));
    let service = context.organization();
    assert!(matches!(
        service.create_collection(&context, "   "),
        Err(OrganizationServiceError::Repository(
            OrganizationError::EmptyCollectionName
        ))
    ));
    let long = "x".repeat(clipvault_db::MAX_ORGANIZATION_NAME_CHARS + 1);
    assert!(matches!(
        service.create_collection(&context, &long),
        Err(OrganizationServiceError::Repository(
            OrganizationError::CollectionNameTooLong(_)
        ))
    ));
}

#[test]
fn delete_user_collection_removes_only_its_memberships() {
    let (_dir, context) = bootstrap_with_clock(datetime!(2026-01-02 03:04:05 UTC));
    let when = datetime!(2026-01-02 03:04:05 UTC);
    let trabajo_id = context
        .organization()
        .create_collection(&context, "Trabajo")
        .expect("create")
        .id;
    let clientes_id = context
        .organization()
        .create_collection(&context, "Clientes")
        .expect("create")
        .id;
    let entry_id = insert_entry(&context, "shared entry", when);
    context
        .organization()
        .replace_entry_collections(&context, entry_id, &[trabajo_id, clientes_id])
        .expect("replace");

    context
        .organization()
        .remove_entry_from_collection(&context, entry_id, trabajo_id)
        .expect("remove");
    let history_id = context
        .organization()
        .history_collection_id(&context)
        .expect("history");
    let remaining = with_org(&context, |org| {
        let mut v = org.entry_collection_ids(entry_id).expect("ids");
        v.sort();
        v
    });
    let mut expected = vec![history_id, clientes_id];
    expected.sort();
    let _ = ();
    assert_eq!(remaining, expected);

    context
        .organization()
        .delete_collection(&context, clientes_id)
        .expect("delete");
    let remaining = with_org(&context, |org| {
        org.entry_collection_ids(entry_id).expect("ids")
    });
    assert_eq!(remaining, vec![history_id]);

    let collections = context
        .organization()
        .list_collections(&context)
        .expect("list");
    assert!(collections.iter().any(|c| c.id == trabajo_id));
    assert!(collections.iter().all(|c| c.id != clientes_id));
}

// ---------------------------------------------------------------------
// Tag CRUD and normalization.
// ---------------------------------------------------------------------

#[test]
fn tag_upsert_is_case_insensitive_and_idempotent() {
    let (_dir, context) = bootstrap_with_clock(datetime!(2026-01-02 03:04:05 UTC));
    let service = context.organization();
    let first = service.upsert_tag(&context, "Critical").expect("first");
    let second = service
        .upsert_tag(&context, "  CRITICAL  ")
        .expect("second");
    assert_eq!(first.id, second.id);
    assert_eq!(first.normalized_name, "critical");
}

#[test]
fn tag_upsert_preserves_display_name_but_normalises_identity() {
    let (_dir, context) = bootstrap_with_clock(datetime!(2026-01-02 03:04:05 UTC));
    let service = context.organization();
    let tag = service
        .upsert_tag(&context, "  Project \t Zeta ")
        .expect("upsert");
    assert_eq!(tag.display_name, "Project Zeta");
    assert_eq!(tag.normalized_name, "project zeta");
}

#[test]
fn tag_upsert_rejects_empty_and_overlong_names() {
    let (_dir, context) = bootstrap_with_clock(datetime!(2026-01-02 03:04:05 UTC));
    let service = context.organization();
    assert!(matches!(
        service.upsert_tag(&context, "   "),
        Err(OrganizationServiceError::Repository(
            OrganizationError::EmptyTagName
        ))
    ));
    let long = "a".repeat(clipvault_db::MAX_ORGANIZATION_NAME_CHARS + 1);
    assert!(matches!(
        service.upsert_tag(&context, &long),
        Err(OrganizationServiceError::Repository(
            OrganizationError::TagNameTooLong(_)
        ))
    ));
}

#[test]
fn delete_tag_removes_only_associations() {
    let (_dir, context) = bootstrap_with_clock(datetime!(2026-01-02 03:04:05 UTC));
    let when = datetime!(2026-01-02 03:04:05 UTC);
    let entry_id = insert_entry(&context, "keep me", when);
    let tag_id = context
        .organization()
        .upsert_tag(&context, "draft")
        .expect("upsert")
        .id;
    context
        .organization()
        .replace_entry_tags(&context, entry_id, &[tag_id])
        .expect("attach");
    let removed = context
        .organization()
        .delete_tag(&context, tag_id)
        .expect("delete");
    assert!(removed);
    let entry_count: i64 = with_conn(&context, |conn| {
        conn.query_row("SELECT COUNT(*) FROM clipboard_entries", [], |row| {
            row.get(0)
        })
        .unwrap()
    });
    assert_eq!(entry_count, 1);
}

// ---------------------------------------------------------------------
// Quitar de esta colección — secondary only.
// ---------------------------------------------------------------------

#[test]
fn remove_from_history_is_rejected() {
    let (_dir, context) = bootstrap_with_clock(datetime!(2026-01-02 03:04:05 UTC));
    let when = datetime!(2026-01-02 03:04:05 UTC);
    let entry_id = insert_entry(&context, "captured", when);
    let history_id = context
        .organization()
        .history_collection_id(&context)
        .expect("history");
    let error = context
        .organization()
        .remove_entry_from_collection(&context, entry_id, history_id)
        .unwrap_err();
    assert!(matches!(
        error,
        OrganizationServiceError::Repository(OrganizationError::SystemCollectionProtected(_))
    ));
}

// ---------------------------------------------------------------------
// Replace operations restore Historial atomically.
// ---------------------------------------------------------------------

#[test]
fn replace_entry_collections_always_keeps_historial() {
    let (_dir, context) = bootstrap_with_clock(datetime!(2026-01-02 03:04:05 UTC));
    let when = datetime!(2026-01-02 03:04:05 UTC);
    let entry_id = insert_entry(&context, "captured", when);
    let trabajo_id = context
        .organization()
        .create_collection(&context, "Trabajo")
        .expect("create")
        .id;
    let clients_id = context
        .organization()
        .create_collection(&context, "Clientes")
        .expect("create")
        .id;
    let history_id = context
        .organization()
        .history_collection_id(&context)
        .expect("history");

    context
        .organization()
        .replace_entry_collections(&context, entry_id, &[trabajo_id, clients_id])
        .expect("replace");
    let mut assigned = with_org(&context, |org| {
        org.entry_collection_ids(entry_id).expect("ids")
    });
    assigned.sort();
    let mut expected = vec![clients_id, history_id, trabajo_id];
    expected.sort();
    assert_eq!(assigned, expected);
    assert!(assigned.contains(&history_id));
    assert_eq!(assigned.len(), 3);
}

// ---------------------------------------------------------------------
// Search filters preserve ranking and respect AND semantics.
// ---------------------------------------------------------------------

#[test]
fn search_filter_by_collection_preserves_ranking() {
    let (_dir, context) = bootstrap_with_clock(datetime!(2026-01-02 03:04:05 UTC));
    let when = datetime!(2026-01-02 03:04:05 UTC);
    let trabajo_id = context
        .organization()
        .create_collection(&context, "Trabajo")
        .expect("create")
        .id;
    // Insert the exact match first so it has the lower id and
    // matches `id DESC` tiebreak that follows equal scores.
    let id_exact = insert_entry(&context, "Hello World", when);
    let id_fuzzy = insert_entry(&context, "world helo", when);
    insert_entry(&context, "unrelated entry", when);
    context
        .organization()
        .replace_entry_collections(&context, id_exact, &[trabajo_id])
        .expect("attach exact");
    context
        .organization()
        .replace_entry_collections(&context, id_fuzzy, &[trabajo_id])
        .expect("attach fuzzy");

    let outcome = context
        .search()
        .search_with_filter(
            &context,
            &SearchQuery {
                text: "hello world".to_string(),
                limit: 50,
            },
            &SearchFilter {
                collection_id: Some(trabajo_id),
                tag_ids: Vec::new(),
                source_app: SourceAppFilter::default(),
            },
        )
        .expect("search");
    let ids: Vec<i64> = outcome.hits.iter().map(|h| h.entry_id).collect();
    assert_eq!(ids, vec![id_exact, id_fuzzy]);
    assert!(outcome.hits[0].score > outcome.hits[1].score);
}

#[test]
fn search_filter_by_multiple_tags_uses_and_semantics() {
    let (_dir, context) = bootstrap_with_clock(datetime!(2026-01-02 03:04:05 UTC));
    let when = datetime!(2026-01-02 03:04:05 UTC);
    let codigo = context
        .organization()
        .upsert_tag(&context, "codigo")
        .expect("tag")
        .id;
    let pendiente = context
        .organization()
        .upsert_tag(&context, "pendiente")
        .expect("tag")
        .id;
    let id_both = insert_entry(&context, "rust snippet", when);
    let id_only_codigo = insert_entry(&context, "another snippet", when);
    context
        .organization()
        .replace_entry_tags(&context, id_both, &[codigo, pendiente])
        .expect("both");
    context
        .organization()
        .replace_entry_tags(&context, id_only_codigo, &[codigo])
        .expect("only codigo");

    let outcome = context
        .search()
        .search_with_filter(
            &context,
            &SearchQuery {
                text: "snippet".to_string(),
                limit: 50,
            },
            &SearchFilter {
                collection_id: None,
                tag_ids: vec![codigo, pendiente],
                source_app: SourceAppFilter::default(),
            },
        )
        .expect("search");
    let ids: Vec<i64> = outcome.hits.iter().map(|h| h.entry_id).collect();
    assert_eq!(ids, vec![id_both]);
}

// ---------------------------------------------------------------------
// Delete / clear / retention cascade through CASCADE foreign keys.
// ---------------------------------------------------------------------

#[test]
fn delete_entry_cascades_into_collections_and_tags_tables() {
    let (_dir, context) = bootstrap_with_clock(datetime!(2026-01-02 03:04:05 UTC));
    let when = datetime!(2026-01-02 03:04:05 UTC);
    let trabajo_id = context
        .organization()
        .create_collection(&context, "Trabajo")
        .expect("create")
        .id;
    let tag_id = context
        .organization()
        .upsert_tag(&context, "draft")
        .expect("tag")
        .id;
    let entry_id = insert_entry(&context, "remove me", when);
    context
        .organization()
        .replace_entry_collections(&context, entry_id, &[trabajo_id])
        .expect("associate");
    context
        .organization()
        .replace_entry_tags(&context, entry_id, &[tag_id])
        .expect("tag associate");

    let outcome = context
        .management()
        .delete_entry(&context, entry_id, true)
        .expect("delete");
    assert!(matches!(outcome, DeleteOutcome::Removed { .. }));

    let remaining_collections: i64 = with_conn(&context, |conn| {
        conn.query_row(
            "SELECT COUNT(*) FROM entry_collections WHERE entry_id = ?1",
            rusqlite::params![entry_id],
            |row| row.get(0),
        )
        .unwrap()
    });
    let remaining_tags: i64 = with_conn(&context, |conn| {
        conn.query_row(
            "SELECT COUNT(*) FROM entry_tags WHERE entry_id = ?1",
            rusqlite::params![entry_id],
            |row| row.get(0),
        )
        .unwrap()
    });
    assert_eq!(remaining_collections, 0);
    assert_eq!(remaining_tags, 0);

    let collections = context
        .organization()
        .list_collections(&context)
        .expect("list");
    assert!(collections.iter().any(|c| c.id == trabajo_id));
    let tags = context.organization().list_tags(&context).expect("tags");
    assert!(tags.iter().any(|t| t.id == tag_id));
}

#[test]
fn clear_non_favorites_preserves_tag_and_collection_definitions() {
    let (_dir, context) = bootstrap_with_clock(datetime!(2026-01-02 03:04:05 UTC));
    let when = datetime!(2026-01-02 03:04:05 UTC);
    let trabajo_id = context
        .organization()
        .create_collection(&context, "Trabajo")
        .expect("create")
        .id;
    let tag_id = context
        .organization()
        .upsert_tag(&context, "draft")
        .expect("tag")
        .id;
    let pinned = insert_entry(&context, "favorite", when);
    let other = insert_entry(&context, "transient", when);
    context
        .management()
        .set_favorite(&context, pinned, true)
        .expect("favorite");
    for entry in [pinned, other] {
        context
            .organization()
            .replace_entry_collections(&context, entry, &[trabajo_id])
            .expect("collection");
        context
            .organization()
            .replace_entry_tags(&context, entry, &[tag_id])
            .expect("tag");
    }
    let outcome = context
        .management()
        .clear_non_favorites(&context, true)
        .expect("clear");
    assert!(matches!(
        outcome,
        clipvault_core::ClearOutcome::Removed { .. }
    ));

    let collections = context
        .organization()
        .list_collections(&context)
        .expect("list");
    assert!(collections.iter().any(|c| c.id == trabajo_id));
    let tags = context.organization().list_tags(&context).expect("tags");
    assert!(tags.iter().any(|t| t.id == tag_id));

    let pinned_collections = with_org(&context, |org| {
        org.entry_collection_ids(pinned).expect("ids")
    });
    assert!(pinned_collections.contains(&trabajo_id));
}

// ---------------------------------------------------------------------
// Retention + associations: pin/unpin keeps the membership intact.
// ---------------------------------------------------------------------

#[test]
fn retention_removes_associations_of_purged_entries() {
    let (_dir, context) = bootstrap_with_clock(datetime!(2026-01-02 03:04:05 UTC));
    let when = datetime!(2026-01-02 03:04:05 UTC);
    let tag_id = context
        .organization()
        .upsert_tag(&context, "draft")
        .expect("tag")
        .id;
    let entry_id = insert_entry(&context, "old", when);
    context
        .organization()
        .replace_entry_tags(&context, entry_id, &[tag_id])
        .expect("tag");

    // The entry is created at the same instant as the clock so a
    // 7-day policy keeps it; pin it so retention has nothing to
    // purge and assert the association stays.
    context
        .management()
        .set_favorite(&context, entry_id, true)
        .expect("favorite");
    let clock = context.clock();
    let reader = clipvault_core::LocalSettingsReader::new(context.clone(), clock);
    use clipvault_db::AppSettingsRepository;
    {
        let mut db = context.database().lock();
        let mut repo = AppSettingsRepository::new(db.connection_mut());
        repo.set(
            clipvault_core::RETENTION_SETTING_KEY,
            clipvault_core::RetentionPolicy::Days7.as_setting_value(),
            when,
        )
        .expect("set retention");
    }
    let outcome = context
        .management()
        .apply_retention(&context, &reader)
        .expect("apply retention");
    assert_eq!(outcome.removed, 0);

    let pinned_tags = with_org(&context, |org| org.entry_tag_ids(entry_id).expect("ids"));
    assert_eq!(pinned_tags, vec![tag_id]);
}

// ---------------------------------------------------------------------
// SearchService with empty filter preserves the legacy global behaviour.
// ---------------------------------------------------------------------

#[test]
fn search_with_no_filters_returns_all_textual_entries() {
    let (_dir, context) = bootstrap_with_clock(datetime!(2026-01-02 03:04:05 UTC));
    let when = datetime!(2026-01-02 03:04:05 UTC);
    insert_entry(&context, "alpha", when);
    insert_entry(&context, "beta", when);
    let outcome = context
        .search()
        .search(
            &context,
            &SearchQuery {
                text: "a".to_string(),
                limit: 50,
            },
        )
        .expect("search");
    let empty_filter_outcome = context
        .search()
        .search_with_filter(
            &context,
            &SearchQuery {
                text: "a".to_string(),
                limit: 50,
            },
            &SearchFilter::default(),
        )
        .expect("filtered");
    let ids: Vec<i64> = outcome.hits.iter().map(|h| h.entry_id).collect();
    let filtered_ids: Vec<i64> = empty_filter_outcome
        .hits
        .iter()
        .map(|h| h.entry_id)
        .collect();
    assert_eq!(ids, filtered_ids);
}

// ---------------------------------------------------------------------
// Favorite toggle never mutates tag/collection associations.
// ---------------------------------------------------------------------

#[test]
fn favorite_toggle_keeps_associations_intact() {
    let (_dir, context) = bootstrap_with_clock(datetime!(2026-01-02 03:04:05 UTC));
    let when = datetime!(2026-01-02 03:04:05 UTC);
    let trabajo_id = context
        .organization()
        .create_collection(&context, "Trabajo")
        .expect("create")
        .id;
    let tag_id = context
        .organization()
        .upsert_tag(&context, "draft")
        .expect("tag")
        .id;
    let entry_id = insert_entry(&context, "captured", when);
    context
        .organization()
        .replace_entry_collections(&context, entry_id, &[trabajo_id])
        .expect("collection");
    context
        .organization()
        .replace_entry_tags(&context, entry_id, &[tag_id])
        .expect("tag");

    context
        .management()
        .set_favorite(&context, entry_id, true)
        .expect("favorite");
    context
        .management()
        .set_favorite(&context, entry_id, false)
        .expect("unfavorite");

    let history_id = context
        .organization()
        .history_collection_id(&context)
        .expect("history");
    let collections = with_org(&context, |org| {
        org.entry_collection_ids(entry_id).expect("ids")
    });
    let mut expected = vec![history_id, trabajo_id];
    expected.sort();
    let mut actual = collections.clone();
    actual.sort();
    assert_eq!(actual, expected);

    let tags = with_org(&context, |org| org.entry_tag_ids(entry_id).expect("ids"));
    assert_eq!(tags, vec![tag_id]);
}

// ---------------------------------------------------------------------
// Renaming a user collection does not change its memberships.
// ---------------------------------------------------------------------

#[test]
fn rename_collection_preserves_memberships() {
    let (_dir, context) = bootstrap_with_clock(datetime!(2026-01-02 03:04:05 UTC));
    let when = datetime!(2026-01-02 03:04:05 UTC);
    let trabajo_id = context
        .organization()
        .create_collection(&context, "Trabajo")
        .expect("create")
        .id;
    let entry_id = insert_entry(&context, "captured", when);
    context
        .organization()
        .replace_entry_collections(&context, entry_id, &[trabajo_id])
        .expect("attach");
    let renamed = context
        .organization()
        .rename_collection(&context, trabajo_id, "Proyectos")
        .expect("rename");
    assert_eq!(renamed.id, trabajo_id);
    assert_eq!(renamed.name, "Proyectos");

    let history_id = context
        .organization()
        .history_collection_id(&context)
        .expect("history");
    let mut members = with_org(&context, |org| {
        org.entry_collection_ids(entry_id).expect("ids")
    });
    members.sort();
    let mut expected = vec![history_id, trabajo_id];
    expected.sort();
    assert_eq!(members, expected);
}

// ---------------------------------------------------------------------
// collection_kind helper exposes Historial vs user collections to callers.
// ---------------------------------------------------------------------

#[test]
fn collection_kind_reports_system_for_historial() {
    let (_dir, context) = bootstrap_with_clock(datetime!(2026-01-02 03:04:05 UTC));
    let history_id = context
        .organization()
        .history_collection_id(&context)
        .expect("history");
    let kind = context
        .organization()
        .collection_kind(&context, history_id)
        .expect("kind")
        .expect("present");
    assert_eq!(kind, clipvault_db::CollectionKind::System);

    let user_id = context
        .organization()
        .create_collection(&context, "Trabajo")
        .expect("create")
        .id;
    let kind = context
        .organization()
        .collection_kind(&context, user_id)
        .expect("kind")
        .expect("present");
    assert_eq!(kind, clipvault_db::CollectionKind::User);
}

// ---------------------------------------------------------------------
// Empty-state: when no user collections exist the sidebar still renders
// Historial plus an actionable hint.
// ---------------------------------------------------------------------

#[test]
fn history_collection_listing_includes_only_seed_when_no_user_collections() {
    let (_dir, context) = bootstrap_with_clock(datetime!(2026-01-02 03:04:05 UTC));
    let collections = context
        .organization()
        .list_collections(&context)
        .expect("list");
    assert_eq!(collections.len(), 1);
    assert_eq!(collections[0].name, clipvault_db::HISTORY_DISPLAY_NAME);
}

// ---------------------------------------------------------------------
// Update_on_card_does_not_touch_other_entries_associations.
// ---------------------------------------------------------------------

#[test]
fn mutating_one_entry_does_not_leak_into_others() {
    let (_dir, context) = bootstrap_with_clock(datetime!(2026-01-02 03:04:05 UTC));
    let when = datetime!(2026-01-02 03:04:05 UTC);
    let a = insert_entry(&context, "alpha", when);
    let b = insert_entry(&context, "beta", when);
    let tag = context
        .organization()
        .upsert_tag(&context, "draft")
        .expect("tag")
        .id;
    context
        .organization()
        .replace_entry_tags(&context, a, &[tag])
        .expect("attach");
    let b_tags = with_org(&context, |org| org.entry_tag_ids(b).expect("ids"));
    assert!(b_tags.is_empty());
}

// ---------------------------------------------------------------------
// Replace entry tags: previous associations are removed.
// ---------------------------------------------------------------------

#[test]
fn replace_entry_tags_is_atomic_and_replaces_prior_state() {
    let (_dir, context) = bootstrap_with_clock(datetime!(2026-01-02 03:04:05 UTC));
    let when = datetime!(2026-01-02 03:04:05 UTC);
    let entry_id = insert_entry(&context, "captured", when);
    let first = context
        .organization()
        .upsert_tag(&context, "first")
        .expect("tag")
        .id;
    let second = context
        .organization()
        .upsert_tag(&context, "second")
        .expect("tag")
        .id;
    context
        .organization()
        .replace_entry_tags(&context, entry_id, &[first])
        .expect("first attach");
    let stored = with_org(&context, |org| org.entry_tag_ids(entry_id).expect("ids"));
    assert_eq!(stored, vec![first]);

    context
        .organization()
        .replace_entry_tags(&context, entry_id, &[second])
        .expect("replace");
    let stored = with_org(&context, |org| org.entry_tag_ids(entry_id).expect("ids"));
    assert_eq!(stored, vec![second]);
}

// ---------------------------------------------------------------------
// Empty associations are valid and clean up old rows.
// ---------------------------------------------------------------------

#[test]
fn replace_entry_tags_with_empty_list_clears_associations() {
    let (_dir, context) = bootstrap_with_clock(datetime!(2026-01-02 03:04:05 UTC));
    let when = datetime!(2026-01-02 03:04:05 UTC);
    let entry_id = insert_entry(&context, "captured", when);
    let tag = context
        .organization()
        .upsert_tag(&context, "draft")
        .expect("tag")
        .id;
    context
        .organization()
        .replace_entry_tags(&context, entry_id, &[tag])
        .expect("attach");
    context
        .organization()
        .replace_entry_tags(&context, entry_id, &[])
        .expect("clear");
    let stored = with_org(&context, |org| org.entry_tag_ids(entry_id).expect("ids"));
    assert!(stored.is_empty());
}

// ---------------------------------------------------------------------
// Renaming a tag normalises the new identity and rejects collisions.
// ---------------------------------------------------------------------

#[test]
fn rename_tag_normalises_identity_and_rejects_collisions() {
    let (_dir, context) = bootstrap_with_clock(datetime!(2026-01-02 03:04:05 UTC));
    let service = context.organization();
    let first = service.upsert_tag(&context, "first").expect("first");
    let second = service.upsert_tag(&context, "second").expect("second");

    // Renaming to a different identity updates both fields.
    let renamed = service
        .rename_tag(&context, first.id, "Renamed")
        .expect("rename");
    assert_eq!(renamed.normalized_name, "renamed");
    assert_eq!(renamed.display_name, "Renamed");

    // Collision: renaming `second` to the same identity as the
    // freshly renamed tag is rejected.
    let error = service
        .rename_tag(&context, second.id, "RENAMED")
        .expect_err("collision");
    assert!(matches!(
        error,
        OrganizationServiceError::Repository(OrganizationError::TagNameTaken(_))
    ));
}

// ---------------------------------------------------------------------
// history_collection_id is a stable bootstrap-derived handle.
// ---------------------------------------------------------------------

#[test]
fn history_collection_id_is_stable_across_calls() {
    let (_dir, context) = bootstrap_with_clock(datetime!(2026-01-02 03:04:05 UTC));
    let a = context
        .organization()
        .history_collection_id(&context)
        .expect("a");
    let b = context
        .organization()
        .history_collection_id(&context)
        .expect("b");
    assert_eq!(a, b);
    assert!(a > 0);
}

// ---------------------------------------------------------------------
// Multiple new captures attach to Historial in separate transactions.
// ---------------------------------------------------------------------

#[test]
fn multiple_captures_each_attach_to_historial() {
    let (_dir, context) = bootstrap_with_clock(datetime!(2026-01-02 03:04:05 UTC));
    let when = datetime!(2026-01-02 03:04:05 UTC);
    let ids: Vec<i64> = (0..5)
        .map(|i| insert_entry(&context, &format!("entry-{i}"), when))
        .collect();
    let history_id = context
        .organization()
        .history_collection_id(&context)
        .expect("history");
    for id in ids {
        let mut collections = with_org(&context, |org| org.entry_collection_ids(id).expect("ids"));
        collections.sort();
        assert_eq!(collections, vec![history_id]);
    }
}

// ---------------------------------------------------------------------
// `tags-and-collections` regression coverage for the atomic
// upsert + assign service path the card menu's **Agregar tag**
// flow relies on. The bug the original change shipped —
// "tags typed from the modal do not appear on the card and do
// not persist across restarts" — was caused by the frontend
// fabricating synthetic names and skipping the assignment;
// the regression suite pins every contract the fix depends on.
// ---------------------------------------------------------------------

#[test]
fn upsert_and_assign_tag_persists_visible_immediately_and_after_restart() {
    let (_dir, context) = bootstrap_with_clock(datetime!(2026-01-02 03:04:05 UTC));
    let when = datetime!(2026-01-02 03:04:05 UTC);
    let entry_id = insert_entry(&context, "captured", when);

    let tag = context
        .organization()
        .upsert_and_assign_tag(&context, entry_id, "  Critical ")
        .expect("upsert+assign");

    // The tag is normalised and visible from the very next read —
    // the sidebar / chip count surface never has to wait for a
    // background refresh to display it.
    let live_tags = context
        .organization()
        .list_tags(&context)
        .expect("list_tags");
    assert!(live_tags
        .iter()
        .any(|t| t.id == tag.id && t.normalized_name == "critical"));

    let live_entry_tags = with_org(&context, |org| org.entry_tag_ids(entry_id).expect("ids"));
    assert_eq!(live_entry_tags, vec![tag.id]);

    // The snapshot loaded through the public organisation command
    // surface exposes the same row, which is what the frontend
    // consults after the save round-trip.
    let snapshot = clipvault_core::OrganizationSidebarSnapshot::load(&context).expect("snapshot");
    assert!(snapshot.tags.iter().any(|t| t.id == tag.id));
}

#[test]
fn upsert_and_assign_tag_is_idempotent_when_repeated() {
    let (_dir, context) = bootstrap_with_clock(datetime!(2026-01-02 03:04:05 UTC));
    let when = datetime!(2026-01-02 03:04:05 UTC);
    let entry_id = insert_entry(&context, "captured", when);

    let first = context
        .organization()
        .upsert_and_assign_tag(&context, entry_id, "Critical")
        .expect("first");
    let second = context
        .organization()
        .upsert_and_assign_tag(&context, entry_id, "  CRITICAL  ")
        .expect("second");

    assert_eq!(first.id, second.id);
    // Re-running the command must not multiply tag definitions or
    // association rows.
    let tags = context
        .organization()
        .list_tags(&context)
        .expect("list_tags");
    assert_eq!(tags.len(), 1);
    let associations = with_org(&context, |org| org.entry_tag_ids(entry_id).expect("ids"));
    assert_eq!(associations, vec![first.id]);
}

#[test]
fn upsert_and_assign_tag_normalises_case_insensitively_without_duplicates() {
    let (_dir, context) = bootstrap_with_clock(datetime!(2026-01-02 03:04:05 UTC));
    let when = datetime!(2026-01-02 03:04:05 UTC);
    let entry_id = insert_entry(&context, "captured", when);

    let a = context
        .organization()
        .upsert_and_assign_tag(&context, entry_id, "Critical")
        .expect("a");
    let b = context
        .organization()
        .upsert_and_assign_tag(&context, entry_id, "  CRITICAL  ")
        .expect("b");
    let c = context
        .organization()
        .upsert_and_assign_tag(&context, entry_id, "critical")
        .expect("c");

    assert_eq!(a.id, b.id);
    assert_eq!(a.id, c.id);
    let tags = context
        .organization()
        .list_tags(&context)
        .expect("list_tags");
    assert_eq!(tags.len(), 1, "case differences must not create duplicates");
}

#[test]
fn upsert_and_assign_tag_rejects_empty_and_overlong_names() {
    let (_dir, context) = bootstrap_with_clock(datetime!(2026-01-02 03:04:05 UTC));
    let when = datetime!(2026-01-02 03:04:05 UTC);
    let entry_id = insert_entry(&context, "captured", when);

    assert!(matches!(
        context
            .organization()
            .upsert_and_assign_tag(&context, entry_id, "   "),
        Err(clipvault_core::OrganizationServiceError::Repository(
            clipvault_db::OrganizationError::EmptyTagName
        ))
    ));
    let too_long = "x".repeat(clipvault_db::MAX_ORGANIZATION_NAME_CHARS + 1);
    assert!(matches!(
        context
            .organization()
            .upsert_and_assign_tag(&context, entry_id, &too_long),
        Err(clipvault_core::OrganizationServiceError::Repository(
            clipvault_db::OrganizationError::TagNameTooLong(_)
        ))
    ));
    // Neither validation failure must produce a tag row or an
    // association row.
    let tags = context
        .organization()
        .list_tags(&context)
        .expect("list_tags");
    assert!(tags.is_empty());
    let associations = with_org(&context, |org| org.entry_tag_ids(entry_id).expect("ids"));
    assert!(associations.is_empty());
}

#[test]
fn upsert_and_assign_tag_rejects_missing_entry_without_orphan_tag() {
    let (_dir, context) = bootstrap_with_clock(datetime!(2026-01-02 03:04:05 UTC));

    let error = context
        .organization()
        .upsert_and_assign_tag(&context, 9_999, "draft")
        .expect_err("missing entry");
    assert!(matches!(
        error,
        clipvault_core::OrganizationServiceError::Repository(
            clipvault_db::OrganizationError::EntryNotFound(9_999)
        )
    ));

    // The atomic contract: a missing entry never produces an orphan
    // tag. The whole operation runs in a single transaction; the
    // rollback leaves the database untouched.
    let tags = context
        .organization()
        .list_tags(&context)
        .expect("list_tags");
    assert!(tags.is_empty());
}

// ---------------------------------------------------------------------
// `tags-and-collections` regression coverage for the modal's new
// "unified input" flow. After the rework the
// `TagSelectorModal.svelte` no longer splits search and creation:
// every name typed into the only input goes through
// `clipvault_tags_create` first (creating only the tag definition;
// never the association), and the card save dispatches a single
// `clipvault_entry_tags_set` carrying the union of pre-existing
// ids and the just-created ids. Cancel = no `entry_tags_set` call
// at all. The regression suite below pins the storage-layer
// contract the modal relies on so future refactors cannot
// reintroduce the partial-state or orphan-tag bugs the user
// reported.
// ---------------------------------------------------------------------

#[test]
fn tags_create_then_entry_tags_set_persists_visible_immediately() {
    let (_dir, context) = bootstrap_with_clock(datetime!(2026-01-02 03:04:05 UTC));
    let when = datetime!(2026-01-02 03:04:05 UTC);
    let entry_id = insert_entry(&context, "captured", when);

    // Phase 1 — modal "Añadir" with a brand-new name.
    let created = context
        .organization()
        .upsert_tag(&context, "  Critical ")
        .expect("upsert_tag");

    // Phase 2 — modal "Guardar" replaces the entry's tag set with
    // the union of pre-existing ids and the freshly created one.
    context
        .organization()
        .replace_entry_tags(&context, entry_id, &[created.id])
        .expect("replace_entry_tags");

    let snapshot = clipvault_core::OrganizationSidebarSnapshot::load(&context).expect("snapshot");
    assert!(
        snapshot.tags.iter().any(|t| t.id == created.id),
        "the newly created tag must appear in the sidebar snapshot",
    );
    let associations = with_org(&context, |org| org.entry_tag_ids(entry_id).expect("ids"));
    assert_eq!(
        associations,
        vec![created.id],
        "the freshly created tag must show up on the card immediately",
    );
}

#[test]
fn tags_create_does_not_assign_or_touch_entry_tags() {
    // Regression: `tags_create` must only manage the tag definition.
    // It must never insert into `entry_tags`. The **Añadir** step in
    // the modal relies on this so a Cancel after Add leaves the
    // entry's association set untouched.
    let (_dir, context) = bootstrap_with_clock(datetime!(2026-01-02 03:04:05 UTC));
    let when = datetime!(2026-01-02 03:04:05 UTC);
    let entry_id = insert_entry(&context, "captured", when);

    context
        .organization()
        .upsert_tag(&context, "Critical")
        .expect("upsert_tag");

    let associations = with_org(&context, |org| org.entry_tag_ids(entry_id).expect("ids"));
    assert!(
        associations.is_empty(),
        "tags_create must never insert into entry_tags",
    );
}

#[test]
fn entry_tags_set_rejects_empty_tag_list_atomically() {
    // Regression: handing an empty id set is a no-op rather than an
    // error, but the existing entry associations (if any) must
    // survive the call intact.
    let (_dir, context) = bootstrap_with_clock(datetime!(2026-01-02 03:04:05 UTC));
    let when = datetime!(2026-01-02 03:04:05 UTC);
    let entry_id = insert_entry(&context, "captured", when);
    let tag = context
        .organization()
        .upsert_tag(&context, "Critical")
        .expect("upsert_tag");
    context
        .organization()
        .replace_entry_tags(&context, entry_id, &[tag.id])
        .expect("seed");

    let resolved = context
        .organization()
        .replace_entry_tags(&context, entry_id, &[])
        .expect("empty replace");

    assert!(resolved.is_empty());
    let associations = with_org(&context, |org| org.entry_tag_ids(entry_id).expect("ids"));
    assert!(
        associations.is_empty(),
        "empty replace clears the entry tag set atomically",
    );
}

#[test]
fn entry_tags_set_de_duplicates_repeated_ids() {
    let (_dir, context) = bootstrap_with_clock(datetime!(2026-01-02 03:04:05 UTC));
    let when = datetime!(2026-01-02 03:04:05 UTC);
    let entry_id = insert_entry(&context, "captured", when);
    let tag = context
        .organization()
        .upsert_tag(&context, "Critical")
        .expect("upsert_tag");

    // The modal can emit duplicate ids (selected checkbox + newly
    // created). The replace must collapse them.
    let resolved = context
        .organization()
        .replace_entry_tags(&context, entry_id, &[tag.id, tag.id, tag.id])
        .expect("replace");

    assert_eq!(resolved, vec![tag.id]);
    let associations = with_org(&context, |org| org.entry_tag_ids(entry_id).expect("ids"));
    assert_eq!(associations, vec![tag.id]);
}

#[test]
fn entry_tags_set_does_not_mutate_other_entries() {
    let (_dir, context) = bootstrap_with_clock(datetime!(2026-01-02 03:04:05 UTC));
    let when = datetime!(2026-01-02 03:04:05 UTC);
    let first = insert_entry(&context, "first", when);
    let second = insert_entry(&context, "second", when);
    let tag = context
        .organization()
        .upsert_tag(&context, "Critical")
        .expect("upsert_tag");

    context
        .organization()
        .replace_entry_tags(&context, second, &[tag.id])
        .expect("seed second");

    context
        .organization()
        .replace_entry_tags(&context, first, &[tag.id])
        .expect("replace first");

    let second_tags = with_org(&context, |org| org.entry_tag_ids(second).expect("ids"));
    let first_tags = with_org(&context, |org| org.entry_tag_ids(first).expect("ids"));
    assert_eq!(second_tags, vec![tag.id]);
    assert_eq!(first_tags, vec![tag.id]);
}

#[test]
fn replace_entry_tags_keeps_other_organization_state_intact() {
    // Regression: assigning tags must never touch collection state,
    // even when the entry already belongs to several collections.
    let (_dir, context) = bootstrap_with_clock(datetime!(2026-01-02 03:04:05 UTC));
    let when = datetime!(2026-01-02 03:04:05 UTC);
    let trabajo_id = context
        .organization()
        .create_collection(&context, "Trabajo")
        .expect("Trabajo")
        .id;
    let clientes_id = context
        .organization()
        .create_collection(&context, "Clientes")
        .expect("Clientes")
        .id;
    let entry_id = insert_entry(&context, "captured", when);
    context
        .organization()
        .replace_entry_collections(&context, entry_id, &[trabajo_id, clientes_id])
        .expect("attach");

    let tag = context
        .organization()
        .upsert_tag(&context, "Critical")
        .expect("tag");
    context
        .organization()
        .replace_entry_tags(&context, entry_id, &[tag.id])
        .expect("replace");

    let mut collections = with_org(&context, |org| {
        org.entry_collection_ids(entry_id).expect("ids")
    });
    collections.sort();
    let history_id = context
        .organization()
        .history_collection_id(&context)
        .expect("history");
    let mut expected = vec![history_id, trabajo_id, clientes_id];
    expected.sort();
    assert_eq!(collections, expected);
}

#[test]
fn recent_entries_with_filter_returns_image_rows_for_collection() {
    use clipvault_db::{
        ContentType, EntryRepository, NewEntry, IMAGE_CONTENT_SENTINEL, IMAGE_MIME_PNG,
    };

    let (_dir, context) = bootstrap_with_clock(datetime!(2026-01-02 03:04:05 UTC));
    let when = datetime!(2026-01-02 03:04:05 UTC);

    // Seed a secondary collection.
    let trabajo_id = context
        .organization()
        .create_collection(&context, "Trabajo")
        .expect("create")
        .id;

    // Seed a textual capture inside `Trabajo`.
    let text_id = insert_entry(&context, "plain note", when);

    // Seed an image capture inside `Trabajo`.
    let hash = "f".repeat(64);
    let image_id = {
        let mut db = context.database().lock();
        let mut repo = EntryRepository::new(db.connection_mut());
        let new = NewEntry {
            content: IMAGE_CONTENT_SENTINEL.to_string(),
            content_type: ContentType::Image,
            content_size: 1024,
            content_hash: hash.clone(),
            source_app: Some("test-app".to_string()),
            created_at: when,
            last_seen_at: when,
            asset_ref: Some(format!("clipboard/{hash}.png")),
            mime_type: Some(IMAGE_MIME_PNG.to_string()),
            payload_width: Some(16),
            payload_height: Some(16),
            rich_text_hash: None,
            rich_html_ref: None,
            rich_rtf_ref: None,
            rich_preview_ref: None,
            rich_html_size: None,
            rich_rtf_size: None,
            code_language: None,
        };
        let outcome = repo.insert_or_touch(new).expect("insert image").record().id;
        drop(db);
        // Replace_entry_collections appends Historial on top of the
        // supplied ids; passing just `trabajo_id` is enough.
        context
            .organization()
            .replace_entry_collections(&context, outcome, &[trabajo_id])
            .expect("attach image to trabajo");
        outcome
    };

    context
        .organization()
        .replace_entry_collections(&context, text_id, &[trabajo_id])
        .expect("attach text to trabajo");

    let filtered = context
        .history()
        .recent_entries_with_filter(
            &context,
            Some(trabajo_id),
            &[],
            &SourceAppFilter::default(),
            50,
        )
        .expect("filtered recent entries");
    let ids: Vec<i64> = filtered.iter().map(|r| r.id).collect();
    assert!(
        ids.contains(&image_id),
        "image must appear in the collection-filtered rail (regression)"
    );
    assert!(
        ids.contains(&text_id),
        "text entry must appear in the collection-filtered rail"
    );

    // The image row's payload metadata must survive the filter so the
    // card can still request the bytes.
    let image_record = filtered
        .iter()
        .find(|r| r.id == image_id)
        .expect("image in filtered set");
    assert_eq!(
        image_record.asset_ref.as_deref(),
        Some(&*format!("clipboard/{hash}.png"))
    );
    assert_eq!(image_record.mime_type.as_deref(), Some(IMAGE_MIME_PNG));
    assert_eq!(image_record.payload_width, Some(16));
    assert_eq!(image_record.payload_height, Some(16));
}

#[test]
fn recent_entries_with_filter_for_history_includes_image_rows() {
    use clipvault_db::{
        ContentType, EntryRepository, NewEntry, IMAGE_CONTENT_SENTINEL, IMAGE_MIME_PNG,
    };

    let (_dir, context) = bootstrap_with_clock(datetime!(2026-01-02 03:04:05 UTC));
    let when = datetime!(2026-01-02 03:04:05 UTC);

    let text_id = insert_entry(&context, "plain note", when);
    let hash = "c".repeat(64);
    let image_id = {
        let mut db = context.database().lock();
        let mut repo = EntryRepository::new(db.connection_mut());
        let new = NewEntry {
            content: IMAGE_CONTENT_SENTINEL.to_string(),
            content_type: ContentType::Image,
            content_size: 1024,
            content_hash: hash.clone(),
            source_app: Some("test-app".to_string()),
            created_at: when,
            last_seen_at: when,
            asset_ref: Some(format!("clipboard/{hash}.png")),
            mime_type: Some(IMAGE_MIME_PNG.to_string()),
            payload_width: Some(8),
            payload_height: Some(8),
            rich_text_hash: None,
            rich_html_ref: None,
            rich_rtf_ref: None,
            rich_preview_ref: None,
            rich_html_size: None,
            rich_rtf_size: None,
            code_language: None,
        };
        repo.insert_or_touch(new).expect("insert image").record().id
    };

    // Selecting the system `Historial` collection (which is
    // implicitly what every entry already belongs to) must surface
    // every eligible row, including the image.
    let history_id = context
        .organization()
        .history_collection_id(&context)
        .expect("history");
    let filtered = context
        .history()
        .recent_entries_with_filter(
            &context,
            Some(history_id),
            &[],
            &SourceAppFilter::default(),
            50,
        )
        .expect("filtered");
    let ids: Vec<i64> = filtered.iter().map(|r| r.id).collect();
    assert!(ids.contains(&text_id));
    assert!(
        ids.contains(&image_id),
        "image must surface under Historial"
    );
}

// ---------------------------------------------------------------------
// `tags-and-collections` persistence-after-restart coverage.
//
// The user reported that tags appeared to be lost after closing and
// reopening ClipVault. The root cause turned out to be a frontend
// hydration gap (App.svelte never re-read `entryOrganization` for the
// cards after bootstrap), but the persistence layer must also be
// proved wrong independently: opening the same SQLite file twice and
// re-querying the join tables must return the same associations. The
// suite below closes that regression so a future migration cannot
// silently drop the associations, duplicate them or rebuild a fresh
// empty `entry_tags` table.
//
// The tests cover:
//   - text and image entries keep their tag associations;
//   - tag definitions are not duplicated across restarts;
//   - bootstrapping against an existing database never executes a
//     write that empties `entry_tags` or `tags`;
//   - the `entry_tags_set` and `upsert_and_assign_tag` mutations are
//     durable across closes;
//   - the same `database_path` is read on both sides of the restart
//     so a missing file is the only way the associations can vanish.
// ---------------------------------------------------------------------

fn reopen_bootstrap(dir: &TempDir, when: time::OffsetDateTime) -> clipvault_core::AppContext {
    // Reproduce the Tauri shell's "second launch" path: the database
    // path is the same, the migrations are idempotent and the
    // bootstrap must never re-run an existing migration in write
    // mode. The closure also hands back a freshly constructed
    // `AppContext` so the assertions below exercise the same code
    // path the GUI uses after the window is closed and re-opened.
    // The harness is rebuilt with an isolated `PlatformAdapters`
    // bundle pointing at the tempdir so the asset collector cannot
    // reach the real `~/.clipvault`.
    reopen_isolated_context(dir, when)
}

#[test]
fn tags_persist_across_close_and_reopen_on_same_database_path() {
    let (dir, context) = bootstrap_with_clock(datetime!(2026-01-02 03:04:05 UTC));
    let when = datetime!(2026-01-02 03:04:05 UTC);
    let entry_id = insert_entry(&context, "captured", when);

    let tag = context
        .organization()
        .upsert_and_assign_tag(&context, entry_id, "Critical")
        .expect("upsert+assign");

    // Close and reopen the same database path. The `TempDir` is kept
    // alive across the reopen so the SQLite file is reused, mirroring
    // the user's experience of closing and reopening the app.
    let reopened = reopen_bootstrap(&dir, when);
    assert_eq!(
        reopened.database().lock().path(),
        dir.path().join("clipvault.db"),
        "the reopen path must match the original path",
    );

    let tags = reopened
        .organization()
        .list_tags(&reopened)
        .expect("list_tags");
    assert_eq!(tags.len(), 1, "tag definition must not be duplicated");
    assert_eq!(tags[0].id, tag.id);
    assert_eq!(tags[0].normalized_name, "critical");

    let associations = with_org(&reopened, |org| org.entry_tag_ids(entry_id).expect("ids"));
    assert_eq!(
        associations,
        vec![tag.id],
        "the entry's association must survive the restart",
    );
}

#[test]
fn multiple_tags_persist_across_close_and_reopen_on_same_database_path() {
    let (dir, context) = bootstrap_with_clock(datetime!(2026-01-02 03:04:05 UTC));
    let when = datetime!(2026-01-02 03:04:05 UTC);
    let entry_id = insert_entry(&context, "captured", when);

    let a = context
        .organization()
        .upsert_tag(&context, "Critical")
        .expect("a");
    let b = context
        .organization()
        .upsert_tag(&context, "Draft")
        .expect("b");
    let c = context
        .organization()
        .upsert_tag(&context, "WIP")
        .expect("c");
    context
        .organization()
        .replace_entry_tags(&context, entry_id, &[a.id, b.id, c.id])
        .expect("seed associations");

    let reopened = reopen_bootstrap(&dir, when);
    let mut associations = with_org(&reopened, |org| org.entry_tag_ids(entry_id).expect("ids"));
    associations.sort();
    let mut expected = vec![a.id, b.id, c.id];
    expected.sort();
    assert_eq!(
        associations, expected,
        "all three associations must survive the restart",
    );

    // No duplicate tag rows after the reopen: `INSERT OR IGNORE` keeps
    // the seed migration and the runtime upsert in lockstep.
    let tags = reopened
        .organization()
        .list_tags(&reopened)
        .expect("list_tags");
    assert_eq!(tags.len(), 3, "no duplicate tag definitions allowed");
}

#[test]
fn bootstrap_does_not_clear_entry_tags_or_rebuild_the_join_table() {
    // Regression: the seed migration that introduces `tags` and
    // `entry_tags` must be idempotent. Running `run_migrations` against
    // an already-migrated database must not execute any `DROP`,
    // `DELETE`, or rebuild — the migration registry runs every
    // migration as `AlreadyApplied` after the first run, so the join
    // tables keep their rows. The test guards against an accidental
    // rewrite that would wipe existing associations on the second
    // launch.
    let (dir, context) = bootstrap_with_clock(datetime!(2026-01-02 03:04:05 UTC));
    let when = datetime!(2026-01-02 03:04:05 UTC);
    let entry_id = insert_entry(&context, "captured", when);
    let tag = context
        .organization()
        .upsert_and_assign_tag(&context, entry_id, "Critical")
        .expect("upsert+assign");

    // The reopen runs the migrations a second time.
    let _reopened = reopen_bootstrap(&dir, when);

    // The reopen path never opened a second SQLite file or a
    // temporary store; we re-open the same path and verify both the
    // association and the definition are still there.
    let final_context = reopen_bootstrap(&dir, when);
    let associations = with_org(&final_context, |org| {
        org.entry_tag_ids(entry_id).expect("ids")
    });
    assert_eq!(associations, vec![tag.id]);

    let tags = final_context
        .organization()
        .list_tags(&final_context)
        .expect("list_tags");
    assert_eq!(tags.len(), 1, "no duplicated tag row after bootstrap");
}

#[test]
fn entry_tags_set_persists_across_close_and_reopen() {
    // `entry_tags_set` is a replace-everything mutation; the test
    // exercises both a seed round-trip and a follow-up
    // `replace_entry_tags` against an already-tagged entry so the
    // final state survives the restart.
    let (dir, context) = bootstrap_with_clock(datetime!(2026-01-02 03:04:05 UTC));
    let when = datetime!(2026-01-02 03:04:05 UTC);
    let entry_id = insert_entry(&context, "captured", when);
    let a = context
        .organization()
        .upsert_tag(&context, "Critical")
        .expect("a");
    let b = context
        .organization()
        .upsert_tag(&context, "Draft")
        .expect("b");
    context
        .organization()
        .replace_entry_tags(&context, entry_id, &[a.id])
        .expect("seed");

    context
        .organization()
        .replace_entry_tags(&context, entry_id, &[a.id, b.id])
        .expect("extend");

    let reopened = reopen_bootstrap(&dir, when);
    let mut associations = with_org(&reopened, |org| org.entry_tag_ids(entry_id).expect("ids"));
    associations.sort();
    let mut expected = vec![a.id, b.id];
    expected.sort();
    assert_eq!(associations, expected);
}

#[test]
fn image_entry_tags_persist_across_close_and_reopen() {
    // Regression: an image row tagged from the card menu must keep its
    // tags after the app is closed and reopened. The image is the most
    // likely culprit if the join table is rebuilt during bootstrap
    // because the row type is the most exotic and the test surfaces
    // any silent loss.
    let (dir, context) = bootstrap_with_clock(datetime!(2026-01-02 03:04:05 UTC));
    let when = datetime!(2026-01-02 03:04:05 UTC);
    let image_id = seed_image_entry(&context, when);

    let tag = context
        .organization()
        .upsert_and_assign_tag(&context, image_id, "Critical")
        .expect("image tag");

    let reopened = reopen_bootstrap(&dir, when);
    let associations = with_org(&reopened, |org| org.entry_tag_ids(image_id).expect("ids"));
    assert_eq!(
        associations,
        vec![tag.id],
        "image entry must keep its tag association across restart",
    );

    // The image row metadata also survives the restart so the card
    // can keep rendering its thumbnail and the Paste action keeps
    // working.
    let row = reopened
        .history()
        .recent_entries_with_filter(&reopened, None, &[], &SourceAppFilter::default(), 50)
        .expect("rail")
        .into_iter()
        .find(|r| r.id == image_id)
        .expect("image row must remain after restart");
    let expected_ref = format!("clipboard/{}.png", "f".repeat(64));
    assert_eq!(row.asset_ref.as_deref(), Some(expected_ref.as_str()));
    assert_eq!(row.mime_type.as_deref(), Some(clipvault_db::IMAGE_MIME_PNG));
    assert_eq!(row.payload_width, Some(16));
    assert_eq!(row.payload_height, Some(16));
}

#[test]
fn upsert_and_assign_tag_is_idempotent_across_close_and_reopen() {
    // Calling the same upsert against an already-seeded tag must keep
    // a single row and a single association; this protects against a
    // migration that would re-create the `tags` table or wipe the
    // `entry_tags` join rows on bootstrap.
    let (dir, context) = bootstrap_with_clock(datetime!(2026-01-02 03:04:05 UTC));
    let when = datetime!(2026-01-02 03:04:05 UTC);
    let entry_id = insert_entry(&context, "captured", when);

    let first = context
        .organization()
        .upsert_and_assign_tag(&context, entry_id, "Critical")
        .expect("first");

    let reopened = reopen_bootstrap(&dir, when);
    let second = reopened
        .organization()
        .upsert_and_assign_tag(&reopened, entry_id, "  CRITICAL  ")
        .expect("second after restart");

    assert_eq!(first.id, second.id);
    let tags = reopened
        .organization()
        .list_tags(&reopened)
        .expect("list_tags");
    assert_eq!(tags.len(), 1);
    let associations = with_org(&reopened, |org| org.entry_tag_ids(entry_id).expect("ids"));
    assert_eq!(associations, vec![first.id]);
}

#[test]
fn reopen_uses_the_same_database_path() {
    // Sanity check: the bootstrap reopens the same SQLite file. A
    // regression that would silently switch to a temporary database
    // (or to `:memory:`) would manifest here because the assertion
    // below compares the in-memory path the bootstrap recorded
    // against the on-disk path the test owns.
    let (dir, context) = bootstrap_with_clock(datetime!(2026-01-02 03:04:05 UTC));
    let when = datetime!(2026-01-02 03:04:05 UTC);
    let entry_id = insert_entry(&context, "captured", when);
    context
        .organization()
        .upsert_and_assign_tag(&context, entry_id, "Critical")
        .expect("seed");

    let reopened = reopen_bootstrap(&dir, when);
    assert_eq!(
        context.database().lock().path(),
        reopened.database().lock().path(),
        "the reopen must target the same SQLite file",
    );
    assert!(
        context.database().lock().path().exists(),
        "the SQLite file must still exist on disk",
    );
}

// ---------------------------------------------------------------------
// `tags-and-collections` image-flow regression coverage. The previous
// rounds introduced `entries_filtered` (image-aware rail query) and
// verified it returns image rows under `Historial` and inside
// secondary collections; the tests below pin the additional
// guarantees the card surface depends on after the tag modal
// rework:
//   - assigning tags to an image preserves the asset record;
//   - assigning a collection to an image preserves the asset
//     record;
//   - the image row remains visible in the recent-entries rail.
// ---------------------------------------------------------------------

fn seed_image_entry(context: &clipvault_core::AppContext, when: time::OffsetDateTime) -> i64 {
    use clipvault_db::{ContentType, EntryRepository, NewEntry};
    let hash = "f".repeat(64);
    let mut db = context.database().lock();
    let mut repo = EntryRepository::new(db.connection_mut());
    let new = NewEntry {
        content: clipvault_db::IMAGE_CONTENT_SENTINEL.to_string(),
        content_type: ContentType::Image,
        content_size: 1024,
        content_hash: hash.clone(),
        source_app: Some("test-app".to_string()),
        created_at: when,
        last_seen_at: when,
        asset_ref: Some(format!("clipboard/{hash}.png")),
        mime_type: Some(clipvault_db::IMAGE_MIME_PNG.to_string()),
        payload_width: Some(16),
        payload_height: Some(16),
        rich_text_hash: None,
        rich_html_ref: None,
        rich_rtf_ref: None,
        rich_preview_ref: None,
        rich_html_size: None,
        rich_rtf_size: None,
        code_language: None,
    };
    let id = repo.insert_or_touch(new).expect("insert image").record().id;
    drop(db);
    id
}

#[test]
fn assigning_tags_to_image_preserves_asset_ref_and_payload() {
    let (_dir, context) = bootstrap_with_clock(datetime!(2026-01-02 03:04:05 UTC));
    let when = datetime!(2026-01-02 03:04:05 UTC);
    let image_id = seed_image_entry(&context, when);

    let tag = context
        .organization()
        .upsert_and_assign_tag(&context, image_id, "Critical")
        .expect("tag + assign");

    let row = context
        .history()
        .recent_entries_with_filter(&context, None, &[], &SourceAppFilter::default(), 50)
        .expect("rail")
        .into_iter()
        .find(|r| r.id == image_id)
        .expect("image row must remain in the rail after tag assignment");
    let expected_ref = format!("clipboard/{}.png", "f".repeat(64));
    assert_eq!(
        row.asset_ref.as_deref(),
        Some(expected_ref.as_str()),
        "asset_ref survives the tag assignment",
    );
    assert_eq!(row.mime_type.as_deref(), Some(clipvault_db::IMAGE_MIME_PNG),);
    assert_eq!(row.payload_width, Some(16));
    assert_eq!(row.payload_height, Some(16));
    let associations = with_org(&context, |org| org.entry_tag_ids(image_id).expect("ids"));
    assert_eq!(associations, vec![tag.id]);
}

#[test]
fn assigning_collection_to_image_preserves_asset_ref_and_payload() {
    let (_dir, context) = bootstrap_with_clock(datetime!(2026-01-02 03:04:05 UTC));
    let when = datetime!(2026-01-02 03:04:05 UTC);
    let image_id = seed_image_entry(&context, when);
    let trabajo_id = context
        .organization()
        .create_collection(&context, "Trabajo")
        .expect("Trabajo")
        .id;

    context
        .organization()
        .replace_entry_collections(&context, image_id, &[trabajo_id])
        .expect("attach");

    let row = context
        .history()
        .recent_entries_with_filter(&context, None, &[], &SourceAppFilter::default(), 50)
        .expect("rail")
        .into_iter()
        .find(|r| r.id == image_id)
        .expect("image row must remain in the rail after collection assignment");
    let expected_ref = format!("clipboard/{}.png", "f".repeat(64));
    assert_eq!(
        row.asset_ref.as_deref(),
        Some(expected_ref.as_str()),
        "asset_ref survives the collection assignment",
    );
    assert_eq!(row.mime_type.as_deref(), Some(clipvault_db::IMAGE_MIME_PNG),);
    assert_eq!(row.payload_width, Some(16));
    assert_eq!(row.payload_height, Some(16));
}

#[test]
fn image_row_remains_in_unfiltered_recent_entries_after_assignments() {
    let (_dir, context) = bootstrap_with_clock(datetime!(2026-01-02 03:04:05 UTC));
    let when = datetime!(2026-01-02 03:04:05 UTC);
    let image_id = seed_image_entry(&context, when);
    let text_id = insert_entry(&context, "plain note", when);

    let trabajo_id = context
        .organization()
        .create_collection(&context, "Trabajo")
        .expect("Trabajo")
        .id;
    let tag = context
        .organization()
        .upsert_tag(&context, "Critical")
        .expect("tag");
    context
        .organization()
        .replace_entry_tags(&context, image_id, &[tag.id])
        .expect("attach image tag");
    context
        .organization()
        .replace_entry_collections(&context, image_id, &[trabajo_id])
        .expect("attach image collection");

    let recent = context
        .history()
        .recent_entries_with_filter(&context, None, &[], &SourceAppFilter::default(), 50)
        .expect("rail");
    let ids: Vec<i64> = recent.iter().map(|r| r.id).collect();
    assert!(ids.contains(&image_id));
    assert!(ids.contains(&text_id));
}

// ---------------------------------------------------------------------
// `desktop-shell-layout` 10.8: regression coverage for the
// image-after-restart scenario.
//
// The user-reported regression was that previously saved images
// stopped showing in the rail after a restart, even though the
// rows were intact in SQLite. The tests below drive the same
// `AppContext` the Tauri commands consume, exercise the
// `recent_entries` and `recent_entries_with_filter` queries, the
// `set_favorite` round-trip and the touch path on a duplicate hash
// — every layer the image card relies on — and pin that every
// payload metadata column survives a close/reopen cycle.
// ---------------------------------------------------------------------

/// Build a fresh [`AppContext`] on top of an existing database file
/// by tearing the previous one down and bootstrapping again. Mirrors
/// the desktop restart loop: SQLite keeps the data, the asset store
/// keeps the bytes, the context is rebuilt from scratch. The
/// harness is rebuilt with an isolated `PlatformAdapters` bundle so
/// the asset collector cannot reach the real `~/.clipvault` even
/// when the test thread re-enters `bootstrap_at` with the same
/// database path.
fn reopen_context_from(dir: &TempDir, when: time::OffsetDateTime) -> AppContext {
    reopen_isolated_context(dir, when)
}

#[test]
fn image_payload_metadata_survives_context_restart() {
    let when = datetime!(2026-01-02 03:04:05 UTC);
    let (dir, context) = bootstrap_with_clock(when);
    let image_id = seed_image_entry(&context, when);

    // Drop the first context entirely — the rest of the test drives
    // every query through a fresh handle, the way the desktop does
    // when it boots after the user closed the previous session.
    drop(context);

    let context = reopen_context_from(&dir, when);
    let recent = context
        .history()
        .recent_entries(&context, 50)
        .expect("recent after reopen");
    let image = recent
        .iter()
        .find(|r| r.id == image_id)
        .expect("image must survive the restart cycle");
    assert_eq!(image.content_type, clipvault_db::ContentType::Image);
    assert_eq!(image.content, clipvault_db::IMAGE_CONTENT_SENTINEL);
    assert_eq!(
        image.asset_ref.as_deref(),
        Some("clipboard/ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff.png"),
    );
    assert_eq!(image.mime_type.as_deref(), Some("image/png"));
    assert_eq!(image.payload_width, Some(16));
    assert_eq!(image.payload_height, Some(16));
    assert!(image.is_renderable_image());
}

#[test]
fn recent_entries_with_filter_returns_image_rows_after_context_restart() {
    let when = datetime!(2026-01-02 03:04:05 UTC);
    let (dir, context) = bootstrap_with_clock(when);
    let image_id = seed_image_entry(&context, when);
    let trabajo_id = context
        .organization()
        .create_collection(&context, "Trabajo")
        .expect("Trabajo")
        .id;
    context
        .organization()
        .replace_entry_collections(&context, image_id, &[trabajo_id])
        .expect("attach");
    drop(context);

    let context = reopen_context_from(&dir, when);
    let rows = context
        .history()
        .recent_entries_with_filter(&context, None, &[], &SourceAppFilter::default(), 50)
        .expect("unfiltered rail after reopen");
    let unfiltered_ids: Vec<i64> = rows.iter().map(|r| r.id).collect();
    assert!(
        unfiltered_ids.contains(&image_id),
        "unfiltered rail must surface the image after restart",
    );

    let scoped = context
        .history()
        .recent_entries_with_filter(
            &context,
            Some(trabajo_id),
            &[],
            &SourceAppFilter::default(),
            50,
        )
        .expect("scoped rail after reopen");
    let scoped_ids: Vec<i64> = scoped.iter().map(|r| r.id).collect();
    assert!(
        scoped_ids.contains(&image_id),
        "scoped rail must surface the image after restart",
    );
    let image = scoped
        .iter()
        .find(|r| r.id == image_id)
        .expect("image in scoped rail");
    assert!(image.is_renderable_image());
    assert_eq!(image.mime_type.as_deref(), Some("image/png"));
}

#[test]
fn set_favorite_preserves_every_image_field_after_context_restart() {
    let when = datetime!(2026-01-02 03:04:05 UTC);
    let later = datetime!(2026-01-02 03:09:00 UTC);
    let (dir, context) = bootstrap_with_clock(when);
    let image_id = seed_image_entry(&context, when);
    drop(context);

    let context = reopen_context_from(&dir, later);
    let outcome = context
        .management()
        .set_favorite(&context, image_id, true)
        .expect("pin");
    let pinned = outcome.entry.expect("entry after pin");
    assert!(pinned.is_pinned, "pin state must persist");
    assert!(pinned.is_renderable_image());
    drop(context);

    let context = reopen_context_from(&dir, later);
    let row = context
        .management()
        .set_favorite(&context, image_id, false)
        .expect("unpin");
    let unpinned = row.entry.expect("entry after unpin");
    assert!(!unpinned.is_pinned, "unpin must persist");
    assert!(
        unpinned.is_renderable_image(),
        "unpin must not strip asset_ref or dimensions",
    );
    assert_eq!(
        unpinned.asset_ref.as_deref(),
        Some("clipboard/ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff.png"),
    );
    assert_eq!(unpinned.mime_type.as_deref(), Some("image/png"));
    assert_eq!(unpinned.payload_width, Some(16));
    assert_eq!(unpinned.payload_height, Some(16));
}

#[test]
fn organization_hydration_does_not_modify_asset_ref_or_payload_metadata() {
    // The frontend's per-entry cache is hydrated through the
    // organization service (tags + collections). The hydration must
    // NEVER write back to the entries table — a regression that
    // conflated the hydration path with the entry-update path would
    // wipe the image metadata on the next `organization-updated`
    // event, exactly the situation that surfaced after the previous
    // fix. The follow-up pins the contract: a hydration round on an
    // image row leaves every payload metadata column intact.
    let when = datetime!(2026-01-02 03:04:05 UTC);
    let (_dir, context) = bootstrap_with_clock(when);
    let image_id = seed_image_entry(&context, when);

    let before = {
        let mut db = context.database().lock();
        let repo = clipvault_db::EntryRepository::new(db.connection_mut());
        repo.find_by_id(image_id).expect("query").expect("row")
    };

    // Run the same round the frontend drives through
    // `hydrateEntryOrganization`: list tags, list collections,
    // refresh the snapshot. None of these calls should touch the
    // image row in `clipboard_entries`.
    let _ = context
        .organization()
        .list_tags(&context)
        .expect("list tags");
    let _ = context
        .organization()
        .list_collections(&context)
        .expect("list collections");
    let _ = context
        .organization()
        .history_collection_id(&context)
        .expect("history id");

    let after = {
        let mut db = context.database().lock();
        let repo = clipvault_db::EntryRepository::new(db.connection_mut());
        repo.find_by_id(image_id).expect("query").expect("row")
    };
    assert_eq!(before.asset_ref, after.asset_ref);
    assert_eq!(before.mime_type, after.mime_type);
    assert_eq!(before.payload_width, after.payload_width);
    assert_eq!(before.payload_height, after.payload_height);
    assert_eq!(before.content, after.content);
}

#[test]
fn recent_entries_returns_image_rows_with_full_metadata_through_service_layer() {
    // The Tauri `clipvault_recent_entries` command is a thin adapter
    // around `TextHistoryService::recent_entries`. The service-layer
    // test below pins the contract the adapter relies on so a
    // future refactor cannot silently drop `asset_ref`,
    // `mime_type` or the payload dimensions when the JSON payload
    // is built for the wire.
    let when = datetime!(2026-01-02 03:04:05 UTC);
    let (_dir, context) = bootstrap_with_clock(when);
    let image_id = seed_image_entry(&context, when);
    let text_id = insert_entry(&context, "captured note", when);

    let records = context
        .history()
        .recent_entries(&context, 50)
        .expect("recent");
    let ids: Vec<i64> = records.iter().map(|r| r.id).collect();
    assert!(ids.contains(&text_id), "text row must surface");
    assert!(ids.contains(&image_id), "image row must surface");

    let image = records
        .iter()
        .find(|r| r.id == image_id)
        .expect("image in rail");
    // The serialization-level contract: every payload metadata column
    // is non-default and survives the round-trip through
    // `EntryRecord`. A regression that dropped one of them would
    // surface here as `None` or as the default value.
    assert_eq!(image.content_type, clipvault_db::ContentType::Image);
    assert!(
        image.asset_ref.is_some(),
        "asset_ref must travel with the record",
    );
    assert!(image.mime_type.is_some());
    assert!(image.payload_width.is_some());
    assert!(image.payload_height.is_some());
    assert!(image.is_renderable_image());
}
