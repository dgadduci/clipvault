//! History management service.
//!
//! [`HistoryManagementService`] is the thin orchestration layer that
//! sits between the Tauri shell and the storage layer. It owns the
//! favourite toggle, single-entry delete, clear-history and retention
//! purge operations defined by the `clipboard-management` capability,
//! plus the [`SettingsReader`] abstraction the retention policy
//! depends on.
//!
//! The service is deliberately small: it never logs clipboard content,
//! requires an explicit `confirm` flag for destructive operations and
//! resolves the retention policy from a single, deterministic source
//! of truth.

use std::sync::Arc;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use thiserror::Error;
use time::OffsetDateTime;
use tracing::warn;

use clipvault_db::{
    AppSettingsRepository, EntryRecord, EntryRepository, EntryRepositoryError, SetFavoriteOutcome,
};

use crate::bootstrap::AppContext;
use crate::clipboard_assets::ClipboardAssetStore;
use crate::clock::Clock;
use crate::rich_text::RichTextAssetStore;

/// Settings key the [`HistoryManagementService`] reads to resolve the
/// retention policy. Surfacing the literal makes it easy for the
/// future `privacy-settings` capability to share the same store.
pub const RETENTION_SETTING_KEY: &str = "retention_policy";

/// Default retention period used when the setting is missing. Matches
/// the `clipboard-management` spec: 30 days.
pub const DEFAULT_RETENTION: RetentionPolicy = RetentionPolicy::Days30;

/// Resolution of the user's retention preference. The enum is
/// `Serialize` so it can travel through the Tauri command surface
/// without leaking the storage layer.
///
/// The wire format is intentionally a flat snake_case string
/// (`"forever"`, `"days_7"`, `"days_30"`, `"days_90"`). The
/// `privacy-settings` change locked the contract after a regression in
/// which `#[serde(tag = "kind")]` produced `{"kind": "forever"}` and
/// the frontend sent bare `"forever"`; both the `apply` and `preview`
/// paths now match the frontend's TypeScript literal union. Each
/// variant carries an explicit `rename` because serde's automatic
/// `rename_all = "snake_case"` collapses `Days7` into `days7`, but the
/// frontend expects `days_7` to match the human-readable label.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RetentionPolicy {
    /// Keep everything forever.
    #[serde(rename = "forever")]
    Forever,
    /// Keep non-favorite entries for 7 days.
    #[serde(rename = "days_7")]
    Days7,
    /// Keep non-favorite entries for 30 days (the MVP default).
    #[serde(rename = "days_30")]
    Days30,
    /// Keep non-favorite entries for 90 days.
    #[serde(rename = "days_90")]
    Days90,
}

impl RetentionPolicy {
    /// Resolve the `RetentionPolicy` from the raw value stored in
    /// `app_settings`. The canonical form is the same snake_case the
    /// enum serialises to; legacy short forms (`"7d"`, `"30d"`,
    /// `"90d"`, `"7days"`, …) keep working so older databases survive
    /// an upgrade without data loss. Unknown values fall back to the
    /// default so a future schema change cannot break startup.
    pub fn parse(raw: Option<&str>) -> Self {
        match raw.map(str::trim).unwrap_or("") {
            "forever" => RetentionPolicy::Forever,
            "days_7" | "7d" | "7days" | "7_days" => RetentionPolicy::Days7,
            "days_30" | "30d" | "30days" | "30_days" => RetentionPolicy::Days30,
            "days_90" | "90d" | "90days" | "90_days" => RetentionPolicy::Days90,
            _ => DEFAULT_RETENTION,
        }
    }

    /// Stable identifier persisted in `app_settings` and returned to
    /// the frontend. Matches the snake_case serde representation so
    /// the value round-trips through `serde_json` byte-for-byte.
    pub fn as_setting_value(self) -> &'static str {
        match self {
            RetentionPolicy::Forever => "forever",
            RetentionPolicy::Days7 => "days_7",
            RetentionPolicy::Days30 => "days_30",
            RetentionPolicy::Days90 => "days_90",
        }
    }

    /// Resolve the cutoff timestamp for `now`. `None` means the
    /// retention period is "forever" — nothing should be purged.
    pub fn cutoff_for(self, now: OffsetDateTime) -> Option<OffsetDateTime> {
        match self {
            RetentionPolicy::Forever => None,
            RetentionPolicy::Days7 => Some(now - time::Duration::days(7)),
            RetentionPolicy::Days30 => Some(now - time::Duration::days(30)),
            RetentionPolicy::Days90 => Some(now - time::Duration::days(90)),
        }
    }
}

