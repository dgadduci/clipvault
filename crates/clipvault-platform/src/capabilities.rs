//! Capability matrix reported by the platform layer.
//!
//! Every [`Capabilities`] field describes whether ClipVault can use a
//! particular desktop integration on the current host. The matrix is a
//! typed, serializable surface so the frontend can render placeholders,
//! disable actions and surface limitations without parsing error strings.
//!
//! The crate exposes two complementary entry points:
//!
//! - [`detect_capabilities`]: pure function driven by [`PlatformInfo`]
//!   alone. Useful for unit tests that drive the matrix against
//!   synthetic [`PlatformInfo`] values. It always reports the "best
//!   case" for the host (so macOS reports [`Capabilities::ALL_AVAILABLE`])
//!   because it does not consult the operating system.
//! - [`detect_capabilities_runtime`]: production entry point that also
//!   calls the macOS Accessibility preflight. On hosts where the
//!   permission has not been granted, `synthetic_paste` is reported as
//!   `false`. The bootstrap and the refresh command call this entry
//!   point so the frontend never sees `synthetic_paste = true` when
//!   the underlying permission is missing.
//! - [`detect_capabilities_with_preflight`]: testable variant that
//!   takes the macOS preflight function as a parameter. Production
//!   code uses [`detect_capabilities_runtime`] which delegates here
//!   with the real preflight.

use serde::Serialize;

use crate::info::{DisplayServer, OsFamily, PlatformInfo};

/// Result of the macOS Accessibility preflight.
///
/// Returns `true` when the current process has been granted permission
/// to publish keyboard events, `false` otherwise. The implementation
/// delegates to `CGPreflightPostEventAccess` when the `macos-native`
/// feature is enabled; on other hosts or feature configurations it
/// returns `false` so the matrix stays conservative.
pub fn macos_preflight_event_access() -> bool {
    #[cfg(all(target_os = "macos", feature = "macos-native"))]
    {
        objc2_core_graphics::CGPreflightPostEventAccess()
    }
    #[cfg(not(all(target_os = "macos", feature = "macos-native")))]
    {
        false
    }
}

/// Bitmask-style summary of what the current desktop session exposes.
///
/// `true` means the capability is implemented and the corresponding
/// adapter should be wired in by the bootstrap; `false` means the
/// platform layer will refuse the operation with
/// [`crate::CapabilityError::Unavailable`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Capabilities {
    pub clipboard_read: bool,
    pub clipboard_write: bool,
    /// Whether the current session can read a raster image from the
    /// clipboard. Evaluated independently from [`Self::clipboard_read`]:
    /// a host that reads text is **not** assumed to read images.
    pub clipboard_read_image: bool,
    /// Whether the current session can write a raster image to the
    /// clipboard. Evaluated independently from
    /// [`Self::clipboard_write`] and from [`Self::clipboard_read_image`]
    /// because a session may allow one direction and not the other.
    pub clipboard_write_image: bool,
    /// Whether the current session can read rich text (HTML or RTF)
    /// alongside plain text. Evaluated independently from
    /// [`Self::clipboard_read`]: a host that reads plain text is
    /// **not** assumed to read rich text.
    pub clipboard_read_rich_text: bool,
    /// Whether the current session can write rich text to the
    /// clipboard. Evaluated independently from
    /// [`Self::clipboard_write`] and from
    /// [`Self::clipboard_read_rich_text`] because a session may allow
    /// one direction and not the other.
    pub clipboard_write_rich_text: bool,
    pub global_hotkey: bool,
    pub synthetic_paste: bool,
    pub active_application: bool,
    pub tray: bool,
}

impl Capabilities {
    /// All capabilities available (the best case, used on macOS).
    pub const ALL_AVAILABLE: Capabilities = Capabilities {
        clipboard_read: true,
        clipboard_write: true,
        clipboard_read_image: true,
        clipboard_write_image: true,
        clipboard_read_rich_text: true,
        clipboard_write_rich_text: true,
        global_hotkey: true,
        synthetic_paste: true,
        active_application: true,
        tray: true,
    };

    /// Capabilities available when no host-side integration is wired in.
    pub const NONE: Capabilities = Capabilities {
        clipboard_read: false,
        clipboard_write: false,
        clipboard_read_image: false,
        clipboard_write_image: false,
        clipboard_read_rich_text: false,
        clipboard_write_rich_text: false,
        global_hotkey: false,
        synthetic_paste: false,
        active_application: false,
        tray: false,
    };
}

/// Result of probing the host for raster-image clipboard transport.
///
/// The two directions are tracked separately because a session can
/// legitimately support one and not the other (for example a Wayland
/// compositor that accepts an offer but cannot serve one back).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageClipboardSupport {
    pub read: bool,
    pub write: bool,
}

