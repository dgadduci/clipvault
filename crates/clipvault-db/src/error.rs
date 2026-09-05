use std::path::PathBuf;

use thiserror::Error;

pub type DbResult<T> = Result<T, DbError>;

#[derive(Debug, Error)]
pub enum DbError {
    #[error("could not resolve the user's home directory")]
    HomeDirectoryNotFound,

    #[error("could not create directory {path}: {source}")]
    CreateDirectory {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("migration {version} failed: {source}")]
    MigrationFailed {
        version: i64,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
}
