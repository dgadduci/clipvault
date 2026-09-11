//! GNOME Shell integration consent and state persistence.
//!
//! Persists the user's consent decision and the last-known
//! technical state of the integration separately. The consent
//! decision (`unknown` / `accepted` / `declined` / `disabled`) tells
//! the bootstrap whether to surface the prompt on next launch; the
//! technical state (`not_installed` / `disabled` / `incompatible` /
//! `activation_pending` / `connected` / `identified` /
//! `no_active_application` / `disconnected` / `unavailable`) is what
//! the diagnostics card renders and NEVER changes the consent
//! decision by itself.
//!
//! The service is platform-agnostic on purpose: ClipVault stores the
//! decision in `app_settings` so a fresh restore of `~/.clipvault`
//! preserves it, and the platform layer queries the persisted value
//! on every Linux startup.

use std::sync::Arc;

use clipvault_db::{AppSetting, AppSettingsError, AppSettingsRepository};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::bootstrap::AppContext;
use crate::clock::Clock;

/// Storage key the consent service uses. Keep stable across releases
/// so a user that upgrades ClipVault keeps the previous decision.
pub const GNOME_CONSENT_STORAGE_KEY: &str = "gnome_shell_integration_consent";

/// Storage key the technical state service uses. Updated by the
/// platform adapter on every transition.
pub const GNOME_STATE_STORAGE_KEY: &str = "gnome_shell_integration_state";

/// User-facing consent states. The first launch resolves to
/// `Unknown` so the UI can prompt exactly once.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GnomeConsentDecision {
    #[default]
    Unknown,
    Accepted,
    Declined,
    Disabled,
}

impl GnomeConsentDecision {
    pub fn as_str(self) -> &'static str {
        match self {
            GnomeConsentDecision::Unknown => "unknown",
            GnomeConsentDecision::Accepted => "accepted",
            GnomeConsentDecision::Declined => "declined",
            GnomeConsentDecision::Disabled => "disabled",
        }
    }

    pub fn from_storage(value: Option<&str>) -> Self {
        let Some(raw) = value else {
            return Self::Unknown;
        };
        match raw.trim() {
            "accepted" => Self::Accepted,
            "declined" => Self::Declined,
            "disabled" => Self::Disabled,
            _ => Self::Unknown,
        }
    }
}

/// Technical lifecycle states the platform adapter publishes. The
/// dashboard reads the persisted value so a future launch can
/// surface "your GNOME integration was `activation_pending` last time
/// and is still `activation_pending` now" without an immediate UI
/// prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GnomeTechnicalState {
    #[default]
    NotInstalled,
    Disabled,
    Incompatible,
    ActivationPending,
    Connected,
    Identified,
    NoActiveApplication,
    Disconnected,
    CommunicationError,
    Unavailable,
}

impl GnomeTechnicalState {
    pub fn as_str(self) -> &'static str {
        match self {
            GnomeTechnicalState::NotInstalled => "not_installed",
            GnomeTechnicalState::Disabled => "disabled",
            GnomeTechnicalState::Incompatible => "incompatible",
            GnomeTechnicalState::ActivationPending => "activation_pending",
            GnomeTechnicalState::Connected => "connected",
            GnomeTechnicalState::Identified => "identified",
            GnomeTechnicalState::NoActiveApplication => "no_active_application",
            GnomeTechnicalState::Disconnected => "disconnected",
            GnomeTechnicalState::CommunicationError => "communication_error",
            GnomeTechnicalState::Unavailable => "unavailable",
        }
    }

    pub fn from_storage(value: Option<&str>) -> Self {
        let Some(raw) = value else {
            return Self::NotInstalled;
        };
        match raw.trim() {
            "disabled" => Self::Disabled,
            "incompatible" => Self::Incompatible,
            "activation_pending" => Self::ActivationPending,
            "connected" => Self::Connected,
            "identified" => Self::Identified,
            "no_active_application" => Self::NoActiveApplication,
            "disconnected" => Self::Disconnected,
            "communication_error" => Self::CommunicationError,
            "unavailable" => Self::Unavailable,
            _ => Self::NotInstalled,
        }
    }
}

