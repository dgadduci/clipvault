//! Linux visual blacklist picker (`linux-blacklist-app-picker`).
//!
//! The Linux picker refuses to invent an application identifier for
//! the user. The [`linux_picker_support`] resolves the runtime
//! classification (X11, XWayland, native Wayland, GNOME Shell
//! extension, unsupported) from the actual [`ActiveAppDiagnostics`]
//! state the capture loop is using, cross-checked against the GNOME
//! integration technical state. The catalog command and the
//! catalog-add command both call into [`resolve_linux_picker_backend`]
//! so they can never disagree on which strategy applies.
//!
//! The resolver is platform-agnostic at the public boundary: it
//! accepts the diagnostics snapshot, the GNOME consent decision and
//! the GNOME technical state directly so unit tests can exercise the
//! matrix without a real GNOME Shell or X11 server.

use clipvault_platform::{ActiveAppBackendKind, LinuxPickerBackend};

use crate::gnome_integration::{
    GnomeConsentDecision as CoreGnomeConsent, GnomeTechnicalState as CoreGnomeTechnicalState,
};

/// Backend string the GNOME Shell extension probe reports when it
/// is the active-app backend. Surfaced as a stable constant so the
/// resolver can compare against the documented wire contract
/// without parsing free-form backend names.
pub const GNOME_SHELL_EXTENSION_BACKEND: &str = "gnome_shell_extension";

/// Snapshot of the runtime state the resolver needs to decide the
/// picker backend. The struct is intentionally narrow: the
/// resolver never inspects free-form text, never logs the cached
/// identifier and never reads clipboard content.
#[derive(Debug, Clone, Copy)]
pub struct LinuxPickerSessionState<'a> {
    /// Active-app backend the diagnostics surface currently
    /// reports. Pass [`None`] when no probe has been installed
    /// (the bootstrap detected an unsupported platform/session).
    pub backend: Option<&'a str>,
    /// `true` once the capture loop has populated the cached
    /// identifier. A backend that is installed but has never
    /// produced a real identifier cannot justify offering the
    /// picker.
    pub cache_populated: bool,
    /// User-visible GNOME integration consent decision.
    pub gnome_consent: Option<GnomeConsentDecision>,
    /// GNOME integration technical state the platform adapter most
    /// recently reported. `None` mirrors "the GNOME integration is
    /// not applicable on this session".
    pub gnome_technical_state: Option<GnomeTechnicalState>,
}

/// Local mirror of [`crate::gnome_integration::GnomeConsentDecision`]
/// used by the resolver. The duplication keeps the resolver
/// independent of the persistence layer and lets the unit tests
/// construct `LinuxPickerSessionState` values without going through
/// SQLite.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GnomeConsentDecision {
    Unknown,
    Accepted,
    Declined,
    Disabled,
}

impl GnomeConsentDecision {
    pub fn from_core(decision: CoreGnomeConsent) -> Self {
        match decision {
            CoreGnomeConsent::Unknown => Self::Unknown,
            CoreGnomeConsent::Accepted => Self::Accepted,
            CoreGnomeConsent::Declined => Self::Declined,
            CoreGnomeConsent::Disabled => Self::Disabled,
        }
    }
}

/// Local mirror of [`crate::gnome_integration::GnomeTechnicalState`].
/// Same justification as [`GnomeConsentDecision`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GnomeTechnicalState {
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
    pub fn from_core(state: CoreGnomeTechnicalState) -> Self {
        use CoreGnomeTechnicalState as Core;
        match state {
            Core::NotInstalled => Self::NotInstalled,
            Core::Disabled => Self::Disabled,
            Core::Incompatible => Self::Incompatible,
            Core::ActivationPending => Self::ActivationPending,
            Core::Connected => Self::Connected,
            Core::Identified => Self::Identified,
            Core::NoActiveApplication => Self::NoActiveApplication,
            Core::Disconnected => Self::Disconnected,
            Core::CommunicationError => Self::CommunicationError,
            Core::Unavailable => Self::Unavailable,
        }
    }
}

