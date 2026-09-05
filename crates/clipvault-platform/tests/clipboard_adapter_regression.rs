//! Adapter-level regression tests for the `clipboard-rich-text`
//! contract.
//!
//! These tests pin the platform-side guarantees the production
//! code must obey after the `clipboard-rich-text` change:
//!
//! - The `RichTextPayload` constructor enforces the invariant
//!   the adapters rely on: a payload with empty plain text or
//!   no rich representation is rejected, so a future refactor
//!   that relaxes the invariant surfaces here.
//! - The `Capability` enum exposes stable snake_case strings
//!   for the rich-read and rich-write capabilities so the
//!   diagnostics endpoint and the frontend can match on known
//!   identifiers.
//! - `RichTextPayload::into_parts` returns the canonical plain
//!   text plus the available rich legs; a refactor that drops
//!   the HTML leg surfaces here.
//!
//! The per-operation `write_rich` branches (HTML publish, RTF
//! refusal, plain fallback) are pinned by the integration
//! suite in `clipvault-core/tests/paste_regression.rs` where
//! the full paste pipeline runs against fakes.

use clipvault_platform::{Capability, ClipboardBackendError, RichTextPayload};

/// `RichTextPayload::new` MUST reject an empty plain text even
/// when the caller supplies rich legs. A future refactor that
/// relaxes the invariant surfaces here.
#[test]
fn rich_text_payload_construction_rejects_empty_plain_text() {
    let result = RichTextPayload::new(String::new(), Some("<p>x</p>".into()), None);
    assert!(matches!(result, Err(ClipboardBackendError::Empty)));
}

/// `RichTextPayload::new` MUST reject a payload with neither
/// rich leg even when the plain text is non-empty. The
/// adapter would never see such a payload in production, so
/// a future refactor that relaxes the invariant surfaces
/// here.
#[test]
fn rich_text_payload_construction_rejects_empty_legs() {
    let result = RichTextPayload::new("x".into(), None, None);
    assert!(matches!(result, Err(ClipboardBackendError::Empty)));
}

/// The `Capability` enum MUST expose a stable string for the
/// rich-write capability so the diagnostics endpoint and the
/// frontend can match on a known identifier.
#[test]
fn capability_string_for_rich_write_is_stable() {
    assert_eq!(
        Capability::ClipboardWriteRichText.as_str(),
        "clipboard_write_rich_text"
    );
    assert_eq!(
        Capability::ClipboardReadRichText.as_str(),
        "clipboard_read_rich_text"
    );
}

/// `RichTextPayload::into_parts` returns the canonical plain
/// text plus the available rich legs. The platform adapter
/// uses this accessor to forward the bytes to the OS
/// clipboard; a refactor that drops the HTML leg surfaces
/// here.
#[test]
fn rich_text_payload_into_parts_preserves_html_and_rtf() {
    let html = "<b>x</b>".to_string();
    let rtf = b"{\\rtf1 x}".to_vec();
    let payload =
        RichTextPayload::new("x".into(), Some(html.clone()), Some(rtf.clone())).expect("valid");
    let (plain, html_back, rtf_back) = payload.into_parts();
    assert_eq!(plain, "x");
    assert_eq!(html_back, Some(html));
    assert_eq!(rtf_back, Some(rtf));
}
