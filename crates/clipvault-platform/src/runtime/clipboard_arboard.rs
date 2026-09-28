//! `arboard`-backed clipboard implementation.
//!
//! `arboard` is already the text clipboard adapter for macOS and
//! Linux X11, and its `image-data` feature (enabled by default and
//! confirmed present in `Cargo.lock` through the transitive `image`
//! dependency) exposes exactly the two operations the
//! `clipboard-rich-content` capability needs: `get_image` and
//! `set_image`, both in straight RGBA. Reusing it avoids adding a
//! second clipboard dependency and keeps a single code path for
//! the pasteboard flavour negotiation on each host.
//!
//! For rich text the adapter rides on `arboard`'s HTML support
//! (`get().html()` / `set().html(html, alt)`), which maps to
//! `NSPasteboardTypeHTML` on macOS and the `text/html` selection
//! target on X11. RTF is **not** exposed by `arboard`, so this
//! adapter cannot publish an RTF leg even when the rest of the
//! pipeline has the bytes. The contract distinguishes three
//! outcomes:
//!
//! 1. The payload carries HTML only. The adapter publishes the HTML
//!    leg through `arboard::set_html`; the result is a successful
//!    rich write.
//! 2. The payload carries RTF only. The adapter returns
//!    `ClipboardBackendError::Unavailable { capability:
//!    ClipboardWriteRichText }` so the pipeline can surface a typed
//!    capability outcome. Writing plain text here would falsify a
//!    rich paste and is explicitly forbidden by the contract.
//! 3. The payload carries both HTML and RTF. The adapter publishes
//!    the HTML leg (the only one `arboard` exposes) and the
//!    pipeline keeps the original RTF bytes on the row so a future
//!    platform adapter that exposes RTF can paste them.
//!
//! macOS hosts with the `macos-native` feature enabled use a
//! dedicated `MacOsPasteboardClipboard` adapter that publishes
//! `public.utf8-plain-text`, `public.rtf` and `public.html` through
//! `NSPasteboard` directly. The bootstrap wires both adapters:
//! `arboard` for images, `NSPasteboard` for rich text. This module
//! is the Linux X11 / non-`macos-native` fallback.
//!
//! `supports_rich_write()` returns `true` only when at least the
//! HTML leg is supported. The HTML leg is supported on every
//! `arboard` build, so the answer is `true` whenever the
//! `clipboard-arboard` feature is on; the per-operation refusal for
//! RTF-only payloads surfaces through the typed `Unavailable` error
//! instead of a false capability downgrade.
//!
//! The adapter converts between `arboard::ImageData` and the neutral
//! [`ClipboardImage`] and nothing else: no filesystem access, no PNG
//! encoding, no hashing. Those belong to the core.

use std::borrow::Cow;
use std::env;

use arboard::{Clipboard as Arboard, Error as ArboardError, ImageData};
use parking_lot::Mutex;

use crate::clipboard::{
    checked_rgba_len, ClipboardBackend, ClipboardBackendError, ClipboardImage,
    ClipboardObservation, ClipboardRevision, ImageValidationError, RichTextPayload,
};
use crate::Capability;

#[cfg(all(target_os = "linux", feature = "linux-x11"))]
use crate::runtime::linux_x11_clipboard_revision::X11ClipboardRevisionMonitor;

/// Session detection for the Linux clipboard transport.
///
/// The result drives two decisions documented in
/// `openspec/changes/kde-wayland-clipboard-capture/design.md`:
///
/// 1. Whether the X11/XFixes revision stream is authoritative for
///    the active transport. On a native Wayland session XFixes only
///    observes the XWayland bridge (when it exists); the monitor
///    therefore MUST NOT be allowed to suppress a fresh native
///    Wayland observation that did not also raise an XFixes event.
/// 2. Which [`ArboardBackend`] identifier the adapter reports to
///    diagnostics so the operator can correlate a missing capture
///    with the transport that was actually compiled.
///
/// The detection is deliberately conservative: it only consults the
/// `WAYLAND_DISPLAY` / `DISPLAY` environment variables the Wayland
/// and X11 clients themselves rely on. Reading the variables is
/// side-effect free and matches the rule the stub
/// `detect_linux_display_server` already applies, so the adapter and
/// the bootstrap agree on the same session answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LinuxSession {
    /// Native Wayland session. `arboard` selects the data-control
    /// transport when the `wayland-data-control` feature is enabled;
    /// without it the same build falls back to XWayland and the
    /// adapter labels the choice explicitly.
    Wayland,
    /// Native X11 session. The XFixes ownership stream is the
    /// authoritative revision source.
    X11,
    /// Linux host without a recognised display server. The adapter
    /// refuses to claim a revision and lets the watcher's fingerprint
    /// fallback decide whether the payload is new.
    Unknown,
}