/// Resolve the picker backend from the runtime state the bootstrap
/// installed.
///
/// The function is total: every session lands on a defined variant.
/// `Unsupported` is the documented "do not offer the picker"
/// outcome and MUST reach the frontend with a stable
/// `unsupported_session` reason so the manual-entry surface stays
/// available.
///
/// `cache_populated` is intentionally **not** part of the decision:
/// the picker derives its identifier from the installed `.desktop`
/// files, so the only thing that has to be live is the active-app
/// backend itself. A session without a focused window does not
/// invalidate the association the catalog installs; the resolver
/// therefore offers the picker as soon as the backend is
/// recognisable, regardless of whether the capture loop has seen a
/// focused window yet.
///
/// ## Decision matrix
///
/// | Active-app backend                                | Consent  | Tech state                                          | Result                  |
/// | ------------------------------------------------- | -------- | --------------------------------------------------- | ----------------------- |
/// | `gnome_shell_extension`                            | Accepted | `Connected` / `Identified` / `NoActiveApplication`  | `GnomeShellExtension`   |
/// | `gnome_shell_extension`                            | Accepted | `ActivationPending` / `Disconnected` / `NotInstalled` / `Disabled` / `Incompatible` / `CommunicationError` / `Unavailable` | `Unsupported`           |
/// | `gnome_shell_extension`                            | anything | anything                                            | `Unsupported`           |
/// | `x11_ewmh` / `xwayland_ewmh`                       | (any)    | (any)                                               | `X11OrXWaylandEwmh`     |
/// | `wayland_foreign_toplevel` / `wayland_wlr_foreign_toplevel` | (any) | (any)                                       | `WaylandNative`         |
/// | Unavailable / unknown backend / no probe            | (any)    | (any)                                               | `Unsupported`           |
pub fn resolve_linux_picker_backend(state: LinuxPickerSessionState<'_>) -> LinuxPickerBackend {
    match state.backend {
        None => LinuxPickerBackend::Unsupported,
        Some(GNOME_SHELL_EXTENSION_BACKEND) => {
            if matches!(state.gnome_consent, Some(GnomeConsentDecision::Accepted))
                && matches!(
                    state.gnome_technical_state,
                    Some(
                        GnomeTechnicalState::Connected
                            | GnomeTechnicalState::Identified
                            | GnomeTechnicalState::NoActiveApplication,
                    )
                )
            {
                LinuxPickerBackend::GnomeShellExtension
            } else {
                LinuxPickerBackend::Unsupported
            }
        }
        Some(backend)
            if backend == backend_kind::X11_EWMH || backend == backend_kind::XWAYLAND_EWMH =>
        {
            LinuxPickerBackend::X11OrXWaylandEwmh
        }
        Some(backend)
            if backend == backend_kind::WAYLAND_FOREIGN_TOPLEVEL
                || backend == backend_kind::WAYLAND_WLR_FOREIGN_TOPLEVEL =>
        {
            LinuxPickerBackend::WaylandNative
        }
        Some(_) => LinuxPickerBackend::Unsupported,
    }
}

/// Stable, snake_case backend identifiers the resolver compares
/// against. The constants live next to the resolver so the
/// decision matrix and the comparison table share a single source
/// of truth.
pub mod backend_kind {
    /// Mirror of the public [`ActiveAppBackendKind::as_str`] values
    /// the resolver cares about. The strings are stable identifiers
    /// the frontend already consumes in the diagnostics endpoint;
    /// renaming any of them is a breaking change for the UI.
    pub const X11_EWMH: &str = "x11_ewmh";
    pub const XWAYLAND_EWMH: &str = "xwayland_ewmh";
    pub const WAYLAND_FOREIGN_TOPLEVEL: &str = "wayland_foreign_toplevel";
    pub const WAYLAND_WLR_FOREIGN_TOPLEVEL: &str = "wayland_wlr_foreign_toplevel";
    pub const MACOS_WORKSPACE: &str = "macos_workspace";
    pub const UNAVAILABLE: &str = "unavailable";
}

