//! Persisted consent for the optional KDE KWin source-application bridge.

use std::sync::Arc;

use clipvault_db::{AppSettingsError, AppSettingsRepository};
use thiserror::Error;

use crate::bootstrap::AppContext;
use crate::clock::Clock;

pub const KDE_KWIN_CONSENT_STORAGE_KEY: &str = "kde_kwin_source_app_consent";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum KdeKwinConsentDecision {
    #[default]
    Unknown,
    Accepted,
    Declined,
    Disabled,
}

impl KdeKwinConsentDecision {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Accepted => "accepted",
            Self::Declined => "declined",
            Self::Disabled => "disabled",
        }
    }

    pub fn from_storage(value: Option<&str>) -> Self {
        match value.map(str::trim) {
            Some("accepted") => Self::Accepted,
            Some("declined") => Self::Declined,
            Some("disabled") => Self::Disabled,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Error)]
pub enum KdeKwinIntegrationError {
    #[error("app settings error: {0}")]
    AppSettings(#[from] AppSettingsError),
}

#[derive(Clone)]
pub struct KdeKwinIntegrationService {
    clock: Arc<dyn Clock>,
    cached_consent: Arc<parking_lot::RwLock<KdeKwinConsentDecision>>,
}

impl KdeKwinIntegrationService {
    pub fn new(clock: Arc<dyn Clock>) -> Self {
        Self {
            clock,
            cached_consent: Arc::new(parking_lot::RwLock::new(KdeKwinConsentDecision::Unknown)),
        }
    }

    pub fn prime_from_database(
        &self,
        database: &parking_lot::Mutex<clipvault_db::Database>,
    ) -> Result<(), KdeKwinIntegrationError> {
        let mut guard = database.lock();
        let repo = AppSettingsRepository::new(guard.connection_mut());
        let decision = repo
            .get(KDE_KWIN_CONSENT_STORAGE_KEY)?
            .map(|row| KdeKwinConsentDecision::from_storage(Some(&row.value)))
            .unwrap_or_default();
        *self.cached_consent.write() = decision;
        Ok(())
    }

    pub fn load_consent(
        &self,
        context: &AppContext,
    ) -> Result<KdeKwinConsentDecision, KdeKwinIntegrationError> {
        let mut db = context.database().lock();
        let repo = AppSettingsRepository::new(db.connection_mut());
        let stored = repo.get(KDE_KWIN_CONSENT_STORAGE_KEY)?;
        let decision =
            KdeKwinConsentDecision::from_storage(stored.as_ref().map(|row| row.value.as_str()));
        drop(db);
        *self.cached_consent.write() = decision;
        Ok(decision)
    }

    pub fn cached_consent(&self) -> KdeKwinConsentDecision {
        *self.cached_consent.read()
    }

    pub fn save_consent(
        &self,
        context: &AppContext,
        decision: KdeKwinConsentDecision,
    ) -> Result<(), KdeKwinIntegrationError> {
        let now = self.clock.now();
        let mut db = context.database().lock();
        let mut repo = AppSettingsRepository::new(db.connection_mut());
        repo.set(KDE_KWIN_CONSENT_STORAGE_KEY, decision.as_str(), now)?;
        drop(db);
        *self.cached_consent.write() = decision;
        Ok(())
    }
}