impl ImageClipboardSupport {
    pub const NONE: ImageClipboardSupport = ImageClipboardSupport {
        read: false,
        write: false,
    };
    pub const BOTH: ImageClipboardSupport = ImageClipboardSupport {
        read: true,
        write: true,
    };
}

/// Result of probing the host for rich-text clipboard transport.
///
/// Mirrors [`ImageClipboardSupport`]: the two directions are tracked
/// separately because a session may expose the `text/html` flavour
/// without an RTF counterpart (or vice versa).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RichTextClipboardSupport {
    pub read: bool,
    pub write: bool,
}

impl RichTextClipboardSupport {
    pub const NONE: RichTextClipboardSupport = RichTextClipboardSupport {
        read: false,
        write: false,
    };
    pub const BOTH: RichTextClipboardSupport = RichTextClipboardSupport {
        read: true,
        write: true,
    };
}

/// Probe the host for image clipboard transport.
///
/// The probe is deliberately conservative and **never** infers image
/// support from the fact that `read_text` works. It answers from two
/// independent inputs:
///
/// 1. whether an image-capable clipboard adapter is actually linked
///    into this build ([`adapter_backed_support`]);
/// 2. whether the current session can transport images at all.
///
/// Per session:
///
/// - **macOS**: `NSPasteboard` exposes `public.tiff` / `public.png`
///   flavours for every application that copies an image, and the
///   `arboard` adapter implements both directions on top of it.
/// - **Linux X11**: the `arboard` X11 backend negotiates `image/png`
///   through the standard selection protocol.
/// - **Linux Wayland**: ClipVault links `arboard` *without* the
///   `wayland-data-control` feature, so a Wayland session is served
///   through XWayland at best. That is a structural limitation of the
///   session, not a permission the user can grant, so the probe reports
///   both directions unavailable. Text history keeps working.
/// - **Anything else** (unknown display server, Windows, other):
///   unavailable.
///
/// ## Why the probe does not open a clipboard handle
///
/// An earlier revision constructed an `arboard::Clipboard` to verify the
/// adapter. That turned out to be **observably side-effecting on
/// macOS**: creating the handle initialises AppKit, which flips the
/// answer of [`macos_preflight_event_access`] for the rest of the
/// process. Capability detection must not mutate the state of another
/// capability, so the probe stays a pure function of the build and the
/// session.
///
/// Real, per-operation failures are not lost: the adapter still returns
/// [`crate::ClipboardBackendError::Unavailable`],
/// [`crate::ClipboardBackendError::UnsupportedFormat`] or a backend
/// error when an actual read/write fails, and the paste pipeline
/// converts those into a typed capability outcome with platform
/// guidance. The matrix is the *conservative up-front answer*; the
/// adapter is the authority at operation time.
pub fn probe_image_clipboard(info: &PlatformInfo) -> ImageClipboardSupport {
    let session_allows = match info.os_family {
        OsFamily::Macos => true,
        OsFamily::Linux => match info.display_server {
            DisplayServer::X11 => true,
            // Structural: no `wayland-data-control` backend is linked.
            DisplayServer::Wayland | DisplayServer::Unknown => false,
        },
        OsFamily::Windows | OsFamily::Other => false,
    };
    if !session_allows {
        return ImageClipboardSupport::NONE;
    }
    adapter_backed_support()
}

/// Probe the host for rich-text clipboard transport.
///
/// Mirrors the structural rules of [`probe_image_clipboard`] so the
/// matrix stays consistent across the asset-style capabilities. The
/// rich-text clipboard rides on the same `arboard` adapter as the
/// image transport; on hosts where the adapter is linked and the
/// session can carry clipboard payloads the answer is "both".
///
/// Per session:
///
/// - **macOS**: `NSPasteboard` exposes `public.html` and `public.rtf`
///   flavours for any application that copies formatted text; `arboard`
///   surfaces both.
/// - **Linux X11**: `arboard` exposes `text/html` and `text/rtf`
///   through the standard selection protocol.
/// - **Linux Wayland**: ClipVault links `arboard` without the
///   `wayland-data-control` backend, so rich-text transport is
///   structural-unavailable. Plain text keeps working.
/// - **Anything else**: unavailable.
pub fn probe_rich_text_clipboard(info: &PlatformInfo) -> RichTextClipboardSupport {
    let session_allows = match info.os_family {
        OsFamily::Macos => true,
        OsFamily::Linux => match info.display_server {
            DisplayServer::X11 => true,
            DisplayServer::Wayland | DisplayServer::Unknown => false,
        },
        OsFamily::Windows | OsFamily::Other => false,
    };
    if !session_allows {
        return RichTextClipboardSupport::NONE;
    }
    adapter_backed_rich_support()
}

