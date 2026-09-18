//! Embedded SQLite layer with explicit, idempotent migrations for ClipVault.
//!
//! This crate is intentionally thin: it owns the schema, the migrations and
//! a small connection-pool wrapper that the rest of the application uses to
//! talk to SQLite. Business rules live in `clipvault-core`; this layer does
//! not know about clipboard entries, search or favourites.

mod app_settings;
mod database;
mod entry;
mod entry_repository;
mod error;
mod ignored_apps;
mod known_peers;
mod migration;
mod organization;
mod registry;
pub mod source_app;

pub use app_settings::{AppSetting, AppSettingsError, AppSettingsRepository};
pub use database::{
    default_database_path, open_default, rollback_migration, Database, RollbackError,
};
pub use entry::{ContentType, EntryRecord, NewEntry, IMAGE_CONTENT_SENTINEL, IMAGE_MIME_PNG};
pub use entry_repository::{
    AggregatedSourceApp, AggregatedSourceApps, EntryOutcome, EntryRepository, EntryRepositoryError,
    SetCodeLanguageOutcome, SetFavoriteOutcome, SetSourceAppMetadataOutcome, SetTitleOutcome,
    UpdateTextOutcome, TEXTUAL_CONTENT_TYPES,
};
pub use error::{DbError, DbResult};
pub use ignored_apps::{IgnoredApp, IgnoredAppRepository, IgnoredAppsError};
pub use known_peers::{
    KnownPeer, KnownPeerRepository, KnownPeersError, PeerObservation, UpsertObservationOutcome,
};
pub use migration::{Migration, MigrationOutcome};
pub use organization::{
    normalise_tag_identity, validate_collection_color, validate_user_collection_name, Collection,
    CollectionKind, OrganizationError, OrganizationRepository, Tag, DEFAULT_COLLECTION_PALETTE,
    HISTORY_DEFAULT_COLOR_HEX, HISTORY_DISPLAY_NAME, HISTORY_STABLE_KEY,
    MAX_ORGANIZATION_NAME_CHARS,
};
pub use registry::builtin_migrations;
pub use source_app::SourceAppFilter;

pub const DEFAULT_FOLDER: &str = ".clipvault";
pub const DEFAULT_DB_FILE: &str = "clipvault.db";