#[derive(Debug, Error)]
pub enum GnomeIntegrationError {
    #[error("app settings error: {0}")]
    AppSettings(#[from] AppSettingsError),
}

/// Pure-data status aggregate the diagnostics surfaces expose. The
/// struct never carries absolute paths, identifiers that look like
/// process identifiers or anything that could fingerprint the user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GnomeIntegrationSnapshot {
    pub consent: GnomeConsentDecision,
    pub technical_state: GnomeTechnicalState,
    pub last_identifier: Option<String>,
}

impl GnomeIntegrationSnapshot {
    pub fn new(
        consent: GnomeConsentDecision,
        technical_state: GnomeTechnicalState,
        last_identifier: Option<String>,
    ) -> Self {
        Self {
            consent,
            technical_state,
            last_identifier,
        }
    }
}

/// Service that persists consent + technical state. Owned by
/// `AppContext` and shared between the platform layer and the Tauri
/// commands. The `Clone` impl is hand-rolled because the in-memory
/// caches use `parking_lot::RwLock` which is not `Clone`: cloning the
/// handle keeps the same cache underneath.
pub struct GnomeIntegrationService {
    clock: Arc<dyn Clock>,
    /// Cached consent decision loaded from `app_settings`. Keeps the
    /// public payload accessible on the very first launch before any
    /// live platform service exists, and the in-memory copy is
    /// refreshed whenever [`save_consent`] commits a new value.
    cached_consent: Arc<parking_lot::RwLock<GnomeConsentDecision>>,
    /// Cached technical state. Mirrors the same lifecycle the
    /// platform layer reports so the diagnostics card can render
    /// the last-known value even when the listener is down.
    cached_technical_state: Arc<parking_lot::RwLock<GnomeTechnicalState>>,
}

impl Clone for GnomeIntegrationService {
    fn clone(&self) -> Self {
        Self {
            clock: Arc::clone(&self.clock),
            cached_consent: Arc::clone(&self.cached_consent),
            cached_technical_state: Arc::clone(&self.cached_technical_state),
        }
    }
}

impl GnomeIntegrationService {
    pub fn new(clock: Arc<dyn Clock>) -> Self {
        Self {
            clock,
            cached_consent: Arc::new(parking_lot::RwLock::new(GnomeConsentDecision::Unknown)),
            cached_technical_state: Arc::new(parking_lot::RwLock::new(
                GnomeTechnicalState::NotInstalled,
            )),
        }
    }

    /// Return the cached consent decision without touching the
    /// database. The cache is updated on every [`load_consent`] /
    /// [`save_consent`] call so the value reflects the last
    /// committed state. Falls back to [`GnomeConsentDecision::Unknown`]
    /// when the helper has never persisted anything yet, which is
    /// exactly the first-launch case the consent modal relies on.
    pub fn load_consent_from_cache(&self) -> GnomeConsentDecision {
        *self.cached_consent.read()
    }

    /// Return the cached technical state without touching the
    /// database. Mirrors the latest transition the platform layer
    /// reported through [`save_technical_state`].
    pub fn load_technical_state_from_cache(&self) -> GnomeTechnicalState {
        *self.cached_technical_state.read()
    }

