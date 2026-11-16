//! Pen tablet input where the windowing library doesn't provide it.
//!
//! winit 0.30 reports pen pressure only on Windows. This crate adds it elsewhere:
//!
//! | platform | source | notes |
//! | --- | --- | --- |
//! | macOS | `NSEvent` local monitor | pressure + tilt of tablet mouse events and tablet points |
//! | Linux X11 | XInput2 motion valuators | "Abs Pressure" / "Abs Tilt X/Y" axes of every tablet device |
//! | Linux Wayland | `zwp_tablet_v2` on the app's own display | full tool events; compositors stop emulating the pointer for clients that bind the tablet seat, so these samples also drive the UI pointer |
//! | Windows | – | handled by winit (Windows Ink) |
//!
//! Every backend pushes [`Sample`]s into a shared queue; the app drains it once per frame.

use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(all(unix, not(target_os = "macos")))]
mod wayland;
#[cfg(all(unix, not(target_os = "macos")))]
pub mod x11;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// Pen touched the surface.
    Down,
    /// Pen moved while touching.
    Move,
    /// Pen lifted.
    Up,
    /// Pen moved above the surface (in proximity, not touching).
    Hover,
}

/// Where a sample's position is measured.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Units {
    /// Logical points (macOS, Wayland surface coordinates).
    Logical,
    /// Physical pixels (X11): divide by the scale factor.
    Physical,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {
    pub phase: Phase,
    /// Position relative to the window's content area, top-left origin.
    pub x: f32,
    pub y: f32,
    pub units: Units,
    /// 0..=1.
    pub pressure: f32,
    /// Tilt in -1..=1 per axis, if the device reports it.
    pub tilt: Option<(f32, f32)>,
    /// The eraser end of the pen.
    pub eraser: bool,
}

/// Shared sample queue (bounded: the app drains every frame; old samples drop if it stalls).
#[derive(Clone, Default)]
pub struct Queue(Arc<Mutex<VecDeque<Sample>>>);

impl Queue {
    pub fn push(&self, s: Sample) {
        if let Ok(mut q) = self.0.lock() {
            if q.len() >= 8192 {
                q.pop_front();
            }
            q.push_back(s);
        }
    }
    pub fn drain(&self) -> Vec<Sample> {
        self.0
            .lock()
            .map(|mut q| q.drain(..).collect())
            .unwrap_or_default()
    }
}

/// An attached tablet backend. Dropping it stops the backend.
pub struct Tablet {
    queue: Queue,
    name: &'static str,
    drives_pointer: bool,
    _backend: Box<dyn std::any::Any>,
}

impl Tablet {
    /// Attach to the window. Returns `None` where no backend applies (Windows) or the
    /// platform refuses (no XInput2, no tablet protocol, …). Never panics.
    pub fn attach(window: &(impl HasWindowHandle + HasDisplayHandle)) -> Option<Tablet> {
        let wh = window.window_handle().ok()?.as_raw();
        let dh = window.display_handle().ok()?.as_raw();
        let queue = Queue::default();
        #[cfg(target_os = "macos")]
        {
            let _ = dh;
            let b = macos::Backend::attach(wh, queue.clone())?;
            return Some(Tablet {
                queue,
                name: "macOS tablet events",
                drives_pointer: false,
                _backend: Box::new(b),
            });
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            use raw_window_handle::{RawDisplayHandle, RawWindowHandle};
            match (wh, dh) {
                (RawWindowHandle::Wayland(w), RawDisplayHandle::Wayland(d)) => {
                    let b = wayland::Backend::attach(
                        d.display.as_ptr(),
                        w.surface.as_ptr(),
                        queue.clone(),
                    )?;
                    return Some(Tablet {
                        queue,
                        name: "Wayland tablet-v2",
                        drives_pointer: true,
                        _backend: Box::new(b),
                    });
                }
                (RawWindowHandle::Xlib(w), _) => {
                    let b = x11::Backend::attach(w.window as u32, queue.clone())?;
                    return Some(Tablet {
                        queue,
                        name: "X11 XInput2",
                        drives_pointer: false,
                        _backend: Box::new(b),
                    });
                }
                (RawWindowHandle::Xcb(w), _) => {
                    let b = x11::Backend::attach(w.window.get(), queue.clone())?;
                    return Some(Tablet {
                        queue,
                        name: "X11 XInput2",
                        drives_pointer: false,
                        _backend: Box::new(b),
                    });
                }
                _ => return None,
            }
        }
        #[allow(unreachable_code)]
        {
            let _ = (wh, dh, queue);
            None
        }
    }

    /// Samples since the last call, oldest first.
    pub fn drain(&self) -> Vec<Sample> {
        self.queue.drain()
    }

    /// Human-readable backend name (status bar).
    pub fn name(&self) -> &'static str {
        self.name
    }

    /// True if the pen produces no pointer events of its own on this platform (Wayland):
    /// the app must turn samples into pointer events so the pen can also click the UI.
    pub fn drives_pointer(&self) -> bool {
        self.drives_pointer
    }
}

/// Map a raw axis value to 0..=1.
pub fn normalize(v: f64, min: f64, max: f64) -> f32 {
    if max <= min {
        return 0.0;
    }
    (((v - min) / (max - min)).clamp(0.0, 1.0)) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_is_bounded_and_ordered() {
        let q = Queue::default();
        for i in 0..9000 {
            q.push(Sample {
                phase: Phase::Move,
                x: i as f32,
                y: 0.0,
                units: Units::Logical,
                pressure: 0.5,
                tilt: None,
                eraser: false,
            });
        }
        let v = q.drain();
        assert_eq!(v.len(), 8192);
        assert_eq!(v[0].x, 808.0);
        assert!(q.drain().is_empty());
    }

    #[test]
    fn normalize_clamps() {
        assert_eq!(normalize(0.0, 0.0, 2047.0), 0.0);
        assert!((normalize(1023.5, 0.0, 2047.0) - 0.5).abs() < 1e-6);
        assert_eq!(normalize(5000.0, 0.0, 2047.0), 1.0);
        assert_eq!(normalize(1.0, 1.0, 1.0), 0.0);
    }
}
