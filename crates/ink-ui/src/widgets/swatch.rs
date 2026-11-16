use super::focus_ring;
use crate::theme::{palette, tokens};
use egui::{Color32, CornerRadius, Rect, Response, Sense, Stroke, StrokeKind, Ui, WidgetInfo, WidgetType, vec2};

fn checker(ui: &Ui, rect: Rect) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, CornerRadius::ZERO, Color32::from_gray(255));
    let s = 4.0;
    let (cols, rows) = ((rect.width() / s).ceil() as i32, (rect.height() / s).ceil() as i32);
    for y in 0..rows {
        for x in 0..cols {
            if (x + y) % 2 == 0 {
                let r = Rect::from_min_size(rect.min + vec2(x as f32 * s, y as f32 * s), vec2(s, s));
                painter.rect_filled(r, CornerRadius::ZERO, Color32::from_gray(204));
            }
        }
    }
}

fn hex(c: Color32) -> String {
    format!("#{:02X}{:02X}{:02X}, {}% opacity", c.r(), c.g(), c.b(), (c.a() as u32 * 100 + 127) / 255)
}

/// A clickable color chip: color sets and history (S06). Transparent colors show a checkerboard.
/// Accessible name includes the hex value so the color is not conveyed by color alone.
pub fn swatch(ui: &mut Ui, color: Color32, size: f32, selected: bool) -> Response {
    let p = palette(ui);
    let (rect, resp) = ui.allocate_exact_size(vec2(size, size), Sense::click());
    resp.widget_info(|| WidgetInfo::selected(WidgetType::Button, ui.is_enabled(), selected, hex(color)));
    if ui.is_rect_visible(rect) {
        if color.a() < 255 {
            checker(ui, rect);
        }
        ui.painter().rect_filled(rect, CornerRadius::same(tokens::radius::SM as u8), color);
        let border = if selected { Stroke::new(2.0, p.focus_ring) } else { Stroke::new(1.0, p.border_strong) };
        ui.painter().rect_stroke(rect, CornerRadius::same(tokens::radius::SM as u8), border, StrokeKind::Inside);
        focus_ring(ui, &resp, rect, tokens::radius::SM);
    }
    resp.on_hover_text(hex(color))
}

/// Main / sub color pair with swap (S06). Returns (main clicked, sub clicked, swap clicked).
pub fn color_pair(ui: &mut Ui, main: Color32, sub: Color32) -> (bool, bool, bool) {
    let p = palette(ui);
    let size = 44.0;
    let (rect, _) = ui.allocate_exact_size(vec2(size + 18.0, size + 18.0), Sense::hover());
    let sub_rect = Rect::from_min_size(rect.min + vec2(18.0, 18.0), vec2(size, size));
    let main_rect = Rect::from_min_size(rect.min, vec2(size, size));
    let s = ui.put(sub_rect, |ui: &mut Ui| swatch(ui, sub, size, false));
    let m = ui.put(main_rect, |ui: &mut Ui| swatch(ui, main, size, true));
    let swap_rect = Rect::from_min_size(rect.min + vec2(size + 2.0, 0.0), vec2(16.0, 16.0));
    let swap = ui.put(swap_rect, super::IconButton::new(crate::icons::ARROWS_LEFT_RIGHT, "Swap main and sub color").size(16.0));
    let _ = p;
    (m.clicked(), s.clicked(), swap.clicked())
}
