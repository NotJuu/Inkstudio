use super::focus_ring;
use crate::theme::{palette, tokens};
use egui::epaint::{Mesh, Vertex};
use egui::{
    Color32, Pos2, Rect, Response, Sense, Shape, Stroke, Ui, WidgetInfo, WidgetType, pos2, vec2,
};
use std::f32::consts::TAU;

/// Hue / saturation / value, all 0..=1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hsv {
    pub h: f32,
    pub s: f32,
    pub v: f32,
}

impl Hsv {
    pub fn to_color32(self) -> Color32 {
        let Hsv { h, s, v } = self;
        let i = (h * 6.0).floor();
        let f = h * 6.0 - i;
        let (p, q, t) = (v * (1.0 - s), v * (1.0 - f * s), v * (1.0 - (1.0 - f) * s));
        let (r, g, b) = match (i as i32).rem_euclid(6) {
            0 => (v, t, p),
            1 => (q, v, p),
            2 => (p, v, t),
            3 => (p, q, v),
            4 => (t, p, v),
            _ => (v, p, q),
        };
        Color32::from_rgb(
            (r * 255.0).round() as u8,
            (g * 255.0).round() as u8,
            (b * 255.0).round() as u8,
        )
    }
}

/// HSV wheel: hue ring around a saturation/value square (S06).
/// Pointer: drag the ring for hue, the square for S/V.
/// Keyboard: focused, ←/→ change hue, ↑/↓ change value, Shift+↑/↓ change saturation.
/// The exact numbers are always available in the sliders tab (keyboard / screen-reader path).
pub fn color_wheel(ui: &mut Ui, hsv: &mut Hsv, size: f32) -> Response {
    let p = palette(ui);
    let (rect, mut resp) = ui.allocate_exact_size(vec2(size, size), Sense::click_and_drag());
    let c = rect.center();
    let outer = size / 2.0 - 1.0;
    let ring = (size * 0.1).max(10.0);
    let inner = outer - ring;
    let half = inner * std::f32::consts::FRAC_1_SQRT_2 - 4.0;
    let square = Rect::from_center_size(c, vec2(half * 2.0, half * 2.0));

    // Interaction: which part was grabbed is remembered for the whole drag.
    let id = resp.id.with("part");
    if let Some(pos) = resp.interact_pointer_pos() {
        if resp.drag_started() || resp.clicked() {
            let d = (pos - c).length();
            let part: u8 = if d >= inner - 2.0 { 1 } else { 2 };
            ui.data_mut(|m| m.insert_temp(id, part));
        }
        match ui.data(|m| m.get_temp::<u8>(id)) {
            Some(1) => {
                let a = (pos - c).angle();
                hsv.h = (a / TAU).rem_euclid(1.0);
                resp.mark_changed();
            }
            Some(2) => {
                hsv.s = ((pos.x - square.left()) / square.width()).clamp(0.0, 1.0);
                hsv.v = 1.0 - ((pos.y - square.top()) / square.height()).clamp(0.0, 1.0);
                resp.mark_changed();
            }
            _ => {}
        }
    }
    if resp.has_focus() {
        let (l, r, u, d, shift) = ui.input(|i| {
            (
                i.key_pressed(egui::Key::ArrowLeft),
                i.key_pressed(egui::Key::ArrowRight),
                i.key_pressed(egui::Key::ArrowUp),
                i.key_pressed(egui::Key::ArrowDown),
                i.modifiers.shift,
            )
        });
        let step = 1.0 / 100.0;
        if l || r {
            hsv.h = (hsv.h + if r { step } else { -step }).rem_euclid(1.0);
            resp.mark_changed();
        }
        if u || d {
            let delta = if u { step } else { -step };
            if shift {
                hsv.s = (hsv.s + delta).clamp(0.0, 1.0);
            } else {
                hsv.v = (hsv.v + delta).clamp(0.0, 1.0);
            }
            resp.mark_changed();
        }
    }
    let col = hsv.to_color32();
    resp.widget_info(|| {
        WidgetInfo::labeled(
            WidgetType::Other,
            ui.is_enabled(),
            format!(
                "Color wheel, hue {:.0}°, saturation {:.0}%, value {:.0}%, #{:02X}{:02X}{:02X}",
                hsv.h * 360.0,
                hsv.s * 100.0,
                hsv.v * 100.0,
                col.r(),
                col.g(),
                col.b()
            ),
        )
    });

    if ui.is_rect_visible(rect) {
        let mut mesh = Mesh::default();
        // Hue ring.
        let n = 96;
        for i in 0..=n {
            let t = i as f32 / n as f32;
            let a = t * TAU;
            let dir = vec2(a.cos(), a.sin());
            let hue = Hsv {
                h: t,
                s: 1.0,
                v: 1.0,
            }
            .to_color32();
            mesh.vertices.push(Vertex {
                pos: c + dir * inner,
                uv: egui::epaint::WHITE_UV,
                color: hue,
            });
            mesh.vertices.push(Vertex {
                pos: c + dir * outer,
                uv: egui::epaint::WHITE_UV,
                color: hue,
            });
            if i < n {
                let b = (2 * i) as u32;
                mesh.add_triangle(b, b + 1, b + 2);
                mesh.add_triangle(b + 1, b + 3, b + 2);
            }
        }
        // S/V square: a grid so the bilinear color interpolation is close to true HSV.
        let g = 12;
        let base = mesh.vertices.len() as u32;
        for y in 0..=g {
            for x in 0..=g {
                let (s, v) = (x as f32 / g as f32, 1.0 - y as f32 / g as f32);
                let pos = pos2(
                    square.left() + s * square.width(),
                    square.top() + (1.0 - v) * square.height(),
                );
                mesh.vertices.push(Vertex {
                    pos,
                    uv: egui::epaint::WHITE_UV,
                    color: Hsv { h: hsv.h, s, v }.to_color32(),
                });
            }
        }
        for y in 0..g {
            for x in 0..g {
                let i = base + y * (g + 1) + x;
                mesh.add_triangle(i, i + 1, i + g + 1);
                mesh.add_triangle(i + 1, i + g + 2, i + g + 1);
            }
        }
        ui.painter().add(Shape::mesh(mesh));
        ui.painter().rect_stroke(
            square,
            0,
            Stroke::new(1.0, p.border),
            egui::StrokeKind::Outside,
        );

        // Markers: dual rings so they read on any color.
        let a = hsv.h * TAU;
        let hue_pos: Pos2 = c + vec2(a.cos(), a.sin()) * (inner + ring / 2.0);
        let sv_pos = pos2(
            square.left() + hsv.s * square.width(),
            square.top() + (1.0 - hsv.v) * square.height(),
        );
        for m in [hue_pos, sv_pos] {
            ui.painter()
                .circle_stroke(m, 5.0, Stroke::new(3.0, Color32::from_black_alpha(160)));
            ui.painter()
                .circle_stroke(m, 5.0, Stroke::new(1.5, Color32::WHITE));
        }
        focus_ring(ui, &resp, rect, tokens::radius::MD);
    }
    resp
}
