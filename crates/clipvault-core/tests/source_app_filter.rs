//! Integration tests for the `source-app-filter` capability.
//!
//! These tests exercise the typed query surface the Tauri shell
//! consumes: `SourceApplicationsQuery` against a real SQLite
//! fixture, plus the recents/search filters combined with the
//! additive `SourceAppFilter`. The suite keeps the same isolation
//! helpers the rest of `clipvault-core` uses so no test reaches
//! `~/.clipvault` or any other developer directory.

use std::sync::Arc;

use clipvault_core::{
    AppBootstrap, AppContext, Clock, SearchFilter, SourceAppFilter, SourceApplicationsQuery,
    SourceApplicationsScope,
};
use clipvault_db::{ContentType, EntryRepository, NewEntry, OrganizationRepository};
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

fn insert_entry_with(
    context: &AppContext,
    content: &str,
    source_app: Option<&str>,
    source_name: Option<&str>,
    source_icon: Option<&str>,
    when: time::OffsetDateTime,
) -> i64 {
    let new = NewEntry::text(
        content.to_string(),
        ContentType::Text,
        content.len() as i64,
        format!("hash::{content}"),
        source_app.map(str::to_string),
        when,
        when,
    );
    let id = {
        let mut db = context.database().lock();
        let mut repo = EntryRepository::new(db.connection_mut());
        repo.insert_or_touch(new).expect("insert").record().id
    };
    if source_name.is_some() || source_icon.is_some() {
        let mut db = context.database().lock();
        let mut repo = EntryRepository::new(db.connection_mut());
        repo.set_source_app_metadata(id, source_name, source_icon, when)
            .expect("metadata");
    }
    id
}

#[test]
fn source_applications_query_returns_todas_first_then_known_then_unknown() {
    let (_dir, context) = bootstrap_with_clock(datetime!(2026-05-01 08:00:00 UTC));
    let when = datetime!(2026-05-01 08:00:00 UTC);
    insert_entry_with(
        &context,
        "editor text",
        Some("com.example.Editor"),
        Some("Editor"),
        Some("application-icons/editor.png"),
        when,
    );
    insert_entry_with(
        &context,
        "terminal text",
        Some("com.apple.Terminal"),
        Some("Terminal"),
        Some("application-icons/terminal.png"),
        when,
    );
    insert_entry_with(&context, "unknown text", None, None, None, when);

    let snapshot = SourceApplicationsQuery::new()
        .load(&context, &SourceApplicationsScope::new(None, &[]))
        .expect("snapshot");
    assert_eq!(snapshot.options.len(), 4);
    assert_eq!(snapshot.options[0].display_name, "Todas");
    assert_eq!(snapshot.options[1].display_name, "Aplicación desconocida");
    // Case-insensitive display_name sort: "Editor" before "Terminal".
    assert_eq!(snapshot.options[2].display_name, "Editor");
    assert_eq!(snapshot.options[3].display_name, "Terminal");
    assert_eq!(
        snapshot.options[2].icon_ref.as_deref(),
        Some("application-icons/editor.png")
    );
}

#[test]
fn source_applications_query_omits_unknown_when_scope_lacks_unidentified_rows() {
    let (_dir, context) = bootstrap_with_clock(datetime!(2026-05-01 08:00:00 UTC));
    let when = datetime!(2026-05-01 08:00:00 UTC);
    insert_entry_with(
        &context,
        "editor text",
        Some("com.example.Editor"),
        Some("Editor"),
        Some("application-icons/editor.png"),
        when,
    );

    let snapshot = SourceApplicationsQuery::new()
        .load(&context, &SourceApplicationsScope::new(None, &[]))
        .expect("snapshot");
    assert_eq!(snapshot.options.len(), 2);
    assert_eq!(snapshot.options[0].display_name, "Todas");
    assert_eq!(snapshot.options[1].display_name, "Editor");
    assert!(!snapshot.options[1].fallback);
}