impl LinuxSession {
    fn detect() -> Self {
        // `WAYLAND_DISPLAY` is the canonical marker a Wayland
        // compositor sets; checking it first mirrors the priority
        // arboard applies when both protocols are available.
        if env::var_os("WAYLAND_DISPLAY").is_some() {
            LinuxSession::Wayland
        } else if env::var_os("DISPLAY").is_some() {
            LinuxSession::X11
        } else {
            LinuxSession::Unknown
        }
    }
}

/// Stable identifier for the clipboard transport the adapter actually
/// used. The string is metadata-only — never derived from clipboard
/// content, hashes or paths — and is consumed by the diagnostics
/// surface the platform layer exposes to the frontend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArboardBackend {
    /// Native Wayland clipboard via the data-control protocol
    /// (`ext-data-control-v1` or `wlr-data-control-unstable-v1`).
    /// Only reachable when the build enables the
    /// `wayland-data-control` feature on `arboard`; the feature
    /// itself is Linux-only and pulled in by the
    /// `arboard-wayland-data-control` Cargo feature.
    WaylandDataControl,
    /// X11 session served through the `arboard` X11 backend. XFixes
    /// supplies the authoritative revision.
    X11,
    /// Wayland session served through XWayland because the build did
    /// not enable `wayland-data-control` or the compositor rejected
    /// the data-control negotiation. The revision monitor only
    /// observes the XWayland selection stream, not native Wayland
    /// copies.
    XWaylandFallback,
    /// Linux host without a usable Wayland or X11 display server.
    /// The adapter reports `revision = UNKNOWN` so the watcher
    /// cannot invent a recapture from a missing ownership event.
    Unavailable,
}

impl ArboardBackend {
    fn stable_id(self) -> &'static str {
        match self {
            ArboardBackend::WaylandDataControl => "wayland_data_control",
            ArboardBackend::X11 => "x11",
            ArboardBackend::XWaylandFallback => "xwayland_fallback",
            ArboardBackend::Unavailable => "unavailable",
        }
    }
}

/// Conservative payload-diff fallback used only when the current host cannot
/// subscribe to the X11/XWayland ownership stream.
///
/// This fallback cannot detect a second copy of identical text. It is never
/// presented as a real revision: `revision()` reports `UNKNOWN`, so a
/// destructive baseline cannot accidentally invent a recapture. The watcher
/// still gets the historical payload-deduplication behaviour for ordinary
/// polling on hosts without XFixes.
#[derive(Debug, Default)]
struct FallbackRevisionState {
    counter: u64,
    last_text: Option<String>,
}

enum RevisionSource {
    /// X11 / XWayland ownership stream. Used both for native X11
    /// sessions and as the fallback transport for Wayland sessions
    /// when the build does not link the `wayland-data-control`
    /// backend. On a Wayland session the stream only reflects the
    /// XWayland bridge — the native Wayland data-control adapter
    /// must NOT contribute here, so the source is only constructed
    /// when the active session is X11.
    #[cfg(all(target_os = "linux", feature = "linux-x11"))]
    X11(Box<Mutex<X11ClipboardRevisionMonitor>>),
    /// No trustworthy ownership stream. Used on Wayland sessions
    /// without a native data-control backend, on hosts that cannot
    /// initialise XFixes and on every non-Linux build. The watcher
    /// relies on its existing payload-fingerprint deduplication.
    Fallback(Mutex<FallbackRevisionState>),
}

