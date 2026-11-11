//! Metadata-only revision monitor for native Wayland clipboard selections.
//!
//! The monitor subscribes to the regular clipboard selection through either
//! `ext-data-control-v1` or `wlr-data-control-unstable-v1`. It never requests
//! an offer, MIME type or clipboard bytes. The first selection event per seat
//! is the protocol's initial state notification and establishes the baseline;
//! subsequent selection events advance a process-local counter.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;

use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::wl_registry::WlRegistry;
use wayland_client::protocol::wl_seat::WlSeat;
use wayland_client::{event_created_child, Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols::ext::data_control::v1::client::ext_data_control_device_v1::{
    self as ext_device, Event as ExtDeviceEvent, ExtDataControlDeviceV1,
};
use wayland_protocols::ext::data_control::v1::client::ext_data_control_manager_v1::ExtDataControlManagerV1;
use wayland_protocols::ext::data_control::v1::client::ext_data_control_offer_v1::ExtDataControlOfferV1;
use wayland_protocols_wlr::data_control::v1::client::zwlr_data_control_device_v1::{
    self as wlr_device, Event as WlrDeviceEvent, ZwlrDataControlDeviceV1,
};
use wayland_protocols_wlr::data_control::v1::client::zwlr_data_control_manager_v1::ZwlrDataControlManagerV1;
use wayland_protocols_wlr::data_control::v1::client::zwlr_data_control_offer_v1::ZwlrDataControlOfferV1;

/// Live selection revision stream. A disconnected event queue becomes
/// `UNKNOWN`, so a stale count cannot authorize recapturing deleted content.
pub(crate) struct WaylandClipboardRevisionMonitor {
    revision: Arc<AtomicU64>,
    healthy: Arc<AtomicBool>,
}

impl WaylandClipboardRevisionMonitor {
    /// Start the listener. `Ok(None)` means the compositor does not publish a
    /// usable data-control manager or seat; `Err(())` means the probe failed
    /// unexpectedly and callers must keep the revision unknown.
    pub(crate) fn new() -> Result<Option<Self>, ()> {
        let revision = Arc::new(AtomicU64::new(0));
        let healthy = Arc::new(AtomicBool::new(true));
        let worker_revision = Arc::clone(&revision);
        let worker_healthy = Arc::clone(&healthy);
        let (startup_sender, startup_receiver) = mpsc::sync_channel(1);

        thread::Builder::new()
            .name("clipvault-wayland-clipboard-revision".into())
            .spawn(move || {
                run_listener(worker_revision, worker_healthy, startup_sender);
            })
            .map_err(|_| ())?;

        match startup_receiver.recv().map_err(|_| ())? {
            Startup::Ready => Ok(Some(Self { revision, healthy })),
            Startup::Unsupported => Ok(None),
            Startup::Failed => Err(()),
        }
    }

    /// Return the current opaque revision without reading selection data.
    pub(crate) fn revision(&self) -> Option<u64> {
        self.healthy
            .load(Ordering::Acquire)
            .then(|| self.revision.load(Ordering::Acquire))
    }
}

enum Startup {
    Ready,
    Unsupported,
    Failed,
}

enum DataControlManager {
    Ext(ExtDataControlManagerV1),
    Wlr(ZwlrDataControlManagerV1),
}

impl DataControlManager {
    fn get_data_device(&self, seat: &WlSeat, qh: &QueueHandle<State>) -> DataControlDevice {
        match self {
            Self::Ext(manager) => {
                DataControlDevice::Ext(manager.get_data_device(seat, qh, seat.clone()))
            }
            Self::Wlr(manager) => {
                DataControlDevice::Wlr(manager.get_data_device(seat, qh, seat.clone()))
            }
        }
    }
}

// The device proxies must remain alive for as long as the event queue runs;
// their callbacks carry the selection stream, so the worker holds them but
// does not otherwise inspect them.
#[allow(dead_code)]
enum DataControlDevice {
    Ext(ExtDataControlDeviceV1),
    Wlr(ZwlrDataControlDeviceV1),
}

struct State {
    revision: Arc<AtomicU64>,
    healthy: Arc<AtomicBool>,
    initialized_seats: HashSet<WlSeat>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SelectionKind {
    Clipboard,
    Primary,
}

impl State {
    fn note_regular_selection(&mut self, seat: &WlSeat) {
        let is_initial = self.initialized_seats.insert(seat.clone());
        advance_selection_revision(&self.revision, is_initial, SelectionKind::Clipboard);
    }

    fn note_primary_selection(&self) {
        advance_selection_revision(&self.revision, false, SelectionKind::Primary);
    }
}

fn advance_selection_revision(
    revision: &AtomicU64,
    is_initial_selection: bool,
    selection_kind: SelectionKind,
) {
    if selection_kind == SelectionKind::Clipboard {
        advance_revision(revision, is_initial_selection);
    }
}

fn advance_revision(revision: &AtomicU64, is_initial_selection: bool) {
    if !is_initial_selection {
        let _ = revision.fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
            Some(current.saturating_add(1))
        });
    }
}