/// Whether a real rich-text-capable clipboard adapter is linked into
/// this build. Without the `clipboard-arboard` feature no adapter
/// exists, so the matrix must not promise rich-text transport.
fn adapter_backed_rich_support() -> RichTextClipboardSupport {
    #[cfg(feature = "clipboard-arboard")]
    {
        RichTextClipboardSupport::BOTH
    }
    #[cfg(not(feature = "clipboard-arboard"))]
    {
        RichTextClipboardSupport::NONE
    }
}

/// Whether a real image-capable clipboard adapter is linked into this
/// build. Without the `clipboard-arboard` feature no adapter exists, so
/// the matrix must not promise image transport.
///
/// This is a compile-time answer on purpose: see the side-effect note on
/// [`probe_image_clipboard`].
fn adapter_backed_support() -> ImageClipboardSupport {
    #[cfg(feature = "clipboard-arboard")]
    {
        ImageClipboardSupport::BOTH
    }
    #[cfg(not(feature = "clipboard-arboard"))]
    {
        ImageClipboardSupport::NONE
    }
}

impl Default for Capabilities {
    fn default() -> Self {
        Self::NONE
    }
}

/// Computes the [`Capabilities`] for a [`PlatformInfo`] detected at
/// startup. The detection is conservative: it never reports a capability
/// that the host may not actually provide.
///
/// The function is pure: callers can invoke it from tests with synthetic
/// `PlatformInfo` values to validate the matrix per OS family and
/// display server. It does **not** consult the macOS Accessibility
/// permission; use [`detect_capabilities_runtime`] for production
/// paths that need the actual preflight result.
pub fn detect_capabilities(info: &PlatformInfo) -> Capabilities {
    capabilities_for(info)
}

/// Recompute the [`Capabilities`] matrix from the host's runtime state.
///
/// On macOS this calls [`macos_preflight_event_access`] so the
/// `synthetic_paste` capability reflects the actual Accessibility
/// grant. On every host it calls [`probe_image_clipboard`] and
/// [`probe_rich_text_clipboard`] so the asset-style capabilities
/// reflect the real adapter/session result instead of the "best case"
/// the pure detection assumes.
///
/// The bootstrap and the `clipvault_refresh_capabilities` Tauri command
/// both call this entry point so the frontend never observes
/// `synthetic_paste = true` when the permission has not been granted,
/// nor `clipboard_*_image = true` or `clipboard_*_rich_text = true` on
/// a session that cannot transport the corresponding payload.
pub fn detect_capabilities_runtime(info: &PlatformInfo) -> Capabilities {
    detect_capabilities_with_probes(
        info,
        macos_preflight_event_access,
        probe_image_clipboard,
        probe_rich_text_clipboard,
    )
}

/// Testable variant that takes the macOS preflight function as a
/// parameter. Production code uses [`detect_capabilities_runtime`]
/// which delegates here with [`macos_preflight_event_access`]. Tests
/// pass a deterministic stub so they do not depend on the developer
/// machine's Accessibility state.
///
/// The asset capabilities keep the pure "best case per host" answer so
/// pre-existing callers observe no behaviour change; use
/// [`detect_capabilities_with_probes`] to drive them explicitly.
pub fn detect_capabilities_with_preflight(
    info: &PlatformInfo,
    macos_preflight: fn() -> bool,
) -> Capabilities {
    detect_capabilities_with_probes(
        info,
        macos_preflight,
        pure_image_support,
        pure_rich_text_support,
    )
}

/// Fully testable detection: the macOS Accessibility preflight, the
/// image-clipboard probe and the rich-text clipboard probe are all
/// injected.
///
/// Both probes are consulted for **every** host so a session that
/// cannot transport the corresponding payload never inherits the "best
/// case" answer. The probes can only *lower* the pure matrix, never
/// raise it: a host that structurally lacks image or rich-text
/// transport (Wayland, unknown display server) stays unavailable even
/// if the probe were to answer `true`.
pub fn detect_capabilities_with_probes(
    info: &PlatformInfo,
    macos_preflight: fn() -> bool,
    image_probe: fn(&PlatformInfo) -> ImageClipboardSupport,
    rich_probe: fn(&PlatformInfo) -> RichTextClipboardSupport,
) -> Capabilities {
    let mut caps = capabilities_for(info);
    if info.os_family == OsFamily::Macos {
        // Only lift `synthetic_paste` when the preflight explicitly
        // succeeds. The pure `capabilities_for` already returns
        // `true` for macOS, so we deliberately overwrite it here.
        caps.synthetic_paste = macos_preflight();
    }
    let probed_image = image_probe(info);
    // Intersection, not assignment: the probe cannot grant a
    // capability the host does not structurally provide.
    caps.clipboard_read_image &= probed_image.read;
    caps.clipboard_write_image &= probed_image.write;
    let probed_rich = rich_probe(info);
    caps.clipboard_read_rich_text &= probed_rich.read;
    caps.clipboard_write_rich_text &= probed_rich.write;
    caps
}

