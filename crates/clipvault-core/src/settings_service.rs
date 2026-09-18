//! Settings service: persists the [`Settings`] aggregate to `app_settings`
//! and propagates blacklist updates to the [`PrivacyGate`].
//!
//! The service is the only layer that knows how to translate between
//! the typed [`Settings`] aggregate and the key/value rows in
//! `app_settings`. Tauri commands and tests consume it through
//! [`AppContext`](crate::bootstrap::AppContext).

use std::sync::Arc;

use clipvault_db::{AppSettingsRepository, IgnoredAppRepository};
use thiserror::Error;

use crate::bootstrap::AppContext;
use crate::clock::Clock;
use crate::management::{RetentionPolicy, RETENTION_SETTING_KEY};
use crate::peer_discovery::PeerDiscoveryRuntime;
use crate::peer_identity::{
    LocalPeerIdentity, LocalPeerProfile, PeerIdentityOutcome, PeerIdentityService,
};
use crate::privacy::PrivacyGate;
use crate::settings::{
    local_peer_sharing_enabled_value, parse_local_peer_sharing_enabled, HotkeySpec, Settings,
    SettingsUpdate, ValidationError, HOTKEY_SETTING_STORAGE_KEY, LOCAL_PEER_DISPLAY_NAME_KEY,
    LOCAL_PEER_SHARING_ENABLED_KEY,
};