    /// Prime the in-memory cache from the persistence layer using the
    /// raw `Database` mutex the bootstrap owns. Bootstrap calls this
    /// once at startup so the public payload can answer the first
    /// status call without forcing the shell to rebuild the full
    /// `AppContext`. Errors are swallowed because a missing key is
    /// the expected state on a fresh install.
    pub fn prime_from_database(
        &self,
        database: &parking_lot::Mutex<clipvault_db::Database>,
    ) -> Result<(), GnomeIntegrationError> {
        let mut guard = database.lock();
        let repo = AppSettingsRepository::new(guard.connection_mut());
        if let Some(row) = repo.get(GNOME_CONSENT_STORAGE_KEY)? {
            *self.cached_consent.write() =
                GnomeConsentDecision::from_storage(Some(row.value.as_str()));
        }
        if let Some(row) = repo.get(GNOME_STATE_STORAGE_KEY)? {
            *self.cached_technical_state.write() =
                GnomeTechnicalState::from_storage(Some(row.value.as_str()));
        }
        Ok(())
    }

    /// Load the persisted consent decision. Missing keys resolve to
    /// [`GnomeConsentDecision::Unknown`]. The result also primes the
    /// in-memory cache so the public payload helper can answer
    /// without taking the database lock.
    pub fn load_consent(
        &self,
        context: &AppContext,
    ) -> Result<GnomeConsentDecision, GnomeIntegrationError> {
        let mut db = context.database().lock();
        let repo = AppSettingsRepository::new(db.connection_mut());
        let stored = repo.get(GNOME_CONSENT_STORAGE_KEY)?;
        let decision =
            GnomeConsentDecision::from_storage(stored.as_ref().map(|row| row.value.as_str()));
        drop(db);
        *self.cached_consent.write() = decision;
        Ok(decision)
    }

    /// Persist the consent decision. Returns the previous value (if
    /// any) so callers can detect a transition. The in-memory cache
    /// is updated atomically with the SQLite commit so subsequent
    /// `load_consent_from_cache` calls observe the new value.
    pub fn save_consent(
        &self,
        context: &AppContext,
        decision: GnomeConsentDecision,
    ) -> Result<Option<GnomeConsentDecision>, GnomeIntegrationError> {
        let now = self.clock.now();
        let mut db = context.database().lock();
        let conn = db.connection_mut();
        let previous = {
            let repo = AppSettingsRepository::new(conn);
            repo.get(GNOME_CONSENT_STORAGE_KEY)
                .map_err(GnomeIntegrationError::AppSettings)?
        };
        let new_value = decision.as_str().to_string();
        let mut repo = AppSettingsRepository::new(conn);
        repo.set(GNOME_CONSENT_STORAGE_KEY, &new_value, now)
            .map_err(GnomeIntegrationError::AppSettings)?;
        drop(db);
        *self.cached_consent.write() = decision;
        Ok(previous.map(|row| GnomeConsentDecision::from_storage(Some(row.value.as_str()))))
    }

    /// Persist the most recent technical state. The helper DOES NOT
    /// alter the consent decision: the two tracks stay independent.
    /// The in-memory cache is updated so the public payload can be
    /// answered without a database round-trip.
    pub fn save_technical_state(
        &self,
        context: &AppContext,
        state: GnomeTechnicalState,
    ) -> Result<(), GnomeIntegrationError> {
        let now = self.clock.now();
        let mut db = context.database().lock();
        let conn = db.connection_mut();
        let mut repo = AppSettingsRepository::new(conn);
        repo.set(GNOME_STATE_STORAGE_KEY, state.as_str(), now)
            .map_err(GnomeIntegrationError::AppSettings)?;
        drop(db);
        *self.cached_technical_state.write() = state;
        Ok(())
    }

    /// Read the most recent technical state, falling back to
    /// [`GnomeTechnicalState::NotInstalled`]. The in-memory cache is
    /// refreshed to mirror the persisted value.
    pub fn load_technical_state(
        &self,
        context: &AppContext,
    ) -> Result<GnomeTechnicalState, GnomeIntegrationError> {
        let mut db = context.database().lock();
        let repo = AppSettingsRepository::new(db.connection_mut());
        let stored = repo.get(GNOME_STATE_STORAGE_KEY)?;
        let state =
            GnomeTechnicalState::from_storage(stored.as_ref().map(|row| row.value.as_str()));
        drop(db);
        *self.cached_technical_state.write() = state;
        Ok(state)
    }

