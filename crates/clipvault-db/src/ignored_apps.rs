//! Local blacklist of applications whose clipboard content must not be
//! captured or stored.
//!
//! The repository stores a single normalised identifier per row plus
//! the optional presentation metadata the picker flow produces. The
//! core layer is responsible for normalising the identifier
//! (trim + lowercase) before calling `upsert` so the matcher can
//! issue a single equality lookup. The metadata columns
//! (`display_name`, `icon_ref`) are nullable on purpose so legacy
//! rows created before the picker landed keep matching the
//! identifier and the frontend can render them with a fallback.

use rusqlite::{params, Connection, OptionalExtension};
use thiserror::Error;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

#[derive(Debug, Error)]
pub enum IgnoredAppsError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

/// One entry in the blacklist as it lives in the database. The
/// metadata columns are optional: rows inserted before the picker
/// existed (or before the bundle metadata could be extracted) carry
/// `None` and the frontend renders a fallback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IgnoredApp {
    pub id: String,
    pub display_name: Option<String>,
    pub icon_ref: Option<String>,
    pub created_at: String,
}

pub struct IgnoredAppRepository<'a> {
    conn: &'a mut Connection,
}

impl<'a> IgnoredAppRepository<'a> {
    pub fn new(conn: &'a mut Connection) -> Self {
        Self { conn }
    }

    /// Insert an identifier if it is not already present. The
    /// `now` parameter lets tests pin the timestamp. Metadata is
    /// `None` for callers that only have the identifier (e.g. the
    /// legacy `clipvault_ignored_apps_add` path).
    pub fn insert(
        &mut self,
        id: &str,
        now: OffsetDateTime,
    ) -> Result<IgnoredApp, IgnoredAppsError> {
        self.upsert(id, None, None, now)
    }

    /// Insert a new row or refresh the metadata on an existing row.
    ///
    /// The `id` MUST be normalised (trim + lowercase) by the caller —
    /// the repository never re-normalises the identifier so the
    /// comparison stays in one place. Re-selecting an existing
    /// application is idempotent: no duplicate row is created and
    /// missing metadata is filled in, but `display_name` / `icon_ref`
    /// are NOT overwritten with `None` when the new payload omits
    /// them (the previous, picker-supplied values stay).
    ///
    /// Returns the resulting row, which the caller forwards to the
    /// frontend.
    pub fn upsert(
        &mut self,
        id: &str,
        display_name: Option<&str>,
        icon_ref: Option<&str>,
        now: OffsetDateTime,
    ) -> Result<IgnoredApp, IgnoredAppsError> {
        let tx = self.conn.transaction()?;
        let created_at = format_timestamp(now);
        // Insert with `OR IGNORE` so a pre-existing row wins. We
        // then run a per-column UPDATE that preserves a previously
        // stored value when the new payload is `None` — that lets
        // the picker refresh the metadata without losing data when
        // icon extraction fails on a subsequent selection.
        tx.execute(
            "INSERT OR IGNORE INTO ignored_apps (id, created_at, display_name, icon_ref)
             VALUES (?1, ?2, ?3, ?4)",
            params![id, created_at, display_name, icon_ref],
        )?;
        tx.execute(
            "UPDATE ignored_apps
             SET display_name = COALESCE(?2, display_name),
                 icon_ref     = COALESCE(?3, icon_ref)
             WHERE id = ?1",
            params![id, display_name, icon_ref],
        )?;
        tx.commit()?;
        let record = self.get(id)?.expect("row inserted or already present");
        Ok(record)
    }

    /// Remove an identifier. Returns `true` when a row was deleted.
    pub fn delete(&mut self, id: &str) -> Result<bool, IgnoredAppsError> {
        let removed = self
            .conn
            .execute("DELETE FROM ignored_apps WHERE id = ?1", params![id])?;
        Ok(removed > 0)
    }

