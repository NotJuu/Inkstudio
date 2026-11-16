//! Wayland: `zwp_tablet_manager_v2` on the app's own `wl_display`, in a separate event queue
//! dispatched by our thread. Tool events (proximity, down/up, motion, pressure, tilt) are
//! accumulated until each `frame` event and then pushed as one sample.

use crate::{Phase, Queue, Sample, Units};
use wayland_backend::client::{Backend as WlBackend, ObjectId};
use wayland_client::protocol::{wl_registry, wl_seat, wl_surface};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols::wp::tablet::zv2::client::{
    zwp_tablet_manager_v2::ZwpTabletManagerV2,
    zwp_tablet_seat_v2::{self, ZwpTabletSeatV2},
    zwp_tablet_tool_v2::{self, ZwpTabletToolV2},
    zwp_tablet_v2::ZwpTabletV2,
};

/// State of one tool between `frame` events.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ToolState {
    pub on_our_surface: bool,
    pub down: bool,
    pub was_down: bool,
    pub x: f64,
    pub y: f64,
    pub pressure: f32,
    pub tilt: Option<(f32, f32)>,
    pub eraser: bool,
    pub changed: bool,
}

impl ToolState {
    /// Called on `frame`: the sample for this frame, if any.
    pub fn frame(&mut self) -> Option<Sample> {
        if !self.changed {
            return None;
        }
        self.changed = false;
        if !self.on_our_surface && !self.was_down {
            return None;
        }
        let phase = match (self.was_down, self.down) {
            (false, true) => Phase::Down,
            (true, true) => Phase::Move,
            (true, false) => Phase::Up,
            (false, false) => Phase::Hover,
        };
        self.was_down = self.down;
        Some(Sample { phase, x: self.x as f32, y: self.y as f32, units: Units::Logical, pressure: if self.down { self.pressure } else { 0.0 }, tilt: self.tilt, eraser: self.eraser })
    }
}

struct State {
    queue: Queue,
    surface: wl_surface::WlSurface,
    manager: Option<ZwpTabletManagerV2>,
    seats: Vec<wl_seat::WlSeat>,
    tools: Vec<(ZwpTabletToolV2, ToolState)>,
}

pub struct Backend {
    _thread: std::thread::JoinHandle<()>,
}

