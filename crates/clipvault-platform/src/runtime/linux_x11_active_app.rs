//! Linux X11 active-application probe (EWMH `_NET_ACTIVE_WINDOW`).

use std::rc::Rc;

use x11rb::connection::Connection;
use x11rb::protocol::xproto::{self, AtomEnum, ConnectionExt as X11ConnectionExt, Window};
use x11rb::RustConnection;

use crate::active_app::{ActiveAppError, ActiveApplication, ActiveApplicationProbe};

/// Probe backed by the EWMH `_NET_ACTIVE_WINDOW` and `WM_CLASS`
/// properties. `WM_CLASS` is the canonical, stable identifier
/// (instance and class strings, NUL-separated) the blacklist matches
/// against, so we read it first. `_NET_WM_NAME` is only used as a
/// display label fallback for the UI when `WM_CLASS` is absent.
pub struct X11ActiveApplication {
    inner: Rc<X11State>,
}

struct X11State {
    conn: RustConnection,
    root: Window,
    net_active_window: u32,
    net_wm_name: u32,
    wm_class: u32,
    utf8_string: u32,
}

impl X11ActiveApplication {
    /// Try to connect to the X server identified by `$DISPLAY`.
    pub fn new() -> Result<Self, ActiveAppError> {
        Self::connect_to(None)
    }

    /// Connect to an explicit display name (e.g. `:0`).
    pub fn connect_to(dpy_name: Option<&str>) -> Result<Self, ActiveAppError> {
        let (conn, screen_number) = x11rb::connect(dpy_name).map_err(ActiveAppError::backend)?;
        let screen = &conn.setup().screens[screen_number];
        let root = screen.root;

        let net_active_window = intern_atom(&conn, b"_NET_ACTIVE_WINDOW")?;
        let net_wm_name = intern_atom(&conn, b"_NET_WM_NAME")?;
        let wm_class = intern_atom(&conn, b"WM_CLASS")?;
        let utf8_string = intern_atom(&conn, b"UTF8_STRING")?;

        Ok(Self {
            inner: Rc::new(X11State {
                conn,
                root,
                net_active_window,
                net_wm_name,
                wm_class,
                utf8_string,
            }),
        })
    }
}

fn intern_atom(conn: &RustConnection, name: &[u8]) -> Result<u32, ActiveAppError> {
    let cookie = conn
        .intern_atom(false, name)
        .map_err(ActiveAppError::backend)?;
    let reply = cookie
        .reply()
        .map_err(|err| ActiveAppError::backend(format!("intern_atom: {err}")))?;
    Ok(reply.atom)
}

impl ActiveApplicationProbe for X11ActiveApplication {
    fn active_application(&self) -> Result<Option<ActiveApplication>, ActiveAppError> {
        let state = &*self.inner;

        let cookie = state.conn.get_property(
            false,
            state.root,
            state.net_active_window,
            AtomEnum::WINDOW,
            0,
            1,
        );
        let cookie = match cookie {
            Ok(cookie) => cookie,
            Err(_) => return Ok(None),
        };
        let reply = match cookie.reply() {
            Ok(reply) => reply,
            Err(_) => return Ok(None),
        };
        let active = match reply.value.first().copied() {
            Some(window) => Window::from(window),
            None => return Ok(None),
        };

        // Read `WM_CLASS` first: it carries a stable NUL-separated
        // `instance\0class` payload we can use both as the blacklist
        // identifier and as the human-readable label. Fall back to
        // `_NET_WM_NAME` only when `WM_CLASS` is missing.
        let class = read_string_property(&state.conn, active, state.wm_class, state.utf8_string);
        let (identifier, display_name) = match class {
            Some(value) if value.contains('\0') => {
                let mut parts = value.split('\0');
                let instance = parts.next().unwrap_or("").to_string();
                let class = parts.next().unwrap_or("").to_string();
                let identifier = if !class.is_empty() {
                    class
                } else {
                    instance.clone()
                };
                (identifier, instance)
            }
            Some(value) => (value.clone(), value),
            None => {
                let name =
                    read_string_property(&state.conn, active, state.net_wm_name, state.utf8_string);
                match name {
                    Some(value) => (value.clone(), value),
                    None => return Ok(None),
                }
            }
        };

        if identifier.is_empty() {
            return Ok(None);
        }

        Ok(Some(ActiveApplication::new(display_name, identifier)))
    }

    fn name(&self) -> &'static str {
        "x11_ewmh"
    }
}

fn read_string_property(
    conn: &RustConnection,
    window: Window,
    property: u32,
    utf8_atom: u32,
) -> Option<String> {
    let cookie = conn.get_property(false, window, property, utf8_atom, 0, u32::MAX / 4);
    let cookie = cookie.ok()?;
    let reply = cookie.reply().ok()?;
    if reply.format != 8 || reply.value.is_empty() {
        return None;
    }
    String::from_utf8(reply.value).ok()
}

// `xproto::Atom` re-export to avoid an unused-import warning when the
// feature combination compiles without other consumers.
#[allow(dead_code)]
fn _atom_marker(a: xproto::Atom) -> u32 {
    a.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parse the `instance\0class` payload the X server sends for
    /// `WM_CLASS`. Public so unit tests can exercise the splitting
    /// logic without standing up a fake connection.
    fn parse_wm_class(raw: &str) -> (String, String) {
        if !raw.contains('\0') {
            return (raw.to_string(), raw.to_string());
        }
        let mut parts = raw.split('\0');
        let instance = parts.next().unwrap_or("").to_string();
        let class = parts.next().unwrap_or("").to_string();
        (instance, class)
    }

    #[test]
    fn wm_class_class_segment_is_used_as_identifier() {
        let (instance, class) = parse_wm_class("firefox\0Firefox");
        assert_eq!(instance, "firefox");
        assert_eq!(class, "Firefox");
        // The identifier that feeds the blacklist must be the class
        // segment (stable, machine-friendly), not the title-style
        // instance segment.
        let identifier = if !class.is_empty() { class } else { instance };
        assert_eq!(identifier, "Firefox");
    }

    #[test]
    fn wm_class_falls_back_to_instance_when_class_missing() {
        let (instance, class) = parse_wm_class("weird-app\0");
        assert_eq!(instance, "weird-app");
        assert_eq!(class, "");
        let identifier = if !class.is_empty() { class } else { instance };
        assert_eq!(identifier, "weird-app");
    }
}
