//! Pen and mouse input → pen samples with pressure.
//!
//! Every input event of a frame is used (not just the last pointer position), so fast
//! strokes keep all their samples even when the app renders slower than the tablet reports.
//!
//! Pressure sources:
//! * **Pen (Windows Ink)**: winit reports pens as touches with normalized force; egui passes
//!   them as `Event::Touch { force }`. That is real pen pressure.
//! * **Mouse / pens without pressure** (macOS and Linux with winit 0.30): pressure is either
//!   1.0 or, if `simulate` is on, derived from speed (slow = heavy, fast = light), which gives
//!   mouse users tapered, lively lines.

use egui::{Event, PointerButton, Pos2, TouchPhase};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PenSample {
    pub pos: Pos2,
    pub pressure: f32,
    pub time: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PenEvent {
    Down(PenSample),
    Move(PenSample),
    Up(PenSample),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Mouse,
    Pen,
}

#[derive(Debug, Default)]
pub struct PenInput {
    down: Option<Source>,
    last: Option<PenSample>,
    sim_pressure: f32,
    /// Derive pressure from speed for devices without pressure.
    pub simulate: bool,
    /// Last real pressure seen (status bar).
    pub last_pressure: f32,
    pub saw_pen: bool,
}

impl PenInput {
    fn mouse_pressure(&mut self, pos: Pos2, time: f64) -> f32 {
        if !self.simulate {
            return 1.0;
        }
        let Some(l) = self.last else { return 0.5 };
        let dt = (time - l.time).max(1e-3) as f32;
        let speed = (pos - l.pos).length() / dt; // screen points per second
        let target = (1.0 - speed / 2500.0).clamp(0.25, 1.0);
        self.sim_pressure = if self.sim_pressure == 0.0 { target } else { self.sim_pressure * 0.75 + target * 0.25 };
        self.sim_pressure
    }

    /// Turn this frame's raw events into pen events. `inside` decides if a stroke may start at a position.
    pub fn process(&mut self, events: &[Event], time: f64, inside: impl Fn(Pos2) -> bool) -> Vec<PenEvent> {
        let mut out = Vec::new();
        let pen_in_frame = events.iter().any(|e| matches!(e, Event::Touch { force: Some(_), .. }));
        for e in events {
            match *e {
                Event::Touch { phase, pos, force: Some(f), .. } => {
                    self.saw_pen = true;
                    let s = PenSample { pos, pressure: f.clamp(0.0, 1.0), time };
                    if phase != TouchPhase::End {
                        self.last_pressure = s.pressure;
                    }
                    match phase {
                        TouchPhase::Start if inside(pos) => {
                            self.down = Some(Source::Pen);
                            out.push(PenEvent::Down(s));
                        }
                        TouchPhase::Move if self.down == Some(Source::Pen) => out.push(PenEvent::Move(s)),
                        TouchPhase::End | TouchPhase::Cancel if self.down == Some(Source::Pen) => {
                            self.down = None;
                            out.push(PenEvent::Up(s));
                        }
                        _ => {}
                    }
                    self.last = Some(s);
                }
                // egui also emits pointer events for the first touch; ignore them while a pen is active.
                Event::PointerButton { pos, button: PointerButton::Primary, pressed, .. } if !pen_in_frame && self.down != Some(Source::Pen) => {
                    if pressed && inside(pos) {
                        self.sim_pressure = 0.0;
                        self.last = None;
                        let p = self.mouse_pressure(pos, time);
                        let s = PenSample { pos, pressure: p, time };
                        self.down = Some(Source::Mouse);
                        out.push(PenEvent::Down(s));
                        self.last = Some(s);
                    } else if !pressed && self.down == Some(Source::Mouse) {
                        let s = PenSample { pos, pressure: self.last.map_or(1.0, |l| l.pressure), time };
                        self.down = None;
                        out.push(PenEvent::Up(s));
                    }
                }
                Event::PointerMoved(pos) if !pen_in_frame && self.down == Some(Source::Mouse) => {
                    let p = self.mouse_pressure(pos, time);
                    let s = PenSample { pos, pressure: p, time };
                    out.push(PenEvent::Move(s));
                    self.last = Some(s);
                }
                Event::PointerGone if self.down == Some(Source::Mouse) => {
                    if let Some(l) = self.last {
                        out.push(PenEvent::Up(l));
                    }
                    self.down = None;
                }
                _ => {}
            }
        }
        out
    }

    pub fn is_down(&self) -> bool {
        self.down.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Modifiers, TouchDeviceId, TouchId, pos2};

    fn touch(phase: TouchPhase, x: f32, f: f32) -> Event {
        Event::Touch { device_id: TouchDeviceId(1), id: TouchId(0), phase, pos: pos2(x, 10.0), force: Some(f) }
    }

    #[test]
    fn pen_pressure_passes_through_and_pointer_echo_is_ignored() {
        let mut p = PenInput::default();
        let ev = [
            touch(TouchPhase::Start, 10.0, 0.2),
            Event::PointerButton { pos: pos2(10.0, 10.0), button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE },
            touch(TouchPhase::Move, 20.0, 0.6),
            touch(TouchPhase::Move, 30.0, 0.9),
        ];
        let out = p.process(&ev, 0.0, |_| true);
        assert_eq!(out.len(), 3, "{out:?}");
        assert!(matches!(out[0], PenEvent::Down(s) if (s.pressure - 0.2).abs() < 1e-6));
        assert!(matches!(out[2], PenEvent::Move(s) if (s.pressure - 0.9).abs() < 1e-6));
        let out = p.process(&[touch(TouchPhase::End, 30.0, 0.0)], 0.1, |_| true);
        assert!(matches!(out[0], PenEvent::Up(_)));
        assert!(!p.is_down());
    }

    #[test]
    fn mouse_strokes_and_all_moves_are_kept() {
        let mut p = PenInput::default();
        let mut ev = vec![Event::PointerButton { pos: pos2(0.0, 0.0), button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE }];
        for i in 1..=5 {
            ev.push(Event::PointerMoved(pos2(i as f32 * 3.0, 0.0)));
        }
        let out = p.process(&ev, 0.0, |_| true);
        assert_eq!(out.len(), 6, "every move in the frame becomes a sample");
        assert!(out.iter().all(|e| match e {
            PenEvent::Down(s) | PenEvent::Move(s) | PenEvent::Up(s) => s.pressure == 1.0,
        }));
    }

    #[test]
    fn strokes_only_start_inside_the_canvas() {
        let mut p = PenInput::default();
        let ev = [Event::PointerButton { pos: pos2(0.0, 0.0), button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE }];
        assert!(p.process(&ev, 0.0, |_| false).is_empty());
        assert!(!p.is_down());
    }

    #[test]
    fn simulated_pressure_drops_when_fast() {
        let mut p = PenInput { simulate: true, ..Default::default() };
        p.process(&[Event::PointerButton { pos: pos2(0.0, 0.0), button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE }], 0.0, |_| true);
        let slow = p.process(&[Event::PointerMoved(pos2(1.0, 0.0))], 0.1, |_| true);
        let mut fast = Vec::new();
        for i in 0..10 {
            fast = p.process(&[Event::PointerMoved(pos2(100.0 * (i + 2) as f32, 0.0))], 0.1 + (i + 1) as f64 * 0.01, |_| true);
        }
        let pr = |v: &[PenEvent]| match v[0] {
            PenEvent::Move(s) => s.pressure,
            _ => panic!(),
        };
        assert!(pr(&fast) < pr(&slow));
    }
}