/// Convert an [`ActiveAppBackendKind`] into the snake_case string
/// the resolver compares against. Centralised here so the
/// constants and the `as_str` strings the platform crate already
/// exposes stay in lock-step.
pub fn backend_kind_str(kind: ActiveAppBackendKind) -> &'static str {
    kind.as_str()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(
        backend: Option<&'static str>,
        consent: Option<GnomeConsentDecision>,
        tech: Option<GnomeTechnicalState>,
    ) -> LinuxPickerSessionState<'static> {
        // `cache_populated` is intentionally fixed to `false` here:
        // the resolver dropped the cache requirement so the test
        // suite must not silently depend on the legacy behaviour.
        LinuxPickerSessionState {
            backend,
            cache_populated: false,
            gnome_consent: consent,
            gnome_technical_state: tech,
        }
    }

    // -------------------------------------------------------------------
    // X11 / XWayland branch. `cache_populated` is no longer a
    // requirement: the picker derives the identifier from the
    // installed `.desktop` files, so the only thing that has to be
    // live is the backend itself.
    // -------------------------------------------------------------------

    #[test]
    fn x11_ewmh_is_supported_without_populated_cache() {
        let result = resolve_linux_picker_backend(state(Some("x11_ewmh"), None, None));
        assert_eq!(result, LinuxPickerBackend::X11OrXWaylandEwmh);
    }

    #[test]
    fn xwayland_ewmh_uses_x11_branch_without_populated_cache() {
        let result = resolve_linux_picker_backend(state(Some("xwayland_ewmh"), None, None));
        assert_eq!(result, LinuxPickerBackend::X11OrXWaylandEwmh);
    }

    // -------------------------------------------------------------------
    // Native Wayland branch — same contract: the backend identity is
    // enough; the cache is irrelevant.
    // -------------------------------------------------------------------

    #[test]
    fn wayland_wlr_is_supported_without_populated_cache() {
        let result =
            resolve_linux_picker_backend(state(Some("wayland_wlr_foreign_toplevel"), None, None));
        assert_eq!(result, LinuxPickerBackend::WaylandNative);
    }

    #[test]
    fn wayland_ext_is_supported_without_populated_cache() {
        let result =
            resolve_linux_picker_backend(state(Some("wayland_foreign_toplevel"), None, None));
        assert_eq!(result, LinuxPickerBackend::WaylandNative);
    }

    // -------------------------------------------------------------------
    // GNOME Shell extension branch. Acceptance requires the consent
    // AND one of the three "the extension is up and able to publish
    // an identifier" states. ActivationPending is intentionally
    // excluded: the extension has not yet finished the handshake
    // and may never publish a stable id if the install fails, so
    // the picker refuses to commit to a mapping.
    // -------------------------------------------------------------------

    #[test]
    fn gnome_shell_extension_with_consent_and_connected_state_is_supported() {
        for operational in [
            GnomeTechnicalState::Connected,
            GnomeTechnicalState::Identified,
            GnomeTechnicalState::NoActiveApplication,
        ] {
            let result = resolve_linux_picker_backend(state(
                Some("gnome_shell_extension"),
                Some(GnomeConsentDecision::Accepted),
                Some(operational),
            ));
            assert_eq!(
                result,
                LinuxPickerBackend::GnomeShellExtension,
                "operational state {operational:?} must classify as GnomeShellExtension"
            );
        }
    }

    #[test]
    fn gnome_shell_extension_with_activation_pending_is_unsupported() {
        // ActivationPending is explicitly out of the supported set:
        // the extension has not yet completed the first handshake and
        // the picker refuses to commit to a mapping it cannot
        // guarantee. The test pins the documented behaviour change.
        for cache_populated in [true, false] {
            let mut snapshot = state(
                Some("gnome_shell_extension"),
                Some(GnomeConsentDecision::Accepted),
                Some(GnomeTechnicalState::ActivationPending),
            );
            snapshot.cache_populated = cache_populated;
            let result = resolve_linux_picker_backend(snapshot);
            assert_eq!(
                result,
                LinuxPickerBackend::Unsupported,
                "ActivationPending must classify as Unsupported regardless of cache_populated={cache_populated}"
            );
        }
    }

    #[test]
    fn gnome_shell_extension_with_consent_but_inactive_state_is_unsupported() {
        for inactive in [
            GnomeTechnicalState::NotInstalled,
            GnomeTechnicalState::Disabled,
            GnomeTechnicalState::Incompatible,
            GnomeTechnicalState::Disconnected,
            GnomeTechnicalState::CommunicationError,
            GnomeTechnicalState::Unavailable,
        ] {
            let result = resolve_linux_picker_backend(state(
                Some("gnome_shell_extension"),
                Some(GnomeConsentDecision::Accepted),
                Some(inactive),
            ));
            assert_eq!(
                result,
                LinuxPickerBackend::Unsupported,
                "inactive GNOME state {inactive:?} must classify as Unsupported"
            );
        }
    }

    #[test]
    fn gnome_shell_extension_without_consent_is_unsupported() {
        for consent in [
            Some(GnomeConsentDecision::Unknown),
            Some(GnomeConsentDecision::Declined),
            Some(GnomeConsentDecision::Disabled),
            None,
        ] {
            let result = resolve_linux_picker_backend(state(
                Some("gnome_shell_extension"),
                consent,
                Some(GnomeTechnicalState::Connected),
            ));
            assert_eq!(
                result,
                LinuxPickerBackend::Unsupported,
                "consent {consent:?} must classify as Unsupported"
            );
        }
    }

    // -------------------------------------------------------------------
    // Unsupported backends.
    // -------------------------------------------------------------------

    #[test]
    fn macos_workspace_backend_on_linux_is_unsupported() {
        let result = resolve_linux_picker_backend(state(Some("macos_workspace"), None, None));
        assert_eq!(result, LinuxPickerBackend::Unsupported);
    }

    #[test]
    fn unavailable_backend_is_unsupported() {
        let result = resolve_linux_picker_backend(state(Some("unavailable"), None, None));
        assert_eq!(result, LinuxPickerBackend::Unsupported);
    }

    #[test]
    fn nonexistent_backend_is_unsupported() {
        let result = resolve_linux_picker_backend(state(Some("brand_new_backend"), None, None));
        assert_eq!(result, LinuxPickerBackend::Unsupported);
    }

    #[test]
    fn none_backend_is_unsupported() {
        let result = resolve_linux_picker_backend(state(None, None, None));
        assert_eq!(result, LinuxPickerBackend::Unsupported);
    }
}

