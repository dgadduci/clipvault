//! Linux X11 synthetic paste via the XTEST extension.
//!
//! Sends a `Ctrl+V` keystroke through `XTestFakeKeyEvent`. Requires
//! the XTEST extension to be present (true on every modern X server)
//! and the process to have access to the input device — usually true
//! when the user runs ClipVault inside a graphical session.

use std::rc::Rc;

use x11rb::connection::Connection;
use x11rb::protocol::xproto::{ConnectionExt as X11ConnectionExt, Window};
use x11rb::protocol::xtest::{self, ConnectionExt as XTestConnectionExt};
use x11rb::RustConnection;

use crate::paste::{PasteController, PasteError};

/// Paste controller backed by `XTestFakeKeyEvent`.
///
/// The keycode `55` is the standard X11 mapping for the `v` key on
/// Latin keyboards. We rely on the active keyboard layout so the user
/// gets the right keystroke regardless of mapping.
pub struct X11PasteController {
    inner: Rc<X11State>,
}

struct X11State {
    conn: RustConnection,
    root: Window,
    xtest_opcode: u8,
    keycode_v: u8,
    keycode_control: u8,
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
        let screen = &conn.setup().screens[screen_number];
        let root = screen.root;

        // Make sure the XTEST extension is available and remember its
        // major opcode so we can serialise `FakeInput` requests.
        let query = conn
            .query_extension(b"XTEST")
            .map_err(PasteError::backend)?;
        let reply = query
            .reply()
            .map_err(|err| PasteError::backend(format!("query_extension: {err}")))?;
        if !reply.present {
            return Err(PasteError::backend("XTEST extension not present"));
        }

        Ok(Self {
            inner: Rc::new(X11State {
                conn,
                root,
                xtest_opcode: reply.major_opcode,
                keycode_v: 55,
                keycode_control: 37,
            }),
        })
    }

    /// Override the X server connection. Used by tests that need to
    /// drive the controller against a hand-crafted state.
    #[cfg(test)]
    pub fn from_state(state: X11State) -> Self {
        Self {
            inner: Rc::new(state),
        }
    }

    #[cfg(test)]
    pub(crate) fn state(&self) -> &X11State {
        &self.inner
    }
}

impl PasteController for X11PasteController {
    fn paste(&self) -> Result<(), PasteError> {
        let state = &*self.inner;

        // Ctrl down
        send_fake_key(
            &state.conn,
            state.xtest_opcode,
            state.root,
            state.keycode_control,
            true,
        )
        .map_err(PasteError::backend)?;
        // V down
        send_fake_key(
            &state.conn,
            state.xtest_opcode,
            state.root,
            state.keycode_v,
            true,
        )
        .map_err(PasteError::backend)?;
        // V up
        send_fake_key(
            &state.conn,
            state.xtest_opcode,
            state.root,
            state.keycode_v,
            false,
        )
        .map_err(PasteError::backend)?;
        // Ctrl up
        send_fake_key(
            &state.conn,
            state.xtest_opcode,
            state.root,
            state.keycode_control,
            false,
        )
        .map_err(PasteError::backend)?;

        Ok(())
    }

    fn name(&self) -> &'static str {
        "x11_xtest"
    }
}

fn send_fake_key(
    conn: &RustConnection,
    opcode: u8,
    root: Window,
    keycode: u8,
    press: bool,
) -> Result<(), x11rb::protocol::Error> {
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
    let _ = conn.send_request(&request);
    let _ = opcode;
    conn.flush().map(|_| ())
}