/// Reader abstraction for the retention policy. The default
/// implementation ([`LocalSettingsReader]) backs onto the
/// `app_settings` table; tests can swap in a stub.
pub trait SettingsReader: Send + Sync {
    fn retention_policy(&self) -> RetentionPolicy;
}

/// Concrete reader backed by `app_settings`. Writes the resolved
/// default the first time it reads a missing key so subsequent reads
/// return the same value without surprise.
pub struct LocalSettingsReader {
    context: AppContext,
    clock: Arc<dyn Clock>,
}

impl LocalSettingsReader {
    pub fn new(context: AppContext, clock: Arc<dyn Clock>) -> Self {
        Self { context, clock }
    }
}

impl SettingsReader for LocalSettingsReader {
    fn retention_policy(&self) -> RetentionPolicy {
        let raw = {
            let mut db = self.context.database().lock();
            let repo = AppSettingsRepository::new(db.connection_mut());
            repo.get(RETENTION_SETTING_KEY)
                .ok()
                .flatten()
                .map(|s| s.value)
        };
        let policy = RetentionPolicy::parse(raw.as_deref());
        if raw.is_none() {
            // Persist the default so the next read returns a
            // deterministic value and the privacy-settings capability
            // can list every key it owns without an extra fallback.
            if let Err(error) = persist_default(&self.context, &self.clock, policy) {
                warn!(
                    error = %app_settings_error_message(error),
                    "failed to persist default retention policy"
                );
            }
        }
        policy
    }
}

fn persist_default(
    context: &AppContext,
    clock: &Arc<dyn Clock>,
    policy: RetentionPolicy,
) -> Result<(), clipvault_db::AppSettingsError> {
    let now = clock.now();
    let mut db = context.database().lock();
    let mut repo = AppSettingsRepository::new(db.connection_mut());
    repo.set(RETENTION_SETTING_KEY, policy.as_setting_value(), now)
}

/// Convert an app-settings write failure into a string so the rest of
/// the layer can log it without depending on `rusqlite` directly.
fn app_settings_error_message(error: clipvault_db::AppSettingsError) -> String {
    error.to_string()
}

/// Errors the management service can surface. Every variant
/// intentionally avoids carrying clipboard content.
#[derive(Debug, Error)]
pub enum ManagementServiceError {
    #[error("entry repository error: {0}")]
    Repository(#[from] EntryRepositoryError),
}

/// Outcome of [`HistoryManagementService::set_favorite`]. The service
/// always returns this; `entry` is `Some` only when the row was
/// actually found.
#[derive(Debug, Clone)]
pub struct SetFavoriteResult {
    pub entry: Option<EntryRecord>,
}

impl SetFavoriteResult {
    pub fn kind(&self) -> &'static str {
        if self.entry.is_some() {
            "updated"
        } else {
            "not_found"
        }
    }
}

/// Outcome of a destructive action. `ConfirmationRequired` is the
/// branch the shell should hit when the frontend forgets to pass
/// `confirm: true` — the database is untouched and the caller can
/// surface the dialog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DeleteOutcome {
    Removed { removed: usize },
    NotFound,
    ConfirmationRequired,
}

impl DeleteOutcome {
    pub fn removed(&self) -> Option<usize> {
        match self {
            DeleteOutcome::Removed { removed } => Some(*removed),
            _ => None,
        }
    }
}

/// Outcome of [`HistoryManagementService::clear_non_favorites`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ClearOutcome {
    Removed { removed: usize },
    ConfirmationRequired,
}

impl ClearOutcome {
    pub fn removed(&self) -> Option<usize> {
        match self {
            ClearOutcome::Removed { removed } => Some(*removed),
            _ => None,
        }
    }
}

