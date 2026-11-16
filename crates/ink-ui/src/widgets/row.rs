use super::{focus_ring, IconButton};
use crate::icons;
use crate::theme::{palette, text, tokens};
use egui::{
    Align2, Color32, CornerRadius, Rect, Response, Sense, Stroke, StrokeKind, Ui, WidgetInfo,
    WidgetType, pos2, vec2,
};

/// Plain selectable row: sub tools (S04), materials (S25), recent files (S01), menus in lists.
pub fn list_row(ui: &mut Ui, icon: Option<&str>, label: &str, selected: bool) -> Response {
    let p = palette(ui);
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(vec2(w, tokens::layout::LIST_ROW_H), Sense::click());
    resp.widget_info(|| WidgetInfo::selected(WidgetType::SelectableLabel, ui.is_enabled(), selected, label));
    if ui.is_rect_visible(rect) {
        let fill = if selected {
            p.accent_subtle
        } else if resp.hovered() {
            p.surface_hover
        } else {
            Color32::TRANSPARENT
        };
        ui.painter().rect_filled(rect, CornerRadius::same(tokens::radius::SM as u8), fill);
        let mut x = rect.left() + tokens::space::S4;
        if let Some(i) = icon {
            ui.painter().text(pos2(x + 8.0, rect.center().y), Align2::CENTER_CENTER, i, crate::theme::icon_font(tokens::icon::MD), p.text_muted);
            x += 16.0 + tokens::space::S4;
        }
        ui.painter().text(pos2(x, rect.center().y), Align2::LEFT_CENTER, label, text::body().resolve(ui.style()), p.text);
        focus_ring(ui, &resp, rect, tokens::radius::SM);
    }
    resp
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayerKind {
    Raster,
    Vector,
    Tone,
    Text,
    Balloon,
    Folder,
    Frame,
    Animation,
    Correction,
    Paper,
}

impl LayerKind {
    fn icon(self) -> &'static str {
        match self {
            LayerKind::Raster => icons::PAINT_BRUSH,
            LayerKind::Vector => icons::BEZIER_CURVE,
            LayerKind::Tone => icons::DOTS_NINE,
            LayerKind::Text => icons::TEXT_T,
            LayerKind::Balloon => icons::CHAT_CIRCLE,
            LayerKind::Folder => icons::FOLDER_SIMPLE,
            LayerKind::Frame => icons::SQUARES_FOUR,
            LayerKind::Animation => icons::FILM_STRIP,
            LayerKind::Correction => icons::SLIDERS_HORIZONTAL,
            LayerKind::Paper => icons::FILE,
        }
    }
    fn name(self) -> &'static str {
        match self {
            LayerKind::Raster => "raster layer",
            LayerKind::Vector => "vector layer",
            LayerKind::Tone => "tone layer",
            LayerKind::Text => "text layer",
            LayerKind::Balloon => "balloon layer",
            LayerKind::Folder => "folder",
            LayerKind::Frame => "frame folder",
            LayerKind::Animation => "animation folder",
            LayerKind::Correction => "correction layer",
            LayerKind::Paper => "paper",
        }
    }
}

/// Everything a layer row shows. The palette owns the data; the row only draws and reports.
#[derive(Clone, Debug)]
pub struct LayerRowState {
    pub name: String,
    pub kind: LayerKind,
    pub depth: u8,
    pub visible: bool,
    pub locked: bool,
    pub clipped: bool,
    pub has_mask: bool,
    pub reference: bool,
    pub draft: bool,
    pub selected: bool,
    pub opacity: u8,
    pub blend: &'static str,
    /// Small preview, painted by the caller's renderer; `None` draws a placeholder.
    pub thumb: Option<egui::TextureId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayerRowEvent {
    None,
    Select,
    ToggleVisible,
    ToggleLock,
    Rename,
}

/// One row of the layer palette (S07).
/// Keyboard: the row is focusable; Space/Enter selects; the eye and lock are their own tab stops;
/// F2 asks to rename. Accessible name: "<name>, <kind>, hidden?, locked?, clipped?, <blend> <opacity>%".
pub fn layer_row(ui: &mut Ui, s: &LayerRowState) -> LayerRowEvent {
    let p = palette(ui);
    let mut ev = LayerRowEvent::None;
    let h = tokens::layout::LAYER_ROW_H;
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(w, h), Sense::hover());

