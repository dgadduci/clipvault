//! Diagnostics reported by the bootstrap command.
//!
//! Reports the bootstrap state plus the size of the captured text
//! history so the frontend can verify that the capture pipeline is
//! producing rows. The `capabilities` block surfaces which desktop
//! integrations are available on the current host.

use serde::Serialize;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use clipvault_db::Database;
use clipvault_platform::Capabilities;

use crate::bootstrap::AppContext;

#[derive(Debug, Clone, Serialize)]
pub struct Diagnostics {
    pub version: String,
    pub database_path: String,
    pub migrations_applied: usize,
    pub started_at: String,
    pub platform_os: String,
    pub display_server: String,
    pub history_entries: i64,
    pub capabilities: Capabilities,
}

#[derive(Debug, Clone, Serialize)]
pub struct DatabasePath {
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MigrationsApplied {
    pub count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct HistoryCount {
    pub count: i64,
}

pub struct DiagnosticsService;

impl DiagnosticsService {
    pub fn snapshot(context: &AppContext) -> Diagnostics {
        let db = context.database();
        let database_path = db_path_string(&db.lock());
        let migrations_applied = count_applied(&db.lock()).unwrap_or(0);
        let history_entries = context.history().history_count(context).unwrap_or(0);

        Diagnostics {
            version: context.version().to_string(),
            database_path,
            migrations_applied,
            started_at: format_timestamp(context.started_at()),
            platform_os: context.platform().os_family.to_string(),
            display_server: context.platform().display_server.to_string(),
            history_entries,
            capabilities: context.capabilities(),
        }
    }

    pub fn database_path(context: &AppContext) -> DatabasePath {
        DatabasePath {
            path: db_path_string(&context.database().lock()),
        }
    }

    pub fn migrations_applied(context: &AppContext) -> MigrationsApplied {
        let count = count_applied(&context.database().lock()).unwrap_or(0);
        MigrationsApplied { count }
    }

    pub fn history_count(context: &AppContext) -> HistoryCount {
        let count = context.history().history_count(context).unwrap_or(0);
        HistoryCount { count }
    }
}

fn db_path_string(db: &Database) -> String {
    db.path().display().to_string()
}

fn count_applied(db: &Database) -> clipvault_db::DbResult<usize> {
    db.applied_migration_count()
}

fn format_timestamp(ts: OffsetDateTime) -> String {
    ts.format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

impl Diagnostics {
    /// Convenience used by the Tauri command layer.
    pub fn from_context(context: &AppContext) -> Self {
        DiagnosticsService::snapshot(context)
    }
}