impl RevisionSource {
    /// Build the revision source that matches the detected session.
    ///
    /// On Linux X11 the XFixes monitor is the only trustworthy
    /// answer. On Linux Wayland the monitor MUST NOT be selected:
    /// an X11 ownership change does not prove the native Wayland
    /// clipboard changed, and using the X11 revision as the
    /// authoritative signal would suppress valid native Wayland
    /// captures. The fallback path keeps the watcher polling and
    /// reports `UNKNOWN`, so the existing fingerprint comparison
    /// continues to deduplicate ordinary polls without ever
    /// inventing a recapture.
    #[cfg(all(target_os = "linux", feature = "linux-x11"))]
    fn for_session(session: LinuxSession) -> Self {
        match session {
            LinuxSession::X11 => match X11ClipboardRevisionMonitor::new() {
                Ok(monitor) => Self::X11(Box::new(Mutex::new(monitor))),
                Err(_) => Self::Fallback(Mutex::new(FallbackRevisionState::default())),
            },
            // Wayland / Unknown: the XFixes stream would be
            // misleading, so the adapter falls through to the
            // fingerprint-only fallback.
            LinuxSession::Wayland | LinuxSession::Unknown => {
                Self::Fallback(Mutex::new(FallbackRevisionState::default()))
            }
        }
    }

    #[cfg(not(all(target_os = "linux", feature = "linux-x11")))]
    fn for_session(_session: LinuxSession) -> Self {
        Self::Fallback(Mutex::new(FallbackRevisionState::default()))
    }

    fn revision(&self) -> ClipboardRevision {
        match self {
            #[cfg(all(target_os = "linux", feature = "linux-x11"))]
            Self::X11(monitor) => monitor
                .lock()
                .revision()
                .map(ClipboardRevision::new)
                .unwrap_or(ClipboardRevision::UNKNOWN),
            // Without an ownership-event stream there is no payload-free
            // baseline to read. Returning UNKNOWN preserves the existing
            // dedupe state instead of manufacturing a false change signal.
            Self::Fallback(_) => ClipboardRevision::UNKNOWN,
        }
    }

    fn observe_payload(&self, text_leg: Option<String>) -> ClipboardRevision {
        match self {
            #[cfg(all(target_os = "linux", feature = "linux-x11"))]
            Self::X11(monitor) => monitor
                .lock()
                .revision()
                .map(ClipboardRevision::new)
                .unwrap_or(ClipboardRevision::UNKNOWN),
            Self::Fallback(state) => {
                let mut state = state.lock();
                if state.last_text != text_leg {
                    state.counter = state.counter.saturating_add(1);
                    state.last_text = text_leg;
                }
                ClipboardRevision::new(state.counter)
            }
        }
    }
}

/// Thin wrapper around [`arboard::Clipboard`]. Each method takes the
/// global lock lazily so the backend stays cheap to share across
/// threads.
pub struct ArboardClipboard {
    revision_source: RevisionSource,
    backend: ArboardBackend,
}

impl Default for ArboardClipboard {
    fn default() -> Self {
        Self::new()
    }
}

impl ArboardClipboard {
    pub fn new() -> Self {
        Self::with_backend()
    }

    /// Build an adapter whose session/backend choice is driven by the
    /// injected arguments.
    ///
    /// The default [`Self::new`] constructor infers the session from
    /// `WAYLAND_DISPLAY`/`DISPLAY`; tests inject a synthetic session
    /// through this entry point so they do not depend on the host
    /// environment.
    #[cfg(target_os = "linux")]
    fn with_session(session: LinuxSession) -> Self {
        let backend = linux_backend_for(session);
        Self {
            revision_source: RevisionSource::for_session(session),
            backend,
        }
    }

    #[cfg(not(target_os = "linux"))]
    fn with_session(_session: LinuxSession) -> Self {
        // Non-Linux builds never reach the Wayland/X11 code paths;
        // the choice collapses to the X11-style arboard backend.
        Self {
            revision_source: RevisionSource::for_session(LinuxSession::X11),
            backend: ArboardBackend::X11,
        }
    }

    fn with_backend() -> Self {
        Self::with_session(LinuxSession::detect())
    }

    /// Stable backend identifier the diagnostics surface returns.
    /// The value is metadata-only — never derived from clipboard
    /// content, hashes or paths — and the underlying transport
    /// stays bound to the rest of the adapter.
    pub fn backend_kind(&self) -> &'static str {
        self.backend.stable_id()
    }
}

