//! Bridge from `ink-tablet` samples to egui input events.
//!
//! Every sample becomes an `Event::Touch` with `force` (the same shape egui-winit produces for
//! Windows Ink), which `PenInput` reads as real pen pressure. Where the pen produces no pointer
//! events of its own (Wayland), samples are also turned into pointer moves and clicks so the
//! pen can operate menus and palettes.

use egui::{Event, Modifiers, PointerButton, TouchDeviceId, TouchId, TouchPhase, pos2};
use ink_tablet::{Phase, Sample, Units};

pub const DEVICE: TouchDeviceId = TouchDeviceId(0x7AB1E7);

pub fn to_events(samples: &[Sample], pixels_per_point: f32, drives_pointer: bool) -> Vec<Event> {
    let mut out = Vec::with_capacity(samples.len() * 2);
    for s in samples {
        let k = if s.units == Units::Physical {
            1.0 / pixels_per_point.max(0.1)
        } else {
            1.0
        };
        let pos = pos2(s.x * k, s.y * k);
        let phase = match s.phase {
            Phase::Down => Some(TouchPhase::Start),
            Phase::Move => Some(TouchPhase::Move),
            Phase::Up => Some(TouchPhase::End),
            Phase::Hover => None,
        };
        if drives_pointer {
            out.push(Event::PointerMoved(pos));
            match s.phase {
                Phase::Down => out.push(Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::NONE,
                }),
                Phase::Up => out.push(Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed: false,
                    modifiers: Modifiers::NONE,
                }),
                _ => {}
            }
        }
        if let Some(phase) = phase {
            out.push(Event::Touch {
                device_id: DEVICE,
                id: TouchId(1),
                phase,
                pos,
                force: Some(s.pressure.clamp(0.0, 1.0)),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(phase: Phase, x: f32, p: f32, units: Units) -> Sample {
        Sample {
            phase,
            x,
            y: 20.0,
            units,
            pressure: p,
            tilt: None,
            eraser: false,
        }
    }

    #[test]
    fn physical_pixels_are_scaled_and_hover_has_no_touch() {
        let ev = to_events(
            &[
                s(Phase::Hover, 100.0, 0.0, Units::Physical),
                s(Phase::Down, 100.0, 0.4, Units::Physical),
            ],
            2.0,
            false,
        );
        assert_eq!(ev.len(), 1);
        match &ev[0] {
            Event::Touch {
                pos, force, phase, ..
            } => {
                assert_eq!(*pos, pos2(50.0, 10.0));
                assert_eq!(*force, Some(0.4));
                assert_eq!(*phase, TouchPhase::Start);
            }
            e => panic!("{e:?}"),
        }
    }

    #[test]
    fn wayland_samples_also_drive_the_pointer() {
        let ev = to_events(
            &[
                s(Phase::Down, 10.0, 0.5, Units::Logical),
                s(Phase::Move, 12.0, 0.6, Units::Logical),
                s(Phase::Up, 12.0, 0.0, Units::Logical),
            ],
            1.5,
            true,
        );
        let presses = ev
            .iter()
            .filter(|e| matches!(e, Event::PointerButton { pressed: true, .. }))
            .count();
        let releases = ev
            .iter()
            .filter(|e| matches!(e, Event::PointerButton { pressed: false, .. }))
            .count();
        let touches = ev
            .iter()
            .filter(|e| matches!(e, Event::Touch { .. }))
            .count();
        assert_eq!((presses, releases, touches), (1, 1, 3));
    }
}