/// "Best case per host" image support used by the pure detection
/// entry points. Mirrors the structural rules of
/// [`probe_image_clipboard`] without touching the host.
fn pure_image_support(info: &PlatformInfo) -> ImageClipboardSupport {
    match info.os_family {
        OsFamily::Macos => ImageClipboardSupport::BOTH,
        OsFamily::Linux => match info.display_server {
            DisplayServer::X11 => ImageClipboardSupport::BOTH,
            DisplayServer::Wayland | DisplayServer::Unknown => ImageClipboardSupport::NONE,
        },
        OsFamily::Windows | OsFamily::Other => ImageClipboardSupport::NONE,
    }
}

/// "Best case per host" rich-text support used by the pure detection
/// entry points. Mirrors [`probe_rich_text_clipboard`] without
/// touching the host.
fn pure_rich_text_support(info: &PlatformInfo) -> RichTextClipboardSupport {
    match info.os_family {
        OsFamily::Macos => RichTextClipboardSupport::BOTH,
        OsFamily::Linux => match info.display_server {
            DisplayServer::X11 => RichTextClipboardSupport::BOTH,
            DisplayServer::Wayland | DisplayServer::Unknown => RichTextClipboardSupport::NONE,
        },
        OsFamily::Windows | OsFamily::Other => RichTextClipboardSupport::NONE,
    }
}

/// Recompute the [`Capabilities`] matrix for a refreshed [`PlatformInfo`].
///
/// Equivalent to [`detect_capabilities_runtime`]; exported separately so
/// callers can document the intent at the call site (the shell uses it
/// when the user returns from system settings after granting a
/// permission).
pub fn refresh_capabilities(info: &PlatformInfo) -> Capabilities {
    detect_capabilities_runtime(info)
}

fn capabilities_for(info: &PlatformInfo) -> Capabilities {
    match info.os_family {
        OsFamily::Macos => Capabilities::ALL_AVAILABLE,
        OsFamily::Linux => match info.display_server {
            DisplayServer::X11 => Capabilities {
                clipboard_read: true,
                clipboard_write: true,
                // The X11 selection protocol negotiates `image/png`
                // the same way it negotiates `UTF8_STRING`; the
                // runtime probe still has the final word.
                clipboard_read_image: true,
                clipboard_write_image: true,
                clipboard_read_rich_text: true,
                clipboard_write_rich_text: true,
                global_hotkey: true,
                synthetic_paste: true,
                active_application: true,
                tray: true,
            },
            DisplayServer::Wayland => Capabilities {
                // arboard works through XDG portals (xdg-desktop-portal
                // + wl-clipboard), which most distributions ship; we mark
                // it available and let the adapter surface failures.
                clipboard_read: true,
                clipboard_write: true,
                // Image transport is NOT inferred from the text result:
                // ClipVault links `arboard` without the
                // `wayland-data-control` backend, so a Wayland session
                // has no verifiable image path. This is a structural
                // session limitation, not a permission, so the UI must
                // show session guidance instead of a permission prompt.
                clipboard_read_image: false,
                clipboard_write_image: false,
                // Same structural rule for rich text: the rich-text
                // transport rides on the same `arboard` adapter. The
                // Wayland session has no verifiable rich-text path.
                clipboard_read_rich_text: false,
                clipboard_write_rich_text: false,
                // global-hotkey uses wlr-global-shortcuts when available;
                // we report it as available and surface failures through
                // HotkeyOutcome::Unsupported.
                global_hotkey: true,
                // No portable synthetic-paste API under Wayland: report
                // unavailable so the UI disables the action.
                synthetic_paste: false,
                // XWayland support is best-effort: when the session
                // exposes a real X11 window the EWMH probe can
                // identify the application, but a native Wayland
                // window never publishes through X11. The probe stays
                // `true` so the bootstrap tries the X11 connection
                // when `$DISPLAY` is set; the diagnostics surface
                // labels the result `xwayland_ewmh` (via
                // [`crate::ActiveAppBackendKind`]) when it succeeds and
                // a native Wayland application keeps reporting
                // `unavailable` instead of a fabricated name.
                active_application: true,
                // Status-notifier-item is supported by every modern
                // Wayland compositor.
                tray: true,
            },
            // Linux without a recognised display server: keep clipboard
            // and tray off because nothing concrete is available; the
            // app still runs with the local database.
            DisplayServer::Unknown => Capabilities::NONE,
        },
        OsFamily::Windows | OsFamily::Other => Capabilities::NONE,
    }
}