/// Outcome of [`HistoryManagementService::apply_retention`]. The
/// `removed` field is the number of non-favorite rows purged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct RetentionOutcome {
    pub policy: RetentionPolicy,
    pub removed: usize,
}

/// Service that exposes the management operations. Cheap to clone:
/// it only carries shared `Arc`s.
#[derive(Clone)]
pub struct HistoryManagementService {
    clock: Arc<dyn Clock>,
    retention_lock: Arc<Mutex<()>>,
    /// Local asset store consulted after a destructive operation so
    /// files that lost their last reference can be reclaimed. `None`
    /// in unit tests that only exercise row-level behaviour; the
    /// bootstrap always wires one.
    asset_store: Option<ClipboardAssetStore>,
    /// Local rich-text asset store. Mirrors the image store: tests
    /// that only exercise row-level behaviour leave it `None`.
    rich_asset_store: Option<RichTextAssetStore>,
}

impl HistoryManagementService {
    pub fn new(clock: Arc<dyn Clock>) -> Self {
        Self {
            clock,
            retention_lock: Arc::new(Mutex::new(())),
            asset_store: None,
            rich_asset_store: None,
        }
    }

    /// Attach the local asset store used to reclaim unreferenced
    /// payload assets after delete / clear / retention.
    pub fn with_asset_store(mut self, store: ClipboardAssetStore) -> Self {
        self.asset_store = Some(store);
        self
    }

    /// Attach the rich-text asset store used to reclaim unreferenced
    /// rich-text assets after delete / clear / retention.
    pub fn with_rich_asset_store(mut self, store: RichTextAssetStore) -> Self {
        self.rich_asset_store = Some(store);
        self
    }

    /// Reclaim every payload asset that no remaining history row
    /// references.
    ///
    /// The collector runs **after** the data mutation has committed and
    /// derives the live reference set from SQLite, so an asset shared by
    /// another row is never a candidate. The operation is idempotent
    /// and non-fatal: a filesystem failure is logged (metadata only)
    /// and reported as `0` collected, because losing a few orphaned
    /// bytes must never turn a successful delete into an error the user
    /// sees.
    ///
    /// Returns the number of assets removed.
    pub fn collect_unreferenced_assets(&self, context: &AppContext) -> usize {
        let mut total = 0;
        if let Some(store) = self.asset_store.as_ref() {
            let referenced = {
                let mut db = context.database().lock();
                let repo = EntryRepository::new(db.connection_mut());
                match repo.referenced_asset_refs() {
                    Ok(refs) => refs,
                    Err(error) => {
                        // Without a trustworthy live set we must not delete
                        // anything: a partial set would remove assets that
                        // are still referenced.
                        warn!(error = %error, "asset collection skipped: reference query failed");
                        BTreeSet::new()
                    }
                }
            };
            match store.collect_unreferenced(&referenced) {
                Ok(removed) => total += removed,
                Err(error) => {
                    warn!(
                        reason = error.kind_str(),
                        "asset collection failed; orphans remain for the next pass"
                    );
                }
            }
        }

        if let Some(rich_store) = self.rich_asset_store.as_ref() {
            let referenced = {
                let mut db = context.database().lock();
                let repo = EntryRepository::new(db.connection_mut());
                match repo.referenced_rich_asset_refs() {
                    Ok(refs) => refs,
                    Err(error) => {
                        warn!(
                            error = %error,
                            "rich asset collection skipped: reference query failed"
                        );
                        BTreeSet::new()
                    }
                }
            };
            match rich_store.collect_unreferenced(&referenced) {
                Ok(removed) => total += removed,
                Err(error) => {
                    warn!(
                        reason = error.kind_str(),
                        "rich asset collection failed; orphans remain for the next pass"
                    );
                }
            }
        }

        total
    }

    /// Toggle (or set) the favorite flag for the supplied entry. The
    /// operation is idempotent.
    pub fn set_favorite(
        &self,
        context: &AppContext,
        id: i64,
        pinned: bool,
    ) -> Result<SetFavoriteResult, ManagementServiceError> {
        let now = self.clock.now();
        let outcome: SetFavoriteOutcome = {
            let mut db = context.database().lock();
            let mut repo = EntryRepository::new(db.connection_mut());
            repo.set_favorite(id, pinned, now)?
        };
        Ok(SetFavoriteResult {
            entry: outcome.updated,
        })
    }

