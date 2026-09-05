//! Quick-paste orchestration.
//!
//! [`PasteService`] glues the platform clipboard backend and the
//! platform paste controller together with the entry repository. It
//! guarantees the user-visible contract: the source history entry is
//! never mutated when the OS-level paste fails, and any actionable
//! failure surfaces as a typed [`PasteOutcome`] carrying a
//! [`PlatformGuidance`] payload the frontend can render directly.
//
// `PasteOutcome::Failed` is intentionally a fat error variant: it
// carries both a free-form `message` and an optional `PlatformGuidance`
// so the renderer can show actionable copy without a second round-trip.
// The size is fine for our use case (a single, infrequent error path)
// so we silence the `result_large_err` lint module-wide rather than
// boxing the variant or wrapping each helper.
#![allow(clippy::result_large_err)]

use std::sync::Arc;

use clipvault_db::{EntryRecord, EntryRepository, EntryRepositoryError};
use thiserror::Error;
use tracing::warn;

use clipvault_platform::{
    backend_unavailable_guidance, linux_unknown_session_guidance,
    linux_wayland_unsupported_guidance, linux_x11_backend_unavailable_guidance, unknown_guidance,
    Capability, ClipboardBackendError, ClipboardPayload, DisplayServer, OsFamily, PasteController,
    PasteError, PlatformGuidance, RichTextPayload,
};

use crate::bootstrap::AppContext;
use crate::clipboard_assets::{decode_png, ClipboardAssetStore};
use crate::paste_suppression::{image_fingerprint, PasteSuppression, SuppressionFingerprint};
use crate::platform::ClipboardBackend;
use crate::rich_text::RichTextAssetStore;

/// Capability name used by [`PasteOutcome::CapabilityUnavailable`].
pub const SYNTHETIC_PASTE_CAPABILITY: &str = "synthetic_paste";

/// Capability name reported when the host cannot put a raster image on
/// the clipboard. Mirrors [`Capability::ClipboardWriteImage`] so the
/// frontend can match the same string it receives in the capability
/// matrix.
pub const CLIPBOARD_WRITE_IMAGE_CAPABILITY: &str = "clipboard_write_image";

/// Capability name reported when the host cannot put rich text on the
/// clipboard. Mirrors [`Capability::ClipboardWriteRichText`].
pub const CLIPBOARD_WRITE_RICH_TEXT_CAPABILITY: &str = "clipboard_write_rich_text";

/// Caller-supplied mode for [`PasteService::paste_entry`]. The default
/// (`Plain`) preserves the legacy quick-paste flow: write the
/// canonical plain text and trigger the paste. `Rich` keeps the
/// available rich representations on the clipboard and only falls
/// back to plain text when the host cannot publish them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasteMode {
    /// Write only the canonical plain text. This is the default and
    /// the mode quick-paste uses.
    Plain,
    /// Write the original rich representations (HTML / RTF) and the
    /// plain text as a fallback flavour in a single clipboard
    /// operation.
    Rich,
}

impl PasteMode {
    /// Stable snake_case wire format the frontend sends and the
    /// Tauri command parses. `None` and any unknown value are mapped
    /// to [`PasteMode::Plain`] so the default always succeeds.
    pub fn from_wire(value: Option<&str>) -> Self {
        match value {
            Some("rich") => PasteMode::Rich,
            _ => PasteMode::Plain,
        }
    }

    pub fn as_wire(self) -> &'static str {
        match self {
            PasteMode::Plain => "plain",
            PasteMode::Rich => "rich",
        }
    }
}