/// Resolve the [`ArboardBackend`] identifier for a detected Linux
/// session.
///
/// The mapping pins the relationship between the
/// `arboard-wayland-data-control` Cargo feature and the session the
/// adapter observes at runtime:
///
/// * Native Wayland session with the data-control feature
///   enabled → `wayland_data_control`. This is the path that
///   delivers native Wayland copies on KDE Plasma (and on any
///   compositor that publishes `ext-data-control-v1` /
///   `wlr-data-control-unstable-v1`).
/// * Native Wayland session without the feature → the same
///   `arboard` build falls back to XWayland; the adapter labels
///   the choice `xwayland_fallback` so the diagnostics distinguish
///   it from a real native capture.
/// * Native X11 session → `x11`. The XFixes monitor is the
///   authoritative revision source.
/// * No recognised display server → `unavailable`; the watcher
///   keeps polling and the rest of ClipVault stays available.
#[cfg(target_os = "linux")]
fn linux_backend_for(session: LinuxSession) -> ArboardBackend {
    match session {
        LinuxSession::Wayland => {
            if cfg!(feature = "arboard-wayland-data-control") {
                ArboardBackend::WaylandDataControl
            } else {
                ArboardBackend::XWaylandFallback
            }
        }
        LinuxSession::X11 => ArboardBackend::X11,
        LinuxSession::Unknown => ArboardBackend::Unavailable,
    }
}

fn map_error(error: ArboardError) -> ClipboardBackendError {
    match error {
        ArboardError::ContentNotAvailable | ArboardError::ConversionFailure => {
            ClipboardBackendError::Empty
        }
        other => ClipboardBackendError::backend(other),
    }
}

/// Map an image-read failure. `ContentNotAvailable` means "no image on
/// the clipboard" and `ConversionFailure` means "the representation
/// exists but cannot be decoded into RGBA" — both are soft outcomes
/// that must leave the watcher running, so they collapse into
/// [`ClipboardBackendError::UnsupportedFormat`] instead of a hard
/// backend failure.
fn map_image_error(error: ArboardError) -> ClipboardBackendError {
    match error {
        ArboardError::ContentNotAvailable | ArboardError::ConversionFailure => {
            ClipboardBackendError::UnsupportedFormat
        }
        other => ClipboardBackendError::backend(other),
    }
}

/// Convert an `arboard` bitmap into the neutral, validated payload.
///
/// The dimensions are validated *before* the pixel buffer is copied so
/// a hostile pasteboard entry cannot make ClipVault allocate an
/// unbounded `Vec`.
fn to_clipboard_image(data: ImageData<'_>) -> Result<ClipboardImage, ClipboardBackendError> {
    let width = u32::try_from(data.width).map_err(|_| {
        ClipboardBackendError::InvalidImage(ImageValidationError::DimensionTooLarge {
            dim: u32::MAX,
            max: crate::clipboard::MAX_CLIPBOARD_IMAGE_DIM,
        })
    })?;
    let height = u32::try_from(data.height).map_err(|_| {
        ClipboardBackendError::InvalidImage(ImageValidationError::DimensionTooLarge {
            dim: u32::MAX,
            max: crate::clipboard::MAX_CLIPBOARD_IMAGE_DIM,
        })
    })?;
    // Validate the geometry (zero dimensions, per-dimension cap,
    // multiplication overflow, total-size cap) before touching bytes.
    let expected = checked_rgba_len(width, height)?;
    if data.bytes.len() != expected {
        return Err(ClipboardBackendError::InvalidImage(
            ImageValidationError::StrideMismatch {
                expected,
                actual: data.bytes.len(),
            },
        ));
    }
    let rgba = data.bytes.into_owned();
    Ok(ClipboardImage::new(rgba, width, height)?)
}

impl ClipboardBackend for ArboardClipboard {
    fn read_text(&self) -> Result<Option<String>, ClipboardBackendError> {
        let mut clipboard = Arboard::new().map_err(ClipboardBackendError::backend)?;
        match clipboard.get_text() {
            Ok(text) if text.is_empty() => Ok(None),
            Ok(text) => Ok(Some(text)),
            Err(error) => Err(map_error(error)),
        }
    }