    /// Lookup an identifier. Used by tests and by the matcher when an
    /// exact match is required.
    pub fn get(&self, id: &str) -> Result<Option<IgnoredApp>, IgnoredAppsError> {
        let record = self
            .conn
            .query_row(
                "SELECT id, display_name, icon_ref, created_at
                 FROM ignored_apps WHERE id = ?1",
                params![id],
                |row| {
                    Ok(IgnoredApp {
                        id: row.get(0)?,
                        display_name: row.get(1)?,
                        icon_ref: row.get(2)?,
                        created_at: row.get(3)?,
                    })
                },
            )
            .optional()?;
        Ok(record)
    }

    /// List every entry, sorted alphabetically by id so the result is
    /// deterministic and the frontend can render without a secondary
    /// sort.
    pub fn list(&self) -> Result<Vec<IgnoredApp>, IgnoredAppsError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, display_name, icon_ref, created_at
             FROM ignored_apps ORDER BY id ASC",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok(IgnoredApp {
                    id: row.get(0)?,
                    display_name: row.get(1)?,
                    icon_ref: row.get(2)?,
                    created_at: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Return the number of blacklisted identifiers.
    pub fn count(&self) -> Result<i64, IgnoredAppsError> {
        let n: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM ignored_apps", [], |row| row.get(0))?;
        Ok(n)
    }
}