#[test]
fn source_applications_query_respects_collection_scope() {
    let (_dir, context) = bootstrap_with_clock(datetime!(2026-05-01 08:00:00 UTC));
    let t1 = datetime!(2026-05-01 08:00:00 UTC);
    let t2 = datetime!(2026-05-01 09:00:00 UTC);
    let trabajo_id = {
        let mut db = context.database().lock();
        let mut org = OrganizationRepository::new(db.connection_mut());
        org.create_user_collection("Trabajo", t1)
            .expect("create")
            .id
    };
    let in_trabajo = insert_entry_with(
        &context,
        "in trabajo",
        Some("com.example.Editor"),
        Some("Editor"),
        Some("application-icons/editor.png"),
        t1,
    );
    let other = insert_entry_with(
        &context,
        "in history",
        Some("com.apple.Terminal"),
        Some("Terminal"),
        Some("application-icons/terminal.png"),
        t2,
    );
    {
        let mut db = context.database().lock();
        let mut org = OrganizationRepository::new(db.connection_mut());
        org.replace_entry_collections(in_trabajo, &[trabajo_id], t1)
            .expect("attach");
    }
    let _ = other;

    let snapshot = SourceApplicationsQuery::new()
        .load(
            &context,
            &SourceApplicationsScope::new(Some(trabajo_id), &[]),
        )
        .expect("snapshot");
    assert_eq!(snapshot.options.len(), 2);
    assert_eq!(snapshot.options[0].display_name, "Todas");
    assert_eq!(snapshot.options[1].display_name, "Editor");
    assert_eq!(
        snapshot.options[1].source_app.as_deref(),
        Some("com.example.Editor")
    );
    assert_eq!(snapshot.scope.collection_id, Some(trabajo_id));
}

#[test]
fn search_filter_with_known_source_app_matches_only_that_identifier() {
    let (_dir, context) = bootstrap_with_clock(datetime!(2026-05-01 08:00:00 UTC));
    let when = datetime!(2026-05-01 08:00:00 UTC);
    // Unique content per row so the dedupe-by-hash contract does not
    // collapse the two captures into a single row.
    let editor_id = insert_entry_with(
        &context,
        "hello editor",
        Some("com.example.Editor"),
        Some("Editor"),
        None,
        when,
    );
    let terminal_id = insert_entry_with(
        &context,
        "hello terminal",
        Some("com.apple.Terminal"),
        Some("Terminal"),
        None,
        when,
    );

    let outcome = context
        .search()
        .search_with_filter(
            &context,
            &clipvault_core::SearchQuery {
                text: "hello".into(),
                limit: 10,
            },
            &SearchFilter {
                collection_id: None,
                tag_ids: vec![],
                source_app: SourceAppFilter::Known {
                    source_app: "com.example.Editor".into(),
                },
            },
        )
        .expect("search");
    let ids: Vec<i64> = outcome.hits.iter().map(|h| h.entry_id).collect();
    assert_eq!(ids, vec![editor_id]);
    assert!(!ids.contains(&terminal_id));
}

#[test]
fn search_filter_with_unknown_source_app_matches_only_rows_without_identifier() {
    let (_dir, context) = bootstrap_with_clock(datetime!(2026-05-01 08:00:00 UTC));
    let when = datetime!(2026-05-01 08:00:00 UTC);
    let unknown_id = insert_entry_with(&context, "hello unknown", None, None, None, when);
    let editor_id = insert_entry_with(
        &context,
        "hello editor",
        Some("com.example.Editor"),
        Some("Editor"),
        None,
        when,
    );

    let outcome = context
        .search()
        .search_with_filter(
            &context,
            &clipvault_core::SearchQuery {
                text: "hello".into(),
                limit: 10,
            },
            &SearchFilter {
                collection_id: None,
                tag_ids: vec![],
                source_app: SourceAppFilter::Unknown,
            },
        )
        .expect("search");
    let ids: Vec<i64> = outcome.hits.iter().map(|h| h.entry_id).collect();
    assert_eq!(ids, vec![unknown_id]);
    assert!(!ids.contains(&editor_id));
}