fn run_listener(
    revision: Arc<AtomicU64>,
    healthy: Arc<AtomicBool>,
    startup_sender: mpsc::SyncSender<Startup>,
) {
    let result = initialize_listener(Arc::clone(&revision), Arc::clone(&healthy));
    let (connection, mut queue, mut state, devices) = match result {
        Ok(Some(initialized)) => initialized,
        Ok(None) => {
            healthy.store(false, Ordering::Release);
            let _ = startup_sender.send(Startup::Unsupported);
            return;
        }
        Err(()) => {
            healthy.store(false, Ordering::Release);
            let _ = startup_sender.send(Startup::Failed);
            return;
        }
    };

    if queue.roundtrip(&mut state).is_err() {
        healthy.store(false, Ordering::Release);
        let _ = startup_sender.send(Startup::Failed);
        return;
    }
    let _ = connection.flush();
    if startup_sender.send(Startup::Ready).is_err() {
        healthy.store(false, Ordering::Release);
        return;
    }

    let _devices = devices;
    while healthy.load(Ordering::Acquire) {
        if queue.blocking_dispatch(&mut state).is_err() {
            break;
        }
    }
    healthy.store(false, Ordering::Release);
}

fn initialize_listener(
    revision: Arc<AtomicU64>,
    healthy: Arc<AtomicBool>,
) -> Result<
    Option<(
        Connection,
        wayland_client::EventQueue<State>,
        State,
        Vec<DataControlDevice>,
    )>,
    (),
> {
    let connection = Connection::connect_to_env().map_err(|_| ())?;
    let (globals, queue) = registry_queue_init::<State>(&connection).map_err(|_| ())?;
    let qh = queue.handle();

    // Match wl-clipboard-rs' selection order: prefer ext-data-control, then
    // wlr-data-control v1 for the regular clipboard.
    let manager = globals
        .bind::<ExtDataControlManagerV1, _, _>(&qh, 1..=1, ())
        .ok()
        .map(DataControlManager::Ext)
        .or_else(|| {
            globals
                .bind::<ZwlrDataControlManagerV1, _, _>(&qh, 1..=1, ())
                .ok()
                .map(DataControlManager::Wlr)
        });
    let Some(manager) = manager else {
        return Ok(None);
    };

    let registry = globals.registry();
    let seats = globals.contents().with_list(|available| {
        available
            .iter()
            .filter(|global| global.interface == WlSeat::interface().name && global.version >= 2)
            .map(|global| registry.bind::<WlSeat, _, _>(global.name, 2, &qh, ()))
            .collect::<Vec<_>>()
    });
    if seats.is_empty() {
        return Ok(None);
    }

    let devices = seats
        .iter()
        .map(|seat| manager.get_data_device(seat, &qh))
        .collect::<Vec<_>>();
    let state = State {
        revision,
        healthy,
        initialized_seats: HashSet::new(),
    };

    Ok(Some((connection, queue, state, devices)))
}

