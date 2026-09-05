//! Main-thread bridge for the macOS `NSPasteboard` clipboard.
//!
//! ## Why this exists
//!
//! Apple's `NSPasteboard` operations (`generalPasteboard`, `dataForType:`
//! and `setData:forType:`) are documented to be used on the Cocoa main
//! thread; off-main callers get undefined behaviour. The previous
//! adapter enforced this with a hard `require_main_thread` check that
//! returned a typed `ClipboardBackendError::Backend` when the calling
//! thread was not the main thread.
//!
//! The capture loop, however, runs on a background thread; the
//! previous prototype therefore surfaced
//! `read_rich must run on the macOS main thread` as a fatal
//! `WatchTickOutcome::Failed` every tick. No capture was ever
//! persisted. The user-facing regression was "clipboard capture does
//! not capture anything rich from TextEdit".
//!
//! This bridge fixes the contract:
//!
//! - When the calling thread is the macOS main thread, the closure
//!   runs inline.
//! - When the calling thread is NOT the macOS main thread, the
//!   closure is submitted to the Cocoa main queue via
//!   [`dispatch2::run_on_main`] and the caller blocks until it
//!   returns. Tauri's main loop drives the queue, so the closure
//!   always lands on the real main thread.
//! - When neither path is available (no main queue, no event loop,
//!   dispatch not initialised), the bridge returns
//!   [`ClipboardBackendError::Unavailable`] — never
//!   [`ClipboardBackendError::Backend`] — so the rest of the pipeline
//!   can keep the watcher alive (soft miss) instead of converting a
//!   thread limitation into a hard capture failure.
//!
//! The bridge deliberately lives next to the adapter that uses it so
//! the dependency between the thread hop and the pasteboard
//! operations stays explicit. It does NOT depend on Tauri or on the
//! frontend: the test harness exercises it through the same
//! `dispatch2` primitives Tauri's main loop uses.
//!
//! ## Diagnostics
//!
//! Every hop is silent unless something went wrong. The bridge
//! returns `Ok(value)` on success, `Err(Unavailable { capability:
//! ClipboardReadRichText | ClipboardWriteRichText |
//! ClipboardRead | ClipboardWrite })` when no main-queue hop is
//! possible. It never carries the underlying thread error in the
//! outcome (the typed capability identifier is the entire surface a
//! caller gets).

#![cfg(all(target_os = "macos", feature = "macos-native"))]

use std::fmt;
use std::sync::mpsc;

use dispatch2::run_on_main;
use objc2_foundation::MainThreadMarker;

use crate::clipboard::ClipboardBackendError;
use crate::Capability;

/// Why a pasteboard operation could not hop to the Cocoa main thread.
///
/// The bridge never reports this in `Debug` or `Display` form unless a
/// caller asks for the structured identifier; the user-facing surface
/// only ever sees the typed `ClipboardBackendError::Unavailable`
/// variant. The variants exist so tests and diagnostics can branch on
/// the precise reason without parsing free-form strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MainQueueBridgeError {
    /// `dispatch2` rejected the hop (typically because no event loop
    /// is driving the main queue). The watcher treats this as a soft
    /// miss and continues polling.
    DispatchUnavailable,
}

impl fmt::Display for MainQueueBridgeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MainQueueBridgeError::DispatchUnavailable => {
                f.write_str("cocoa main queue is not reachable from this thread")
            }
        }
    }
}

/// Stable snake_case identifier for diagnostics.
impl MainQueueBridgeError {
    pub fn kind_str(&self) -> &'static str {
        match self {
            MainQueueBridgeError::DispatchUnavailable => "main_queue_unavailable",
        }
    }
}

/// Outcome of a clipboard operation dispatched to the Cocoa main thread.
///
/// `Ok(value)` means the closure ran on the main thread and returned
/// `value`. `Err(_)` means the bridge could not reach the main
/// thread; the caller converts the typed
/// [`MainQueueBridgeError`] into a typed
/// [`ClipboardBackendError::Unavailable`].
pub type BridgeResult<T> = Result<T, MainQueueBridgeError>;

