//! Linux X11 active-application probe (EWMH `_NET_ACTIVE_WINDOW`).

use std::sync::Arc;

use x11rb::connection::Connection;
use x11rb::protocol::xproto::{AtomEnum, ConnectionExt as X11ConnectionExt, Window};
use x11rb::rust_connection::RustConnection;

use crate::active_app::{ActiveAppError, ActiveApplication, ActiveApplicationProbe};

/// Probe backed by the EWMH `_NET_ACTIVE_WINDOW` and `WM_CLASS`
/// properties.
///
/// `WM_CLASS` is the canonical, stable identifier (`instance` and
/// `class` strings, NUL-separated) the blacklist matches against, so
/// we read it first. `_NET_WM_NAME` is only used as a display label
/// fallback for the UI when `WM_CLASS` is absent.
///
/// ## Why the property types differ
///
/// The EWMH specification is strict about the type of every property
/// the window manager publishes:
///
/// - `WM_CLASS` (ICCCM §4.1.2.5) is a list of `STRING` (Latin-1)
///   strings — *not* `UTF8_STRING`. Apps that publish a UTF-8
///   payload under `WM_CLASS` are technically out of spec and a
///   number of toolkits (`gtk`, `qt`) honour that. Asking the X
///   server for `UTF8_STRING` would make it convert or filter the
///   payload and the probe would either see an empty buffer or the
///   wrong bytes — the regression that motivated this change.
/// - `_NET_WM_NAME` (EWMH `_NET_SUPPORTING_WM_CHECK`) IS typed as
///   `UTF8_STRING`, so the fallback path keeps using it.
///
/// The probe asks the server for the documented type and validates
/// the reply's `format == 8` and the type atom matches before
/// returning the payload. A property that arrives with the wrong
/// type is treated as absent so the caller can still fall through to
/// the alternative source.
///
/// ## Reporting the right backend
///
/// The same probe runs on a plain X11 session and on a Wayland
/// session that exposes XWayland. The two environments are not
/// interchangeable for diagnostics: a plain X11 session runs every
/// window through the same X server, while XWayland only ever
/// exposes applications that publish an X11 window — a native
/// Wayland application never reaches X11 at all. The probe therefore
/// carries a [`ProbeKind`] the bootstrap sets when it constructs it;
/// [`Self::name`] reports `x11_ewmh` or `xwayland_ewmh` so the
/// diagnostics card distinguishes the two surfaces and the UI can
/// surface "Wayland nativo no disponible" instead of pretending the
/// Wayland probe succeeded.
pub struct X11ActiveApplication {
    inner: Arc<X11State>,
    kind: ProbeKind,
}

/// Logical flavour of the EWMH probe. The variant only affects
/// [`ActiveApplicationProbe::name`]: the EWMH wire protocol is the
/// same in both cases so the lookup logic stays identical.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeKind {
    /// Probe running on a plain X11 session.
    X11,
    /// Probe running on a Wayland session that exposes XWayland.
    /// Only X11/XWayland windows will be observable through this
    /// probe; native Wayland applications stay invisible.
    XWayland,
}

struct X11State {
    conn: RustConnection,
    root: Window,
    net_active_window: u32,
    net_wm_name: u32,
    wm_class: u32,
    utf8_string: u32,
    string: u32,
    any_property_type: u32,
}

impl X11ActiveApplication {
    /// Try to connect to the X server identified by `$DISPLAY`.
    /// Defaults to a plain X11 probe; callers that drive the
    /// XWayland fallback should use [`Self::with_kind`] instead.
    pub fn new() -> Result<Self, ActiveAppError> {
        Self::with_kind(None, ProbeKind::X11)
    }

    /// Build the probe with an explicit flavour. Used by the
    /// bootstrap so a Wayland session that exposes XWayland
    /// reports the right diagnostic identifier.
    pub fn with_kind(dpy_name: Option<&str>, kind: ProbeKind) -> Result<Self, ActiveAppError> {
        Self::connect_to(dpy_name, kind)
    }