fn format_timestamp(ts: OffsetDateTime) -> String {
    ts.format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    fn open_temp_db() -> (tempfile::TempDir, crate::Database) {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = crate::Database::open(dir.path().join("clipvault.db")).expect("open");
        let mut db = db;
        db.run_migrations(&crate::builtin_migrations())
            .expect("migrate");
        (dir, db)
    }

    #[test]
    fn insert_then_get_round_trip() {
        let (_dir, mut db) = open_temp_db();
        let when = datetime!(2026-01-02 03:04:05 UTC);
        {
            let mut repo = IgnoredAppRepository::new(db.connection_mut());
            let row = repo.insert("com.apple.Terminal", when).expect("insert");
            assert_eq!(row.id, "com.apple.Terminal");
            assert!(row.display_name.is_none());
            assert!(row.icon_ref.is_none());
        }
        let repo = IgnoredAppRepository::new(db.connection_mut());
        let row = repo.get("com.apple.Terminal").unwrap().expect("present");
        assert_eq!(row.id, "com.apple.Terminal");
    }

    #[test]
    fn insert_is_idempotent() {
        let (_dir, mut db) = open_temp_db();
        let when = datetime!(2026-01-02 03:04:05 UTC);
        {
            let mut repo = IgnoredAppRepository::new(db.connection_mut());
            repo.insert("com.apple.Terminal", when).expect("first");
            repo.insert("com.apple.Terminal", when).expect("second");
        }
        let repo = IgnoredAppRepository::new(db.connection_mut());
        assert_eq!(repo.count().unwrap(), 1);
    }

    #[test]
    fn upsert_inserts_metadata_when_row_is_new() {
        let (_dir, mut db) = open_temp_db();
        let when = datetime!(2026-01-02 03:04:05 UTC);
        {
            let mut repo = IgnoredAppRepository::new(db.connection_mut());
            let row = repo
                .upsert(
                    "com.apple.Terminal",
                    Some("Terminal"),
                    Some("ignored-apps/com.apple.terminal.png"),
                    when,
                )
                .expect("upsert");
            assert_eq!(row.display_name.as_deref(), Some("Terminal"));
            assert_eq!(
                row.icon_ref.as_deref(),
                Some("ignored-apps/com.apple.terminal.png")
            );
        }
    }

    #[test]
    fn upsert_refreshes_metadata_without_duplicating() {
        // Re-selecting an existing application must not create a
        // duplicate row and must refresh the metadata.
        let (_dir, mut db) = open_temp_db();
        let when = datetime!(2026-01-02 03:04:05 UTC);
        {
            let mut repo = IgnoredAppRepository::new(db.connection_mut());
            repo.upsert("com.apple.Terminal", Some("Terminal"), None, when)
                .expect("first");
            repo.upsert(
                "com.apple.Terminal",
                Some("Terminal"),
                Some("ignored-apps/com.apple.terminal.png"),
                when,
            )
            .expect("second");
        }
        let repo = IgnoredAppRepository::new(db.connection_mut());
        assert_eq!(repo.count().unwrap(), 1);
        let row = repo.get("com.apple.Terminal").unwrap().expect("present");
        assert_eq!(
            row.icon_ref.as_deref(),
            Some("ignored-apps/com.apple.terminal.png")
        );
    }

    #[test]
    fn upsert_preserves_existing_metadata_when_payload_is_none() {
        // Icon failure on a subsequent selection must NOT blank out
        // the previously stored icon reference.
        let (_dir, mut db) = open_temp_db();
        let when = datetime!(2026-01-02 03:04:05 UTC);
        {
            let mut repo = IgnoredAppRepository::new(db.connection_mut());
            repo.upsert(
                "com.apple.Terminal",
                Some("Terminal"),
                Some("ignored-apps/com.apple.terminal.png"),
                when,
            )
            .expect("first");
            repo.upsert("com.apple.Terminal", Some("Terminal"), None, when)
                .expect("second (icon failed)");
        }
        let repo = IgnoredAppRepository::new(db.connection_mut());
        let row = repo.get("com.apple.Terminal").unwrap().expect("present");
        assert_eq!(
            row.icon_ref.as_deref(),
            Some("ignored-apps/com.apple.terminal.png")
        );
    }

    #[test]
    fn delete_returns_true_when_row_existed() {
        let (_dir, mut db) = open_temp_db();
        let when = datetime!(2026-01-02 03:04:05 UTC);
        {
            let mut repo = IgnoredAppRepository::new(db.connection_mut());
            repo.insert("firefox", when).expect("insert");
        }
        let removed = {
            let mut repo = IgnoredAppRepository::new(db.connection_mut());
            repo.delete("firefox").expect("delete")
        };
        assert!(removed);
    }

    #[test]
    fn delete_returns_false_when_absent() {
        let (_dir, mut db) = open_temp_db();
        let removed = {
            let mut repo = IgnoredAppRepository::new(db.connection_mut());
            repo.delete("ghost").expect("delete")
        };
        assert!(!removed);
    }

    #[test]
    fn list_returns_rows_sorted_by_id() {
        let (_dir, mut db) = open_temp_db();
        let when = datetime!(2026-01-02 03:04:05 UTC);
        {
            let mut repo = IgnoredAppRepository::new(db.connection_mut());
            for id in ["firefox", "com.apple.Terminal", "alacritty"] {
                repo.insert(id, when).expect("insert");
            }
        }
        let repo = IgnoredAppRepository::new(db.connection_mut());
        let rows = repo.list().unwrap();
        let ids: Vec<&str> = rows.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(
            ids,
            vec!["alacritty", "com.apple.Terminal", "firefox"],
            "rows must be sorted alphabetically"
        );
    }

    #[test]
    fn list_includes_metadata_columns() {
        // Migration 0006 must surface `display_name` and `icon_ref`
        // through the `list` API so the settings panel can render
        // the picker metadata without an extra round-trip.
        let (_dir, mut db) = open_temp_db();
        let when = datetime!(2026-01-02 03:04:05 UTC);
        {
            let mut repo = IgnoredAppRepository::new(db.connection_mut());
            repo.upsert(
                "com.apple.Terminal",
                Some("Terminal"),
                Some("ignored-apps/com.apple.terminal.png"),
                when,
            )
            .expect("upsert");
            repo.insert("firefox", when).expect("legacy insert");
        }
        let repo = IgnoredAppRepository::new(db.connection_mut());
        let rows = repo.list().unwrap();
        let terminal = rows
            .iter()
            .find(|r| r.id == "com.apple.Terminal")
            .expect("present");
        assert_eq!(terminal.display_name.as_deref(), Some("Terminal"));
        assert_eq!(
            terminal.icon_ref.as_deref(),
            Some("ignored-apps/com.apple.terminal.png")
        );
        let firefox = rows.iter().find(|r| r.id == "firefox").expect("present");
        assert!(firefox.display_name.is_none());
        assert!(firefox.icon_ref.is_none());
    }
}