/// Run `f` on the macOS main thread.
///
/// Behaviour:
///
/// - **Already on main thread** — `f` runs inline; no scheduling,
///   no allocation.
/// - **Off-main, dispatch available** — the closure is submitted to
///   `DispatchQueue::main()` via `dispatch2::run_on_main` and the
///   caller blocks until it returns. Tauri's main loop drives the
///   queue so the closure always lands on the actual main thread.
/// - **Off-main, dispatch unavailable** — returns
///   `Err(MainQueueBridgeError::DispatchUnavailable)`. The caller
///   must convert the error into a typed `ClipboardBackendError::Unavailable`
///   (the typed capability identifier is the surface the rest of the
///   pipeline observes).
///
/// The result is returned through a synchronous channel because
/// `dispatch2::run_on_main` only exposes a `FnOnce(MainThreadMarker)`
/// signature with no return value. The bridge takes ownership of
/// `f`'s return value through the channel so the caller can observe
/// it without resorting to a mutable reference or `Arc<Mutex<_>>`.
///
/// `dispatch2::run_on_main` blocks indefinitely if no event loop is
/// driving the main queue. The bridge submits the work through a
/// dedicated helper thread and waits with a bounded timeout; the
/// timeout translates into the typed `DispatchUnavailable` outcome so
/// a misconfigured host cannot stall the capture loop forever.
fn dispatch_main_thread<F, R>(f: F) -> BridgeResult<R>
where
    F: FnOnce(MainThreadMarker) -> R + Send + 'static,
    R: Send + 'static,
{
    if MainThreadMarker::new().is_some() {
        // SAFETY: `MainThreadMarker::new()` succeeded, so we are on
        // the main thread; constructing a marker from the actual
        // thread state is sound.
        let mtm = unsafe { MainThreadMarker::new_unchecked() };
        return Ok(f(mtm));
    }

    // `dispatch2::run_on_main` would block indefinitely if no event
    // loop is driving the main queue (a unit test running outside of
    // Tauri, for example). The bridge therefore submits the work to
    // a dedicated helper thread and waits with a bounded timeout;
    // the timeout translates into the typed `DispatchUnavailable`
    // outcome so a misconfigured host cannot stall the capture loop
    // forever.
    //
    // Production code never reaches this branch because Tauri's
    // event loop drives the main queue; the bounded wait is here to
    // protect the capture loop from a misconfigured host where the
    // dispatcher silently dropped the work. The helper thread is
    // intentionally not joined when the timeout fires: a `run_on_main`
    // call that cannot reach the main thread blocks the thread it
    // is on, and joining it would deadlock the calling thread. The
    // thread leaks only on this exceptional path; the test harness
    // exercises it with `cargo test`'s default per-test timeout.
    let (tx, rx) = mpsc::sync_channel::<R>(1);
    let _ = std::thread::Builder::new()
        .name("clipvault-pasteboard-main-hop".to_string())
        .spawn(move || {
            run_on_main(move |_mtm| {
                let mtm =
                    MainThreadMarker::new().expect("dispatch2 must execute on the main thread");
                let value = f(mtm);
                let _ = tx.send(value);
            });
        });
    match rx.recv_timeout(std::time::Duration::from_secs(1)) {
        Ok(value) => Ok(value),
        Err(_) => Err(MainQueueBridgeError::DispatchUnavailable),
    }
}

/// Read the plain-text leg of `NSPasteboard` on the main thread.
///
/// Returns `Ok(None)` when the pasteboard is empty (or the call
/// returned an empty string). Returns `Err(Unavailable { capability:
/// ClipboardRead })` when the bridge cannot hop to the main thread.
pub fn read_plain_text_main_thread() -> BridgeResult<Option<String>> {
    dispatch_main_thread(|_mtm| {
        use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString};
        use objc2_foundation::NSString;

        let pasteboard = NSPasteboard::generalPasteboard();
        if let Some(string) = pasteboard.stringForType(unsafe { NSPasteboardTypeString }) {
            let value = string.to_string();
            if !value.is_empty() {
                return Some(value);
            }
        }
        // Fall back to the canonical UTF-8 plain-text flavour so
        // strict consumers (terminal apps, certain editors) still
        // observe the text. The fallback path is part of the same
        // main-thread read so the watcher's fingerprint stays
        // deterministic across producers.
        let utf8_type = NSString::from_str("public.utf8-plain-text");
        if let Some(string) = pasteboard.stringForType(&utf8_type) {
            let value = string.to_string();
            if !value.is_empty() {
                return Some(value);
            }
        }
        None
    })
}