/// Outcome of attempting to paste a history entry.
///
/// The enum intentionally avoids carrying the clipboard payload: the
/// caller already has the entry, and the variants are pure status.
/// `guidance` is only populated for actionable failures and never
/// contains clipboard content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PasteOutcome {
    /// Clipboard write + paste trigger both succeeded.
    Pasted { id: i64 },
    /// Rich-text paste was requested but the session can only publish
    /// plain text: ClipVault wrote the canonical plain text, triggered
    /// the paste and reports this branch so the UI can explain the
    /// downgrade without surfacing it as a failure.
    PastedPlainFallback { id: i64 },
    /// The requested entry id no longer exists in the local database.
    /// We do not modify anything.
    Failed {
        kind: &'static str,
        message: String,
        guidance: Option<PlatformGuidance>,
    },
    /// The paste action could not run because the platform layer does
    /// not support the requested capability (synthetic paste, image
    /// write or rich-text write). When the underlying cause is
    /// actionable, `guidance` carries a typed remediation payload.
    CapabilityUnavailable {
        capability: &'static str,
        guidance: Option<PlatformGuidance>,
    },
}

impl PasteOutcome {
    pub fn kind(&self) -> &'static str {
        match self {
            PasteOutcome::Pasted { .. } => "pasted",
            PasteOutcome::PastedPlainFallback { .. } => "pasted_plain_fallback",
            PasteOutcome::Failed { .. } => "failed",
            PasteOutcome::CapabilityUnavailable { .. } => "capability_unavailable",
        }
    }

    pub fn guidance(&self) -> Option<&PlatformGuidance> {
        match self {
            PasteOutcome::Pasted { .. } | PasteOutcome::PastedPlainFallback { .. } => None,
            PasteOutcome::Failed { guidance, .. }
            | PasteOutcome::CapabilityUnavailable { guidance, .. } => guidance.as_ref(),
        }
    }
}

#[derive(Debug, Error)]
pub enum PasteServiceError {
    #[error("entry repository error: {0}")]
    Repository(#[from] EntryRepositoryError),
}

/// Orchestrator that performs "load → write → trigger paste" without
/// mutating the history row.
#[derive(Clone)]
pub struct PasteService {
    clipboard: Arc<dyn ClipboardBackend>,
    paste: Arc<dyn PasteController>,
    /// Local asset store used to load the bytes of an image entry.
    /// `None` in unit tests that only exercise the textual path.
    asset_store: Option<ClipboardAssetStore>,
    /// Local rich-text asset store used to load the original HTML / RTF
    /// bytes a rich entry references.
    rich_asset_store: Option<RichTextAssetStore>,
    /// Suppression registry consulted by the capture watcher. The
    /// service arms a token before every clipboard write and clears
    /// it immediately on a write failure so the watcher never
    /// creates a card for a payload the user did not actually copy.
    /// Optional so legacy tests can exercise the textual path
    /// without wiring a registry; the production bootstrap always
    /// attaches one through [`Self::with_paste_suppression`].
    paste_suppression: Option<PasteSuppression>,
}

impl PasteService {
    pub fn new(clipboard: Arc<dyn ClipboardBackend>, paste: Arc<dyn PasteController>) -> Self {
        Self {
            clipboard,
            paste,
            asset_store: None,
            rich_asset_store: None,
            paste_suppression: None,
        }
    }

    /// Attach the local asset store so image entries can be pasted.
    pub fn with_asset_store(mut self, store: ClipboardAssetStore) -> Self {
        self.asset_store = Some(store);
        self
    }

    /// Attach the rich-text asset store.
    pub fn with_rich_asset_store(mut self, store: RichTextAssetStore) -> Self {
        self.rich_asset_store = Some(store);
        self
    }

    /// Attach the suppression registry consulted by the capture
    /// watcher. The bootstrap always wires one so a paste that
    /// writes to the clipboard never creates an autocapture.
    pub fn with_paste_suppression(mut self, registry: PasteSuppression) -> Self {
        self.paste_suppression = Some(registry);
        self
    }

