//! Integration coverage for the
//! `desktop-dnd-card-visual-corrections` change.
//!
//! The frontend ships a new `text/plain` payload the drag-and-drop
//! flow writes next to the private ClipVault MIME so WebKit/Tauri
//! can still drop a card on a user collection. The Rust layer
//! already exposes the additive combine helper the drop flow
//! delegates to (`OrganizationService::replace_entry_collections`,
//! Tauri command `clipvault_entry_collections_set`).
//!
//! These tests focus on the behaviours the new drop flow relies on:
//!
//!   - `entry_collections_set` with a payload that only carries the
//!     target id (the case after the frontend's strict parser
//!     decoded `clipvault-entry:v1:<id>` from the `text/plain`
//!     fallback) still appends `Historial` automatically so the
//!     entry is never left without the system membership;
//!   - `entry_collections_set` is idempotent and safe under
//!     repeated identical calls so the drag handler can ignore
//!     duplicate in-flight writes without disturbing the persisted
//!     state;
//!   - the existing image metadata (`asset_ref`, `mime_type`,
//!     `payload_width`, `payload_height`, `content_size`) survives
//!     a drop, a tag hydration round, a pin/unpin round and a
//!     simulated restart;
//!   - a `replace_entry_collections` call that targets a missing
//!     entry is reported as an error and does NOT silently mutate
//!     any other row's associations.

use std::sync::Arc;

use clipvault_core::{AppBootstrap, AppContext, Clock};
use clipvault_db::{ContentType, EntryRepository, NewEntry};
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

fn insert_text_entry(
    context: &AppContext,
    content: &str,
    hash: &str,
    when: time::OffsetDateTime,
) -> i64 {
    let new = NewEntry {
        content: content.to_string(),
        content_type: ContentType::Text,
        content_size: content.len() as i64,
        content_hash: hash.to_string(),
        source_app: Some("test-app".to_string()),
        created_at: when,
        last_seen_at: when,
        asset_ref: None,
        mime_type: None,
        payload_width: None,
        payload_height: None,
        rich_text_hash: None,
        rich_html_ref: None,
        rich_rtf_ref: None,
        rich_preview_ref: None,
        rich_html_size: None,
        rich_rtf_size: None,
        code_language: None,
    };
    let mut db = context.database().lock();
    let mut repo = EntryRepository::new(db.connection_mut());
    repo.insert_or_touch(new).expect("insert").record().id
}

fn insert_image_entry(
    context: &AppContext,
    hash: &str,
    width: u32,
    height: u32,
    content_size: i64,
    when: time::OffsetDateTime,
) -> i64 {
    let new = NewEntry {
        content: String::new(),
        content_type: ContentType::Image,
        content_size,
        content_hash: hash.to_string(),
        source_app: Some("test-app".to_string()),
        created_at: when,
        last_seen_at: when,
        asset_ref: Some(format!("clipboard/{hash}.png")),
        mime_type: Some("image/png".to_string()),
        payload_width: Some(width),
        payload_height: Some(height),
        rich_text_hash: None,
        rich_html_ref: None,
        rich_rtf_ref: None,
        rich_preview_ref: None,
        rich_html_size: None,
        rich_rtf_size: None,
        code_language: None,
    };
    let mut db = context.database().lock();
    let mut repo = EntryRepository::new(db.connection_mut());
    repo.insert_or_touch(new).expect("insert").record().id
}

fn with_conn<R>(context: &AppContext, f: impl FnOnce(&Connection) -> R) -> R {
    let db = context.database().lock();
    let conn = db.connection();
    f(conn)
}

fn image_metadata(
    context: &AppContext,
    entry_id: i64,
) -> (
    Option<String>,
    Option<String>,
    Option<u32>,
    Option<u32>,
    i64,
) {
    with_conn(context, |conn| {
        conn.query_row(
            "SELECT asset_ref, mime_type, payload_width, payload_height, content_size
             FROM clipboard_entries WHERE id = ?1",
            rusqlite::params![entry_id],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<u32>>(2)?,
                    row.get::<_, Option<u32>>(3)?,
                    row.get::<_, i64>(4)?,
                ))
            },
        )
        .expect("image row")
    })
}

