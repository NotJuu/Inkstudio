//! X11: XInput2 motion events on the app window, read on our own connection.
//!
//! winit already selects XI2 button events on its window (only one client may), so we
//! select XI_Motion only (not exclusive). Each motion carries the device's valuators;
//! the pressure / tilt axes are found by their labels ("Abs Pressure", "Abs Tilt X/Y").
//! Pen down/up is derived from button 1 in the event's button mask.

use crate::{Phase, Queue, Sample, Units, normalize};
use std::collections::HashMap;
use x11rb::connection::Connection;
use x11rb::protocol::Event;
use x11rb::protocol::xinput::{self, ConnectionExt as _, DeviceClassData, Fp3232};
use x11rb::protocol::xproto::ConnectionExt as _;

/// Axis indices and ranges of one device.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Axes {
    pub pressure: Option<(u16, f64, f64)>,
    pub tilt_x: Option<(u16, f64, f64)>,
    pub tilt_y: Option<(u16, f64, f64)>,
    pub eraser: bool,
}

pub struct Backend {
    _thread: std::thread::JoinHandle<()>,
}

fn fp3232(v: Fp3232) -> f64 {
    v.integral as f64 + v.frac as f64 / 4_294_967_296.0
}

/// Values of the axes present in `mask`, in axis order (XI2 packs only the set bits).
pub fn axis_values(mask: &[u32], values: &[f64]) -> HashMap<u16, f64> {
    let mut out = HashMap::new();
    let mut vi = 0;
    for (word, bits) in mask.iter().enumerate() {
        for bit in 0..32 {
            if bits & (1 << bit) != 0 {
                if let Some(v) = values.get(vi) {
                    out.insert((word * 32 + bit) as u16, *v);
                }
                vi += 1;
            }
        }
    }
    out
}

/// Build a sample from a motion event's data. `None` if the device has no pressure axis.
pub fn sample_from(
    axes: &Axes,
    vals: &HashMap<u16, f64>,
    x: f64,
    y: f64,
    button1: bool,
    was_down: bool,
) -> Option<Sample> {
    let (pi, pmin, pmax) = axes.pressure?;
    let pressure = vals.get(&pi).map(|v| normalize(*v, pmin, pmax))?;
    let tilt = match (axes.tilt_x, axes.tilt_y) {
        (Some((xi, xmin, xmax)), Some((yi, ymin, ymax))) => match (vals.get(&xi), vals.get(&yi)) {
            (Some(tx), Some(ty)) => Some((
                normalize(*tx, xmin, xmax) * 2.0 - 1.0,
                normalize(*ty, ymin, ymax) * 2.0 - 1.0,
            )),
            _ => None,
        },
        _ => None,
    };
    let phase = match (was_down, button1) {
        (false, true) => Phase::Down,
        (true, true) => Phase::Move,
        (true, false) => Phase::Up,
        (false, false) => Phase::Hover,
    };
    Some(Sample {
        phase,
        x: x as f32,
        y: y as f32,
        units: Units::Physical,
        pressure,
        tilt,
        eraser: axes.eraser,
    })
}

fn query_axes(conn: &impl Connection) -> HashMap<u16, Axes> {
    let mut out = HashMap::new();
    let Ok(reply) = conn
        .xinput_xi_query_device(xinput::Device::ALL)
        .and_then(|c| Ok(c.reply()))
    else {
        return out;
    };
    let Ok(reply) = reply else { return out };
    let atom_name = |a: u32| -> String {
        conn.get_atom_name(a)
            .ok()
            .and_then(|c| c.reply().ok())
            .map(|r| String::from_utf8_lossy(&r.name).into_owned())
            .unwrap_or_default()
    };
    for info in reply.infos {
        let name = String::from_utf8_lossy(&info.name).to_lowercase();
        let mut axes = Axes {
            eraser: name.contains("eraser"),
            ..Default::default()
        };
        for class in &info.classes {
            if let DeviceClassData::Valuator(v) = &class.data {
                let label = atom_name(v.label);
                let range = (v.number, fp3232(v.min), fp3232(v.max));
                match label.as_str() {
                    "Abs Pressure" => axes.pressure = Some(range),
                    "Abs Tilt X" => axes.tilt_x = Some(range),
                    "Abs Tilt Y" => axes.tilt_y = Some(range),
                    _ => {}
                }
            }
        }
        if axes.pressure.is_some() {
            out.insert(info.deviceid, axes);
        }
    }
    out
}