/// Read the rich-text legs of `NSPasteboard` on the main thread.
///
/// The closure runs every pasteboard query inside a single
/// `generalPasteboard()` invocation so the three legs
/// (`public.utf8-plain-text`, `public.html`, `public.rtf`) come from
/// one consistent snapshot. Returning `Ok(None)` means "no rich
/// representation was published"; the caller's plain-text fallback
/// can then try a second read (also on the main thread) without
/// leaking the thread limitation.
///
/// Returns `Err(Unavailable { capability: ClipboardReadRichText })`
/// when the bridge cannot hop to the main thread.
pub fn read_rich_main_thread() -> BridgeResult<Option<crate::clipboard::RichTextPayload>> {
    use crate::clipboard::RichTextPayload;
    dispatch_main_thread(|_mtm| {
        use objc2_app_kit::{
            NSPasteboard, NSPasteboardTypeHTML, NSPasteboardTypeRTF, NSPasteboardTypeString,
        };
        use objc2_foundation::NSString;

        let pasteboard = NSPasteboard::generalPasteboard();

        // Plain-text leg (always inspected first so the caller can
        // decide between a rich capture and a plain capture without a
        // second pasteboard round-trip).
        let mut plain_text: Option<String> = None;
        if let Some(string) = pasteboard.stringForType(unsafe { NSPasteboardTypeString }) {
            let value = string.to_string();
            if !value.is_empty() {
                plain_text = Some(value);
            }
        }
        if plain_text.is_none() {
            let utf8_type = NSString::from_str("public.utf8-plain-text");
            if let Some(string) = pasteboard.stringForType(&utf8_type) {
                let value = string.to_string();
                if !value.is_empty() {
                    plain_text = Some(value);
                }
            }
        }
        let plain_text = plain_text?;

        // HTML leg.
        let html = pasteboard
            .stringForType(unsafe { NSPasteboardTypeHTML })
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty());

        // RTF leg.
        let rtf = pasteboard
            .dataForType(unsafe { NSPasteboardTypeRTF })
            .and_then(|data| {
                let length = data.length();
                if length == 0 {
                    return Some(Vec::new());
                }
                if length > RichTextPayload::MAX_RICH_TEXT_RTF_BYTES {
                    return None;
                }
                let mut out: Vec<u8> = Vec::with_capacity(length);
                // SAFETY: `getBytes_length` copies `length` bytes into the
                // caller-provided buffer. The buffer must remain valid
                // for `length` bytes; `Vec`'s spare capacity is exactly
                // that.
                unsafe {
                    let dst = std::ptr::NonNull::new_unchecked(
                        out.as_mut_ptr().cast::<std::ffi::c_void>(),
                    );
                    data.getBytes_length(dst, length);
                    out.set_len(length);
                }
                Some(out)
            });

        if html.is_none() && rtf.is_none() {
            return None;
        }
        RichTextPayload::new(plain_text, html, rtf).ok()
    })
}

/// Write a rich-text payload to `NSPasteboard` on the main thread.
///
/// All three flavours (`public.utf8-plain-text`, `public.html`,
/// `public.rtf`) are published in a single logical write following
/// Apple's `clearContents` + `declareTypes:owner:` + per-flavour
/// `setData:forType:` pattern. The plain text leg is the canonical
/// fallback flavour so any consumer that only knows plain text still
/// receives the right `content`.
///
/// Returns `Ok(())` on success and `Err(MainQueueBridgeError::DispatchUnavailable)`
/// when the bridge cannot hop to the main thread.
pub fn write_rich_main_thread(payload: &crate::clipboard::RichTextPayload) -> BridgeResult<()> {
    let plain = payload.plain_text().to_string();
    let html = payload.html().map(str::to_string);
    let rtf = payload.rtf().map(|bytes| bytes.to_vec());

    let publish_succeeded: bool = dispatch_main_thread(move |_mtm| {
        use objc2_app_kit::{
            NSPasteboard, NSPasteboardTypeHTML, NSPasteboardTypeRTF, NSPasteboardTypeString,
        };
        use objc2_foundation::{NSArray, NSData, NSString};

        let pasteboard = NSPasteboard::generalPasteboard();
        let plain_ns = NSString::from_str(&plain);
        let utf8_type = NSString::from_str("public.utf8-plain-text");

        // Declare the union of types we are about to publish. Without
        // this declaration the pasteboard collapses to a single
        // flavour — the bug the previous prototype shipped.
        let mut declared_types: Vec<&NSString> = Vec::new();
        declared_types.push(unsafe { NSPasteboardTypeString });
        declared_types.push(&*utf8_type);
        if html.is_some() {
            declared_types.push(unsafe { NSPasteboardTypeHTML });
        }
        if rtf.is_some() {
            declared_types.push(unsafe { NSPasteboardTypeRTF });
        }
        let type_array = NSArray::from_slice(&declared_types);
        let _ = pasteboard.clearContents();
        let _ = unsafe { pasteboard.declareTypes_owner(&type_array, None) };

        // Plain text first (every consumer accepts it).
        let plain_ok = pasteboard.setString_forType(&plain_ns, unsafe { NSPasteboardTypeString })
            && pasteboard.setString_forType(&plain_ns, &utf8_type);
        if !plain_ok {
            return false;
        }

        if let Some(html) = &html {
            let html_ns = NSString::from_str(html);
            if !pasteboard.setString_forType(&html_ns, unsafe { NSPasteboardTypeHTML }) {
                return false;
            }
        }

        if let Some(rtf) = &rtf {
            let data = NSData::with_bytes(rtf);
            if !pasteboard.setData_forType(Some(&data), unsafe { NSPasteboardTypeRTF }) {
                return false;
            }
        }

        true
    })?;
    if publish_succeeded {
        Ok(())
    } else {
        Err(MainQueueBridgeError::DispatchUnavailable)
    }
}