/// Wiring test: prove that
/// [`crate::bootstrap::AppContext::linux_picker_backend`] follows the
/// probe the capture loop is using right now — including after
/// [`crate::bootstrap::AppContext::swap_active_app_probe`] rewrites
/// it. The previous implementation read the backend from
/// [`crate::ActiveAppDiagnosticsState`], which kept the original
/// `DisplayServer`-derived hint after the GNOME swap, so the
/// catalog command and the `add` command kept reporting
/// `X11OrXWaylandEwmh` even after the GNOME integration took over.
#[cfg(test)]
mod wiring {
    use std::sync::Arc;

    use clipvault_platform::{
        ActiveAppError, ActiveApplication, ActiveApplicationProbe, CachedActiveApplication,
        DisplayServer, NoopApplicationMetadataProvider, OsFamily, PlatformInfo,
    };

    use crate::bootstrap::AppContext;
    use crate::fakes::{
        FakeClipboardBackend, FakeHotkeyManager, FakePasteController, FakeSettingsNavigator,
        FakeTrayController,
    };
    use crate::gnome_integration::{GnomeConsentDecision, GnomeTechnicalState};
    use crate::platform_adapters::PlatformAdapters;
    use crate::test_support::fixed_clock;
    use crate::{AppBootstrap, Capabilities, FakeClipboard};

