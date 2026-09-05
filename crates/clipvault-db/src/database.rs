use std::path::{Path, PathBuf};

use rusqlite::Connection;
use thiserror::Error;

use crate::error::{DbError, DbResult};
use crate::migration::{Migration, MigrationOutcome};
use crate::registry::builtin_migrations;
use crate::{DEFAULT_DB_FILE, DEFAULT_FOLDER};

/// Default location of the user database: `~/.clipvault/clipvault.db`.
pub fn default_database_path() -> DbResult<PathBuf> {
    let home = dirs::home_dir().ok_or(DbError::HomeDirectoryNotFound)?;
    Ok(home.join(DEFAULT_FOLDER).join(DEFAULT_DB_FILE))
}

/// Wrapper around a single SQLite connection that owns the schema and the
/// migration runner.
pub struct Database {
    conn: Connection,
    path: PathBuf,
}

impl Database {
    /// Open (and create if needed) the database at `path`.
    pub fn open(path: impl AsRef<Path>) -> DbResult<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|err| DbError::CreateDirectory {
                path: parent.to_path_buf(),
                source: err,
            })?;
        }

        let conn = Connection::open(&path)?;
        configure_connection(&conn)?;
        let mut db = Self { conn, path };
        db.ensure_schema_migrations_table()?;
        Ok(db)
    }

    /// Path to the SQLite file on disk.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Apply every pending migration from `migrations` in order.
    ///
    /// Returns the list of outcomes (one per migration in `migrations`,
    /// including the ones that were already applied).
    pub fn run_migrations(&mut self, migrations: &[Migration]) -> DbResult<Vec<MigrationOutcome>> {
        let applied = self.applied_versions()?;
        let mut outcomes = Vec::with_capacity(migrations.len());

        for migration in migrations {
            if applied.contains(&migration.version) {
                outcomes.push(MigrationOutcome::AlreadyApplied {
                    version: migration.version,
                });
                continue;
            }

            self.apply_migration(migration)?;
            outcomes.push(MigrationOutcome::Applied {
                version: migration.version,
                description: migration.description.to_string(),
            });
        }

        Ok(outcomes)
    }

    /// Number of migrations already recorded in `schema_migrations`.
    pub fn applied_migration_count(&self) -> DbResult<usize> {
        let count: i64 =
            self.conn
                .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
                    row.get(0)
                })?;
        Ok(count as usize)
    }

    /// Underlying connection for use by other crates that already depend on
    /// `clipvault-db`. Mutations must remain serialised through the owning
    /// `Database` instance.
    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    /// Mutable access to the underlying connection for crates that need to
    /// run their own transactions (e.g. the entry repository). Callers must
    /// hold the lock returned by [`crate::Database`]'s owner to keep writes
    /// serialised.
    pub fn connection_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }

    fn ensure_schema_migrations_table(&mut self) -> DbResult<()> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                version INTEGER PRIMARY KEY,
                description TEXT NOT NULL,
                applied_at TEXT NOT NULL
            )",
            [],
        )?;
        Ok(())
    }

    fn applied_versions(&self) -> DbResult<Vec<i64>> {
        let mut stmt = self
            .conn
            .prepare("SELECT version FROM schema_migrations ORDER BY version ASC")?;
        let rows = stmt.query_map([], |row| row.get::<_, i64>(0))?;
        let mut versions = Vec::new();
        for row in rows {
            versions.push(row?);
        }
        Ok(versions)
    }

    fn apply_migration(&mut self, migration: &Migration) -> DbResult<()> {
        let tx = self.conn.transaction()?;
        tx.execute_batch(migration.up_sql)
            .map_err(|source| DbError::MigrationFailed {
                version: migration.version,
                source: Box::new(source),
            })?;
        tx.execute(
            "INSERT INTO schema_migrations (version, description, applied_at) VALUES (?1, ?2, ?3)",
            (
                migration.version,
                &migration.description,
                current_timestamp(),
            ),
        )?;
        tx.commit().map_err(|source| DbError::MigrationFailed {
            version: migration.version,
            source: Box::new(source),
        })?;
        tracing::info!(version = migration.version, description = %migration.description, "applied migration");
        Ok(())
    }
}

fn configure_connection(conn: &Connection) -> DbResult<()> {
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;
         PRAGMA foreign_keys = ON;",
    )?;
    Ok(())
}

fn current_timestamp() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

/// Convenience constructor that opens the default `~/.clipvault/clipvault.db`
/// and applies the built-in migrations.
pub fn open_default() -> DbResult<Database> {
    let path = default_database_path()?;
    let mut db = Database::open(path)?;
    let migrations = builtin_migrations();
    db.run_migrations(&migrations)?;
    Ok(db)
}

/// Apply a single migration inside a transaction, exposing its `down_sql`.
///
/// Primarily used by tests to validate reversibility.
#[derive(Debug, Error)]
pub enum RollbackError {
    #[error(transparent)]
    Db(#[from] DbError),
}

pub fn rollback_migration(db: &mut Database, migration: &Migration) -> DbResult<()> {
    let conn = db.connection_mut();
    let tx = conn.transaction()?;
    tx.execute_batch(migration.down_sql)
        .map_err(|source| DbError::MigrationFailed {
            version: migration.version,
            source: Box::new(source),
        })?;
    tx.execute(
        "DELETE FROM schema_migrations WHERE version = ?1",
        [migration.version],
    )?;
    tx.commit().map_err(|source| DbError::MigrationFailed {
        version: migration.version,
        source: Box::new(source),
    })?;
    Ok(())
}
