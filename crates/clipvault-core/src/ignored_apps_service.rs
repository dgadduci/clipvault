//! Application-picker service: coordinates the platform adapter, the
//! repository and the privacy gate so the rest of the codebase (and
//! the shell) never touches platform specifics.
//!
//! The service is the single owner of the picker flow:
//!
//! 1. It asks the [`ApplicationPicker`] for a selection.
//! 2. It validates the identifier (trim + lowercase, length check).
//! 3. It persists the row idempotently through
//!    [`IgnoredAppRepository::upsert`].
//! 4. It keeps the [`PrivacyGate`] snapshot in sync.
//!
//! Icon failure is handled independently: the picker may return a
//! selection with `icon_ref = None` and the service still persists
//! the row, so a valid blacklist identifier is never blocked by a
//! cosmetic failure.

use std::sync::Arc;

use clipvault_db::IgnoredAppRepository;
use clipvault_platform::ApplicationPicker;
use thiserror::Error;
use tracing::warn;

use crate::bootstrap::AppContext;
use crate::clock::Clock;
use crate::ignored_apps::{
    normalize_identifier, IgnoredAppEntry, IgnoredAppError, PickAndAddOutcome,
};
use crate::privacy::PrivacyGate;

#[derive(Debug, Error)]
pub enum IgnoredAppsServiceError {
    #[error("ignored-apps service: {0}")]
    Persistence(#[from] clipvault_db::IgnoredAppsError),
    #[error("ignored-apps service: {0}")]
    Domain(#[from] IgnoredAppError),
}

#[derive(Clone)]
pub struct IgnoredAppsService {
    clock: Arc<dyn Clock>,
    privacy_gate: PrivacyGate,
}

impl IgnoredAppsService {
    pub fn new(clock: Arc<dyn Clock>, privacy_gate: PrivacyGate) -> Self {
        Self {
            clock,
            privacy_gate,
        }
    }

    /// List every blacklisted application with its presentation
    /// metadata. The frontend renders one row per entry; legacy
    /// rows (with `display_name` / `icon_ref` set to `None`) flow
    /// through with the same shape so the panel can apply a safe
    /// fallback.
    pub fn list(
        &self,
        context: &AppContext,
    ) -> Result<Vec<IgnoredAppEntry>, IgnoredAppsServiceError> {
        let mut db = context.database().lock();
        let conn = db.connection_mut();
        let repo = IgnoredAppRepository::new(conn);
        let rows = repo.list()?;
        Ok(rows
            .into_iter()
            .map(|row| IgnoredAppEntry {
                id: row.id,
                display_name: row.display_name,
                icon_ref: row.icon_ref,
                created_at: row.created_at,
            })
            .collect())
    }

    /// Drive the picker, normalise the identifier, persist the row
    /// idempotently and update the privacy gate. The icon failure
    /// is independent: when the adapter returns `icon_ref = None`
    /// the row is still added so the blacklist rule stays active.
    pub fn pick_and_add(
        &self,
        context: &AppContext,
        picker: &dyn ApplicationPicker,
    ) -> Result<PickAndAddOutcome, IgnoredAppsServiceError> {
        let selected = match picker.pick() {
            Ok(value) => value,
            Err(picker_error) => {
                return Err(IgnoredAppError::from(picker_error).into());
            }
        };

        let normalized = normalize_identifier(&selected.identifier);
        if normalized.is_empty() {
            return Err(IgnoredAppError::MissingIdentifier.into());
        }

        let was_present = {
            let mut db = context.database().lock();
            let conn = db.connection_mut();
            let repo = IgnoredAppRepository::new(conn);
            repo.get(&normalized)?.is_some()
        };

        let now = self.clock.now();
        let row = {
            let mut db = context.database().lock();
            let conn = db.connection_mut();
            let mut repo = IgnoredAppRepository::new(conn);
            repo.upsert(
                &normalized,
                Some(selected.display_name.as_str()),
                selected.icon_ref.as_deref(),
                now,
            )?
        };

        // Always refresh the gate snapshot so the matcher observes
        // the new row. The cost is a single Arc clone + atomic
        // pointer swap.
        let ignored: Vec<String> = self
            .list(context)?
            .into_iter()
            .map(|entry| normalize_identifier(&entry.id))
            .filter(|id| !id.is_empty())
            .collect();
        self.privacy_gate.update_ignored(ignored);

        if was_present {
            warn!(
                identifier = %row.id,
                "picker produced an already-present identifier; refreshed metadata without duplicating"
            );
        }

        let entry = IgnoredAppEntry {
            id: row.id,
            display_name: row.display_name,
            icon_ref: row.icon_ref,
            created_at: row.created_at,
        };
        Ok(if was_present {
            PickAndAddOutcome::Updated(entry)
        } else {
            PickAndAddOutcome::Added(entry)
        })
    }

    /// Convenience entry point that constructs the platform-default
    /// picker from the [`AppContext`]'s platform info. Used by the
    /// Tauri shell so the command body stays a one-liner.
    ///
    /// The method never blocks waiting for the picker — the picker
    /// itself blocks until the user dismisses the dialog. Linux hosts
    /// always reach the [`IgnoredAppError::UnsupportedSession`] path
    /// until a safe `.desktop ↔ WM_CLASS/app_id` mapping is wired in.
    pub fn pick_and_add_with_default_picker(
        &self,
        context: &AppContext,
    ) -> Result<PickAndAddOutcome, IgnoredAppsServiceError> {
        use clipvault_platform::OsFamily;

        let info = context.platform();
        match info.os_family {
            OsFamily::Macos => {
                #[cfg(all(target_os = "macos", feature = "macos-native"))]
                {
                    let assets_dir = info.data_dir.join("assets");
                    let picker =
                        clipvault_platform::runtime::macos_app_picker::MacOsApplicationPicker::new(
                            assets_dir,
                        );
                    self.pick_and_add(context, &picker)
                }
                #[cfg(not(all(target_os = "macos", feature = "macos-native")))]
                {
                    let _ = info;
                    Err(IgnoredAppError::BackendUnavailable {
                        reason: "macos-native feature not enabled".into(),
                    }
                    .into())
                }
            }
            _ => {
                #[cfg(target_os = "linux")]
                {
                    let picker =
                        clipvault_platform::runtime::linux_app_picker::LinuxApplicationPicker::new(
                        );
                    self.pick_and_add(context, &picker)
                }
                #[cfg(not(target_os = "linux"))]
                {
                    Err(IgnoredAppError::UnsupportedSession {
                        reason: "application picker is not implemented for this platform".into(),
                    }
                    .into())
                }
            }
        }
    }
}

/// Convert the platform-layer picker error into the core-layer
/// domain error so the shell sees a single error type. The shell
/// then maps both shapes to a single Tauri response.
impl From<clipvault_platform::ApplicationPickerError> for IgnoredAppError {
    fn from(error: clipvault_platform::ApplicationPickerError) -> Self {
        match error {
            clipvault_platform::ApplicationPickerError::Cancelled => IgnoredAppError::Cancelled,
            clipvault_platform::ApplicationPickerError::InvalidSelection { reason } => {
                IgnoredAppError::InvalidSelection { reason }
            }
            clipvault_platform::ApplicationPickerError::MissingIdentifier => {
                IgnoredAppError::MissingIdentifier
            }
            clipvault_platform::ApplicationPickerError::BackendUnavailable { reason } => {
                IgnoredAppError::BackendUnavailable { reason }
            }
            clipvault_platform::ApplicationPickerError::UnsupportedSession { reason } => {
                IgnoredAppError::UnsupportedSession { reason }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clipvault_db::IgnoredAppRepository;
    use clipvault_platform::{
        ApplicationPicker, ApplicationPickerError, NoopActiveApplicationProbe, SelectedApplication,
    };
    use std::path::PathBuf;

    struct FixedPicker(Result<SelectedApplication, ApplicationPickerError>);

    impl ApplicationPicker for FixedPicker {
        fn pick(&self) -> Result<SelectedApplication, ApplicationPickerError> {
            self.0.clone()
        }
        fn name(&self) -> &'static str {
            "fixed"
        }
    }

    fn fixed_probe() -> Arc<dyn clipvault_platform::ActiveApplicationProbe> {
        Arc::new(NoopActiveApplicationProbe)
    }

    fn build_context(tempdir: &tempfile::TempDir) -> AppContext {
        let clock: Arc<dyn Clock> = Arc::new(crate::clock::SystemClock);
        let clipboard: Arc<dyn crate::Clipboard> = Arc::new(crate::FakeClipboard::new());
        let context = crate::AppBootstrap::new()
            .with_clock(clock)
            .with_clipboard(clipboard)
            .bootstrap_at(tempdir.path().join("clipvault.db"))
            .expect("bootstrap");
        context
    }

    #[test]
    fn list_returns_legacy_rows_with_none_metadata() {
        let dir = tempfile::tempdir().expect("tempdir");
        let context = build_context(&dir);
        {
            let mut db = context.database().lock();
            let conn = db.connection_mut();
            let mut repo = IgnoredAppRepository::new(conn);
            repo.insert("firefox", time::OffsetDateTime::now_utc())
                .expect("insert");
        }
        let gate = PrivacyGate::from_probe(fixed_probe(), vec![]);
        let service = IgnoredAppsService::new(context.clock(), gate);
        let entries = service.list(&context).expect("list");
        assert_eq!(entries.len(), 1);
        let row = &entries[0];
        assert_eq!(row.id, "firefox");
        assert!(row.display_name.is_none());
        assert!(row.icon_ref.is_none());
    }

    #[test]
    fn pick_and_add_persists_metadata_and_normalises_identifier() {
        let dir = tempfile::tempdir().expect("tempdir");
        let context = build_context(&dir);
        let gate = PrivacyGate::from_probe(fixed_probe(), vec![]);
        let service = IgnoredAppsService::new(context.clock(), gate.clone());

        let picker = FixedPicker(Ok(SelectedApplication {
            identifier: "  Com.Apple.Terminal  ".to_string(),
            display_name: "Terminal".to_string(),
            icon_ref: Some("ignored-apps/com.apple.terminal.png".to_string()),
        }));
        let outcome = service
            .pick_and_add(&context, &picker)
            .expect("pick_and_add");
        match outcome {
            PickAndAddOutcome::Added(entry) => {
                assert_eq!(entry.id, "com.apple.terminal");
                assert_eq!(entry.display_name.as_deref(), Some("Terminal"));
                assert_eq!(
                    entry.icon_ref.as_deref(),
                    Some("ignored-apps/com.apple.terminal.png")
                );
            }
            other => panic!("expected Added, got {other:?}"),
        }
        assert!(matches!(
            gate.evaluate(Some("com.apple.terminal")),
            crate::CaptureDecision::Discard { .. }
        ));
    }

    #[test]
    fn pick_and_add_is_idempotent_when_selection_repeats() {
        let dir = tempfile::tempdir().expect("tempdir");
        let context = build_context(&dir);
        let gate = PrivacyGate::from_probe(fixed_probe(), vec![]);
        let service = IgnoredAppsService::new(context.clock(), gate);

        let picker = FixedPicker(Ok(SelectedApplication {
            identifier: "com.apple.terminal".to_string(),
            display_name: "Terminal".to_string(),
            icon_ref: Some("ignored-apps/com.apple.terminal.png".to_string()),
        }));
        let first = service.pick_and_add(&context, &picker).expect("first");
        let second = service.pick_and_add(&context, &picker).expect("second");
        assert!(matches!(first, PickAndAddOutcome::Added(_)));
        assert!(matches!(second, PickAndAddOutcome::Updated(_)));

        let entries = service.list(&context).expect("list");
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn pick_and_add_preserves_rule_when_icon_is_missing() {
        // Icon failure MUST NOT block the privacy rule.
        let dir = tempfile::tempdir().expect("tempdir");
        let context = build_context(&dir);
        let gate = PrivacyGate::from_probe(fixed_probe(), vec![]);
        let service = IgnoredAppsService::new(context.clock(), gate.clone());

        let picker = FixedPicker(Ok(SelectedApplication {
            identifier: "com.apple.terminal".to_string(),
            display_name: "Terminal".to_string(),
            icon_ref: None,
        }));
        let outcome = service
            .pick_and_add(&context, &picker)
            .expect("pick_and_add");
        match outcome {
            PickAndAddOutcome::Added(entry) => {
                assert_eq!(entry.id, "com.apple.terminal");
                assert!(entry.icon_ref.is_none());
            }
            other => panic!("expected Added, got {other:?}"),
        }
        assert!(matches!(
            gate.evaluate(Some("com.apple.terminal")),
            crate::CaptureDecision::Discard { .. }
        ));
    }

    #[test]
    fn pick_and_add_surfaces_cancellation_without_mutating_list() {
        let dir = tempfile::tempdir().expect("tempdir");
        let context = build_context(&dir);
        let gate = PrivacyGate::from_probe(fixed_probe(), vec![]);
        let service = IgnoredAppsService::new(context.clock(), gate.clone());

        let picker = FixedPicker(Err(ApplicationPickerError::Cancelled));
        let err = service
            .pick_and_add(&context, &picker)
            .expect_err("cancelled");
        match err {
            IgnoredAppsServiceError::Domain(IgnoredAppError::Cancelled) => {}
            other => panic!("expected Cancelled, got {other:?}"),
        }
        let entries = service.list(&context).expect("list");
        assert!(entries.is_empty());
    }

    #[test]
    fn pick_and_add_rejects_unsupported_session() {
        // Linux currently reports unsupported_session. The service
        // must propagate the typed error so the UI can render a
        // specific message.
        let dir = tempfile::tempdir().expect("tempdir");
        let context = build_context(&dir);
        let gate = PrivacyGate::from_probe(fixed_probe(), vec![]);
        let service = IgnoredAppsService::new(context.clock(), gate);

        let picker = FixedPicker(Err(ApplicationPickerError::UnsupportedSession {
            reason: "wayland".into(),
        }));
        let err = service.pick_and_add(&context, &picker).expect_err("err");
        match err {
            IgnoredAppsServiceError::Domain(IgnoredAppError::UnsupportedSession { reason }) => {
                assert_eq!(reason, "wayland");
            }
            other => panic!("expected UnsupportedSession, got {other:?}"),
        }
    }

    #[test]
    fn pick_and_add_rejects_missing_identifier() {
        let dir = tempfile::tempdir().expect("tempdir");
        let context = build_context(&dir);
        let gate = PrivacyGate::from_probe(fixed_probe(), vec![]);
        let service = IgnoredAppsService::new(context.clock(), gate);

        let picker = FixedPicker(Ok(SelectedApplication {
            identifier: "".to_string(),
            display_name: "Ghost".to_string(),
            icon_ref: None,
        }));
        let err = service.pick_and_add(&context, &picker).expect_err("err");
        match err {
            IgnoredAppsServiceError::Domain(IgnoredAppError::MissingIdentifier) => {}
            other => panic!("expected MissingIdentifier, got {other:?}"),
        }
    }

    /// Path-only sanity check: the service never needs the absolute
    /// path of a bundle, so it cannot end up persisting one.
    #[test]
    fn selected_application_does_not_carry_a_path() {
        let selected = SelectedApplication {
            identifier: "com.example".to_string(),
            display_name: "Example".to_string(),
            icon_ref: Some("ignored-apps/com.example.png".to_string()),
        };
        let _ = PathBuf::new();
        let json = serde_json::to_string(&selected).expect("serialise");
        assert!(!json.contains("path"));
        assert!(!json.contains("bundle_url"));
    }

    /// Pin the wiring of the `macos-native` feature.
    ///
    /// Declaring the feature in `clipvault-core/Cargo.toml` but
    /// leaving it empty (the previous wiring) silently kept the
    /// `cfg(feature = "macos-native")` check false on every build,
    /// which routed every macOS picker call through the
    /// `BackendUnavailable` arm of
    /// `pick_and_add_with_default_picker`. Forwarding the feature to
    /// `clipvault-platform/macos-native` is the only thing that
    /// compiles the `MacOsApplicationPicker` adapter into the core's
    /// dependency graph.
    ///
    /// The test references the adapter through the
    /// `ApplicationPicker` trait so a regression that drops the
    /// forwarding surfaces at build time: if the feature is declared
    /// but left empty the symbol does not exist and the test fails
    /// to compile, which is the desired failure mode (the previous
    /// regression shipped without a build-time check and only
    /// surfaced at runtime).
    #[cfg(all(target_os = "macos", feature = "macos-native"))]
    #[test]
    fn macos_native_feature_resolves_the_macos_application_picker() {
        use clipvault_platform::runtime::macos_app_picker::MacOsApplicationPicker;
        use clipvault_platform::ApplicationPicker;

        let picker = MacOsApplicationPicker::new(PathBuf::from("/tmp"));
        assert_eq!(picker.name(), "macos_app_picker");
    }
}