#[test]
fn recent_entries_with_filter_combines_source_app_with_collection() {
    let (_dir, context) = bootstrap_with_clock(datetime!(2026-05-01 08:00:00 UTC));
    let t1 = datetime!(2026-05-01 08:00:00 UTC);
    let t2 = datetime!(2026-05-01 09:00:00 UTC);
    let trabajo_id = {
        let mut db = context.database().lock();
        let mut org = OrganizationRepository::new(db.connection_mut());
        org.create_user_collection("Trabajo", t1)
            .expect("create")
            .id
    };
    let in_collection_editor = insert_entry_with(
        &context,
        "alpha",
        Some("com.example.Editor"),
        Some("Editor"),
        None,
        t1,
    );
    let in_collection_terminal = insert_entry_with(
        &context,
        "beta",
        Some("com.apple.Terminal"),
        Some("Terminal"),
        None,
        t2,
    );
    let in_history_editor = insert_entry_with(
        &context,
        "gamma",
        Some("com.example.Editor"),
        Some("Editor"),
        None,
        t2,
    );
    {
        let mut db = context.database().lock();
        let mut org = OrganizationRepository::new(db.connection_mut());
        org.replace_entry_collections(in_collection_editor, &[trabajo_id], t1)
            .expect("attach a");
        org.replace_entry_collections(in_collection_terminal, &[trabajo_id], t1)
            .expect("attach b");
    }
    let _ = in_history_editor;

    let records = context
        .history()
        .recent_entries_with_filter(
            &context,
            Some(trabajo_id),
            &[],
            &SourceAppFilter::Known {
                source_app: "com.example.Editor".into(),
            },
            50,
        )
        .expect("filtered");
    let ids: Vec<i64> = records.iter().map(|r| r.id).collect();
    assert_eq!(ids, vec![in_collection_editor]);
}

#[test]
fn source_applications_query_scope_round_trips() {
    let scope = SourceApplicationsScope::new(Some(2), &[1, 2]);
    let cloned = scope.clone();
    assert_eq!(scope, cloned);
    let different = SourceApplicationsScope::new(Some(3), &[1, 2]);
    assert_ne!(scope, different);
}

// -------------------------------------------------------------------
// Bootstrap regression: the Historial view (no collection filter) must
// honour the source-app facet the same way a user collection does.
//
// The frontend used to branch on `selectedCollectionId === null` and
// fall back to the unfiltered `clipvault_recent_entries` command,
// which silently dropped the `source_app` argument. The combobox
// still loaded its options through
// `clipvault_source_applications`, so the bug looked like the
// combobox and the rail disagreed. The fix routes Historial through
// the same `recent_entries_with_filter` call a user collection
// uses, with `collection_id = None`. The tests below pin the
// Historial scope:
//   - `All` reproduces the unfiltered history (matches the prior
//     `recent_entries` semantics bit-for-bit);
//   - `Known(id)` restricts the rail to that identifier;
//   - `Unknown` restricts the rail to rows whose `source_app` is
//     NULL or empty;
//   - the chronological ordering (`created_at DESC, id DESC`) is
//     preserved regardless of the filter.
// -------------------------------------------------------------------

#[test]
fn recent_entries_with_filter_history_scope_all_matches_unfiltered_recent() {
    let (_dir, context) = bootstrap_with_clock(datetime!(2026-05-01 08:00:00 UTC));
    let t1 = datetime!(2026-05-01 08:00:00 UTC);
    let t2 = datetime!(2026-05-01 09:00:00 UTC);
    let editor_id = insert_entry_with(
        &context,
        "alpha editor",
        Some("com.example.Editor"),
        Some("Editor"),
        None,
        t1,
    );
    let terminal_id = insert_entry_with(
        &context,
        "beta terminal",
        Some("com.apple.Terminal"),
        Some("Terminal"),
        None,
        t2,
    );
    let unknown_id = insert_entry_with(&context, "gamma unknown", None, None, None, t2);

    let unfiltered = context
        .history()
        .recent_entries(&context, 50)
        .expect("unfiltered");
    let unfiltered_ids: Vec<i64> = unfiltered.iter().map(|r| r.id).collect();
    let filtered = context
        .history()
        .recent_entries_with_filter(&context, None, &[], &SourceAppFilter::All, 50)
        .expect("filtered");
    let filtered_ids: Vec<i64> = filtered.iter().map(|r| r.id).collect();
    assert_eq!(
        unfiltered_ids, filtered_ids,
        "All must reproduce the unfiltered history bit-for-bit",
    );
    let _ = (editor_id, terminal_id, unknown_id);
}

#[test]
fn recent_entries_with_filter_history_scope_known_returns_only_that_identifier() {
    let (_dir, context) = bootstrap_with_clock(datetime!(2026-05-01 08:00:00 UTC));
    let when = datetime!(2026-05-01 08:00:00 UTC);
    let editor_id = insert_entry_with(
        &context,
        "alpha editor",
        Some("com.example.Editor"),
        Some("Editor"),
        None,
        when,
    );
    let terminal_id = insert_entry_with(
        &context,
        "beta terminal",
        Some("com.apple.Terminal"),
        Some("Terminal"),
        None,
        when,
    );

    let filtered = context
        .history()
        .recent_entries_with_filter(
            &context,
            None,
            &[],
            &SourceAppFilter::Known {
                source_app: "com.example.Editor".into(),
            },
            50,
        )
        .expect("filtered");
    let ids: Vec<i64> = filtered.iter().map(|r| r.id).collect();
    assert_eq!(ids, vec![editor_id]);
    assert!(!ids.contains(&terminal_id));
}

