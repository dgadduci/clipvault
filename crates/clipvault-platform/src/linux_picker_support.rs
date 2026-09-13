//! Runtime classification of the active-app backend the Linux
//! blacklist picker needs to honour.
//!
//! The Linux visual blacklist picker must never invent an
//! identifier for the user. The picker therefore refuses to offer a
//! candidate unless the running session exposes an active-app backend
//! whose published identifier we can reproduce deterministically from
//! the installed `.desktop` files.
//!
//! The decision does **not** depend on the persisted `DisplayServer`
//! hint or on the GNOME integration consent alone. It consults the
//! actual [`ActiveAppBackendKind`] the diagnostics surface exposes
//! (which is the backend the capture loop is using *right now*) and
//! cross-checks it against the GNOME integration state the
//! `GnomeIntegrationService` reports (consent + installed + active +
//! connected). A session classified as `Unsupported` keeps the manual
//! identifier entry surface available; the frontend MUST NOT offer
//! the visual picker modal in that case.
//!
//! ## Decision matrix
//!
//! | Active-app backend                         | GNOME consent | GNOME technical state                            | `LinuxPickerBackend`           | Strategy          |
//! | ------------------------------------------ | ------------- | ------------------------------------------------ | ------------------------------ | ----------------- |
//! | `x11_ewmh` / `xwayland_ewmh`               | (any)         | (any)                                            | `X11OrXWaylandEwmh`            | `WmClass`         |
//! | `wayland_foreign_toplevel` / `wayland_wlr_foreign_toplevel` | (any) | (any)                                            | `WaylandNative`                | `WmClass`         |
//! | `gnome_shell_extension`                    | `accepted`    | connected                                        | `GnomeShellExtension`          | `DesktopFileId`   |
//! | `gnome_shell_extension`                    | anything else | anything                                         | `Unsupported`                  | -                 |
//! | `unavailable` / `macos_workspace` (Linux) | (any)         | (any)                                            | `Unsupported`                  | -                 |

use serde::{Deserialize, Serialize};

/// Concrete classification the picker policy derives from the
/// running session.
///
/// The variant is purely declarative: the picker command decides
/// the strategy from the variant the resolution helper produces
/// and never inspects the raw [`ActiveAppBackendKind`] or the
/// GNOME integration state itself. Keeping the mapping in one
/// place ensures the catalog command and the catalog-add command
/// can never disagree on which strategy applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinuxPickerBackend {
    /// EWMH probe is the active-app backend. Applies to plain X11
    /// sessions and to Wayland sessions that expose XWayland. The
    /// published identifier is the `WM_CLASS` class segment which
    /// the `.desktop` matcher resolves through
    /// `StartupWMClass > X-GNOME-WMClass > filename stem >
    /// Exec basename`.
    X11OrXWaylandEwmh,
    /// A native Wayland probe is the active-app backend. Applies to
    /// sessions that bound `ext-foreign-toplevel-list-v1` or
    /// `zwlr_foreign_toplevel_management_unstable_v1`. The
    /// published `app_id` matches `StartupWMClass` by freedesktop
    /// convention, so the same `WmClass` strategy applies.
    WaylandNative,
    /// The GNOME Shell extension is the active-app backend AND the
    /// user accepted the integration AND the extension is installed
    /// AND the listener is connected. The published identifier is
    /// the Desktop File ID the extension forwards verbatim
    /// (`firefox.desktop`).
    GnomeShellExtension,
    /// The session cannot guarantee a deterministic mapping. The
    /// picker MUST return the `Unsupported` envelope and the
    /// frontend MUST keep the manual-entry surface available.
    Unsupported,
}

impl LinuxPickerBackend {
    /// Stable snake_case identifier for diagnostics.
    pub fn as_str(self) -> &'static str {
        match self {
            LinuxPickerBackend::X11OrXWaylandEwmh => "x11_or_xwayland_ewmh",
            LinuxPickerBackend::WaylandNative => "wayland_native",
            LinuxPickerBackend::GnomeShellExtension => "gnome_shell_extension",
            LinuxPickerBackend::Unsupported => "unsupported",
        }
    }

    /// Identifier strategy the catalog must apply for this backend.
    /// Returns `None` when the backend does not justify offering the
    /// picker.
    pub fn strategy(self) -> Option<crate::IdentifierStrategy> {
        match self {
            LinuxPickerBackend::X11OrXWaylandEwmh | LinuxPickerBackend::WaylandNative => {
                Some(crate::IdentifierStrategy::WmClass)
            }
            LinuxPickerBackend::GnomeShellExtension => {
                Some(crate::IdentifierStrategy::DesktopFileId)
            }
            LinuxPickerBackend::Unsupported => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn as_str_is_stable_for_each_variant() {
        assert_eq!(
            LinuxPickerBackend::X11OrXWaylandEwmh.as_str(),
            "x11_or_xwayland_ewmh"
        );
        assert_eq!(LinuxPickerBackend::WaylandNative.as_str(), "wayland_native");
        assert_eq!(
            LinuxPickerBackend::GnomeShellExtension.as_str(),
            "gnome_shell_extension"
        );
        assert_eq!(LinuxPickerBackend::Unsupported.as_str(), "unsupported");
    }

    #[test]
    fn strategy_maps_to_identifier_strategy_for_supported_backends() {
        assert_eq!(
            LinuxPickerBackend::X11OrXWaylandEwmh.strategy(),
            Some(crate::IdentifierStrategy::WmClass)
        );
        assert_eq!(
            LinuxPickerBackend::WaylandNative.strategy(),
            Some(crate::IdentifierStrategy::WmClass)
        );
        assert_eq!(
            LinuxPickerBackend::GnomeShellExtension.strategy(),
            Some(crate::IdentifierStrategy::DesktopFileId)
        );
    }

    #[test]
    fn unsupported_backend_has_no_strategy() {
        assert_eq!(LinuxPickerBackend::Unsupported.strategy(), None);
    }

    #[test]
    fn variants_round_trip_through_serde() {
        for variant in [
            LinuxPickerBackend::X11OrXWaylandEwmh,
            LinuxPickerBackend::WaylandNative,
            LinuxPickerBackend::GnomeShellExtension,
            LinuxPickerBackend::Unsupported,
        ] {
            let json = serde_json::to_string(&variant).expect("serialise");
            let parsed: LinuxPickerBackend = serde_json::from_str(&json).expect("parse");
            assert_eq!(parsed, variant);
        }
    }
}