    /// Paste the entry identified by `entry_id` in `mode`.
    ///
    /// The default mode (no `mode` argument) preserves the legacy
    /// quick-paste behaviour: write the canonical plain text and
    /// trigger the paste. `PasteMode::Rich` writes the available rich
    /// representations and only falls back to plain text when the host
    /// cannot publish them.
    ///
    /// The method is total: it never panics on platform errors and
    /// never modifies the history row. A failure to load the entry,
    /// write the clipboard or trigger the paste is converted into the
    /// appropriate [`PasteOutcome::Failed`] or
    /// [`PasteOutcome::CapabilityUnavailable`] variant, optionally
    /// carrying typed [`PlatformGuidance`].
    pub fn paste_entry(
        &self,
        context: &AppContext,
        entry_id: i64,
        mode: PasteMode,
    ) -> PasteOutcome {
        // Step 1: load the entry without mutating anything.
        let record = match self.load_record(context, entry_id) {
            Ok(record) => record,
            Err(outcome) => return outcome,
        };

        // Step 2: write the matching payload to the clipboard. The
        // dispatch is the documented one: an image row writes its
        // bitmap, a rich row writes its rich representations in the
        // chosen mode, every other row writes its text. Rich mode on a
        // plain-text row collapses to the textual write.
        let mut used_plain_fallback = false;
        if let Some(outcome) = self.write_payload_for_record(context, &record, mode) {
            match outcome {
                WriteOutcome::StopWith(outcome) => return outcome,
                WriteOutcome::FallBackToPlain => {
                    // The session can publish plain text but not rich
                    // text. Fall through and write the canonical text
                    // so the paste still completes.
                    used_plain_fallback = true;
                }
            }
        }

        if used_plain_fallback {
            if let Err(error) = self.clipboard.write_text(&record.content) {
                warn!(kind = error.kind_str(), "paste: clipboard write failed");
                let guidance = classify_clipboard_failure(context, Capability::ClipboardWrite);
                return PasteOutcome::Failed {
                    kind: "clipboard",
                    message: error.to_string(),
                    guidance,
                };
            }
        }

        // Step 3: trigger the paste. If the platform layer refuses,
        // surface a typed capability error so the UI can disable the
        // action and show the user what to do.
        let paste_result = self.paste.paste();
        if let Err(error) = paste_result {
            // A failed synthetic paste MUST NOT leave the token
            // armed: the user-visible "paste" never happened, so a
            // later copy of the same content must be captured
            // normally. The next paste arms a fresh token.
            self.clear_suppression();
            return match error {
                PasteError::Unavailable => PasteOutcome::CapabilityUnavailable {
                    capability: SYNTHETIC_PASTE_CAPABILITY,
                    guidance: classify_unavailable(context),
                },
                PasteError::PermissionRequired { guidance } => {
                    PasteOutcome::CapabilityUnavailable {
                        capability: SYNTHETIC_PASTE_CAPABILITY,
                        guidance: Some(guidance),
                    }
                }
                PasteError::UnsupportedSession { guidance } => {
                    PasteOutcome::CapabilityUnavailable {
                        capability: SYNTHETIC_PASTE_CAPABILITY,
                        guidance: Some(guidance),
                    }
                }
                PasteError::Backend { details } => {
                    warn!(error = %details, "paste: paste trigger failed");
                    PasteOutcome::Failed {
                        kind: "paste",
                        message: details,
                        guidance: classify_backend_failure(context),
                    }
                }
            };
        }

        if used_plain_fallback {
            PasteOutcome::PastedPlainFallback { id: record.id }
        } else {
            PasteOutcome::Pasted { id: record.id }
        }
    }

    /// Convenience overload that preserves the legacy signature:
    /// quick-paste always writes plain text and triggers the paste.
    pub fn paste_plain_entry(&self, context: &AppContext, entry_id: i64) -> PasteOutcome {
        self.paste_entry(context, entry_id, PasteMode::Plain)
    }

    fn load_record(
        &self,
        context: &AppContext,
        entry_id: i64,
    ) -> Result<EntryRecord, PasteOutcome> {
        let mut db = context.database().lock();
        let repo = EntryRepository::new(db.connection_mut());
        match repo.find_by_id(entry_id) {
            Ok(Some(record)) => Ok(record),
            Ok(None) => Err(PasteOutcome::Failed {
                kind: "not_found",
                message: format!("entry id {entry_id} not found in history"),
                guidance: None,
            }),
            Err(error) => {
                warn!(error = %error, "paste: failed to load entry");
                Err(PasteOutcome::Failed {
                    kind: "repository",
                    message: error.to_string(),
                    guidance: None,
                })
            }
        }
    }