#[test]
fn recent_entries_with_filter_history_scope_unknown_returns_only_null_or_empty_rows() {
    let (_dir, context) = bootstrap_with_clock(datetime!(2026-05-01 08:00:00 UTC));
    let when = datetime!(2026-05-01 08:00:00 UTC);
    let null_id = insert_entry_with(&context, "unknown one", None, None, None, when);
    let empty_id = insert_entry_with(&context, "unknown two", Some(""), None, None, when);
    let editor_id = insert_entry_with(
        &context,
        "editor row",
        Some("com.example.Editor"),
        Some("Editor"),
        None,
        when,
    );

    let filtered = context
        .history()
        .recent_entries_with_filter(&context, None, &[], &SourceAppFilter::Unknown, 50)
        .expect("filtered");
    let ids: Vec<i64> = filtered.iter().map(|r| r.id).collect();
    assert!(ids.contains(&null_id));
    assert!(ids.contains(&empty_id));
    assert!(!ids.contains(&editor_id));
}

#[test]
fn recent_entries_with_filter_history_scope_preserves_chronological_order() {
    let (_dir, context) = bootstrap_with_clock(datetime!(2026-05-01 08:00:00 UTC));
    let t1 = datetime!(2026-05-01 08:00:00 UTC);
    let t2 = datetime!(2026-05-01 09:00:00 UTC);
    let t3 = datetime!(2026-05-01 10:00:00 UTC);
    let first_id = insert_entry_with(
        &context,
        "first editor",
        Some("com.example.Editor"),
        Some("Editor"),
        None,
        t1,
    );
    let second_id = insert_entry_with(
        &context,
        "second editor",
        Some("com.example.Editor"),
        Some("Editor"),
        None,
        t2,
    );
    let third_id = insert_entry_with(
        &context,
        "third editor",
        Some("com.example.Editor"),
        Some("Editor"),
        None,
        t3,
    );

    let filtered = context
        .history()
        .recent_entries_with_filter(
            &context,
            None,
            &[],
            &SourceAppFilter::Known {
                source_app: "com.example.Editor".into(),
            },
            50,
        )
        .expect("filtered");
    let ids: Vec<i64> = filtered.iter().map(|r| r.id).collect();
    assert_eq!(
        ids,
        vec![third_id, second_id, first_id],
        "History-scope filter must keep created_at DESC ordering",
    );
}

#[test]
fn search_filter_history_scope_known_combines_with_text_query() {
    // The bootstrap regression also covered the combination of text
    // search with a source-app filter in Historial: the search
    // command goes through `search_with_filter` with
    // `collection_id = None` and must still honour the source-app
    // facet. The expected behaviour: only entries that match the
    // text AND have the requested source_app are eligible.
    let (_dir, context) = bootstrap_with_clock(datetime!(2026-05-01 08:00:00 UTC));
    let when = datetime!(2026-05-01 08:00:00 UTC);
    let matching_editor = insert_entry_with(
        &context,
        "hello editor",
        Some("com.example.Editor"),
        Some("Editor"),
        None,
        when,
    );
    let _other_editor = insert_entry_with(
        &context,
        "world editor",
        Some("com.example.Editor"),
        Some("Editor"),
        None,
        when,
    );
    let matching_terminal = insert_entry_with(
        &context,
        "hello terminal",
        Some("com.apple.Terminal"),
        Some("Terminal"),
        None,
        when,
    );

    let outcome = context
        .search()
        .search_with_filter(
            &context,
            &clipvault_core::SearchQuery {
                text: "hello".into(),
                limit: 10,
            },
            &SearchFilter {
                collection_id: None,
                tag_ids: vec![],
                source_app: SourceAppFilter::Known {
                    source_app: "com.example.Editor".into(),
                },
            },
        )
        .expect("search");
    let ids: Vec<i64> = outcome.hits.iter().map(|h| h.entry_id).collect();
    assert_eq!(ids, vec![matching_editor]);
    assert!(!ids.contains(&matching_terminal));
}