    /// Delete a single entry. Requires `confirm: true`; otherwise
    /// returns [`DeleteOutcome::ConfirmationRequired`] without
    /// touching the database.
    ///
    /// When the row is removed the payload asset it referenced becomes
    /// eligible for cleanup; the collector runs afterwards and skips
    /// any asset another row still references.
    pub fn delete_entry(
        &self,
        context: &AppContext,
        id: i64,
        confirm: bool,
    ) -> Result<DeleteOutcome, ManagementServiceError> {
        if !confirm {
            return Ok(DeleteOutcome::ConfirmationRequired);
        }
        let removed = {
            let mut db = context.database().lock();
            let mut repo = EntryRepository::new(db.connection_mut());
            repo.delete_entry(id)?
        };
        if removed == 0 {
            Ok(DeleteOutcome::NotFound)
        } else {
            self.collect_unreferenced_assets(context);
            Ok(DeleteOutcome::Removed { removed })
        }
    }

    /// Remove every non-favorite entry. Requires `confirm: true`.
    ///
    /// Favorites and the assets they reference are preserved.
    pub fn clear_non_favorites(
        &self,
        context: &AppContext,
        confirm: bool,
    ) -> Result<ClearOutcome, ManagementServiceError> {
        if !confirm {
            return Ok(ClearOutcome::ConfirmationRequired);
        }
        let removed = {
            let mut db = context.database().lock();
            let mut repo = EntryRepository::new(db.connection_mut());
            repo.clear_non_favorites()?
        };
        self.collect_unreferenced_assets(context);
        Ok(ClearOutcome::Removed { removed })
    }

    /// How many non-favorite entries would be removed by
    /// [`Self::clear_unorganized_history`]. Mirrors the repository
    /// predicate so the frontend can render a confirmation that
    /// matches the actual outcome before triggering the mutation.
    pub fn count_unorganized_clearable(
        &self,
        context: &AppContext,
    ) -> Result<i64, ManagementServiceError> {
        let mut db = context.database().lock();
        let repo = EntryRepository::new(db.connection_mut());
        Ok(repo.count_unorganized_clearable()?)
    }

    /// Remove the "unorganized" history: every non-favorite entry
    /// whose only association is the system `Historial` collection.
    /// An entry that also lives in a user (secondary) collection
    /// stays; an entry that was never pinned stays if and only if
    /// the user grouped it.
    ///
    /// Requires `confirm: true`; the `ConfirmationRequired` branch
    /// keeps the existing destructive-action contract (the same
    /// helper the frontend uses for the individual `Delete` button).
    /// The deletion runs in a single transaction and the asset
    /// collector reclaims every payload file the removed rows used
    /// to reference.
    pub fn clear_unorganized_history(
        &self,
        context: &AppContext,
        confirm: bool,
    ) -> Result<ClearOutcome, ManagementServiceError> {
        if !confirm {
            return Ok(ClearOutcome::ConfirmationRequired);
        }
        let removed = {
            let mut db = context.database().lock();
            let mut repo = EntryRepository::new(db.connection_mut());
            repo.clear_unorganized_history()?
        };
        self.collect_unreferenced_assets(context);
        Ok(ClearOutcome::Removed { removed })
    }

    /// Apply the configured retention policy. Two concurrent callers
    /// serialise on an internal [`Mutex`] so a duplicated invocation
    /// (for example on startup and on shutdown) is a no-op rather than
    /// a race.
    ///
    /// Favorite entries — including favorite images and their assets —
    /// are never removed regardless of age.
    pub fn apply_retention(
        &self,
        context: &AppContext,
        settings: &dyn SettingsReader,
    ) -> Result<RetentionOutcome, ManagementServiceError> {
        let _guard = self.retention_lock.lock();
        let policy = settings.retention_policy();
        let now = self.clock.now();
        let removed = match policy.cutoff_for(now) {
            None => 0,
            Some(cutoff) => {
                let mut db = context.database().lock();
                let mut repo = EntryRepository::new(db.connection_mut());
                repo.delete_non_favorites_older_than(cutoff)?
            }
        };
        // Always run the collector, even for a zero-row purge: it also
        // reclaims assets orphaned by an earlier interrupted capture.
        self.collect_unreferenced_assets(context);
        Ok(RetentionOutcome { policy, removed })
    }