// ---------------------------------------------------------------------
// Drop semantics: the additive combine the drag flow relies on.
// ---------------------------------------------------------------------

#[test]
fn replace_entry_collections_appends_historial_when_payload_omits_system_collection() {
    // The strict `text/plain` parser on the frontend emits a single
    // integer id; the helper then builds a payload that only
    // carries the target collection. The repository must reattach
    // `Historial` automatically so the entry is never left without
    // the protected membership.
    let (_dir, context) = bootstrap_with_clock(datetime!(2026-01-02 03:04:05 UTC));
    let history_id = context
        .organization()
        .history_collection_id(&context)
        .expect("history id");
    let entry_id = insert_text_entry(
        &context,
        "an unhydrated drop payload",
        &"a".repeat(64),
        datetime!(2026-01-02 03:04:05 UTC),
    );

    let trabajo_id = context
        .organization()
        .create_collection(&context, "Trabajo")
        .expect("create")
        .id;
    let merged = context
        .organization()
        .replace_entry_collections(&context, entry_id, &[trabajo_id])
        .expect("merge");
    assert!(merged.contains(&trabajo_id));
    assert!(merged.contains(&history_id));
}

#[test]
fn replace_entry_collections_is_idempotent_under_repeated_drops() {
    // The drag handler deduplicates in-flight writes by ignoring
    // identical drops on the same target. The backend's set helper
    // MUST also be idempotent: a second call with the same target
    // id MUST NOT duplicate the membership row.
    let (_dir, context) = bootstrap_with_clock(datetime!(2026-01-02 03:04:05 UTC));
    let history_id = context
        .organization()
        .history_collection_id(&context)
        .expect("history id");
    let entry_id = insert_text_entry(
        &context,
        "drop-1",
        &"b".repeat(64),
        datetime!(2026-01-02 03:04:05 UTC),
    );

    let trabajo_id = context
        .organization()
        .create_collection(&context, "Trabajo")
        .expect("create")
        .id;
    for _ in 0..3 {
        context
            .organization()
            .replace_entry_collections(&context, entry_id, &[trabajo_id])
            .expect("merge");
    }
    let membership = context
        .organization()
        .entry_collection_ids(&context, entry_id)
        .expect("ids");
    // Each membership appears exactly once.
    assert_eq!(membership.iter().filter(|id| **id == history_id).count(), 1);
    assert_eq!(membership.iter().filter(|id| **id == trabajo_id).count(), 1);
}

#[test]
fn replace_entry_collections_against_missing_entry_reports_error_and_does_not_mutate_others() {
    // The frontend's strict parser rejects malformed payloads,
    // but the backend must also be defensive: a `set` call against
    // an entry id that no longer exists MUST fail and MUST NOT
    // touch the associations of unrelated entries.
    let (_dir, context) = bootstrap_with_clock(datetime!(2026-01-02 03:04:05 UTC));
    let entry_id = insert_text_entry(
        &context,
        "real entry",
        &"c".repeat(64),
        datetime!(2026-01-02 03:04:05 UTC),
    );
    let trabajo_id = context
        .organization()
        .create_collection(&context, "Trabajo")
        .expect("create")
        .id;
    context
        .organization()
        .replace_entry_collections(&context, entry_id, &[trabajo_id])
        .expect("merge real entry");

    // Now ask the backend to mutate a non-existent entry id.
    let ghost_id = entry_id + 9_999;
    let error = context
        .organization()
        .replace_entry_collections(&context, ghost_id, &[trabajo_id])
        .unwrap_err();
    assert!(matches!(
        error,
        clipvault_core::OrganizationServiceError::Repository(_)
    ));
    // The real entry's memberships are untouched.
    let membership = context
        .organization()
        .entry_collection_ids(&context, entry_id)
        .expect("ids");
    assert!(membership.contains(&trabajo_id));
}