    /// Build the snapshot the diagnostics surface consumes. Mirrors
    /// the wire-level struct a Tauri command emits.
    pub fn snapshot(
        &self,
        context: &AppContext,
    ) -> Result<GnomeIntegrationSnapshot, GnomeIntegrationError> {
        let consent = self.load_consent(context)?;
        let technical_state = self.load_technical_state(context)?;
        Ok(GnomeIntegrationSnapshot::new(
            consent,
            technical_state,
            None,
        ))
    }

    pub fn read_setting(
        &self,
        context: &AppContext,
        key: &str,
    ) -> Result<Option<AppSetting>, GnomeIntegrationError> {
        let mut db = context.database().lock();
        let repo = AppSettingsRepository::new(db.connection_mut());
        repo.get(key).map_err(GnomeIntegrationError::AppSettings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{fixed_clock, isolated_harness_at};

    fn context_with_clock() -> (tempfile::TempDir, AppContext) {
        isolated_harness_at(time::OffsetDateTime::UNIX_EPOCH)
    }

    #[test]
    fn consent_round_trips_through_storage() {
        let (_dir, context) = context_with_clock();
        let service = GnomeIntegrationService::new(fixed_clock(time::OffsetDateTime::UNIX_EPOCH));
        let loaded = service.load_consent(&context).expect("load");
        assert_eq!(loaded, GnomeConsentDecision::Unknown);
        service
            .save_consent(&context, GnomeConsentDecision::Accepted)
            .expect("save accepted");
        let loaded = service.load_consent(&context).expect("load");
        assert_eq!(loaded, GnomeConsentDecision::Accepted);
    }

    #[test]
    fn technical_state_round_trips() {
        let (_dir, context) = context_with_clock();
        let service = GnomeIntegrationService::new(fixed_clock(time::OffsetDateTime::UNIX_EPOCH));
        service
            .save_technical_state(&context, GnomeTechnicalState::ActivationPending)
            .expect("save");
        let loaded = service.load_technical_state(&context).expect("load");
        assert_eq!(loaded, GnomeTechnicalState::ActivationPending);
    }

    #[test]
    fn decision_storage_string_is_stable() {
        assert_eq!(GnomeConsentDecision::Accepted.as_str(), "accepted");
        assert_eq!(GnomeConsentDecision::Declined.as_str(), "declined");
        assert_eq!(GnomeConsentDecision::Disabled.as_str(), "disabled");
        assert_eq!(GnomeConsentDecision::Unknown.as_str(), "unknown");
    }

    #[test]
    fn unknown_storage_is_default() {
        assert_eq!(
            GnomeConsentDecision::from_storage(None),
            GnomeConsentDecision::Unknown
        );
        assert_eq!(
            GnomeConsentDecision::from_storage(Some("")),
            GnomeConsentDecision::Unknown
        );
        assert_eq!(
            GnomeConsentDecision::from_storage(Some("garbage")),
            GnomeConsentDecision::Unknown
        );
    }

    #[test]
    fn technical_state_storage_string_is_stable() {
        assert_eq!(GnomeTechnicalState::Identified.as_str(), "identified");
        assert_eq!(
            GnomeTechnicalState::ActivationPending.as_str(),
            "activation_pending"
        );
        assert_eq!(
            GnomeTechnicalState::NoActiveApplication.as_str(),
            "no_active_application"
        );
        assert_eq!(
            GnomeTechnicalState::CommunicationError.as_str(),
            "communication_error"
        );
    }

    #[test]
    fn technical_state_round_trips_communication_error() {
        let value = GnomeTechnicalState::CommunicationError;
        let raw = value.as_str();
        assert_eq!(
            GnomeTechnicalState::from_storage(Some(raw)),
            GnomeTechnicalState::CommunicationError
        );
    }
}