    /// Programmable probe whose `name()` returns a caller-supplied
    /// backend string. The probe never produces an application so
    /// the cache stays empty; the wiring test only inspects
    /// `linux_picker_backend()`, which now derives the backend from
    /// `CachedActiveApplication::name()` rather than from the cache
    /// populated flag.
    struct NamedProbe(&'static str);

    impl ActiveApplicationProbe for NamedProbe {
        fn active_application(&self) -> Result<Option<ActiveApplication>, ActiveAppError> {
            Ok(None)
        }
        fn name(&self) -> &'static str {
            self.0
        }
    }

    fn build_context(initial_backend: &'static str) -> AppContext {
        let dir = tempfile::tempdir().expect("tempdir");
        let home_dir = dir.path().to_path_buf();
        let data_dir = dir.path().join("data");
        std::fs::create_dir_all(&data_dir).expect("data dir");
        let info = PlatformInfo {
            home_dir,
            data_dir,
            os_family: OsFamily::Linux,
            display_server: DisplayServer::X11,
        };
        let probe: Arc<dyn ActiveApplicationProbe> = Arc::new(NamedProbe(initial_backend));
        let adapters = PlatformAdapters::new(
            Arc::new(FakeClipboardBackend::new()) as Arc<_>,
            Arc::new(FakeHotkeyManager::new()) as Arc<_>,
            probe.clone() as Arc<_>,
            Arc::new(FakePasteController::new()) as Arc<_>,
            Arc::new(FakeTrayController::new()) as Arc<_>,
            Arc::new(FakeSettingsNavigator::new()) as Arc<_>,
            Arc::new(NoopApplicationMetadataProvider) as Arc<_>,
            Capabilities::ALL_AVAILABLE,
            info,
        );
        AppBootstrap::new()
            .with_clock(fixed_clock(time::OffsetDateTime::UNIX_EPOCH))
            .with_clipboard(Arc::new(FakeClipboard::new()))
            .with_platform_adapters(adapters)
            .bootstrap_at(dir.path().join("clipvault.db"))
            .expect("bootstrap")
    }

    /// Drive a `linux_picker_backend()` roundtrip while swapping the
    /// probe the capture loop uses. The test is the regression the
    /// `linux-blacklist-app-picker` change ships: the resolver must
    /// observe the swap and re-classify the session without a
    /// restart.
    #[test]
    fn linux_picker_backend_follows_swap_active_app_probe() {
        let context = build_context("x11_ewmh");

        // Initial probe is X11 EWMH. Consent / technical state are
        // not relevant for this branch — the resolver classifies X11
        // regardless.
        assert_eq!(
            context.linux_picker_backend(),
            clipvault_platform::LinuxPickerBackend::X11OrXWaylandEwmh
        );

        // Swap to a GNOME-backed probe. The diagnostics snapshot
        // still reports the initial `x11_ewmh` hint, but the
        // resolver now consults the wrapped probe's `name()`, so the
        // picker backend MUST switch to `GnomeShellExtension` once
        // the consent and the technical state align.
        context.swap_active_app_probe(Arc::new(NamedProbe("gnome_shell_extension")));
        // Consent is fresh by default: `Unknown`.
        assert_eq!(
            context.linux_picker_backend(),
            clipvault_platform::LinuxPickerBackend::Unsupported,
            "GNOME backend without accepted consent must stay Unsupported"
        );
        context
            .gnome_integration()
            .save_consent(&context, GnomeConsentDecision::Accepted)
            .expect("save consent");
        context
            .gnome_integration()
            .save_technical_state(&context, GnomeTechnicalState::Identified)
            .expect("save technical state");
        assert_eq!(
            context.linux_picker_backend(),
            clipvault_platform::LinuxPickerBackend::GnomeShellExtension,
            "after the swap + accepted consent + connected state the resolver MUST classify GnomeShellExtension"
        );

        // Swap to a native Wayland probe. The resolver must follow
        // the swap and classify `WaylandNative` without consulting
        // the persisted GNOME consent (which only matters for the
        // GNOME branch).
        context.swap_active_app_probe(Arc::new(NamedProbe("wayland_wlr_foreign_toplevel")));
        assert_eq!(
            context.linux_picker_backend(),
            clipvault_platform::LinuxPickerBackend::WaylandNative
        );
    }