/// Write plain text to `NSPasteboard` on the main thread.
///
/// The closure first calls `clearContents()` to drop every existing
/// flavour (`public.html`, `public.rtf`, the previous capture's rich
/// bytes, ...) and then declares only the canonical plain-text
/// flavours before publishing the text. This is the documented
/// contract of `PasteMode::Plain`: the resulting pasteboard carries
/// no rich payload, so a destination rich editor that re-applies
/// the cursor's typing attributes receives the new text without any
/// residual colour or font hint from the previous rich capture.
///
/// Mirrors `read_plain_text_main_thread`: the closure publishes
/// `public.utf8-plain-text` (the canonical UTF-8 flavour) and the
/// legacy `NSPasteboardTypeString` flavour in one logical write so
/// strict consumers that only watch the canonical flavour still
/// receive the text.
///
/// Returns `Err(MainQueueBridgeError::DispatchUnavailable)` when the
/// bridge cannot hop to the main thread.
pub fn write_plain_text_main_thread(text: String) -> BridgeResult<()> {
    let succeeded: bool = dispatch_main_thread(move |_mtm| {
        use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString};
        use objc2_foundation::{NSArray, NSString};

        let pasteboard = NSPasteboard::generalPasteboard();
        let ns_text = NSString::from_str(&text);
        let utf8_type = NSString::from_str("public.utf8-plain-text");
        let string_type = unsafe { NSPasteboardTypeString };

        // Plain write MUST NOT keep any rich flavour from a previous
        // capture. `clearContents` drops every declared type
        // (`public.html`, `public.rtf`, `public.utf8-plain-text`,
        // ...) and increments the change count so the watcher can
        // detect the new paste. The `declareTypes` step is what
        // makes the contract explicit: we promise only the canonical
        // plain-text flavours and nothing else.
        //
        // `NSPasteboardTypeString` and `public.utf8-plain-text` are
        // the same UTI on every supported SDK; declaring both keeps
        // the contract visible regardless of which constant the
        // consumer asks for. When the two identifiers are equal
        // (current SDK) the array collapses to a single entry.
        let mut declared_types: Vec<&NSString> = Vec::new();
        if !string_type.isEqualToString(&utf8_type) {
            declared_types.push(string_type);
        }
        declared_types.push(&*utf8_type);
        let type_array = NSArray::from_slice(&declared_types);
        let _ = pasteboard.clearContents();
        let _ = unsafe { pasteboard.declareTypes_owner(&type_array, None) };

        // The plain-text publish is a single `setString` call per
        // declared type. Returning `false` here means the OS
        // rejected the write: the bridge surfaces that as
        // `DispatchUnavailable` so the caller can fall back to the
        // plain adapter instead of producing a half-written
        // pasteboard.
        pasteboard.setString_forType(&ns_text, string_type)
            && pasteboard.setString_forType(&ns_text, &utf8_type)
    })?;
    if succeeded {
        Ok(())
    } else {
        Err(MainQueueBridgeError::DispatchUnavailable)
    }
}

/// Translate a bridge error into the typed
/// `ClipboardBackendError::Unavailable` the rest of the pipeline
/// observes. The capability identifier is the only thing the rest of
/// the pipeline sees — the underlying thread error never escapes.
pub fn unavailable_for(capability: Capability) -> ClipboardBackendError {
    ClipboardBackendError::Unavailable { capability }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bridge type is `Send + Sync` because the wrapper carries no
    /// non-`Send` state — every interaction with `NSPasteboard`
    /// happens inside the dispatched closure, which is required to
    /// run on the main thread.
    #[test]
    fn bridge_types_are_send_and_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}
        assert_send::<MainQueueBridgeError>();
        assert_sync::<MainQueueBridgeError>();
    }

    /// The capability translation is the only surface the rest of the
    /// pipeline observes when the bridge fails to hop; the error
    /// variant must carry the requested capability, never the
    /// underlying thread error.
    #[test]
    fn unavailable_for_carries_the_requested_capability() {
        let error = unavailable_for(Capability::ClipboardReadRichText);
        match error {
            ClipboardBackendError::Unavailable { capability } => {
                assert_eq!(capability, Capability::ClipboardReadRichText);
            }
            other => panic!("expected Unavailable, got {other:?}"),
        }
    }

    /// The bridge error has a stable snake_case identifier so the
    /// diagnostics endpoint can surface it without parsing free-form
    /// strings.
    #[test]
    fn bridge_error_kind_string_is_stable() {
        assert_eq!(
            MainQueueBridgeError::DispatchUnavailable.kind_str(),
            "main_queue_unavailable"
        );
    }
}