    /// Decide which payload to write and execute the write. The
    /// returned [`WriteOutcome`] tells the caller whether to stop
    /// (a typed outcome must be surfaced), fall back to plain text
    /// (the session cannot publish rich text), or continue (the
    /// payload was published and the paste trigger should run).
    ///
    /// Every successful write arms a suppression token on the
    /// registry so the capture watcher skips the next observation
    /// of the same payload. The token is metadata-only — never the
    /// text, HTML, RTF or image bytes.
    fn write_payload_for_record(
        &self,
        context: &AppContext,
        record: &EntryRecord,
        mode: PasteMode,
    ) -> Option<WriteOutcome> {
        if record.is_renderable_image() {
            // Image rows keep their plain-paste path. The rich-text
            // branch is irrelevant for an image entry: the rich
            // payload is the bitmap itself, not HTML/RTF.
            return self
                .write_image_payload(context, record)
                .map(WriteOutcome::StopWith);
        }

        if record.has_rich_text() {
            return match self.write_rich_payload(context, record, mode) {
                Some(WriteOutcome::StopWith(outcome)) => Some(WriteOutcome::StopWith(outcome)),
                Some(WriteOutcome::FallBackToPlain) => Some(WriteOutcome::FallBackToPlain),
                None => None,
            };
        }

        // Plain-text row (textual or any non-rich, non-image variant).
        // Arm the suppression token before the write so the next
        // watcher tick — whether the paste trigger succeeds or fails —
        // is masked for the same plain text. The arm is a no-op when
        // the registry is not wired (legacy unit tests).
        self.arm_text_suppression(&record.content, None);
        if let Err(error) = self.clipboard.write_text(&record.content) {
            // Drop the token: a failed write means the clipboard
            // does not contain the payload, so masking the next
            // observation would discard a legitimate later copy.
            self.clear_suppression();
            warn!(kind = error.kind_str(), "paste: clipboard write failed");
            let guidance = classify_clipboard_failure(context, Capability::ClipboardWrite);
            return Some(WriteOutcome::StopWith(PasteOutcome::Failed {
                kind: "clipboard",
                message: error.to_string(),
                guidance,
            }));
        }
        None
    }