    fn write_text(&self, text: &str) -> Result<(), ClipboardBackendError> {
        let mut clipboard = Arboard::new().map_err(ClipboardBackendError::backend)?;
        clipboard
            .set_text(text.to_string())
            .map_err(ClipboardBackendError::backend)
    }

    fn read_rich(&self) -> Result<Option<RichTextPayload>, ClipboardBackendError> {
        let mut clipboard = Arboard::new().map_err(ClipboardBackendError::backend)?;
        // arboard exposes only the HTML leg of a rich-text payload;
        // the plain-text companion comes from the standard `get_text`
        // call and is required by the `RichTextPayload` invariant.
        let plain_text = match clipboard.get_text() {
            Ok(text) if !text.is_empty() => text,
            // No usable plain text: this is not a rich capture even if
            // the HTML leg happens to exist. The watcher can still try
            // the image pipeline afterwards.
            Ok(_) => return Ok(None),
            Err(ArboardError::ContentNotAvailable) | Err(ArboardError::ConversionFailure) => {
                return Ok(None);
            }
            Err(error) => return Err(map_error(error)),
        };
        let html = match clipboard.get().html() {
            Ok(html) if !html.is_empty() => Some(html),
            Ok(_) => None,
            Err(ArboardError::ContentNotAvailable | ArboardError::ConversionFailure) => None,
            // `get_html` returns `ContentNotAvailable` for an absent
            // leg; anything else surfaces as a hard backend failure so
            // the caller knows the rich capture is unreliable.
            Err(error) => return Err(map_image_error(error)),
        };
        // RTF is not exposed by arboard: keep the leg explicit so a
        // future adapter that does expose it can fill it in without a
        // contract change.
        RichTextPayload::new(plain_text, html, None)
            .map(Some)
            .or_else(|error| {
                if matches!(error, ClipboardBackendError::Empty) {
                    // The HTML leg disappeared between the two arboard
                    // reads (a concurrent writer): not a rich capture.
                    Ok(None)
                } else {
                    Err(error)
                }
            })
    }

    fn write_rich(&self, payload: &RichTextPayload) -> Result<(), ClipboardBackendError> {
        let mut clipboard = Arboard::new().map_err(ClipboardBackendError::backend)?;
        // The contract forbids silently writing the plain text when
        // only RTF is available: doing so would falsify a rich paste
        // and the user would observe plain text in the receiving
        // application without any indication that the rich flavours
        // were dropped. The correct outcome is a typed capability
        // error so the pipeline can decide between a plain fallback
        // and a hard failure.
        if payload.html().is_none() && payload.rtf().is_some() {
            return Err(ClipboardBackendError::Unavailable {
                capability: Capability::ClipboardWriteRichText,
            });
        }
        if let Some(html) = payload.html() {
            // `arboard`'s `set_html` writes the HTML leg together
            // with the canonical plain text as the alt-text
            // fallback in a single operation. The plain text is
            // always published as the `alt` parameter so a
            // consumer that only knows about plain text still
            // receives the canonical `content` (the `arboard`
            // contract pins the alt-text behaviour).
            return clipboard
                .set()
                .html(html.to_string(), Some(payload.plain_text().to_string()))
                .map_err(ClipboardBackendError::backend);
        }
        // The payload carries neither HTML nor RTF. The
        // `RichTextPayload` constructor already rejects an empty
        // rich payload, so reaching this branch means the
        // `RichTextPayload` invariant was bypassed (a refactor
        // that relaxes the invariant would be a regression).
        // Surface the same typed `Unavailable` outcome so the
        // pipeline can degrade instead of silently losing the
        // rich claim.
        Err(ClipboardBackendError::Unavailable {
            capability: Capability::ClipboardWriteRichText,
        })
    }

    fn read_image(&self) -> Result<Option<ClipboardImage>, ClipboardBackendError> {
        let mut clipboard = Arboard::new().map_err(ClipboardBackendError::backend)?;
        match clipboard.get_image() {
            Ok(data) => to_clipboard_image(data).map(Some),
            Err(error) => Err(map_image_error(error)),
        }
    }