    // Eye + lock column.
    let eye_rect = Rect::from_min_size(rect.min + vec2(2.0, (h - 24.0) / 2.0), vec2(24.0, 24.0));
    let lock_rect = eye_rect.translate(vec2(24.0, 0.0));
    let eye = ui.put(eye_rect, IconButton::new(if s.visible { icons::EYE } else { icons::EYE_SLASH }, crate::tr(if s.visible { "Hide layer" } else { "Show layer" })).size(24.0));
    let lock = ui.put(lock_rect, IconButton::new(if s.locked { icons::LOCK } else { icons::LOCK_OPEN }, crate::tr(if s.locked { "Unlock layer" } else { "Lock layer" })).size(24.0));
    if eye.clicked() {
        ev = LayerRowEvent::ToggleVisible;
    }
    if lock.clicked() {
        ev = LayerRowEvent::ToggleLock;
    }

    let body = Rect::from_min_max(pos2(lock_rect.right() + 2.0, rect.top()), rect.max);
    let resp = ui.interact(body, ui.id().with(("layer-row", &s.name)), Sense::click());
    let mut desc = format!("{}, {}", s.name, s.kind.name());
    if !s.visible {
        desc.push_str(", hidden");
    }
    if s.locked {
        desc.push_str(", locked");
    }
    if s.clipped {
        desc.push_str(", clipped to layer below");
    }
    desc.push_str(&format!(", {} {}%", s.blend, (s.opacity as u32 * 100 + 127) / 255));
    resp.widget_info(|| WidgetInfo::selected(WidgetType::SelectableLabel, ui.is_enabled(), s.selected, &desc));
    if resp.clicked() {
        ev = LayerRowEvent::Select;
    }
    if resp.double_clicked() || (resp.has_focus() && ui.input(|i| i.key_pressed(egui::Key::F2))) {
        ev = LayerRowEvent::Rename;
    }

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let fill = if s.selected { p.accent_subtle } else if resp.hovered() { p.surface_hover } else { p.surface };
        painter.rect_filled(body, CornerRadius::ZERO, fill);
        painter.line_segment([rect.left_bottom(), rect.right_bottom()], Stroke::new(1.0, p.border));

        let mut x = body.left() + 4.0 + s.depth as f32 * tokens::space::S5;
        if s.clipped {
            // Clipping marker: a vertical bar in the indicator color left of the thumbnail.
            painter.rect_filled(Rect::from_min_size(pos2(x, body.top() + 6.0), vec2(3.0, h - 12.0)), CornerRadius::same(1), p.accent_indicator);
            x += 6.0;
        }
        let t = tokens::layout::LAYER_THUMB;
        let thumb = Rect::from_min_size(pos2(x, body.center().y - t / 2.0), vec2(t, t));
        match s.thumb {
            Some(tex) => {
                painter.image(tex, thumb, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
            }
            None => {
                painter.rect_filled(thumb, CornerRadius::same(2), Color32::WHITE);
                painter.text(thumb.center(), Align2::CENTER_CENTER, s.kind.icon(), crate::theme::icon_font(tokens::icon::MD), Color32::from_gray(90));
            }
        }
        painter.rect_stroke(thumb, CornerRadius::same(2), Stroke::new(1.0, p.border_strong), StrokeKind::Inside);
        x = thumb.right() + tokens::space::S2;
        if s.has_mask {
            let m = thumb.translate(vec2(t + 2.0, 0.0));
            painter.rect_filled(m, CornerRadius::same(2), Color32::from_gray(200));
            painter.rect_stroke(m, CornerRadius::same(2), Stroke::new(1.0, p.border_strong), StrokeKind::Inside);
            x = m.right() + tokens::space::S2;
        }
        let x_text = x + tokens::space::S2;
        let title_y = body.top() + 12.0;
        painter.text(pos2(x_text, title_y), Align2::LEFT_CENTER, &s.name, text::body_strong().resolve(ui.style()), if s.visible { p.text } else { p.text_muted });
        let mut meta = format!("{} · {}%", s.blend, (s.opacity as u32 * 100 + 127) / 255);
        if s.reference {
            meta.push_str(" · ref");
        }
        if s.draft {
            meta.push_str(" · draft");
        }
        painter.text(pos2(x_text, title_y + 15.0), Align2::LEFT_CENTER, meta, text::caption().resolve(ui.style()), p.text_muted);
        painter.text(pos2(body.right() - 12.0, body.center().y), Align2::CENTER_CENTER, s.kind.icon(), crate::theme::icon_font(tokens::icon::SM), p.text_muted);
        focus_ring(ui, &resp, body.shrink(1.0), 0.0);
    }
    ev
}