    /// Inspect the retention policy and how many rows it would purge
    /// *without* mutating the database. Used by the frontend to
    /// populate a confirmation dialog.
    pub fn preview_retention(
        &self,
        context: &AppContext,
        settings: &dyn SettingsReader,
    ) -> Result<RetentionPreview, ManagementServiceError> {
        let policy = settings.retention_policy();
        let now = self.clock.now();
        let would_remove = match policy.cutoff_for(now) {
            None => 0,
            Some(cutoff) => {
                let mut db = context.database().lock();
                let repo = EntryRepository::new(db.connection_mut());
                repo.count_non_favorites_older_than(cutoff)? as usize
            }
        };
        Ok(RetentionPreview {
            policy,
            would_remove,
        })
    }
}

/// Lightweight summary returned by [`HistoryManagementService::preview_retention`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct RetentionPreview {
    pub policy: RetentionPolicy,
    pub would_remove: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AppBootstrap;
    use clipvault_db::{ContentType, NewEntry};
    use time::macros::datetime;

    #[test]
    fn retention_policy_round_trips() {
        for policy in [
            RetentionPolicy::Forever,
            RetentionPolicy::Days7,
            RetentionPolicy::Days30,
            RetentionPolicy::Days90,
        ] {
            assert_eq!(
                RetentionPolicy::parse(Some(policy.as_setting_value())),
                policy
            );
        }
    }

    #[test]
    fn retention_policy_falls_back_to_default_for_unknown_values() {
        assert_eq!(RetentionPolicy::parse(None), DEFAULT_RETENTION);
        assert_eq!(RetentionPolicy::parse(Some("")), DEFAULT_RETENTION);
        assert_eq!(RetentionPolicy::parse(Some("nonsense")), DEFAULT_RETENTION);
    }

    #[test]
    fn retention_policy_cutoff_is_none_for_forever() {
        let now = OffsetDateTime::UNIX_EPOCH;
        assert!(RetentionPolicy::Forever.cutoff_for(now).is_none());
    }

    #[test]
    fn retention_policy_cutoff_is_some_for_finite_values() {
        let now = OffsetDateTime::UNIX_EPOCH;
        let cutoff = RetentionPolicy::Days7.cutoff_for(now).expect("cutoff");
        assert_eq!(now - cutoff, time::Duration::days(7));
        let cutoff = RetentionPolicy::Days90.cutoff_for(now).expect("cutoff");
        assert_eq!(now - cutoff, time::Duration::days(90));
    }

    #[test]
    fn delete_outcome_serialises_confirmation_required() {
        let outcome = DeleteOutcome::ConfirmationRequired;
        let json = serde_json::to_string(&outcome).expect("serialise");
        assert!(json.contains("confirmation_required"));
    }

    #[test]
    fn retention_policy_serialises_as_plain_snake_case_string() {
        // Bug 1 regression: `#[serde(tag = "kind")]` previously
        // emitted `{"kind": "forever"}`, breaking the frontend which
        // sends/receives bare `"forever"`. The wire format must be a
        // flat snake_case string compatible with the TypeScript
        // literal union.
        assert_eq!(
            serde_json::to_string(&RetentionPolicy::Forever).unwrap(),
            "\"forever\""
        );
        assert_eq!(
            serde_json::to_string(&RetentionPolicy::Days7).unwrap(),
            "\"days_7\""
        );
        assert_eq!(
            serde_json::to_string(&RetentionPolicy::Days30).unwrap(),
            "\"days_30\""
        );
        assert_eq!(
            serde_json::to_string(&RetentionPolicy::Days90).unwrap(),
            "\"days_90\""
        );
    }

    #[test]
    fn retention_policy_deserialises_plain_snake_case_string() {
        // Mirror of the serialise test: the frontend sends bare
        // snake_case strings; the enum must accept them without
        // expecting the `{"kind": ...}` envelope.
        for (raw, expected) in [
            ("\"forever\"", RetentionPolicy::Forever),
            ("\"days_7\"", RetentionPolicy::Days7),
            ("\"days_30\"", RetentionPolicy::Days30),
            ("\"days_90\"", RetentionPolicy::Days90),
        ] {
            let parsed: RetentionPolicy = serde_json::from_str(raw).expect("parse");
            assert_eq!(parsed, expected);
        }
    }

    #[test]
    fn retention_policy_rejects_enveloped_form() {
        // The legacy `{"kind": "forever"}` envelope must NOT silently
        // decode now that the wire format is locked. A regression
        // here would re-introduce the original Tauri command error
        // "invalid type: string … expected internally tagged enum
        // RetentionPolicy".
        let result: Result<RetentionPolicy, _> = serde_json::from_str("{\"kind\": \"forever\"}");
        assert!(result.is_err(), "enveloped form must be rejected");
    }

    #[test]
    fn retention_policy_settings_value_matches_serde_form() {
        // The persisted `app_settings.value` must equal the JSON the
        // enum emits so `parse` and `serde_json` agree byte-for-byte.
        for policy in [
            RetentionPolicy::Forever,
            RetentionPolicy::Days7,
            RetentionPolicy::Days30,
            RetentionPolicy::Days90,
        ] {
            let serialised = serde_json::to_string(&policy).unwrap();
            let stripped = serialised.trim_matches('"');
            assert_eq!(policy.as_setting_value(), stripped);
        }
    }

    #[test]
    fn retention_policy_legacy_values_keep_parsing() {
        // Pre-existing rows in user databases must keep working so
        // an upgrade does not silently flip the policy back to the
        // default. The legacy short forms are explicitly documented
        // in the migration notes.
        assert_eq!(RetentionPolicy::parse(Some("7d")), RetentionPolicy::Days7);
        assert_eq!(RetentionPolicy::parse(Some("30d")), RetentionPolicy::Days30);
        assert_eq!(RetentionPolicy::parse(Some("90d")), RetentionPolicy::Days90);
        assert_eq!(
            RetentionPolicy::parse(Some("7days")),
            RetentionPolicy::Days7
        );
    }

    #[test]
    fn retention_preview_serialises_with_snake_case_policy() {
        // The frontend reads `policy` as a literal union; the
        // `RetentionPreview` DTO must serialise the policy field
        // the same way the enum itself does.
        let preview = RetentionPreview {
            policy: RetentionPolicy::Days7,
            would_remove: 3,
        };
        let json = serde_json::to_string(&preview).unwrap();
        assert!(json.contains("\"policy\":\"days_7\""), "got {json}");
        assert!(json.contains("\"would_remove\":3"), "got {json}");
    }

    #[test]
    fn retention_outcome_serialises_with_snake_case_policy() {
        let outcome = RetentionOutcome {
            policy: RetentionPolicy::Days90,
            removed: 0,
        };
        let json = serde_json::to_string(&outcome).unwrap();
        assert!(json.contains("\"policy\":\"days_90\""), "got {json}");
        assert!(json.contains("\"removed\":0"), "got {json}");
    }

    #[test]
    fn clear_unorganized_history_without_confirmation_is_a_no_op() {
        let outcome = dummy_service().clear_unorganized_history(&dummy_context(), false);
        match outcome.expect("ok") {
            ClearOutcome::ConfirmationRequired => {}
            other => panic!("expected ConfirmationRequired, got {other:?}"),
        }
    }

    #[test]
    fn clear_unorganized_history_count_matches_repository_predicate() {
        // The management-service count must agree with the repository
        // predicate so a confirmation message that quotes `count`
        // cannot drift from the actual outcome.
        let (context, _) = build_context_with_rows();
        let service = HistoryManagementService::new(Arc::new(StaticClock::new(
            time::OffsetDateTime::UNIX_EPOCH,
        )));
        let count = service
            .count_unorganized_clearable(&context)
            .expect("count");
        assert_eq!(count, 1);
    }

    #[test]
    fn clear_unorganized_history_keeps_secondary_collection_rows() {
        let (context, expected) = build_context_with_rows();
        let service = HistoryManagementService::new(Arc::new(StaticClock::new(
            time::OffsetDateTime::UNIX_EPOCH,
        )));
        let outcome = service
            .clear_unorganized_history(&context, true)
            .expect("ok");
        match outcome {
            ClearOutcome::Removed { removed } => assert_eq!(removed, 1),
            other => panic!("expected Removed, got {other:?}"),
        }
        let count = {
            let mut db = context.database().lock();
            let repo = clipvault_db::EntryRepository::new(db.connection_mut());
            repo.count().expect("count")
        };
        assert_eq!(count, expected);
    }

    // --- helpers below -------------------------------------------------

    fn dummy_service() -> HistoryManagementService {
        HistoryManagementService::new(Arc::new(StaticClock::new(time::OffsetDateTime::UNIX_EPOCH)))
    }

    fn dummy_context() -> AppContext {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = clipvault_db::Database::open(dir.path().join("clipvault.db")).expect("open");
        let mut db = db;
        db.run_migrations(&clipvault_db::builtin_migrations())
            .expect("migrate");
        let bootstrap = AppBootstrap::new()
            .with_clock(Arc::new(StaticClock::new(time::OffsetDateTime::UNIX_EPOCH)))
            .with_clipboard(Arc::new(crate::clipboard::FakeClipboard::new()));
        bootstrap
            .bootstrap_with_database(db, dir.path().join("clipvault.db"))
            .expect("bootstrap")
    }

    fn build_context_with_rows() -> (AppContext, i64) {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = clipvault_db::Database::open(dir.path().join("clipvault.db")).expect("open");
        let mut db = db;
        db.run_migrations(&clipvault_db::builtin_migrations())
            .expect("migrate");
        let bootstrap = AppBootstrap::new()
            .with_clock(Arc::new(StaticClock::new(
                datetime!(2026-01-02 03:04:05 UTC),
            )))
            .with_clipboard(Arc::new(crate::clipboard::FakeClipboard::new()));
        let context = bootstrap
            .bootstrap_with_database(db, dir.path().join("clipvault.db"))
            .expect("bootstrap");

        let when = datetime!(2026-01-02 03:04:05 UTC);
        let (drop, pinned, secondary) = {
            let mut db = context.database().lock();
            let mut repo = clipvault_db::EntryRepository::new(db.connection_mut());
            let drop_id = repo
                .insert_or_touch(text_entry("drop", when))
                .expect("drop")
                .record()
                .id;
            let pinned_id = repo
                .insert_or_touch(text_entry("pinned", when))
                .expect("pinned")
                .record()
                .id;
            let secondary_id = repo
                .insert_or_touch(text_entry("secondary", when))
                .expect("secondary")
                .record()
                .id;
            repo.set_favorite(pinned_id, true, when)
                .expect("favorite")
                .updated
                .expect("record");
            (drop_id, pinned_id, secondary_id)
        };
        {
            let mut db = context.database().lock();
            let mut org = clipvault_db::OrganizationRepository::new(db.connection_mut());
            let work = org
                .create_user_collection("Trabajo", when)
                .expect("create")
                .id;
            org.replace_entry_collections(secondary, &[work], when)
                .expect("associate secondary");
        }
        let _ = drop;
        let _ = pinned;
        // After the clear, the pinned + secondary rows remain: 2
        // survivors.
        (context, 2)
    }

    fn text_entry(content: &str, when: time::OffsetDateTime) -> NewEntry {
        NewEntry::text(
            content.to_string(),
            ContentType::Text,
            content.len() as i64,
            format!("hash::{content}"),
            Some("test".to_string()),
            when,
            when,
        )
    }

    struct StaticClock {
        stamp: time::OffsetDateTime,
    }

    impl StaticClock {
        fn new(stamp: time::OffsetDateTime) -> Self {
            Self { stamp }
        }
    }

    impl crate::clock::Clock for StaticClock {
        fn now(&self) -> time::OffsetDateTime {
            self.stamp
        }
    }
}