impl Dispatch<WlRegistry, GlobalListContents> for State {
    fn event(
        _state: &mut Self,
        _proxy: &WlRegistry,
        _event: <WlRegistry as Proxy>::Event,
        _data: &GlobalListContents,
        _connection: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlSeat, ()> for State {
    fn event(
        _state: &mut Self,
        _proxy: &WlSeat,
        _event: <WlSeat as Proxy>::Event,
        _data: &(),
        _connection: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ExtDataControlManagerV1, ()> for State {
    fn event(
        _state: &mut Self,
        _proxy: &ExtDataControlManagerV1,
        _event: <ExtDataControlManagerV1 as Proxy>::Event,
        _data: &(),
        _connection: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwlrDataControlManagerV1, ()> for State {
    fn event(
        _state: &mut Self,
        _proxy: &ZwlrDataControlManagerV1,
        _event: <ZwlrDataControlManagerV1 as Proxy>::Event,
        _data: &(),
        _connection: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ExtDataControlDeviceV1, WlSeat> for State {
    fn event(
        state: &mut Self,
        proxy: &ExtDataControlDeviceV1,
        event: ExtDeviceEvent,
        seat: &WlSeat,
        _connection: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            ExtDeviceEvent::Selection { id } => {
                if let Some(offer) = id {
                    offer.destroy();
                }
                state.note_regular_selection(seat);
            }
            ExtDeviceEvent::PrimarySelection { id } => {
                if let Some(offer) = id {
                    offer.destroy();
                }
                state.note_primary_selection();
            }
            ExtDeviceEvent::Finished => {
                state.healthy.store(false, Ordering::Release);
                proxy.destroy();
            }
            _ => {}
        }
    }

    event_created_child!(
        State,
        ExtDataControlDeviceV1,
        [ext_device::EVT_DATA_OFFER_OPCODE => (ExtDataControlOfferV1, ())]
    );
}

impl Dispatch<ZwlrDataControlDeviceV1, WlSeat> for State {
    fn event(
        state: &mut Self,
        proxy: &ZwlrDataControlDeviceV1,
        event: WlrDeviceEvent,
        seat: &WlSeat,
        _connection: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            WlrDeviceEvent::Selection { id } => {
                if let Some(offer) = id {
                    offer.destroy();
                }
                state.note_regular_selection(seat);
            }
            WlrDeviceEvent::PrimarySelection { id } => {
                if let Some(offer) = id {
                    offer.destroy();
                }
                state.note_primary_selection();
            }
            WlrDeviceEvent::Finished => {
                state.healthy.store(false, Ordering::Release);
                proxy.destroy();
            }
            _ => {}
        }
    }

    event_created_child!(
        State,
        ZwlrDataControlDeviceV1,
        [wlr_device::EVT_DATA_OFFER_OPCODE => (ZwlrDataControlOfferV1, ())]
    );
}

impl Dispatch<ExtDataControlOfferV1, ()> for State {
    fn event(
        _state: &mut Self,
        _proxy: &ExtDataControlOfferV1,
        _event: <ExtDataControlOfferV1 as Proxy>::Event,
        _data: &(),
        _connection: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwlrDataControlOfferV1, ()> for State {
    fn event(
        _state: &mut Self,
        _proxy: &ZwlrDataControlOfferV1,
        _event: <ZwlrDataControlOfferV1 as Proxy>::Event,
        _data: &(),
        _connection: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_selection_establishes_baseline_and_later_selection_advances() {
        let revision = AtomicU64::new(0);

        advance_revision(&revision, true);
        assert_eq!(revision.load(Ordering::Acquire), 0);

        advance_revision(&revision, false);
        assert_eq!(revision.load(Ordering::Acquire), 1);
    }

    #[test]
    fn revision_counter_saturates_instead_of_wrapping() {
        let revision = AtomicU64::new(u64::MAX);

        advance_revision(&revision, false);
        assert_eq!(revision.load(Ordering::Acquire), u64::MAX);
    }

    #[test]
    fn primary_selection_does_not_advance_clipboard_revision() {
        let revision = AtomicU64::new(4);

        advance_selection_revision(&revision, false, SelectionKind::Primary);

        assert_eq!(revision.load(Ordering::Acquire), 4);
    }

    #[test]
    fn disconnected_listener_reports_no_revision() {
        let monitor = WaylandClipboardRevisionMonitor {
            revision: Arc::new(AtomicU64::new(3)),
            healthy: Arc::new(AtomicBool::new(false)),
        };

        assert_eq!(monitor.revision(), None);
    }
}