/// Canonical, machine-friendly name for each capability. Used by the
/// frontend, the `capability_unavailable` events emitted from the tray
/// and any future settings UI to refer to a capability without using the
/// field name verbatim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capability {
    ClipboardRead,
    ClipboardWrite,
    /// Raster-image read support, independent from [`Self::ClipboardRead`].
    ClipboardReadImage,
    /// Raster-image write support, independent from [`Self::ClipboardWrite`].
    ClipboardWriteImage,
    /// Rich-text read support, independent from [`Self::ClipboardRead`].
    ClipboardReadRichText,
    /// Rich-text write support, independent from [`Self::ClipboardWrite`].
    ClipboardWriteRichText,
    GlobalHotkey,
    SyntheticPaste,
    ActiveApplication,
    Tray,
}

impl std::fmt::Display for Capability {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Capability {
    pub fn as_str(self) -> &'static str {
        match self {
            Capability::ClipboardRead => "clipboard_read",
            Capability::ClipboardWrite => "clipboard_write",
            Capability::ClipboardReadImage => "clipboard_read_image",
            Capability::ClipboardWriteImage => "clipboard_write_image",
            Capability::ClipboardReadRichText => "clipboard_read_rich_text",
            Capability::ClipboardWriteRichText => "clipboard_write_rich_text",
            Capability::GlobalHotkey => "global_hotkey",
            Capability::SyntheticPaste => "synthetic_paste",
            Capability::ActiveApplication => "active_application",
            Capability::Tray => "tray",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn info(os: OsFamily, display: DisplayServer) -> PlatformInfo {
        PlatformInfo {
            home_dir: PathBuf::from("/tmp"),
            data_dir: PathBuf::from("/tmp/.clipvault"),
            os_family: os,
            display_server: display,
        }
    }

    #[test]
    fn macos_is_fully_capable() {
        let caps = detect_capabilities(&info(OsFamily::Macos, DisplayServer::Unknown));
        assert_eq!(caps, Capabilities::ALL_AVAILABLE);
    }

    #[test]
    fn linux_x11_supports_synthetic_paste_and_active_app() {
        let caps = detect_capabilities(&info(OsFamily::Linux, DisplayServer::X11));
        assert!(caps.clipboard_read);
        assert!(caps.clipboard_write);
        assert!(caps.global_hotkey);
        assert!(caps.synthetic_paste);
        assert!(caps.active_application);
        assert!(caps.tray);
    }

    #[test]
    fn linux_wayland_disables_synthetic_paste_but_keeps_active_app_xwayland_attempt() {
        let caps = detect_capabilities(&info(OsFamily::Linux, DisplayServer::Wayland));
        assert!(caps.clipboard_read);
        assert!(caps.clipboard_write);
        assert!(caps.global_hotkey);
        assert!(!caps.synthetic_paste);
        // XWayland support is best-effort: the capability stays
        // `true` so the bootstrap can attempt the EWMH probe when
        // `$DISPLAY` is set, but a native Wayland window never
        // publishes through X11 so the bootstrap falls back to the
        // no-op probe without fabricating a name.
        assert!(caps.active_application);
        assert!(caps.tray);
    }

    #[test]
    fn linux_without_display_reports_nothing() {
        let caps = detect_capabilities(&info(OsFamily::Linux, DisplayServer::Unknown));
        assert_eq!(caps, Capabilities::NONE);
    }

    #[test]
    fn windows_and_other_report_nothing() {
        assert_eq!(
            detect_capabilities(&info(OsFamily::Windows, DisplayServer::Unknown)),
            Capabilities::NONE
        );
        assert_eq!(
            detect_capabilities(&info(OsFamily::Other, DisplayServer::Unknown)),
            Capabilities::NONE
        );
    }

    #[test]
    fn capability_names_are_stable() {
        assert_eq!(Capability::ClipboardRead.as_str(), "clipboard_read");
        assert_eq!(Capability::ClipboardWrite.as_str(), "clipboard_write");
        assert_eq!(
            Capability::ClipboardReadImage.as_str(),
            "clipboard_read_image"
        );
        assert_eq!(
            Capability::ClipboardWriteImage.as_str(),
            "clipboard_write_image"
        );
        assert_eq!(Capability::GlobalHotkey.as_str(), "global_hotkey");
        assert_eq!(Capability::SyntheticPaste.as_str(), "synthetic_paste");
        assert_eq!(Capability::ActiveApplication.as_str(), "active_application");
        assert_eq!(Capability::Tray.as_str(), "tray");
    }

    // -----------------------------------------------------------------
    // `clipboard-rich-content`: image capabilities must be reported
    // independently from the text capabilities and must differ per
    // display server. The tests below pin the matrix so a future tweak
    // that starts inferring image support from text support surfaces
    // here instead of as a false promise in the UI.
    // -----------------------------------------------------------------

    fn no_image(_info: &PlatformInfo) -> ImageClipboardSupport {
        ImageClipboardSupport::NONE
    }

    fn both_images(_info: &PlatformInfo) -> ImageClipboardSupport {
        ImageClipboardSupport::BOTH
    }

    fn read_only_images(_info: &PlatformInfo) -> ImageClipboardSupport {
        ImageClipboardSupport {
            read: true,
            write: false,
        }
    }

    fn both_rich_text(_info: &PlatformInfo) -> RichTextClipboardSupport {
        RichTextClipboardSupport::BOTH
    }

    fn read_only_rich_text(_info: &PlatformInfo) -> RichTextClipboardSupport {
        RichTextClipboardSupport {
            read: true,
            write: false,
        }
    }

    fn granted() -> bool {
        true
    }

    #[test]
    fn text_capabilities_survive_when_image_probe_reports_nothing() {
        // Contract: "a platform can read and write text but cannot
        // verify image support" MUST keep text history usable.
        let caps = detect_capabilities_with_probes(
            &info(OsFamily::Macos, DisplayServer::Unknown),
            granted,
            no_image,
            both_rich_text,
        );
        assert!(caps.clipboard_read);
        assert!(caps.clipboard_write);
        assert!(!caps.clipboard_read_image);
        assert!(!caps.clipboard_write_image);
        assert!(caps.clipboard_read_rich_text);
        assert!(caps.clipboard_write_rich_text);
    }

    #[test]
    fn image_read_and_write_are_reported_independently() {
        let caps = detect_capabilities_with_probes(
            &info(OsFamily::Macos, DisplayServer::Unknown),
            granted,
            read_only_images,
            both_rich_text,
        );
        assert!(caps.clipboard_read_image);
        assert!(!caps.clipboard_write_image);
    }

    #[test]
    fn wayland_never_inherits_x11_image_support() {
        // Even a probe that claims full support cannot lift the
        // structural Wayland limitation: the matrix is an
        // intersection, never an assignment.
        let wayland = detect_capabilities_with_probes(
            &info(OsFamily::Linux, DisplayServer::Wayland),
            granted,
            both_images,
            both_rich_text,
        );
        assert!(!wayland.clipboard_read_image);
        assert!(!wayland.clipboard_write_image);
        // Text history keeps working under Wayland.
        assert!(wayland.clipboard_read);
        assert!(wayland.clipboard_write);
        // Same rule for rich text.
        assert!(!wayland.clipboard_read_rich_text);
        assert!(!wayland.clipboard_write_rich_text);

        let x11 = detect_capabilities_with_probes(
            &info(OsFamily::Linux, DisplayServer::X11),
            granted,
            both_images,
            both_rich_text,
        );
        assert!(x11.clipboard_read_image);
        assert!(x11.clipboard_write_image);
        assert!(x11.clipboard_read_rich_text);
        assert!(x11.clipboard_write_rich_text);
    }

    #[test]
    fn pure_detection_reports_image_support_per_display_server() {
        let macos = detect_capabilities(&info(OsFamily::Macos, DisplayServer::Unknown));
        assert!(macos.clipboard_read_image);
        assert!(macos.clipboard_write_image);
        assert!(macos.clipboard_read_rich_text);
        assert!(macos.clipboard_write_rich_text);

        let x11 = detect_capabilities(&info(OsFamily::Linux, DisplayServer::X11));
        assert!(x11.clipboard_read_image);
        assert!(x11.clipboard_write_image);
        assert!(x11.clipboard_read_rich_text);
        assert!(x11.clipboard_write_rich_text);

        let wayland = detect_capabilities(&info(OsFamily::Linux, DisplayServer::Wayland));
        assert!(!wayland.clipboard_read_image);
        assert!(!wayland.clipboard_write_image);
        assert!(!wayland.clipboard_read_rich_text);
        assert!(!wayland.clipboard_write_rich_text);

        let unknown = detect_capabilities(&info(OsFamily::Linux, DisplayServer::Unknown));
        assert!(!unknown.clipboard_read_image);
        assert!(!unknown.clipboard_write_image);
        assert!(!unknown.clipboard_read_rich_text);
        assert!(!unknown.clipboard_write_rich_text);
    }

    #[test]
    fn probe_image_clipboard_refuses_wayland_and_unknown_sessions() {
        // The real probe must never promise image transport on a
        // session ClipVault cannot serve, regardless of build features.
        assert_eq!(
            probe_image_clipboard(&info(OsFamily::Linux, DisplayServer::Wayland)),
            ImageClipboardSupport::NONE
        );
        assert_eq!(
            probe_image_clipboard(&info(OsFamily::Linux, DisplayServer::Unknown)),
            ImageClipboardSupport::NONE
        );
        assert_eq!(
            probe_image_clipboard(&info(OsFamily::Windows, DisplayServer::Unknown)),
            ImageClipboardSupport::NONE
        );
        assert_eq!(
            probe_image_clipboard(&info(OsFamily::Other, DisplayServer::Unknown)),
            ImageClipboardSupport::NONE
        );
    }

    #[test]
    fn probe_rich_text_clipboard_refuses_wayland_and_unknown_sessions() {
        // Mirrors the image probe rule: rich-text transport rides on
        // the same `arboard` adapter, so the structural Wayland
        // limitation applies equally.
        assert_eq!(
            probe_rich_text_clipboard(&info(OsFamily::Linux, DisplayServer::Wayland)),
            RichTextClipboardSupport::NONE
        );
        assert_eq!(
            probe_rich_text_clipboard(&info(OsFamily::Linux, DisplayServer::Unknown)),
            RichTextClipboardSupport::NONE
        );
        assert_eq!(
            probe_rich_text_clipboard(&info(OsFamily::Windows, DisplayServer::Unknown)),
            RichTextClipboardSupport::NONE
        );
        assert_eq!(
            probe_rich_text_clipboard(&info(OsFamily::Other, DisplayServer::Unknown)),
            RichTextClipboardSupport::NONE
        );
    }

    #[test]
    fn probe_image_clipboard_matches_the_linked_adapter_on_supported_sessions() {
        // On a session that structurally supports images the answer is
        // the build's answer: both directions when an image-capable
        // adapter is linked, neither otherwise.
        let expected = if cfg!(feature = "clipboard-arboard") {
            ImageClipboardSupport::BOTH
        } else {
            ImageClipboardSupport::NONE
        };
        assert_eq!(
            probe_image_clipboard(&info(OsFamily::Macos, DisplayServer::Unknown)),
            expected
        );
        assert_eq!(
            probe_image_clipboard(&info(OsFamily::Linux, DisplayServer::X11)),
            expected
        );
    }

    #[test]
    fn probe_rich_text_clipboard_matches_the_linked_adapter_on_supported_sessions() {
        // The rich-text probe is compiled off the same feature gate as
        // the image probe: a build that lacks the `arboard` adapter
        // must not promise rich-text transport either.
        let expected = if cfg!(feature = "clipboard-arboard") {
            RichTextClipboardSupport::BOTH
        } else {
            RichTextClipboardSupport::NONE
        };
        assert_eq!(
            probe_rich_text_clipboard(&info(OsFamily::Macos, DisplayServer::Unknown)),
            expected
        );
        assert_eq!(
            probe_rich_text_clipboard(&info(OsFamily::Linux, DisplayServer::X11)),
            expected
        );
    }

    #[test]
    fn rich_text_capabilities_are_reported_independently_per_session() {
        fn granted() -> bool {
            true
        }
        fn probe_image(_info: &PlatformInfo) -> ImageClipboardSupport {
            ImageClipboardSupport::BOTH
        }
        let info = |os: OsFamily, display: DisplayServer| PlatformInfo {
            home_dir: std::path::PathBuf::from("/tmp"),
            data_dir: std::path::PathBuf::from("/tmp/.clipvault"),
            os_family: os,
            display_server: display,
        };

        let macos = detect_capabilities_with_probes(
            &info(OsFamily::Macos, DisplayServer::Unknown),
            granted,
            probe_image,
            both_rich_text,
        );
        assert!(macos.clipboard_read_rich_text && macos.clipboard_write_rich_text);

        let x11 = detect_capabilities_with_probes(
            &info(OsFamily::Linux, DisplayServer::X11),
            granted,
            probe_image,
            both_rich_text,
        );
        assert!(x11.clipboard_read_rich_text && x11.clipboard_write_rich_text);

        let wayland = detect_capabilities_with_probes(
            &info(OsFamily::Linux, DisplayServer::Wayland),
            granted,
            probe_image,
            both_rich_text,
        );
        assert!(
            !wayland.clipboard_read_rich_text && !wayland.clipboard_write_rich_text,
            "Wayland must not inherit X11 rich-text support"
        );
    }

    #[test]
    fn rich_text_read_and_write_are_reported_independently() {
        let caps = detect_capabilities_with_probes(
            &info(OsFamily::Macos, DisplayServer::Unknown),
            granted,
            both_images,
            read_only_rich_text,
        );
        assert!(caps.clipboard_read_rich_text);
        assert!(!caps.clipboard_write_rich_text);
    }

    #[test]
    fn probe_image_clipboard_is_pure_and_repeatable() {
        // Regression guard for a real defect: an earlier revision opened
        // an `arboard` handle here, which initialised AppKit on macOS and
        // flipped the result of `macos_preflight_event_access` for the
        // rest of the process — capability detection silently mutated
        // another capability. The probe must be a pure function of the
        // build and the session.
        let target = info(OsFamily::Macos, DisplayServer::Unknown);
        let before = macos_preflight_event_access();
        let first = probe_image_clipboard(&target);
        let second = probe_image_clipboard(&target);
        let after = macos_preflight_event_access();
        assert_eq!(first, second, "the probe must be repeatable");
        assert_eq!(
            before, after,
            "probing image support must not disturb the Accessibility preflight"
        );
    }

    #[test]
    fn none_and_all_available_cover_every_image_field() {
        const { assert!(!Capabilities::NONE.clipboard_read_image) };
        const { assert!(!Capabilities::NONE.clipboard_write_image) };
        const { assert!(Capabilities::ALL_AVAILABLE.clipboard_read_image) };
        const { assert!(Capabilities::ALL_AVAILABLE.clipboard_write_image) };
        const { assert!(!Capabilities::NONE.clipboard_read_rich_text) };
        const { assert!(!Capabilities::NONE.clipboard_write_rich_text) };
        const { assert!(Capabilities::ALL_AVAILABLE.clipboard_read_rich_text) };
        const { assert!(Capabilities::ALL_AVAILABLE.clipboard_write_rich_text) };
    }

    #[test]
    fn capabilities_serialise_the_image_fields() {
        // The frontend reads the matrix as a flat object; the new
        // fields must appear with their stable snake_case names.
        let json = serde_json::to_string(&Capabilities::ALL_AVAILABLE).expect("serialise");
        assert!(json.contains("\"clipboard_read_image\":true"), "got {json}");
        assert!(
            json.contains("\"clipboard_write_image\":true"),
            "got {json}"
        );
        assert!(
            json.contains("\"clipboard_read_rich_text\":true"),
            "got {json}"
        );
        assert!(
            json.contains("\"clipboard_write_rich_text\":true"),
            "got {json}"
        );
    }

    #[test]
    fn capability_names_are_stable_with_rich_text() {
        assert_eq!(
            Capability::ClipboardReadRichText.as_str(),
            "clipboard_read_rich_text"
        );
        assert_eq!(
            Capability::ClipboardWriteRichText.as_str(),
            "clipboard_write_rich_text"
        );
    }

    #[test]
    fn macos_without_accessibility_reports_synthetic_paste_unavailable() {
        fn denied() -> bool {
            false
        }
        let caps = detect_capabilities_with_preflight(
            &info(OsFamily::Macos, DisplayServer::Unknown),
            denied,
        );
        // Every other capability is still available; only `synthetic_paste`
        // is gated by the preflight.
        assert!(caps.clipboard_read);
        assert!(caps.clipboard_write);
        assert!(caps.global_hotkey);
        assert!(caps.active_application);
        assert!(caps.tray);
        assert!(!caps.synthetic_paste);
    }

    #[test]
    fn macos_with_accessibility_reports_synthetic_paste_available() {
        fn granted() -> bool {
            true
        }
        let caps = detect_capabilities_with_preflight(
            &info(OsFamily::Macos, DisplayServer::Unknown),
            granted,
        );
        assert_eq!(caps, Capabilities::ALL_AVAILABLE);
    }

    #[test]
    fn preflight_only_affects_macos_synthetic_paste() {
        fn denied() -> bool {
            false
        }
        // Linux Wayland: preflight must not lift Wayland's structural
        // limitation away.
        let caps = detect_capabilities_with_preflight(
            &info(OsFamily::Linux, DisplayServer::Wayland),
            denied,
        );
        assert!(!caps.synthetic_paste);
        assert!(caps.tray);
    }

    #[test]
    fn refresh_after_granting_accessibility_lifts_synthetic_paste() {
        let info = info(OsFamily::Macos, DisplayServer::Unknown);
        let denied = detect_capabilities_with_preflight(&info, || false);
        assert!(!denied.synthetic_paste);
        let granted = detect_capabilities_with_preflight(&info, || true);
        assert!(granted.synthetic_paste);
        // Re-denying the preflight must drop the capability back down.
        let denied_again = detect_capabilities_with_preflight(&info, || false);
        assert!(!denied_again.synthetic_paste);
    }

    #[test]
    fn refresh_capabilities_matches_runtime_detection() {
        let info = info(OsFamily::Macos, DisplayServer::Unknown);
        // `refresh_capabilities` must call the runtime detection so the
        // matrix reflects the actual Accessibility state.
        let caps = refresh_capabilities(&info);
        assert_eq!(caps.synthetic_paste, macos_preflight_event_access());
    }
}
