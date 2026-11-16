use super::focus_ring;
use crate::theme::{palette, text, tokens};
use egui::{Align2, Color32, CornerRadius, Sense, Stroke, Ui, WidgetInfo, WidgetType, pos2, vec2};

/// Tab strip for grouped palettes (Color wheel | Sliders | Sets) and dialog sections.
/// Keyboard: each tab is a tab stop; Left/Right moves the selection while a tab is focused.
/// Returns true when the selection changed.
pub fn tab_strip(ui: &mut Ui, tabs: &[&str], selected: &mut usize) -> bool {
    let p = palette(ui);
    let mut changed = false;
    let h = tokens::layout::PALETTE_TAB_H;
    let font = text::body_strong().resolve(ui.style());
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for (i, name) in tabs.iter().enumerate() {
            let galley = ui.painter().layout_no_wrap(name.to_string(), font.clone(), Color32::PLACEHOLDER);
            let w = galley.size().x + 2.0 * tokens::pad_x::LG;
            let (rect, resp) = ui.allocate_exact_size(vec2(w, h), Sense::click());
            let on = *selected == i;
            resp.widget_info(|| WidgetInfo::selected(WidgetType::SelectableLabel, ui.is_enabled(), on, *name));
            if resp.clicked() && !on {
                *selected = i;
                changed = true;
            }
            if resp.has_focus() {
                let (l, r) = ui.input(|inp| (inp.key_pressed(egui::Key::ArrowLeft), inp.key_pressed(egui::Key::ArrowRight)));
                if l && *selected > 0 {
                    *selected -= 1;
                    changed = true;
                }
                if r && *selected + 1 < tabs.len() {
                    *selected += 1;
                    changed = true;
                }
            }
            if ui.is_rect_visible(rect) {
                if resp.hovered() && !on {
                    ui.painter().rect_filled(rect, CornerRadius::same(tokens::radius::SM as u8), p.surface_hover);
                }
                let fg = if on { p.text } else { p.text_muted };
                ui.painter().text(pos2(rect.center().x, rect.center().y), Align2::CENTER_CENTER, *name, font.clone(), fg);
                if on {
                    ui.painter().line_segment(
                        [pos2(rect.left() + 4.0, rect.bottom() - 1.0), pos2(rect.right() - 4.0, rect.bottom() - 1.0)],
                        Stroke::new(2.0, p.accent_indicator),
                    );
                }
                focus_ring(ui, &resp, rect, tokens::radius::SM);
            }
        }
    });
    changed
}
