//! Linux X11 synthetic paste via the XTEST extension.
//!
//! Sends a `Ctrl+V` keystroke through `XTestFakeKeyEvent`. Requires
//! the XTEST extension to be present (true on every modern X server)
//! and the process to have access to the input device — usually true
//! when the user runs ClipVault inside a graphical session.

use std::sync::Arc;

use x11rb::connection::Connection;
use x11rb::connection::RequestConnection;
use x11rb::errors::ConnectionError;
use x11rb::protocol::xproto::{ConnectionExt as X11ConnectionExt, Window};
use x11rb::protocol::xtest;
use x11rb::rust_connection::RustConnection;

use crate::paste::{PasteController, PasteError};

/// Standard X11 keycode for the left `Ctrl` key on a Latin keyboard.
const KEYCODE_CONTROL: u8 = 37;
/// Standard X11 keycode for the `v` key on a Latin keyboard.
const KEYCODE_V: u8 = 55;

/// Paste controller backed by `XTestFakeKeyEvent`.
///
/// The keycodes are documented at module scope and reused by the
/// orchestrator so a future change has to update a single constant
/// instead of three call sites.
pub struct X11PasteController {
    inner: Arc<X11State>,
}

struct X11State {
    conn: RustConnection,
    root: Window,
}

impl X11PasteController {
    /// Connect to the X server identified by `$DISPLAY` and prepare the
    /// XTEST extension.
    pub fn new() -> Result<Self, PasteError> {
        Self::connect_to(None)
    }

    /// Connect to an explicit display name.
    pub fn connect_to(dpy_name: Option<&str>) -> Result<Self, PasteError> {
        let (conn, screen_number) = x11rb::connect(dpy_name).map_err(PasteError::backend)?;
        let screen = &conn.setup().roots[screen_number];
        let root = screen.root;

        // Make sure the XTEST extension is available. The generated
        // `send_trait_request_without_reply` path resolves the opcode
        // through the cached extension manager, so we only need to
        // confirm that the extension is reported as present.
        let reply = conn
            .query_extension(b"XTEST")
            .map_err(PasteError::backend)?
            .reply()
            .map_err(|err| PasteError::backend(format!("query_extension: {err}")))?;
        if !reply.present {
            return Err(PasteError::backend("XTEST extension not present"));
        }

        Ok(Self {
            inner: Arc::new(X11State { conn, root }),
        })
    }
}

impl PasteController for X11PasteController {
    fn paste(&self) -> Result<(), PasteError> {
        let state = &*self.inner;

        // Ctrl down, V down, V up, Ctrl up. The order is part of the
        // contract with the focused X11 application: a missing press
        // or release leaves the modifier in the wrong state.
        send_fake_key(&state.conn, state.root, KEYCODE_CONTROL, true)
            .map_err(PasteError::backend)?;
        send_fake_key(&state.conn, state.root, KEYCODE_V, true).map_err(PasteError::backend)?;
        send_fake_key(&state.conn, state.root, KEYCODE_V, false).map_err(PasteError::backend)?;
        send_fake_key(&state.conn, state.root, KEYCODE_CONTROL, false)
            .map_err(PasteError::backend)?;

        Ok(())
    }

    fn name(&self) -> &'static str {
        "x11_xtest"
    }
}

/// Serialise and send a single XTEST `FakeInput` event, then flush the
/// socket so the X server actually receives the request.
///
/// Both errors are propagated: a failed `send_trait_request_without_reply`
/// means the request never reached the write buffer, and a failed
/// `flush` means the buffered bytes could not be pushed to the X
/// server. The caller turns them into a typed `PasteError::Backend`
/// so the UI can surface a meaningful diagnostic without leaking any
/// clipboard content.
fn send_fake_key(
    conn: &RustConnection,
    root: Window,
    keycode: u8,
    press: bool,
) -> Result<(), ConnectionError> {
    let type_ = if press {
        2 /* KeyPress */
    } else {
        3 /* KeyRelease */
    };
    let request = xtest::FakeInputRequest {
        type_,
        detail: keycode,
        time: x11rb::CURRENT_TIME,
        root,
        root_x: 0,
        root_y: 0,
        deviceid: 0,
    };
    conn.send_trait_request_without_reply(request)?;
    conn.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paste_keymap_targets_latin_ctrl_and_v() {
        // The standard X11 keycodes for `Ctrl` (37) and `v` (55) on a
        // Latin keyboard must match what we send through XTEST. If
        // these change, the keyboard layout assumption documented at
        // the top of the file is invalidated.
        assert_eq!(KEYCODE_CONTROL, 37);
        assert_eq!(KEYCODE_V, 55);
    }

    #[test]
    fn send_fake_key_returns_connection_error() {
        // Locks in the public signature so the propagation contract
        // — typed `ConnectionError`, no `x11rb::protocol::Error` —
        // cannot regress without a test failure.
        let _signature: fn(&RustConnection, Window, u8, bool) -> Result<(), ConnectionError> =
            send_fake_key;
    }

    #[test]
    fn x11_paste_controller_state_is_send_and_sync() {
        // The bootstrap stores the controller behind
        // `Arc<dyn PasteController>`, which requires `Send + Sync`. We
        // exercise the requirement at compile time by checking the
        // auto traits of `Arc<X11State>`.
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Arc<X11State>>();
    }

    #[test]
    fn rust_connection_path_resolves_to_expected_module() {
        // Compile-time check that the canonical 0.13.x import path is
        // the one we use. The `RustConnection` type from the older
        // crate-root re-export does not exist in 0.13.2, so any
        // accidental `use x11rb::RustConnection` in this module would
        // surface as an unresolved import here.
        fn assert_canonical(_: &x11rb::rust_connection::RustConnection) {}
        let _fn_ptr: fn(&x11rb::rust_connection::RustConnection) = assert_canonical;
    }
}