#[derive(Debug, Error)]
pub enum SettingsServiceError {
    #[error("validation error: {0}")]
    Validation(#[from] ValidationError),
    #[error("app settings error: {0}")]
    AppSettings(#[from] clipvault_db::AppSettingsError),
    #[error("ignored apps error: {0}")]
    IgnoredApps(#[from] clipvault_db::IgnoredAppsError),
}

#[derive(Clone)]
pub struct SettingsService {
    clock: Arc<dyn Clock>,
    privacy_gate: PrivacyGate,
    peer_identity: PeerIdentityService,
}

impl SettingsService {
    /// Build a settings service backed by the supplied clock and
    /// privacy gate. The peer identity service defaults to
    /// [`PeerIdentityService::unavailable`], which always reports
    /// [`PeerIdentityOutcome::Unavailable`]: a fresh `SettingsService`
    /// never silently mints an in-memory identity, so the shell
    /// never advertises a `peer_id` it cannot reload on the next
    /// boot.
    ///
    /// Production shells that wire the platform keychain must call
    /// [`Self::with_peer_identity_service`] before handing the
    /// service to the bootstrap.
    pub fn new(clock: Arc<dyn Clock>, privacy_gate: PrivacyGate) -> Self {
        Self {
            clock,
            privacy_gate,
            peer_identity: PeerIdentityService::unavailable(),
        }
    }

    /// Inject a peer identity service. Used by the bootstrap to
    /// wire the production keychain-backed implementation; tests
    /// pass a deterministic fake so the Identity section can be
    /// exercised without standing up a real `keyring` backend.
    pub fn with_peer_identity_service(mut self, service: PeerIdentityService) -> Self {
        self.peer_identity = service;
        self
    }

    /// Handle to the underlying [`PeerIdentityService`]. The Tauri
    /// command surface uses it to render the Identity section
    /// without re-loading the identity through the database. The
    /// accessor returns a clone so callers cannot mutate the
    /// service field.
    pub fn peer_identity(&self) -> PeerIdentityService {
        self.peer_identity.clone()
    }

    /// Load the effective [`Settings`] aggregate from `app_settings` and
    /// the `ignored_apps` table. Missing values fall back to
    /// [`Settings::defaults`].
    pub fn load(&self, context: &AppContext) -> Settings {
        let mut settings = Settings::defaults();
        let mut db = context.database().lock();

        let retention_raw = {
            let repo = AppSettingsRepository::new(db.connection_mut());
            repo.get(RETENTION_SETTING_KEY)
                .ok()
                .flatten()
                .map(|s| s.value)
        };
        settings.retention = RetentionPolicy::parse(retention_raw.as_deref());

        let hotkey_raw = {
            let repo = AppSettingsRepository::new(db.connection_mut());
            repo.get(HOTKEY_SETTING_STORAGE_KEY)
                .ok()
                .flatten()
                .map(|s| s.value)
        };
        settings.quick_paste_hotkey = match HotkeySpec::parse(hotkey_raw.as_deref()) {
            Some(Ok(spec)) => Some(spec),
            Some(Err(_)) => None,
            None => None,
        };

        let display_name_raw = {
            let repo = AppSettingsRepository::new(db.connection_mut());
            repo.get(LOCAL_PEER_DISPLAY_NAME_KEY)
                .ok()
                .flatten()
                .map(|s| s.value)
        };
        // Re-validate on read so a manually edited / corrupt
        // `app_settings` row never ends up surfaced to the UI as
        // raw bytes; the loader silently falls back to `None` and
        // the user can re-save a clean value through the panel.
        settings.local_peer_display_name = display_name_raw
            .as_deref()
            .and_then(|raw| crate::settings::validate_peer_display_name(raw).ok());

        let sharing_enabled_raw = {
            let repo = AppSettingsRepository::new(db.connection_mut());
            repo.get(LOCAL_PEER_SHARING_ENABLED_KEY)
                .ok()
                .flatten()
                .map(|s| s.value)
        };
        // The parser collapses anything that is not the literal
        // `true` (case-insensitive, trimmed) to `false` so a
        // manually edited row can never accidentally enable
        // sharing — the contract the `local-peer-discovery` spec
        // pins for the opt-in toggle.
        settings.local_peer_sharing_enabled =
            parse_local_peer_sharing_enabled(sharing_enabled_raw.as_deref());

        let ignored = {
            let repo = IgnoredAppRepository::new(db.connection_mut());
            repo.list()
                .map(|rows| rows.into_iter().map(|r| r.id).collect::<Vec<_>>())
                .unwrap_or_default()
        };
        settings.ignored_apps = ignored;

        settings
    }

    /// Validate the update and persist the resulting aggregate. When
    /// the blacklist changes, the [`PrivacyGate`] is updated
    /// atomically so subsequent captures see the new snapshot.
    pub fn apply(
        &self,
        context: &AppContext,
        update: &SettingsUpdate,
    ) -> Result<Settings, SettingsServiceError> {
        let current = self.load(context);
        let next = update
            .validate(&current)
            .map_err(SettingsServiceError::Validation)?;

        let now = self.clock.now();
        let mut db = context.database().lock();

        // Retention.
        let policy_changed = next.retention != current.retention;
        if policy_changed {
            let conn = db.connection_mut();
            let mut repo = AppSettingsRepository::new(conn);
            repo.set(
                RETENTION_SETTING_KEY,
                next.retention.as_setting_value(),
                now,
            )?;
        }

        // Hotkey.
        let hotkey_changed = next.quick_paste_hotkey != current.quick_paste_hotkey;
        if hotkey_changed {
            let conn = db.connection_mut();
            let mut repo = AppSettingsRepository::new(conn);
            let value = next
                .quick_paste_hotkey
                .as_ref()
                .map(|s| s.as_setting_value());
            match value {
                Some(v) if !v.is_empty() => repo.set(HOTKEY_SETTING_STORAGE_KEY, &v, now)?,
                _ => repo.set(HOTKEY_SETTING_STORAGE_KEY, "", now)?,
            }
        }

        // Local peer display name.
        let display_name_changed = next.local_peer_display_name != current.local_peer_display_name;
        if display_name_changed {
            let conn = db.connection_mut();
            let mut repo = AppSettingsRepository::new(conn);
            match next.local_peer_display_name.as_deref() {
                Some(value) if !value.is_empty() => {
                    repo.set(LOCAL_PEER_DISPLAY_NAME_KEY, value, now)?;
                }
                _ => {
                    // Empty / cleared values are persisted as an
                    // empty string so the next `load()` does not
                    // silently resurrect the previous name from a
                    // deleted-but-orphan row.
                    repo.set(LOCAL_PEER_DISPLAY_NAME_KEY, "", now)?;
                }
            }
        }

        // Local peer sharing toggle.
        let sharing_enabled_changed =
            next.local_peer_sharing_enabled != current.local_peer_sharing_enabled;
        if sharing_enabled_changed {
            let conn = db.connection_mut();
            let mut repo = AppSettingsRepository::new(conn);
            repo.set(
                LOCAL_PEER_SHARING_ENABLED_KEY,
                local_peer_sharing_enabled_value(next.local_peer_sharing_enabled),
                now,
            )?;
        }

        // Ignored apps.
        for id in &update.ignored_apps_remove {
            let conn = db.connection_mut();
            let mut repo = IgnoredAppRepository::new(conn);
            let _ = repo.delete(id)?;
        }
        for id in &update.ignored_apps_add {
            let conn = db.connection_mut();
            let mut repo = IgnoredAppRepository::new(conn);
            repo.insert(id, now)?;
        }

        // Refresh the privacy gate with the new snapshot.
        let snapshot = next
            .ignored_apps
            .iter()
            .map(|id| crate::privacy::normalize(id))
            .filter(|id| !id.is_empty())
            .collect();
        self.privacy_gate.update_ignored(snapshot);

        drop(db);
        Ok(next)
    }

    /// Add a single identifier to the blacklist with validation.
    pub fn add_ignored(
        &self,
        context: &AppContext,
        id: &str,
    ) -> Result<Settings, SettingsServiceError> {
        let update = SettingsUpdate {
            ignored_apps_add: vec![id.to_string()],
            ..SettingsUpdate::default()
        };
        self.apply(context, &update)
    }

    /// Remove a single identifier from the blacklist.
    pub fn remove_ignored(
        &self,
        context: &AppContext,
        id: &str,
    ) -> Result<Settings, SettingsServiceError> {
        let update = SettingsUpdate {
            ignored_apps_remove: vec![id.to_string()],
            ..SettingsUpdate::default()
        };
        self.apply(context, &update)
    }

    /// Update the retention policy with validation.
    pub fn set_retention(
        &self,
        context: &AppContext,
        policy: RetentionPolicy,
    ) -> Result<Settings, SettingsServiceError> {
        let update = SettingsUpdate {
            retention: Some(policy),
            ..SettingsUpdate::default()
        };
        self.apply(context, &update)
    }

    /// Store the quick-paste hotkey. `None` clears the binding.
    pub fn set_hotkey(
        &self,
        context: &AppContext,
        hotkey: Option<HotkeySpec>,
    ) -> Result<Settings, SettingsServiceError> {
        let update = SettingsUpdate {
            quick_paste_hotkey: Some(hotkey),
            ..SettingsUpdate::default()
        };
        self.apply(context, &update)
    }

    /// Persist the validated local peer display name. The value
    /// goes through [`crate::settings::validate_peer_display_name`]
    /// first; invalid input is surfaced as
    /// [`SettingsServiceError::Validation`] without persisting or
    /// logging the rejected bytes.
    pub fn set_local_peer_display_name(
        &self,
        context: &AppContext,
        name: Option<&str>,
    ) -> Result<Settings, SettingsServiceError> {
        let update = SettingsUpdate {
            local_peer_display_name: Some(name.map(str::to_string)),
            ..SettingsUpdate::default()
        };
        self.apply(context, &update)
    }

    /// Persist the opt-in `Compartir en red local` toggle. The
    /// shell owns the identity prerequisite check (it surfaces a
    /// typed `Unavailable` outcome through the existing
    /// [`Self::local_peer_profile`] helper); the service stays a
    /// thin validator and never inspects the secure store. A
    /// `false` value is always accepted: turning sharing off is
    /// allowed even when the identity foundation is unavailable so
    /// the user is never trapped behind an opt-in they cannot
    /// undo.
    pub fn set_local_peer_sharing_enabled(
        &self,
        context: &AppContext,
        enabled: bool,
    ) -> Result<Settings, SettingsServiceError> {
        let update = SettingsUpdate {
            local_peer_sharing_enabled: Some(enabled),
            ..SettingsUpdate::default()
        };
        self.apply(context, &update)
    }

    /// Synchronise the runtime with the secure store: the shell
    /// calls this every time it loads the local profile so the
    /// runtime can self-filter events with the canonical
    /// `peer_id` / fingerprint. The runtime's `start` returns
    /// `IdentityUnavailable` until the foundation is reachable, so
    /// passing `None` here is always safe — the runtime surfaces
    /// the typed `runtime_stopped` reason to the UI without
    /// accepting an observation.
    pub fn sync_runtime_local_identity(
        &self,
        context: &AppContext,
        identity: Option<&LocalPeerIdentity>,
    ) {
        let display_name = context
            .settings()
            .load(context)
            .local_peer_display_name
            .clone();
        context.refresh_peer_discovery_local_identity(identity, display_name.as_deref());
    }

    /// Drive the discovery runtime in lockstep with the persisted
    /// toggle. The shell calls this after every settings update
    /// and on bootstrap so the runtime is `running` whenever the
    /// toggle is on (and an identity is reachable) and stopped
    /// otherwise. The runtime is idempotent: a redundant `start`
    /// is a no-op so the shell can call this on every bootstrap
    /// path without coordinating state.
    pub fn sync_runtime_with_settings(
        &self,
        context: &AppContext,
        runtime: &PeerDiscoveryRuntime,
        settings: &Settings,
    ) {
        if !settings.local_peer_sharing_enabled {
            // Best-effort: the noop adapter is a no-op on `stop`,
            // so a missing mDNS backend never blocks the toggle.
            let _ = runtime.stop();
            return;
        }
        // Sharing is on: gate the runtime on a reachable identity
        // so the UI never announces itself with an unverifiable
        // `peer_id`. `start` is idempotent.
        if let PeerIdentityOutcome::Ok(identity) = self.peer_identity.load_identity() {
            context.refresh_peer_discovery_local_identity(
                Some(&identity),
                settings.local_peer_display_name.as_deref(),
            );
            let _ = runtime.start();
        } else {
            context.refresh_peer_discovery_local_identity(None, None);
            let _ = runtime.stop();
        }
    }

    /// Metadata-only local peer profile merging the cryptographic
    /// identity (loaded from the secure store) with the persisted
    /// display name. The shell calls this to render the Identity
    /// section of Settings. Returns [`PeerIdentityOutcome::Unavailable`]
    /// when the platform secure store cannot satisfy the request;
    /// the caller maps the outcome to a stable user-facing copy
    /// without leaking the underlying platform error.
    pub fn local_peer_profile(
        &self,
        context: &AppContext,
    ) -> PeerIdentityOutcome<LocalPeerProfile> {
        let display_name = self
            .load(context)
            .local_peer_display_name
            .and_then(|value| {
                let trimmed = value.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            });
        self.peer_identity.profile(display_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_service() -> SettingsService {
        use crate::clock::SystemClock;
        use crate::peer_identity::{InMemoryPeerIdentityStore, PeerIdentityService};
        use clipvault_platform::NoopActiveApplicationProbe;
        use std::sync::Arc;

        let probe = Arc::new(NoopActiveApplicationProbe);
        let matcher = crate::privacy::CoreBlacklistMatcher::with_ignored(probe, vec![]);
        let gate = crate::privacy::PrivacyGate::new(matcher);
        SettingsService::new(Arc::new(SystemClock), gate).with_peer_identity_service(
            PeerIdentityService::new(Arc::new(InMemoryPeerIdentityStore::new())),
        )
    }

    #[test]
    fn validation_round_trips_for_minimal_update() {
        let service = minimal_service();
        let _ = service;
        let update = SettingsUpdate {
            retention: Some(RetentionPolicy::Days7),
            ..SettingsUpdate::default()
        };
        let current = Settings::defaults();
        let next = update.validate(&current).expect("valid");
        assert_eq!(next.retention, RetentionPolicy::Days7);
    }

    #[test]
    fn set_local_peer_display_name_round_trips_through_persistence() {
        use crate::peer_identity::InMemoryPeerIdentityStore;
        use crate::test_support::isolated_harness;
        use std::sync::Arc;

        let (_dir, context) = isolated_harness();
        let clock: Arc<dyn crate::clock::Clock> = Arc::new(crate::clock::SystemClock);
        let probe = Arc::new(clipvault_platform::NoopActiveApplicationProbe);
        let matcher = crate::privacy::CoreBlacklistMatcher::with_ignored(probe, vec![]);
        let gate = crate::privacy::PrivacyGate::new(matcher);
        let service = SettingsService::new(clock, gate).with_peer_identity_service(
            crate::peer_identity::PeerIdentityService::new(Arc::new(
                InMemoryPeerIdentityStore::new(),
            )),
        );
        let stored = service
            .set_local_peer_display_name(&context, Some("Studio"))
            .expect("set");
        assert_eq!(stored.local_peer_display_name.as_deref(), Some("Studio"));
        let reloaded = service.load(&context);
        assert_eq!(reloaded.local_peer_display_name.as_deref(), Some("Studio"));
    }

    #[test]
    fn set_local_peer_display_name_rejects_empty_value() {
        use crate::peer_identity::InMemoryPeerIdentityStore;
        use crate::test_support::isolated_harness;
        use std::sync::Arc;

        let (_dir, context) = isolated_harness();
        let clock: Arc<dyn crate::clock::Clock> = Arc::new(crate::clock::SystemClock);
        let probe = Arc::new(clipvault_platform::NoopActiveApplicationProbe);
        let matcher = crate::privacy::CoreBlacklistMatcher::with_ignored(probe, vec![]);
        let gate = crate::privacy::PrivacyGate::new(matcher);
        let service = SettingsService::new(clock, gate).with_peer_identity_service(
            crate::peer_identity::PeerIdentityService::new(Arc::new(
                InMemoryPeerIdentityStore::new(),
            )),
        );
        let err = service
            .set_local_peer_display_name(&context, Some("   "))
            .expect_err("empty name must be rejected");
        assert!(matches!(err, SettingsServiceError::Validation(_)));
    }

    /// The 2-arg constructor MUST produce a service whose identity
    /// path always reports `Unavailable`. The bootstrap wires a real
    /// `PeerIdentityService` through the builder; this test pins the
    /// default the rest of the suite relies on.
    #[test]
    fn default_constructor_yields_unavailable_identity_outcome() {
        use crate::peer_identity::PeerIdentityOutcome;
        use clipvault_platform::NoopActiveApplicationProbe;
        use std::sync::Arc;

        let probe = Arc::new(NoopActiveApplicationProbe);
        let matcher = crate::privacy::CoreBlacklistMatcher::with_ignored(probe, vec![]);
        let gate = crate::privacy::PrivacyGate::new(matcher);
        let service = SettingsService::new(Arc::new(crate::clock::SystemClock), gate);
        let outcome = service.peer_identity().profile(Some("Studio".into()));
        assert!(
            matches!(outcome, PeerIdentityOutcome::Unavailable),
            "default SettingsService must surface Unavailable without an injected service",
        );
    }

    /// Updating the validated display name MUST NOT rotate the
    /// cryptographic identity: `peer_id` and `fingerprint` stay
    /// stable across every name edit, which is the contract the
    /// spec scenario "User changes visible name" pins.
    #[test]
    fn renaming_via_settings_service_does_not_rotate_identity() {
        use crate::peer_identity::{
            InMemoryPeerIdentityStore, PeerIdentityOutcome, PeerIdentityService,
        };
        use crate::test_support::isolated_harness;
        use std::sync::Arc;

        let (_dir, context) = isolated_harness();
        let clock: Arc<dyn crate::clock::Clock> = Arc::new(crate::clock::SystemClock);
        let probe = Arc::new(clipvault_platform::NoopActiveApplicationProbe);
        let matcher = crate::privacy::CoreBlacklistMatcher::with_ignored(probe, vec![]);
        let gate = crate::privacy::PrivacyGate::new(matcher);
        let identity_service = PeerIdentityService::new(Arc::new(InMemoryPeerIdentityStore::new()));
        let service =
            SettingsService::new(clock, gate).with_peer_identity_service(identity_service);

        let initial = match service.local_peer_profile(&context) {
            PeerIdentityOutcome::Ok(value) => value,
            other => panic!("expected Ok profile, got {other:?}"),
        };

        let stored = service
            .set_local_peer_display_name(&context, Some("Studio Renombrado"))
            .expect("name update");
        assert_eq!(
            stored.local_peer_display_name.as_deref(),
            Some("Studio Renombrado")
        );

        let after = match service.local_peer_profile(&context) {
            PeerIdentityOutcome::Ok(value) => value,
            other => panic!("expected Ok profile, got {other:?}"),
        };
        assert_eq!(initial.peer_id, after.peer_id);
        assert_eq!(initial.fingerprint, after.fingerprint);
        assert_eq!(after.display_name.as_deref(), Some("Studio Renombrado"));
    }

    /// The validated name MUST persist even when the secure store
    /// is unavailable. The SettingsService must not depend on the
    /// identity store to write `app_settings`, and the shell must
    /// surface the typed `Unavailable` outcome on the next profile
    /// load without losing the user's edit.
    #[test]
    fn name_persists_when_secure_store_is_unavailable() {
        use crate::peer_identity::{
            InMemoryPeerIdentityStore, PeerIdentityOutcome, PeerIdentityService,
        };
        use crate::test_support::isolated_harness;
        use std::sync::Arc;

        let (_dir, context) = isolated_harness();
        let clock: Arc<dyn crate::clock::Clock> = Arc::new(crate::clock::SystemClock);
        let probe = Arc::new(clipvault_platform::NoopActiveApplicationProbe);
        let matcher = crate::privacy::CoreBlacklistMatcher::with_ignored(probe, vec![]);
        let gate = crate::privacy::PrivacyGate::new(matcher);
        let unavailable_store = Arc::new(InMemoryPeerIdentityStore::always_unavailable());
        let service = SettingsService::new(clock, gate)
            .with_peer_identity_service(PeerIdentityService::new(unavailable_store));

        // Editing the name while the secure store is unreachable
        // MUST still land in `app_settings`.
        let stored = service
            .set_local_peer_display_name(&context, Some("Studio Sin Llave"))
            .expect("name update must succeed even with unavailable keychain");
        assert_eq!(
            stored.local_peer_display_name.as_deref(),
            Some("Studio Sin Llave"),
        );

        // And the identity path stays Unavailable — there is no
        // implicit fallback to an in-memory identity that would
        // mint a peer_id the next boot cannot reload.
        let outcome = service.local_peer_profile(&context);
        assert!(matches!(outcome, PeerIdentityOutcome::Unavailable));
    }

    /// The opt-in sharing toggle must round-trip through the
    /// persistence layer: a `true` write is observable on the next
    /// `load()` and survives a service rebuild, mirroring the
    /// display-name round-trip the `local-peer-identity-foundation`
    /// change already pins.
    #[test]
    fn set_local_peer_sharing_enabled_round_trips_through_persistence() {
        use crate::peer_identity::InMemoryPeerIdentityStore;
        use crate::test_support::isolated_harness;
        use std::sync::Arc;

        let (_dir, context) = isolated_harness();
        let clock: Arc<dyn crate::clock::Clock> = Arc::new(crate::clock::SystemClock);
        let probe = Arc::new(clipvault_platform::NoopActiveApplicationProbe);
        let matcher = crate::privacy::CoreBlacklistMatcher::with_ignored(probe, vec![]);
        let gate = crate::privacy::PrivacyGate::new(matcher);
        let service = SettingsService::new(clock, gate).with_peer_identity_service(
            crate::peer_identity::PeerIdentityService::new(Arc::new(
                InMemoryPeerIdentityStore::new(),
            )),
        );

        let stored = service
            .set_local_peer_sharing_enabled(&context, true)
            .expect("toggle");
        assert!(stored.local_peer_sharing_enabled);

        let reloaded = service.load(&context);
        assert!(reloaded.local_peer_sharing_enabled);

        let stored = service
            .set_local_peer_sharing_enabled(&context, false)
            .expect("toggle back");
        assert!(!stored.local_peer_sharing_enabled);
        let reloaded = service.load(&context);
        assert!(!reloaded.local_peer_sharing_enabled);
    }

    /// The toggle MUST stay default-disabled when the row is
    /// absent from `app_settings`. A missing key collapses to
    /// `false` so a fresh install never auto-enables sharing.
    #[test]
    fn local_peer_sharing_enabled_defaults_to_false_when_missing() {
        use crate::peer_identity::InMemoryPeerIdentityStore;
        use crate::test_support::isolated_harness;
        use std::sync::Arc;

        let (_dir, context) = isolated_harness();
        let clock: Arc<dyn crate::clock::Clock> = Arc::new(crate::clock::SystemClock);
        let probe = Arc::new(clipvault_platform::NoopActiveApplicationProbe);
        let matcher = crate::privacy::CoreBlacklistMatcher::with_ignored(probe, vec![]);
        let gate = crate::privacy::PrivacyGate::new(matcher);
        let service = SettingsService::new(clock, gate).with_peer_identity_service(
            crate::peer_identity::PeerIdentityService::new(Arc::new(
                InMemoryPeerIdentityStore::new(),
            )),
        );
        let loaded = service.load(&context);
        assert!(
            !loaded.local_peer_sharing_enabled,
            "fresh install must default the toggle to false"
        );
    }
}
