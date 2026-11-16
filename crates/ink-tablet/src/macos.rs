//! macOS: an `NSEvent` local monitor sees every mouse/tablet event before winit does.
//!
//! Wacom, Huion, XP-Pen and Apple Pencil via Sidecar deliver pen input as mouse events with
//! subtype `TabletPoint` (plus pure `TabletPoint` events); `pressure` and `tilt` are valid on
//! those. The monitor copies them into the queue and returns the event unchanged, so winit
//! still produces its normal pointer events for the UI.

use crate::{Phase, Queue, Sample, Units};
use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_app_kit::{NSEvent, NSEventMask, NSEventSubtype, NSEventType, NSView};
use raw_window_handle::RawWindowHandle;
use std::ptr::NonNull;

pub struct Backend {
    monitor: Option<Retained<AnyObject>>,
}

impl Drop for Backend {
    fn drop(&mut self) {
        if let Some(m) = self.monitor.take() {
            // SAFETY: removing the monitor we installed.
            unsafe { NSEvent::removeMonitor(&m) };
        }
    }
}

impl Backend {
    pub fn attach(window: RawWindowHandle, queue: Queue) -> Option<Backend> {
        let RawWindowHandle::AppKit(h) = window else { return None };
        // SAFETY: the handle points at the live content NSView of our window; we retain it.
        let view: Retained<NSView> = unsafe { Retained::retain(h.ns_view.as_ptr().cast()) }?;
        let mask = NSEventMask::LeftMouseDown | NSEventMask::LeftMouseUp | NSEventMask::LeftMouseDragged | NSEventMask::MouseMoved | NSEventMask::TabletPoint;
        let block = RcBlock::new(move |event: NonNull<NSEvent>| -> *mut NSEvent {
            // SAFETY: AppKit passes a valid event for the duration of the call.
            let ev = unsafe { event.as_ref() };
            if let Some(s) = sample(ev, &view) {
                queue.push(s);
            }
            event.as_ptr()
        });
        // SAFETY: the block lives as long as the monitor (AppKit copies it).
        let monitor = unsafe { NSEvent::addLocalMonitorForEventsMatchingMask_handler(mask, &block) };
        Some(Backend { monitor })
    }
}

fn sample(ev: &NSEvent, view: &NSView) -> Option<Sample> {
    let ty = ev.r#type();
    let is_tablet = ty == NSEventType::TabletPoint || ev.subtype() == NSEventSubtype::TabletPoint;
    if !is_tablet {
        return None;
    }
    // Only events for our window. Local monitors run on the main thread.
    let mtm = objc2::MainThreadMarker::new()?;
    let ours = match (ev.window(mtm), view.window()) {
        (Some(a), Some(b)) => Retained::as_ptr(&a) == Retained::as_ptr(&b),
        _ => false,
    };
    if !ours {
        return None;
    }
    let p = view.convertPoint_fromView(ev.locationInWindow(), None);
    let h = view.frame().size.height;
    let y = if view.isFlipped() { p.y } else { h - p.y };
    let phase = match ty {
        NSEventType::LeftMouseDown => Phase::Down,
        NSEventType::LeftMouseUp => Phase::Up,
        NSEventType::LeftMouseDragged => Phase::Move,
        NSEventType::TabletPoint if ev.pressure() > 0.0 => Phase::Move,
        _ => Phase::Hover,
    };
    let t = ev.tilt();
    Some(Sample {
        phase,
        x: p.x as f32,
        y: y as f32,
        units: Units::Logical,
        pressure: ev.pressure().clamp(0.0, 1.0),
        tilt: Some((t.x as f32, t.y as f32)),
        eraser: false,
    })
}
