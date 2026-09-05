//! Key-value store for local application settings.
//!
//! The table is intentionally minimal: the `clipboard-management`
//! change needs the retention policy and the `privacy-settings`
//! capability will extend it. Both layers agree on the same
//! connection because they share the [`Database`].

use rusqlite::{params, Connection, OptionalExtension};
use thiserror::Error;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

#[derive(Debug, Error)]
pub enum AppSettingsError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

/// A single key-value row stored in `app_settings`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppSetting {
    pub key: String,
    pub value: String,
    pub updated_at: String,
}

pub struct AppSettingsRepository<'a> {
    conn: &'a mut Connection,
}

impl<'a> AppSettingsRepository<'a> {
    pub fn new(conn: &'a mut Connection) -> Self {
        Self { conn }
    }

    /// Fetch a single setting by key. Returns `None` when the key is
    /// absent so the caller can apply defaults.
    pub fn get(&self, key: &str) -> Result<Option<AppSetting>, AppSettingsError> {
        let record = self
            .conn
            .query_row(
                "SELECT key, value, updated_at FROM app_settings WHERE key = ?1",
                params![key],
                |row| {
                    Ok(AppSetting {
                        key: row.get(0)?,
                        value: row.get(1)?,
                        updated_at: row.get(2)?,
                    })
                },
            )
            .optional()?;
        Ok(record)
    }

    /// Insert or update a setting. The `updated_at` timestamp is set
    /// from the supplied clock so tests can pin it.
    pub fn set(
        &mut self,
        key: &str,
        value: &str,
        now: OffsetDateTime,
    ) -> Result<(), AppSettingsError> {
        let tx = self.conn.transaction()?;
        let updated_at = format_timestamp(now);
        tx.execute(
            "INSERT INTO app_settings (key, value, updated_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value = ?2, updated_at = ?3",
            params![key, value, updated_at],
        )?;
        tx.commit()?;
        Ok(())
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
    fn get_returns_none_for_missing_key() {
        let (_dir, mut db) = open_temp_db();
        let repo = AppSettingsRepository::new(db.connection_mut());
        assert!(repo.get("missing").unwrap().is_none());
    }

    #[test]
    fn set_then_get_round_trip() {
        let (_dir, mut db) = open_temp_db();
        let when = datetime!(2026-01-02 03:04:05 UTC);
        {
            let mut repo = AppSettingsRepository::new(db.connection_mut());
            repo.set("retention", "30d", when).expect("set");
        }
        let repo = AppSettingsRepository::new(db.connection_mut());
        let record = repo.get("retention").unwrap().expect("present");
        assert_eq!(record.key, "retention");
        assert_eq!(record.value, "30d");
    }

    #[test]
    fn set_overwrites_existing_value() {
        let (_dir, mut db) = open_temp_db();
        let t1 = datetime!(2026-01-02 03:04:05 UTC);
        let t2 = datetime!(2026-01-03 03:04:05 UTC);
        {
            let mut repo = AppSettingsRepository::new(db.connection_mut());
            repo.set("retention", "30d", t1).expect("first");
            repo.set("retention", "90d", t2).expect("second");
        }
        let repo = AppSettingsRepository::new(db.connection_mut());
        let record = repo.get("retention").unwrap().expect("present");
        assert_eq!(record.value, "90d");
    }
}