impl Backend {
    pub fn attach(display: *mut std::ffi::c_void, surface: *mut std::ffi::c_void, queue: Queue) -> Option<Backend> {
        // SAFETY: the pointers come from the live window's raw handles; the display outlives
        // the window, and we only add our own objects in a separate event queue.
        let backend = unsafe { WlBackend::from_foreign_display(display.cast()) };
        let conn = Connection::from_backend(backend);
        let surface_id = unsafe { ObjectId::from_ptr(wl_surface::WlSurface::interface(), surface.cast()) }.ok()?;
        let surface = wl_surface::WlSurface::from_id(&conn, surface_id).ok()?;
        let mut event_queue = conn.new_event_queue::<State>();
        let qh = event_queue.handle();
        conn.display().get_registry(&qh, ());
        let mut state = State { queue, surface, manager: None, seats: Vec::new(), tools: Vec::new() };
        // Two roundtrips: globals, then tablet seats and tools.
        event_queue.roundtrip(&mut state).ok()?;
        let manager = state.manager.clone()?;
        for seat in state.seats.clone() {
            manager.get_tablet_seat(&seat, &qh, ());
        }
        event_queue.roundtrip(&mut state).ok()?;
        let thread = std::thread::Builder::new()
            .name("ink-tablet-wayland".into())
            .spawn(move || {
                let _conn = conn;
                while event_queue.blocking_dispatch(&mut state).is_ok() {}
            })
            .ok()?;
        Some(Backend { _thread: thread })
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(state: &mut Self, registry: &wl_registry::WlRegistry, event: wl_registry::Event, _: &(), _: &Connection, qh: &QueueHandle<Self>) {
        if let wl_registry::Event::Global { name, interface, version } = event {
            match interface.as_str() {
                "zwp_tablet_manager_v2" => state.manager = Some(registry.bind(name, version.min(1), qh, ())),
                "wl_seat" => state.seats.push(registry.bind(name, version.min(5), qh, ())),
                _ => {}
            }
        }
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for State {
    fn event(_: &mut Self, _: &wl_seat::WlSeat, _: wl_seat::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<ZwpTabletManagerV2, ()> for State {
    fn event(_: &mut Self, _: &ZwpTabletManagerV2, _: <ZwpTabletManagerV2 as Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<ZwpTabletSeatV2, ()> for State {
    fn event(state: &mut Self, _: &ZwpTabletSeatV2, event: zwp_tablet_seat_v2::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if let zwp_tablet_seat_v2::Event::ToolAdded { id } = event {
            state.tools.push((id, ToolState::default()));
        }
    }

    wayland_client::event_created_child!(State, ZwpTabletSeatV2, [
        zwp_tablet_seat_v2::EVT_TABLET_ADDED_OPCODE => (ZwpTabletV2, ()),
        zwp_tablet_seat_v2::EVT_TOOL_ADDED_OPCODE => (ZwpTabletToolV2, ()),
        zwp_tablet_seat_v2::EVT_PAD_ADDED_OPCODE => (wayland_protocols::wp::tablet::zv2::client::zwp_tablet_pad_v2::ZwpTabletPadV2, ()),
    ]);
}

impl Dispatch<ZwpTabletV2, ()> for State {
    fn event(_: &mut Self, _: &ZwpTabletV2, _: <ZwpTabletV2 as Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<wayland_protocols::wp::tablet::zv2::client::zwp_tablet_pad_v2::ZwpTabletPadV2, ()> for State {
    fn event(_: &mut Self, _: &wayland_protocols::wp::tablet::zv2::client::zwp_tablet_pad_v2::ZwpTabletPadV2, _: <wayland_protocols::wp::tablet::zv2::client::zwp_tablet_pad_v2::ZwpTabletPadV2 as Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}

    wayland_client::event_created_child!(State, wayland_protocols::wp::tablet::zv2::client::zwp_tablet_pad_v2::ZwpTabletPadV2, [
        wayland_protocols::wp::tablet::zv2::client::zwp_tablet_pad_v2::EVT_GROUP_OPCODE => (wayland_protocols::wp::tablet::zv2::client::zwp_tablet_pad_group_v2::ZwpTabletPadGroupV2, ()),
    ]);
}

impl Dispatch<wayland_protocols::wp::tablet::zv2::client::zwp_tablet_pad_group_v2::ZwpTabletPadGroupV2, ()> for State {
    fn event(_: &mut Self, _: &wayland_protocols::wp::tablet::zv2::client::zwp_tablet_pad_group_v2::ZwpTabletPadGroupV2, _: <wayland_protocols::wp::tablet::zv2::client::zwp_tablet_pad_group_v2::ZwpTabletPadGroupV2 as Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}

    wayland_client::event_created_child!(State, wayland_protocols::wp::tablet::zv2::client::zwp_tablet_pad_group_v2::ZwpTabletPadGroupV2, [
        wayland_protocols::wp::tablet::zv2::client::zwp_tablet_pad_group_v2::EVT_RING_OPCODE => (wayland_protocols::wp::tablet::zv2::client::zwp_tablet_pad_ring_v2::ZwpTabletPadRingV2, ()),
        wayland_protocols::wp::tablet::zv2::client::zwp_tablet_pad_group_v2::EVT_STRIP_OPCODE => (wayland_protocols::wp::tablet::zv2::client::zwp_tablet_pad_strip_v2::ZwpTabletPadStripV2, ()),
    ]);
}

impl Dispatch<wayland_protocols::wp::tablet::zv2::client::zwp_tablet_pad_ring_v2::ZwpTabletPadRingV2, ()> for State {
    fn event(_: &mut Self, _: &wayland_protocols::wp::tablet::zv2::client::zwp_tablet_pad_ring_v2::ZwpTabletPadRingV2, _: <wayland_protocols::wp::tablet::zv2::client::zwp_tablet_pad_ring_v2::ZwpTabletPadRingV2 as Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<wayland_protocols::wp::tablet::zv2::client::zwp_tablet_pad_strip_v2::ZwpTabletPadStripV2, ()> for State {
    fn event(_: &mut Self, _: &wayland_protocols::wp::tablet::zv2::client::zwp_tablet_pad_strip_v2::ZwpTabletPadStripV2, _: <wayland_protocols::wp::tablet::zv2::client::zwp_tablet_pad_strip_v2::ZwpTabletPadStripV2 as Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<ZwpTabletToolV2, ()> for State {
    fn event(state: &mut Self, tool: &ZwpTabletToolV2, event: zwp_tablet_tool_v2::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        let surface = state.surface.clone();
        let queue = state.queue.clone();
        let Some((_, t)) = state.tools.iter_mut().find(|(x, _)| x == tool) else { return };
        use zwp_tablet_tool_v2::Event as E;
        match event {
            E::Type { tool_type } => t.eraser = matches!(tool_type, wayland_client::WEnum::Value(zwp_tablet_tool_v2::Type::Eraser)),
            E::ProximityIn { surface: s, .. } => {
                t.on_our_surface = s == surface;
                t.changed = true;
            }
            E::ProximityOut => {
                t.on_our_surface = false;
                t.down = false;
                t.changed = true;
            }
            E::Down { .. } => {
                t.down = true;
                t.changed = true;
            }
            E::Up => {
                t.down = false;
                t.changed = true;
            }
            E::Motion { x, y } => {
                t.x = x;
                t.y = y;
                t.changed = true;
            }
            E::Pressure { pressure } => {
                t.pressure = pressure as f32 / 65535.0;
                t.changed = true;
            }
            E::Tilt { tilt_x, tilt_y } => {
                // Degrees (-90..90) → -1..1.
                t.tilt = Some(((tilt_x / 90.0) as f32, (tilt_y / 90.0) as f32));
                t.changed = true;
            }
            E::Frame { .. } => {
                if let Some(s) = t.frame() {
                    queue.push(s);
                }
            }
            E::Removed => t.on_our_surface = false,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_turn_tool_events_into_samples() {
        let mut t = ToolState { on_our_surface: true, ..Default::default() };
        assert!(t.frame().is_none(), "nothing changed");
        t.x = 10.0;
        t.y = 20.0;
        t.changed = true;
        assert_eq!(t.frame().unwrap().phase, Phase::Hover);
        t.down = true;
        t.pressure = 0.4;
        t.changed = true;
        let s = t.frame().unwrap();
        assert_eq!(s.phase, Phase::Down);
        assert!((s.pressure - 0.4).abs() < 1e-6);
        t.x = 12.0;
        t.changed = true;
        assert_eq!(t.frame().unwrap().phase, Phase::Move);
        t.down = false;
        t.changed = true;
        let up = t.frame().unwrap();
        assert_eq!(up.phase, Phase::Up);
        assert_eq!(up.pressure, 0.0);
        // Hovering over another window: ignored.
        t.on_our_surface = false;
        t.changed = true;
        assert!(t.frame().is_none());
    }
}