    /// Put the image referenced by `record` on the clipboard.
    ///
    /// Returns `Some(outcome)` when the caller must stop and surface
    /// that outcome, `None` when the write succeeded and the paste
    /// trigger should run. The history row is never touched on any
    /// branch.
    fn write_image_payload(
        &self,
        context: &AppContext,
        record: &EntryRecord,
    ) -> Option<PasteOutcome> {
        // Refuse early when the session cannot transport images at all:
        // this is the branch that must never silently fall back to
        // pasting the (empty) textual sentinel.
        if !self.clipboard.supports_image_write() {
            return Some(PasteOutcome::CapabilityUnavailable {
                capability: CLIPBOARD_WRITE_IMAGE_CAPABILITY,
                guidance: classify_image_capability(context),
            });
        }

        let Some(store) = self.asset_store.as_ref() else {
            return Some(PasteOutcome::Failed {
                kind: "asset_store_unavailable",
                message: "clipboard asset store unavailable".to_string(),
                guidance: None,
            });
        };
        let Some(asset_ref) = record.asset_ref.as_deref().filter(|r| !r.is_empty()) else {
            return Some(PasteOutcome::Failed {
                kind: "asset_missing",
                message: "image entry has no asset reference".to_string(),
                guidance: None,
            });
        };

        let bytes = match store.read_bytes(asset_ref) {
            Ok(bytes) => bytes,
            Err(error) => {
                // Metadata only: never the reference, never the bytes.
                warn!(reason = error.kind_str(), "paste: asset read failed");
                return Some(PasteOutcome::Failed {
                    kind: "asset_read",
                    message: error.kind_str().to_string(),
                    guidance: None,
                });
            }
        };
        let image = match decode_png(&bytes) {
            Ok(image) => image,
            Err(error) => {
                warn!(reason = error.kind_str(), "paste: asset decode failed");
                return Some(PasteOutcome::Failed {
                    kind: "asset_decode",
                    message: error.kind_str().to_string(),
                    guidance: None,
                });
            }
        };

        // Arm the suppression token with the watcher's image
        // fingerprint so the next observation of the same bitmap is
        // masked. The token lives in the registry for the default
        // TTL only.
        self.arm_image_suppression(&image);
        if let Err(error) = self.clipboard.write_image(&image) {
            self.clear_suppression();
            warn!(kind = error.kind_str(), "paste: image write failed");
            // An `Unavailable` answer from the adapter is a capability
            // problem, not a transient failure: report it as such so
            // the UI reuses the platform guidance surface.
            if let ClipboardBackendError::Unavailable { .. } = error {
                return Some(PasteOutcome::CapabilityUnavailable {
                    capability: CLIPBOARD_WRITE_IMAGE_CAPABILITY,
                    guidance: classify_image_capability(context),
                });
            }
            return Some(PasteOutcome::Failed {
                kind: "clipboard_image",
                message: error.to_string(),
                guidance: classify_clipboard_failure(context, Capability::ClipboardWriteImage),
            });
        }
        None
    }

    /// Put the rich-text representation of `record` on the clipboard.
    ///
    /// Mirrors the image branch: a `None` return means the paste
    /// trigger should run; `Some(StopWith(_))` means a typed outcome
    /// must be surfaced; `Some(FallBackToPlain)` means the session
    /// can only publish plain text and the caller should retry with
    /// the canonical text payload.
    fn write_rich_payload(
        &self,
        context: &AppContext,
        record: &EntryRecord,
        mode: PasteMode,
    ) -> Option<WriteOutcome> {
        let plain = record.content.clone();
        let payload = match self.build_rich_payload(record) {
            Ok(payload) => payload,
            Err(outcome) => return Some(WriteOutcome::StopWith(outcome)),
        };

        if matches!(mode, PasteMode::Rich) {
            self.arm_rich_suppression(&plain, &payload);
            match self
                .clipboard
                .write_payload(&ClipboardPayload::RichText(payload.clone()))
            {
                Ok(()) => return None,
                Err(ClipboardBackendError::Unavailable { .. }) => {
                    // The session cannot publish rich text. Fall back
                    // to plain text rather than failing: this is the
                    // documented `pasted_plain_fallback` path.
                    // The suppression token must be re-armed for the
                    // plain-text fallback so the next watcher tick
                    // is still masked.
                    warn!("paste: rich text unavailable, falling back to plain text");
                    self.arm_text_suppression(&plain, None);
                    return Some(WriteOutcome::FallBackToPlain);
                }
                Err(error) => {
                    self.clear_suppression();
                    warn!(
                        kind = error.kind_str(),
                        "paste: rich clipboard write failed"
                    );
                    let guidance =
                        classify_clipboard_failure(context, Capability::ClipboardWriteRichText);
                    return Some(WriteOutcome::StopWith(PasteOutcome::Failed {
                        kind: "clipboard_rich",
                        message: error.to_string(),
                        guidance,
                    }));
                }
            }
        }

        // `PasteMode::Plain` (or the implicit default) always writes
        // the canonical plain text for a rich entry. The token is
        // armed with no rich hash: the suppression contract is that
        // a plain write is identified by the plain text only, so the
        // watcher does not need a rich hash to recognise it.
        self.arm_text_suppression(&plain, None);
        if let Err(error) = self.clipboard.write_text(&plain) {
            self.clear_suppression();
            warn!(kind = error.kind_str(), "paste: clipboard write failed");
            let guidance = classify_clipboard_failure(context, Capability::ClipboardWrite);
            return Some(WriteOutcome::StopWith(PasteOutcome::Failed {
                kind: "clipboard",
                message: error.to_string(),
                guidance,
            }));
        }
        None
    }