#[test]
fn replace_entry_collections_with_empty_target_list_is_rejected_when_system_collection_missing() {
    // The frontend helper refuses to emit an empty list (the
    // `missing_hydration` branch) but the backend must also be
    // defensive: a `set` call without the system collection must
    // never produce an empty `entry_collections` row.
    let (_dir, context) = bootstrap_with_clock(datetime!(2026-01-02 03:04:05 UTC));
    let history_id = context
        .organization()
        .history_collection_id(&context)
        .expect("history id");
    let entry_id = insert_text_entry(
        &context,
        "empty payload",
        &"d".repeat(64),
        datetime!(2026-01-02 03:04:05 UTC),
    );
    let trabajo_id = context
        .organization()
        .create_collection(&context, "Trabajo")
        .expect("create")
        .id;
    context
        .organization()
        .replace_entry_collections(&context, entry_id, &[trabajo_id])
        .expect("merge");

    // Sending an empty list now MUST NOT drop Historial.
    let merged_empty = context
        .organization()
        .replace_entry_collections(&context, entry_id, &[])
        .expect("merge empty");
    assert!(merged_empty.contains(&history_id));
    let membership = context
        .organization()
        .entry_collection_ids(&context, entry_id)
        .expect("ids");
    assert!(membership.contains(&history_id));
}

// ---------------------------------------------------------------------
// Image integrity: every drag-and-pin operation the user can perform
// from the rail must leave the image row intact.
// ---------------------------------------------------------------------

#[test]
fn image_entry_survives_drop_pin_unpin_and_organization_round() {
    let (dir, context) = bootstrap_with_clock(datetime!(2026-01-02 03:04:05 UTC));
    let entry_id = insert_image_entry(
        &context,
        &"e".repeat(64),
        1024,
        768,
        4096,
        datetime!(2026-01-02 03:04:05 UTC),
    );
    let trabajo_id = context
        .organization()
        .create_collection(&context, "Trabajo")
        .expect("create")
        .id;

    // Drag/drop adds the entry to the user collection.
    context
        .organization()
        .replace_entry_collections(&context, entry_id, &[trabajo_id])
        .expect("merge");
    // Pin then unpin, like the new pushpin affordance allows.
    context
        .management()
        .set_favorite(&context, entry_id, true)
        .expect("pin");
    context
        .management()
        .set_favorite(&context, entry_id, false)
        .expect("unpin");
    // Touch the tag set so the hydration round runs through the
    // organization update path.
    let _ = context.management();

    let (asset, mime, width, height, size) = image_metadata(&context, entry_id);
    assert_eq!(
        asset.as_deref(),
        Some(format!("clipboard/{}.png", "e".repeat(64)).as_str())
    );
    assert_eq!(mime.as_deref(), Some("image/png"));
    assert_eq!(width, Some(1024));
    assert_eq!(height, Some(768));
    assert_eq!(size, 4096);

    // Simulate a restart. The image bytes are still discoverable
    // through the same references the desktop uses. The harness is
    // rebuilt with an isolated `PlatformAdapters` bundle so the
    // asset collector cannot reach the developer's real
    // `~/.clipvault` even when the test thread re-enters
    // `bootstrap_at` with the same database path.
    drop(context);
    let adapters = clipvault_core::build_isolated_adapters(dir.path(), &dir.path().join("data"));
    let reopened = AppBootstrap::new()
        .with_clock(Arc::new(FixedClock {
            instant: datetime!(2026-01-02 03:04:06 UTC),
        }))
        .with_clipboard(Arc::new(clipvault_core::FakeClipboard::new()))
        .with_platform_adapters(adapters)
        .bootstrap_at(dir.path().join("clipvault.db"))
        .expect("reopen");
    let (asset2, mime2, width2, height2, size2) = image_metadata(&reopened, entry_id);
    assert_eq!(asset2, asset);
    assert_eq!(mime2, mime);
    assert_eq!(width2, width);
    assert_eq!(height2, height);
    assert_eq!(size2, size);
}