    /// The probe name is the source of truth. The harness's
    /// `ActiveAppDiagnosticsState::backend` is initialised to the
    /// [`ActiveAppBackendKind`] the [`PlatformInfo`] advertises; the
    /// wiring test makes sure the diagnostics state and the probe
    /// name can disagree after a swap without confusing the
    /// resolver.
    #[test]
    fn linux_picker_backend_reads_probe_name_not_diagnostics_backend() {
        let context = build_context("x11_ewmh");
        // The diagnostics surface initialises `backend` from the
        // `PlatformInfo` (X11 -> `x11_ewmh`) and never updates it.
        let snapshot = context.active_app_diagnostics();
        assert_eq!(snapshot.backend, "x11_ewmh");

        // After swapping to a GNOME-backed probe, the probe `name()`
        // returns `gnome_shell_extension` but the diagnostics
        // snapshot still reports the original `x11_ewmh`. The
        // resolver must pick the probe name, not the diagnostics
        // string.
        context.swap_active_app_probe(Arc::new(NamedProbe("gnome_shell_extension")));
        context
            .gnome_integration()
            .save_consent(&context, GnomeConsentDecision::Accepted)
            .expect("save consent");
        context
            .gnome_integration()
            .save_technical_state(&context, GnomeTechnicalState::Connected)
            .expect("save technical state");
        let post_swap_snapshot = context.active_app_diagnostics();
        assert_eq!(
            post_swap_snapshot.backend, "x11_ewmh",
            "diagnostics backend stays at the initial hint after swap"
        );
        assert_eq!(
            context.linux_picker_backend(),
            clipvault_platform::LinuxPickerBackend::GnomeShellExtension,
            "resolver must follow the probe name, not the stale diagnostics hint"
        );

        // Sanity check: the `CachedActiveApplication::name()`
        // accessor surfaces the post-swap backend, which is the
        // single piece of evidence the resolver needs to keep up
        // with the swap.
        let cached: CachedActiveApplication = context.cached_active_app_probe();
        assert_eq!(cached.name(), "gnome_shell_extension");
    }

    #[test]
    fn gnome_runtime_no_active_application_overrides_activation_pending_fallback() {
        // Reproduces the Ubuntu GNOME Wayland regression: the listener has
        // completed its handshake and reports no application because ClipVault
        // now owns focus, while SQLite still contains the startup state saved
        // before the peer connected. The picker must use the live enum and
        // therefore remain available.
        let context = build_context("gnome_shell_extension");
        let integration = context.gnome_integration();
        integration
            .save_consent(&context, GnomeConsentDecision::Accepted)
            .expect("save accepted consent");
        integration
            .save_technical_state(&context, GnomeTechnicalState::ActivationPending)
            .expect("save startup fallback");
        integration.set_runtime_technical_state(GnomeTechnicalState::NoActiveApplication);

        assert_eq!(
            integration.load_technical_state_from_cache(),
            GnomeTechnicalState::ActivationPending,
            "the durable startup fallback stays untouched"
        );
        assert_eq!(
            context.linux_picker_backend(),
            clipvault_platform::LinuxPickerBackend::GnomeShellExtension,
            "a live NoActiveApplication state is operational for catalog selection"
        );
    }
}