    /// Reconstruct a [`RichTextPayload`] from a persisted entry's
    /// asset references. The original HTML and RTF bytes are read
    /// through the rich-text asset store; any decoding failure is
    /// surfaced as a typed [`PasteOutcome::Failed`].
    fn build_rich_payload(&self, record: &EntryRecord) -> Result<RichTextPayload, PasteOutcome> {
        let plain = record.content.clone();
        if plain.is_empty() {
            return Err(PasteOutcome::Failed {
                kind: "asset_missing",
                message: "rich entry has no plain text".to_string(),
                guidance: None,
            });
        }

        let html = record
            .rich_html_ref
            .as_deref()
            .filter(|r| !r.is_empty())
            .map(|reference| self.read_rich_asset(reference, "rich_html"))
            .transpose()?
            .and_then(|bytes| String::from_utf8(bytes).ok());

        let rtf = record
            .rich_rtf_ref
            .as_deref()
            .filter(|r| !r.is_empty())
            .map(|reference| self.read_rich_asset(reference, "rich_rtf"))
            .transpose()?;

        if html.is_none() && rtf.is_none() {
            return Err(PasteOutcome::Failed {
                kind: "asset_missing",
                message: "rich entry has no HTML or RTF bytes".to_string(),
                guidance: None,
            });
        }

        RichTextPayload::new(plain, html, rtf).map_err(|error| PasteOutcome::Failed {
            kind: "rich_payload_invalid",
            message: error.kind_str().to_string(),
            guidance: None,
        })
    }

    fn read_rich_asset(
        &self,
        reference: &str,
        label: &'static str,
    ) -> Result<Vec<u8>, PasteOutcome> {
        let Some(store) = self.rich_asset_store.as_ref() else {
            return Err(PasteOutcome::Failed {
                kind: "asset_store_unavailable",
                message: "rich text asset store unavailable".to_string(),
                guidance: None,
            });
        };
        store.read_bytes(reference).map_err(|error| {
            warn!(
                reason = error.kind_str(),
                asset = label,
                "paste: rich asset read failed"
            );
            PasteOutcome::Failed {
                kind: "asset_read",
                message: error.kind_str().to_string(),
                guidance: None,
            }
        })
    }

    /// Arm the suppression token for a plain-text write. `rich_hash`
    /// is supplied when the paste also published rich
    /// representations and the token needs to match the rich
    /// observation too; the caller's contract is that a plain
    /// payload passes `None`.
    fn arm_text_suppression(&self, plain: &str, _rich_hash: Option<String>) {
        let Some(registry) = self.paste_suppression.as_ref() else {
            return;
        };
        // The plain text identity is sufficient for `PasteMode::Plain`
        // because the watcher also observes the OS-normalised plain
        // text after the synthetic paste. The rich_hash parameter is
        // kept for callers that want to record an extra witness
        // (currently unused — kept for future rich-paste fallback
        // paths that may want to re-arm with both legs).
        let mut fingerprint =
            SuppressionFingerprint::from_payload(&ClipboardPayload::Text(plain.to_string()));
        fingerprint.rich_text_hash = _rich_hash;
        registry.arm(fingerprint);
    }

    /// Arm the suppression token for a rich-text write. The token
    /// carries both the plain-text hash and the canonical rich-text
    /// hash so an observation whose plain text matches but whose
    /// rich text is missing is still suppressed.
    fn arm_rich_suppression(&self, _plain: &str, payload: &RichTextPayload) {
        let Some(registry) = self.paste_suppression.as_ref() else {
            return;
        };
        let fingerprint =
            SuppressionFingerprint::from_payload(&ClipboardPayload::RichText(payload.clone()));
        let _ = _plain; // identifier inlined into the fingerprint above
        registry.arm(fingerprint);
    }

