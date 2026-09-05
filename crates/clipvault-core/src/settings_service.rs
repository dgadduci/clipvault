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
use crate::privacy::PrivacyGate;
use crate::settings::{
    HotkeySpec, Settings, SettingsUpdate, ValidationError, HOTKEY_SETTING_STORAGE_KEY,
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
}

impl SettingsService {
    pub fn new(clock: Arc<dyn Clock>, privacy_gate: PrivacyGate) -> Self {
        Self {
            clock,
            privacy_gate,
        }
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_service() -> SettingsService {
        use crate::clock::SystemClock;
        use clipvault_platform::NoopActiveApplicationProbe;
        use std::sync::Arc;

        let probe = Arc::new(NoopActiveApplicationProbe);
        let matcher = crate::privacy::CoreBlacklistMatcher::with_ignored(probe, vec![]);
        let gate = crate::privacy::PrivacyGate::new(matcher);
        SettingsService {
            clock: Arc::new(SystemClock),
            privacy_gate: gate,
        }
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
}