impl Backend {
    pub fn attach(window: u32, queue: Queue) -> Option<Backend> {
        let (conn, _screen) = x11rb::connect(None).ok()?;
        // XInput 2.2+.
        let v = conn.xinput_xi_query_version(2, 2).ok()?.reply().ok()?;
        if (v.major_version, v.minor_version) < (2, 2) {
            return None;
        }
        let mask = xinput::EventMask {
            deviceid: xinput::Device::ALL.into(),
            mask: vec![(xinput::XIEventMask::MOTION | xinput::XIEventMask::HIERARCHY).into()],
        };
        conn.xinput_xi_select_events(window, &[mask])
            .ok()?
            .check()
            .ok()?;
        conn.flush().ok()?;
        let thread = std::thread::Builder::new()
            .name("ink-tablet-x11".into())
            .spawn(move || {
                let mut devices = query_axes(&conn);
                let mut down: HashMap<u16, bool> = HashMap::new();
                while let Ok(ev) = conn.wait_for_event() {
                    match ev {
                        Event::XinputMotion(m) => {
                            if !devices.contains_key(&m.sourceid)
                                && !devices.contains_key(&m.deviceid)
                            {
                                continue;
                            }
                            let id = if devices.contains_key(&m.sourceid) {
                                m.sourceid
                            } else {
                                m.deviceid
                            };
                            let axes = &devices[&id];
                            let values: Vec<f64> =
                                m.axisvalues.iter().map(|v| fp3232(*v)).collect();
                            let vals = axis_values(&m.valuator_mask, &values);
                            let button1 = m.button_mask.first().is_some_and(|w| w & (1 << 1) != 0);
                            let was = *down.get(&id).unwrap_or(&false);
                            let x = m.event_x as f64 / 65536.0;
                            let y = m.event_y as f64 / 65536.0;
                            if let Some(s) = sample_from(axes, &vals, x, y, button1, was) {
                                down.insert(id, button1);
                                queue.push(s);
                            }
                        }
                        Event::XinputHierarchy(_) => devices = query_axes(&conn),
                        _ => {}
                    }
                }
            })
            .ok()?;
        Some(Backend { _thread: thread })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packed_valuators_follow_the_mask() {
        // Axes 0, 1, 2 and 33 set; values packed in that order.
        let mask = [0b111, 0b10];
        let vals = axis_values(&mask, &[10.0, 20.0, 1024.0, 7.0]);
        assert_eq!(vals[&0], 10.0);
        assert_eq!(vals[&2], 1024.0);
        assert_eq!(vals[&33], 7.0);
        assert_eq!(vals.len(), 4);
    }

    #[test]
    fn pressure_tilt_and_phases() {
        let axes = Axes {
            pressure: Some((2, 0.0, 2048.0)),
            tilt_x: Some((3, -64.0, 63.0)),
            tilt_y: Some((4, -64.0, 63.0)),
            eraser: false,
        };
        let vals: HashMap<u16, f64> = [(2, 1024.0), (3, -0.5), (4, 63.0)].into_iter().collect();
        let s = sample_from(&axes, &vals, 100.0, 50.0, true, false).unwrap();
        assert_eq!(s.phase, Phase::Down);
        assert!((s.pressure - 0.5).abs() < 1e-3);
        let (tx, ty) = s.tilt.unwrap();
        assert!(tx.abs() < 0.02 && (ty - 1.0).abs() < 1e-3);
        assert_eq!(
            sample_from(&axes, &vals, 0.0, 0.0, true, true)
                .unwrap()
                .phase,
            Phase::Move
        );
        assert_eq!(
            sample_from(&axes, &vals, 0.0, 0.0, false, true)
                .unwrap()
                .phase,
            Phase::Up
        );
        assert_eq!(
            sample_from(&axes, &vals, 0.0, 0.0, false, false)
                .unwrap()
                .phase,
            Phase::Hover
        );
        let mouse = Axes::default();
        assert!(
            sample_from(&mouse, &vals, 0.0, 0.0, true, false).is_none(),
            "devices without pressure are ignored"
        );
    }
}