    /// Arm the suppression token for an image write. The fingerprint
    /// is computed from the shared image-hash helper so the
    /// suppression decision and the dedupe decision agree.
    fn arm_image_suppression(&self, image: &clipvault_platform::ClipboardImage) {
        let Some(registry) = self.paste_suppression.as_ref() else {
            return;
        };
        let image_hash = image_fingerprint(image);
        let fingerprint = SuppressionFingerprint {
            plain_text_hash: None,
            rich_text_hash: None,
            image_hash: Some(image_hash),
        };
        registry.arm(fingerprint);
    }

    /// Drop the suppression token. Called after the synthetic paste
    /// resolves or when a clipboard write fails: in both cases the
    /// registry must not stay armed past the paste round-trip.
    fn clear_suppression(&self) {
        if let Some(registry) = self.paste_suppression.as_ref() {
            registry.clear();
        }
    }
}

/// Internal control-flow return for the write stage.
#[derive(Debug, Clone)]
enum WriteOutcome {
    /// The caller must stop and surface the wrapped outcome.
    StopWith(PasteOutcome),
    /// The rich write failed for capability reasons; fall back to the
    /// plain text payload.
    FallBackToPlain,
}

/// Classify an "image clipboard write unavailable" error.
///
/// Wayland must **not** receive a permission prompt: ClipVault links no
/// Wayland data-control backend, so the limitation is structural to the
/// session. `linux_wayland_unsupported_guidance` is the
/// unsupported-session variant, which is exactly the honest message.
fn classify_image_capability(context: &AppContext) -> Option<PlatformGuidance> {
    let info = context.platform();
    let capability = CLIPBOARD_WRITE_IMAGE_CAPABILITY;
    let guidance = match (info.os_family, info.display_server) {
        (OsFamily::Linux, DisplayServer::Wayland) => linux_wayland_unsupported_guidance(capability),
        (OsFamily::Linux, DisplayServer::X11) => linux_x11_backend_unavailable_guidance(capability),
        (OsFamily::Linux, DisplayServer::Unknown) => linux_unknown_session_guidance(capability),
        (OsFamily::Macos, _) => backend_unavailable_guidance(capability),
        (OsFamily::Windows, _) | (OsFamily::Other, _) => unknown_guidance(capability),
    };
    Some(guidance)
}

/// Classify a "synthetic paste unavailable" error using the current
/// platform info. The classification is conservative: every Linux
/// Wayland session falls into the Wayland guidance, every Linux X11
/// session falls into the X11 backend guidance, and unknown sessions
/// get the catch-all guidance. macOS hosts only reach this branch
/// when the controller is the noop variant, which cannot happen in
/// production builds but is exercised by tests.
fn classify_unavailable(context: &AppContext) -> Option<PlatformGuidance> {
    let info = context.platform();
    let capability = SYNTHETIC_PASTE_CAPABILITY;
    let guidance = match (info.os_family, info.display_server) {
        (OsFamily::Linux, DisplayServer::Wayland) => linux_wayland_unsupported_guidance(capability),
        (OsFamily::Linux, DisplayServer::X11) => linux_x11_backend_unavailable_guidance(capability),
        (OsFamily::Linux, DisplayServer::Unknown) => linux_unknown_session_guidance(capability),
        (OsFamily::Macos, _) => backend_unavailable_guidance(capability),
        (OsFamily::Windows, _) | (OsFamily::Other, _) => unknown_guidance(capability),
    };
    Some(guidance)
}

fn classify_backend_failure(context: &AppContext) -> Option<PlatformGuidance> {
    let info = context.platform();
    let capability = SYNTHETIC_PASTE_CAPABILITY;
    let guidance = match (info.os_family, info.display_server) {
        (OsFamily::Linux, DisplayServer::X11) => linux_x11_backend_unavailable_guidance(capability),
        (OsFamily::Linux, DisplayServer::Wayland) => linux_wayland_unsupported_guidance(capability),
        (OsFamily::Linux, DisplayServer::Unknown) => linux_unknown_session_guidance(capability),
        (OsFamily::Macos, _) => backend_unavailable_guidance(capability),
        (OsFamily::Windows, _) | (OsFamily::Other, _) => unknown_guidance(capability),
    };
    Some(guidance)
}

fn classify_clipboard_failure(
    context: &AppContext,
    capability: Capability,
) -> Option<PlatformGuidance> {
    let info = context.platform();
    let capability = capability.as_str();
    let guidance = match (info.os_family, info.display_server) {
        (OsFamily::Linux, DisplayServer::X11) => linux_x11_backend_unavailable_guidance(capability),
        (OsFamily::Linux, DisplayServer::Wayland) => linux_wayland_unsupported_guidance(capability),
        (OsFamily::Linux, DisplayServer::Unknown) => linux_unknown_session_guidance(capability),
        (OsFamily::Macos, _) => backend_unavailable_guidance(capability),
        (OsFamily::Windows, _) | (OsFamily::Other, _) => unknown_guidance(capability),
    };
    Some(guidance)
}

// `PasteError` is re-exported through the crate root by `lib.rs` so
// callers can reach it as `clipvault_core::PasteError`. Tests in this
// module reach it through the `use` at the top of the file.

#[cfg(test)]
mod tests {
    use super::*;
    use clipvault_platform::PasteBackendKind;