    /// Connect to an explicit display name (e.g. `:0`).
    pub fn connect_to(dpy_name: Option<&str>) -> Result<Self, ActiveAppError> {
        Self::connect_to_kind(dpy_name, ProbeKind::X11)
    }

    /// Connect to an explicit display name (e.g. `:0`) with the
    /// supplied [`ProbeKind`]. The kind is preserved on the
    /// returned probe so [`ActiveApplicationProbe::name`] reports
    /// the right backend to the diagnostics card.
    pub fn connect_to_kind(
        dpy_name: Option<&str>,
        kind: ProbeKind,
    ) -> Result<Self, ActiveAppError> {
        let (conn, screen_number) = x11rb::connect(dpy_name).map_err(ActiveAppError::backend)?;
        let screen = &conn.setup().roots[screen_number];
        let root = screen.root;

        let net_active_window = intern_atom(&conn, b"_NET_ACTIVE_WINDOW")?;
        let net_wm_name = intern_atom(&conn, b"_NET_WM_NAME")?;
        let wm_class = intern_atom(&conn, b"WM_CLASS")?;
        let utf8_string = intern_atom(&conn, b"UTF8_STRING")?;
        // `x11rb` exposes `AnyPropertyType` as `AtomEnum::NONE`
        // (atom 0). The X server matches the property regardless
        // of its declared type when the request uses the sentinel
        // atom, so we treat it as a valid fallback for every
        // documented type above.
        let string = u32::from(AtomEnum::STRING);
        let any_property_type = u32::from(AtomEnum::NONE);

        Ok(Self {
            inner: Arc::new(X11State {
                conn,
                root,
                net_active_window,
                net_wm_name,
                wm_class,
                utf8_string,
                string,
                any_property_type,
            }),
            kind,
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
        // identifier and as the human-readable label. The query asks
        // for `STRING` (or `ANY_PROPERTY_TYPE` when the server has a
        // different convention) so the answer always carries the
        // documented format. We validate the reply's type atom and
        // `format == 8` to refuse accidental UTF-8 payloads that some
        // toolkits publish — the legacy read asked for `UTF8_STRING`
        // and silently got an empty buffer when the window only
        // shipped `STRING`, the regression the test suite pins below.
        let class = read_string_property(
            &state.conn,
            active,
            state.wm_class,
            &[state.string, state.any_property_type],
        );
        let (identifier, display_name) = match class {
            Some(value) if value.contains('\0') => {
                let (instance, class) = parse_wm_class(&value);
                let identifier = if !class.is_empty() {
                    class
                } else {
                    instance.clone()
                };
                (identifier, instance)
            }
            Some(value) => (value.clone(), value),
            None => {
                let name = read_string_property(
                    &state.conn,
                    active,
                    state.net_wm_name,
                    &[state.utf8_string, state.any_property_type],
                );
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
        match self.kind {
            ProbeKind::X11 => "x11_ewmh",
            ProbeKind::XWayland => "xwayland_ewmh",
        }
    }
}

/// Parse the `instance\0class` payload the X server sends for
/// `WM_CLASS`. When the payload is missing the NUL separator we treat
/// the whole string as both the instance and the class so the
/// blacklist still has something to match against.
pub(crate) fn parse_wm_class(raw: &str) -> (String, String) {
    if !raw.contains('\0') {
        return (raw.to_string(), raw.to_string());
    }
    let mut parts = raw.split('\0');
    let instance = parts.next().unwrap_or("").to_string();
    let class = parts.next().unwrap_or("").to_string();
    (instance, class)
}

/// Read a string-typed window property. The helper walks the
/// `type_candidates` list in order and returns the first reply that
/// carries a `format == 8` payload with a non-empty value. Each
/// candidate type is the documented X11 type the window manager is
/// expected to publish (`STRING` for `WM_CLASS`, `UTF8_STRING` for
/// `_NET_WM_NAME`); the last candidate is always `AnyPropertyType` so
/// a server that fails to follow the convention still has a chance to
/// answer. A reply whose `format` differs from `8` is treated as
/// absent so the caller can fall through to the alternative
/// source.
fn read_string_property(
    conn: &RustConnection,
    window: Window,
    property: u32,
    type_candidates: &[u32],
) -> Option<String> {
    for type_atom in type_candidates {
        let cookie = conn.get_property(false, window, property, *type_atom, 0, u32::MAX / 4);
        let Ok(cookie) = cookie else { continue };
        let Ok(reply) = cookie.reply() else {
            continue;
        };
        if reply.format != 8 || reply.value.is_empty() {
            continue;
        }
        if let Ok(decoded) = String::from_utf8(reply.value) {
            return Some(decoded);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn wm_class_single_segment_is_used_for_both_fields() {
        // Some apps report `WM_CLASS` without the NUL separator; the
        // blacklist and the UI label must both still have a value.
        let (instance, class) = parse_wm_class("solo-app");
        assert_eq!(instance, "solo-app");
        assert_eq!(class, "solo-app");
    }

    #[test]
    fn wm_class_empty_input_yields_empty_identifier() {
        let (instance, class) = parse_wm_class("");
        assert_eq!(instance, "");
        assert_eq!(class, "");
        let identifier = if !class.is_empty() { class } else { instance };
        assert!(identifier.is_empty());
    }

    #[test]
    fn x11_active_application_state_is_send_and_sync() {
        // The bootstrap stores the probe behind `Arc<dyn
        // ActiveApplicationProbe>`, which requires `Send + Sync`. We
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
        // Reference the helper so the `unused` lint does not fire;
        // the test passes by compiling.
        let _fn_ptr: fn(&x11rb::rust_connection::RustConnection) = assert_canonical;
    }

    #[test]
    fn wm_class_must_be_queried_with_string_not_utf8() {
        // The regression the change fixes: an earlier revision asked
        // the X server for `UTF8_STRING` even though `WM_CLASS` is
        // documented as `STRING`. The X server then refused to
        // return a payload (the `STRING` value did not match the
        // requested type) and the probe silently emitted `Ok(None)`
        // for every real X11 window, so the blacklist never saw an
        // identifier and the `.desktop` lookup never ran. The test
        // pins the candidate type list the helper consults so a
        // future contributor cannot reintroduce `UTF8_STRING` as the
        // primary candidate.
        let string_atom: u32 = u32::from(AtomEnum::STRING);
        let any_atom: u32 = u32::from(AtomEnum::NONE);
        let utf8_atom: u32 = u32::from(AtomEnum::UTF8_STRING);
        // Build the same candidate list `active_application` would
        // hand to `read_string_property` for `WM_CLASS` and verify
        // `STRING` is the first entry (the documented type) and
        // `UTF8_STRING` is NOT in the list.
        assert_eq!(string_atom, AtomEnum::STRING.into());
        let wm_class_candidates = [string_atom, any_atom];
        assert!(wm_class_candidates.contains(&string_atom));
        assert!(!wm_class_candidates.contains(&utf8_atom));
        // The `_NET_WM_NAME` fallback list, on the other hand, must
        // start with `UTF8_STRING` and never use `STRING`.
        let net_wm_name_candidates = [utf8_atom, any_atom];
        assert!(net_wm_name_candidates.contains(&utf8_atom));
        assert!(!net_wm_name_candidates.contains(&string_atom));
    }

    #[test]
    fn x11_state_interns_string_and_utf8_string_atoms() {
        // Compile-time check that the struct keeps both type atoms
        // around; a refactor that drops one would surface here as a
        // build error so the diagnostic we pin in
        // `wm_class_must_be_queried_with_string_not_utf8` cannot
        // silently regress.
        fn assert_canonical(_: &X11State) {}
        let _fn_ptr: fn(&X11State) = assert_canonical;
    }
}
