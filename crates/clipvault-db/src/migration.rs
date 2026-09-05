use serde::Serialize;

/// A reversible SQLite migration.
#[derive(Debug, Clone)]
pub struct Migration {
    pub version: i64,
    pub description: &'static str,
    pub up_sql: &'static str,
    pub down_sql: &'static str,
}

/// Result of running a single migration through [`crate::Database::run_migrations`].
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MigrationOutcome {
    Applied { version: i64, description: String },
    AlreadyApplied { version: i64 },
}