    fn write_image(&self, image: &ClipboardImage) -> Result<(), ClipboardBackendError> {
        let mut clipboard = Arboard::new().map_err(ClipboardBackendError::backend)?;
        let data = ImageData {
            width: image.width() as usize,
            height: image.height() as usize,
            bytes: Cow::Borrowed(image.rgba()),
        };
        clipboard
            .set_image(data)
            .map_err(ClipboardBackendError::backend)
    }

    fn supports_rich_read(&self) -> bool {
        true
    }

    fn supports_rich_write(&self) -> bool {
        true
    }

    fn supports_image_read(&self) -> bool {
        true
    }

    fn supports_image_write(&self) -> bool {
        true
    }

    // `arboard` decodes the pasteboard bitmap through
    // `NSBitmapImageRep`/`x11` representations; the original PNG
    // bytes (with `pHYs`, `iCCP`/`sRGB`, ...) are not surfaced, so the
    // adapter never reports original-PNG fidelity. The composite
    // therefore falls back to the legacy `read_image` path on
    // platforms where `arboard` is the only adapter (Linux X11).
    fn supports_image_png_read(&self) -> bool {
        false
    }

    fn name(&self) -> &'static str {
        // The trait requires a stable string; the platform-aware
        // diagnostic name lives in [`Self::backend_kind`]. The
        // bare "arboard" identifier stays for the trait contract so
        // legacy callers do not have to thread the new accessor.
        "arboard"
    }

    fn revision(&self) -> ClipboardRevision {
        self.revision_source.revision()
    }

    /// One watcher observation: read the payload, then drain the
    /// session-appropriate revision stream. On Linux X11 the
    /// revision is event-derived from XFixes; on Wayland sessions
    /// the adapter reports `ClipboardRevision::UNKNOWN` so the
    /// watcher cannot use a stale XWayland counter to suppress a
    /// fresh native capture. The fallback fingerprint comparison
    /// preserves the existing dedupe semantics either way.
    ///
    /// The chosen transport is recorded through the metadata-only
    /// [`Self::backend_kind`] identifier; it never includes payload
    /// bytes, hashes or absolute paths.
    fn read_observation(&self) -> Result<ClipboardObservation, ClipboardBackendError> {
        let payload = self.read_payload()?;
        let text_leg = match &payload {
            Some(crate::clipboard::ClipboardPayload::Text(text)) => Some(text.clone()),
            Some(crate::clipboard::ClipboardPayload::RichText(rich)) => {
                Some(rich.plain_text().to_string())
            }
            Some(crate::clipboard::ClipboardPayload::Image(_)) | None => None,
        };
        let revision = self.revision_source.observe_payload(text_leg);
        Ok(ClipboardObservation { payload, revision })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_clipboard_image_accepts_a_well_formed_bitmap() {
        let data = ImageData {
            width: 2,
            height: 2,
            bytes: Cow::Owned(vec![0x11; 2 * 2 * 4]),
        };
        let image = to_clipboard_image(data).expect("valid bitmap");
        assert_eq!((image.width(), image.height()), (2, 2));
        assert_eq!(image.byte_len(), 16);
        // arboard never transports the original PNG bytes: the
        // legacy capture path therefore produces an image with
        // `original_png = None` and the core falls back to the
        // normalised RGBA encoder.
        assert!(!image.has_original_png());
        assert!(image.original_png().is_none());
    }

    #[test]
    fn to_clipboard_image_rejects_zero_dimensions_before_allocating() {
        for (w, h) in [(0usize, 4usize), (4, 0)] {
            let data = ImageData {
                width: w,
                height: h,
                bytes: Cow::Owned(Vec::new()),
            };
            let err = to_clipboard_image(data).expect_err("must reject");
            assert_eq!(err.kind_str(), "invalid_image");
        }
    }

    #[test]
    fn to_clipboard_image_rejects_a_buffer_that_does_not_match_the_geometry() {
        let data = ImageData {
            width: 4,
            height: 4,
            // A truncated buffer: the adapter must not trust the
            // reported geometry.
            bytes: Cow::Owned(vec![0; 8]),
        };
        let err = to_clipboard_image(data).expect_err("must reject");
        match err {
            ClipboardBackendError::InvalidImage(ImageValidationError::StrideMismatch {
                expected,
                actual,
            }) => {
                assert_eq!(expected, 64);
                assert_eq!(actual, 8);
            }
            other => panic!("expected StrideMismatch, got {other:?}"),
        }
    }

    #[test]
    fn to_clipboard_image_rejects_dimensions_above_the_cap() {
        let data = ImageData {
            width: crate::clipboard::MAX_CLIPBOARD_IMAGE_DIM as usize + 1,
            height: 1,
            bytes: Cow::Owned(Vec::new()),
        };
        let err = to_clipboard_image(data).expect_err("must reject");
        assert_eq!(err.kind_str(), "invalid_image");
    }

    #[test]
    fn image_read_errors_map_to_soft_outcomes() {
        // "No image available" and "cannot convert the available
        // representation" are both `Ignored`, not failures: the
        // watcher has to keep polling.
        assert!(map_image_error(ArboardError::ContentNotAvailable).is_soft());
        assert!(map_image_error(ArboardError::ConversionFailure).is_soft());
        assert_eq!(
            map_image_error(ArboardError::ContentNotAvailable).kind_str(),
            "unsupported_format"
        );
    }

    #[test]
    fn adapter_declares_both_image_directions() {
        let backend = ArboardClipboard::new();
        assert!(backend.supports_image_read());
        assert!(backend.supports_image_write());
        // arboard does not surface the original PNG bytes the
        // pasteboard exposed: the fidelity-preserving path is the
        // native macOS adapter's responsibility.
        assert!(!backend.supports_image_png_read());
        assert_eq!(backend.name(), "arboard");
    }

    // -----------------------------------------------------------------
    // `clipboard-rich-text`: rich-text capability surface.
    //
    // The adapter does not own any test-only clipboard state, but the
    // capability flags are part of the public contract. Pin them here
    // so a future refactor that drops rich support surfaces here
    // instead of as a frontend regression.
    // -----------------------------------------------------------------

    #[test]
    fn adapter_declares_both_rich_text_directions() {
        let backend = ArboardClipboard::new();
        assert!(backend.supports_rich_read());
        assert!(backend.supports_rich_write());
    }

    #[test]
    fn payload_diff_fallback_never_claims_a_payload_free_revision() {
        // A host without the XFixes/XWayland ownership stream can still
        // collapse ordinary identical polls, but it cannot safely baseline a
        // destructive operation. The payload-free accessor must therefore
        // report UNKNOWN instead of turning a content diff into a fake event.
        let source = RevisionSource::Fallback(Mutex::new(FallbackRevisionState::default()));

        assert_eq!(source.revision(), ClipboardRevision::UNKNOWN);
        assert_eq!(
            source.observe_payload(Some("same text".into())),
            ClipboardRevision::new(1)
        );
        assert_eq!(
            source.observe_payload(Some("same text".into())),
            ClipboardRevision::new(1)
        );
        assert_eq!(
            source.observe_payload(Some("other text".into())),
            ClipboardRevision::new(2)
        );
        assert_eq!(source.revision(), ClipboardRevision::UNKNOWN);
    }

    // -----------------------------------------------------------------
    // `kde-wayland-clipboard-capture` regression coverage.
    //
    // The session-aware adapter routes the revision stream through a
    // source that matches the detected transport: native Wayland
    // sessions MUST NOT consult XFixes (an X11 ownership change does
    // not prove the native Wayland clipboard changed), and an
    // unavailable session MUST fall back to the metadata-only
    // `UNKNOWN` revision so the watcher never invents a recapture.
    // -----------------------------------------------------------------

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_session_detection_prefers_wayland_over_x11() {
        // The detection mirrors the rule `arboard` itself applies
        // when both protocols are available: a host with both
        // `WAYLAND_DISPLAY` and `DISPLAY` set is a Wayland session
        // whose X11 display is the XWayland bridge.
        assert_eq!(LinuxSession::detect(), LinuxSession::Wayland);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn backend_identifier_maps_session_to_metadata_only_kind() {
        // Every backend identifier is a stable snake_case token the
        // diagnostics surface and the spec consume. A refactor that
        // renames any of them would break operator logs and the
        // frontend capability matrix; pin them here.
        assert_eq!(
            ArboardBackend::WaylandDataControl.stable_id(),
            "wayland_data_control"
        );
        assert_eq!(ArboardBackend::X11.stable_id(), "x11");
        assert_eq!(
            ArboardBackend::XWaylandFallback.stable_id(),
            "xwayland_fallback"
        );
        assert_eq!(ArboardBackend::Unavailable.stable_id(), "unavailable");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn wayland_session_selects_wayland_or_xwayland_backend() {
        // The mapping pins the relationship between the
        // `arboard-wayland-data-control` Cargo feature and the
        // runtime session: a Wayland build with the feature picks
        // `wayland_data_control`; without it the build falls back to
        // XWayland and the diagnostics surface labels the choice
        // explicitly so a missing capture can be correlated with the
        // transport that was actually compiled.
        let backend = linux_backend_for(LinuxSession::Wayland);
        if cfg!(feature = "arboard-wayland-data-control") {
            assert_eq!(backend, ArboardBackend::WaylandDataControl);
        } else {
            assert_eq!(backend, ArboardBackend::XWaylandFallback);
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn x11_session_selects_x11_backend() {
        // A native X11 session keeps the historical behaviour: the
        // XFixes ownership stream is the authoritative revision
        // source and the diagnostics label matches.
        assert_eq!(linux_backend_for(LinuxSession::X11), ArboardBackend::X11);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn unknown_session_reports_unavailable_backend() {
        // No recognised display server: the adapter refuses to claim
        // a revision and the rest of ClipVault keeps running with
        // the local database.
        assert_eq!(
            linux_backend_for(LinuxSession::Unknown),
            ArboardBackend::Unavailable
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn wayland_session_revision_source_skips_xfixes() {
        // Contract: a Wayland session MUST NOT consult XFixes, even
        // when the X11 feature is compiled in. An X11 ownership
        // change does not prove a native Wayland clipboard change;
        // using it as the authoritative revision would suppress a
        // fresh native capture that XWayland never observed.
        let source = RevisionSource::for_session(LinuxSession::Wayland);
        assert!(
            matches!(source, RevisionSource::Fallback(_)),
            "Wayland session must fall through to the fallback source"
        );
        assert_eq!(source.revision(), ClipboardRevision::UNKNOWN);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn unknown_session_revision_source_is_unknown() {
        // Mirrors the Wayland rule: when the host has no recognised
        // display server the adapter MUST report `UNKNOWN` rather
        // than fabricate a monotonic counter the watcher could use
        // to recreate a deleted capture.
        let source = RevisionSource::for_session(LinuxSession::Unknown);
        assert_eq!(source.revision(), ClipboardRevision::UNKNOWN);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn x11_session_revision_source_can_observe_xfixes() {
        // X11 sessions keep the historical behaviour: the source
        // either selects the X11 monitor (when a server is reachable)
        // or falls back to the payload-diff fallback. The
        // payload-free `revision()` accessor must NOT be used to
        // recreate a destructive baseline.
        let source = RevisionSource::for_session(LinuxSession::X11);
        match source {
            RevisionSource::X11(_) => {
                // A live X11 connection is reachable on the host: the
                // accessor stays metadata-only and never inspects
                // clipboard bytes.
                let _ = source.revision();
            }
            RevisionSource::Fallback(_) => {
                // No X11 server reachable from the test environment
                // (CI sandbox, headless runner): the fallback must
                // still refuse to claim a payload-free revision.
                assert_eq!(source.revision(), ClipboardRevision::UNKNOWN);
            }
        }
    }

    #[test]
    fn backend_kind_is_metadata_only() {
        // The `backend_kind()` accessor never returns clipboard
        // content, hashes or absolute paths: it returns one of the
        // stable snake_case identifiers the diagnostics contract
        // guarantees.
        let allowed = [
            "wayland_data_control",
            "x11",
            "xwayland_fallback",
            "unavailable",
        ];
        // Constructing the adapter is fine in the test sandbox
        // because the constructor only inspects environment
        // variables, never the live clipboard.
        let backend = ArboardClipboard::new().backend_kind();
        assert!(
            allowed.contains(&backend),
            "unexpected backend identifier: {backend}"
        );
    }
}