    #[test]
    fn paste_outcome_kind_strings_are_stable() {
        assert_eq!(PasteOutcome::Pasted { id: 1 }.kind(), "pasted");
        assert_eq!(
            PasteOutcome::PastedPlainFallback { id: 1 }.kind(),
            "pasted_plain_fallback"
        );
        assert_eq!(
            PasteOutcome::Failed {
                kind: "x",
                message: "y".into(),
                guidance: None,
            }
            .kind(),
            "failed"
        );
        assert_eq!(
            PasteOutcome::CapabilityUnavailable {
                capability: SYNTHETIC_PASTE_CAPABILITY,
                guidance: None,
            }
            .kind(),
            "capability_unavailable"
        );
    }

    #[test]
    fn guidance_accessor_returns_none_for_success() {
        let outcome = PasteOutcome::Pasted { id: 7 };
        assert!(outcome.guidance().is_none());
        let outcome = PasteOutcome::PastedPlainFallback { id: 7 };
        assert!(outcome.guidance().is_none());
    }

    #[test]
    fn capability_unavailable_carries_guidance_when_present() {
        let outcome = PasteOutcome::CapabilityUnavailable {
            capability: SYNTHETIC_PASTE_CAPABILITY,
            guidance: Some(linux_wayland_unsupported_guidance(
                SYNTHETIC_PASTE_CAPABILITY,
            )),
        };
        let guidance = outcome.guidance().expect("guidance present");
        assert_eq!(guidance.capability, SYNTHETIC_PASTE_CAPABILITY);
    }

    #[test]
    fn synthetic_paste_capability_label_is_stable() {
        assert_eq!(SYNTHETIC_PASTE_CAPABILITY, "synthetic_paste");
    }

    #[test]
    fn paste_mode_round_trips_through_wire_strings() {
        for mode in [PasteMode::Plain, PasteMode::Rich] {
            assert_eq!(PasteMode::from_wire(Some(mode.as_wire())), mode);
        }
        assert_eq!(PasteMode::from_wire(None), PasteMode::Plain);
        assert_eq!(PasteMode::from_wire(Some("unknown")), PasteMode::Plain);
    }

    #[test]
    fn paste_backend_kind_strings_are_stable() {
        assert_eq!(
            PasteBackendKind::MacOsCoreGraphics.as_str(),
            "macos_cgevent"
        );
        assert_eq!(PasteBackendKind::X11Test.as_str(), "x11_xtest");
        assert_eq!(PasteBackendKind::Unavailable.as_str(), "unavailable");
    }
}
