//! X11/XWayland clipboard ownership revision monitor.
//!
//! `arboard` exposes the clipboard payload but not the X11 selection-owner
//! notifications that say a producer wrote the selection again. Comparing the
//! payload alone cannot distinguish a new copy of `A` from `A` simply remaining
//! on the clipboard. This small monitor subscribes to XFixes
//! `SetSelectionOwnerNotify` for `CLIPBOARD` and keeps a metadata-only counter.
//!
//! On a Wayland session ClipVault's current text adapter uses XWayland (the
//! build intentionally does not enable arboard's privileged data-control
//! backend). Therefore the same `$DISPLAY` selection stream is also the change
//! source for the Wayland/XWayland path. If there is no X server or no XFixes
//! extension, construction fails and the caller must use its conservative
//! fallback rather than inventing a revision.

#![cfg(all(target_os = "linux", feature = "linux-x11"))]

use x11rb::connection::Connection;
use x11rb::protocol::xfixes::{
    ConnectionExt as XFixesConnectionExt, SelectionEvent, SelectionEventMask,
};
use x11rb::protocol::xproto::{Atom, ConnectionExt as X11ConnectionExt};
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;

/// Stateful subscription to `CLIPBOARD` ownership notifications.
///
/// The counter has process-local meaning only. It is deliberately not derived
/// from clipboard bytes, owner ids, window titles or paths, and is never
/// persisted or logged.
pub(crate) struct X11ClipboardRevisionMonitor {
    connection: RustConnection,
    revision: u64,
}

impl X11ClipboardRevisionMonitor {
    /// Subscribe to the XFixes selection stream for the display selected by
    /// `$DISPLAY`. A failed connection or unsupported extension is a normal
    /// capability outcome; callers fall back conservatively.
    pub(crate) fn new() -> Result<Self, String> {
        let (connection, screen_number) =
            x11rb::connect(None).map_err(|error| format!("connect: {error}"))?;
        let root = connection
            .setup()
            .roots
            .get(screen_number)
            .ok_or_else(|| "X11 setup did not include the selected screen".to_string())?
            .root;
        let clipboard = intern_atom(&connection, b"CLIPBOARD")?;

        // Query before subscribing so servers without XFixes fail here rather
        // than leaving an apparently live monitor which can never receive a
        // selection notification.
        connection
            .xfixes_query_version(5, 0)
            .map_err(|error| format!("xfixes query: {error}"))?
            .reply()
            .map_err(|error| format!("xfixes query reply: {error}"))?;
        connection
            .xfixes_select_selection_input(root, clipboard, SelectionEventMask::SET_SELECTION_OWNER)
            .map_err(|error| format!("xfixes selection subscription: {error}"))?;
        connection
            .flush()
            .map_err(|error| format!("xfixes selection subscription flush: {error}"))?;

        Ok(Self {
            connection,
            revision: 0,
        })
    }

    /// Drain every queued ownership notification and return the current
    /// revision. Receiving several events between polling ticks is one fresh
    /// observation, so the exact count is intentionally opaque to callers.
    pub(crate) fn revision(&mut self) -> Result<u64, String> {
        loop {
            match self
                .connection
                .poll_for_event()
                .map_err(|error| format!("xfixes selection event: {error}"))?
            {
                Some(event) => record_ownership_event(&mut self.revision, &event),
                None => return Ok(self.revision),
            }
        }
    }
}

fn record_ownership_event(revision: &mut u64, event: &Event) {
    if matches!(
        event,
        Event::XfixesSelectionNotify(event)
            if event.subtype == SelectionEvent::SET_SELECTION_OWNER
    ) {
        *revision = revision.saturating_add(1);
    }
}

fn intern_atom(connection: &RustConnection, name: &[u8]) -> Result<Atom, String> {
    connection
        .intern_atom(false, name)
        .map_err(|error| format!("intern CLIPBOARD atom: {error}"))?
        .reply()
        .map(|reply| reply.atom)
        .map_err(|error| format!("intern CLIPBOARD atom reply: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use x11rb::protocol::xfixes::SelectionNotifyEvent;

    fn selection_event(subtype: SelectionEvent) -> Event {
        Event::XfixesSelectionNotify(SelectionNotifyEvent {
            response_type: 0,
            subtype,
            sequence: 0,
            window: 0,
            owner: 0,
            selection: 0,
            timestamp: 0,
            selection_timestamp: 0,
        })
    }

    #[test]
    fn only_set_selection_owner_notifications_advance_the_revision() {
        let mut revision = 0;
        record_ownership_event(
            &mut revision,
            &selection_event(SelectionEvent::SELECTION_CLIENT_CLOSE),
        );
        assert_eq!(revision, 0);

        record_ownership_event(
            &mut revision,
            &selection_event(SelectionEvent::SET_SELECTION_OWNER),
        );
        assert_eq!(revision, 1);
    }
}
