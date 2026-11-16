use super::{Button, ButtonVariant, IconButton};
use crate::icons;
use crate::theme::{palette, text, tokens};
use egui::{Frame, Margin, RichText, Stroke, Ui};

/// Header bar of a dockable palette (S04–S09, S21, S22, S25).
/// Docking, tabbing and floating are provided by egui_dock in the app shell;
/// this draws the title and the palette's menu button. Returns true if the menu button was clicked.
pub fn palette_header(ui: &mut Ui, title: &str) -> bool {
    let p = palette(ui);
    let mut menu = false;
    Frame::new()
        .fill(p.bg)
        .stroke(Stroke::new(tokens::stroke::HAIRLINE, p.border))
        .inner_margin(Margin::symmetric(tokens::space::S4 as i8, 0))
        .show(ui, |ui| {
            ui.set_height(tokens::layout::PALETTE_TAB_H);
            ui.horizontal_centered(|ui| {
                ui.label(RichText::new(title).text_style(text::body_strong()).color(p.text));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    menu = ui.add(IconButton::new(icons::DOTS_THREE, format!("{title} menu")).size(20.0)).clicked();
                });
            });
        });
    menu
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialogResult {
    Open,
    Ok,
    Cancel,
}

/// Modal dialog body: title, content, OK / Cancel footer (S02, S14, S16, S17, S20, S23, S24).
/// The app shell draws the scrim and centres it; this is the panel itself, so it can also be
/// rendered inline in the gallery.
/// Keyboard: Enter = OK (when `ok_enabled`), Escape = Cancel; focus starts on the first field.
pub fn dialog_frame(
    ui: &mut Ui,
    title: &str,
    ok_label: &str,
    ok_enabled: bool,
    width: f32,
    body: impl FnOnce(&mut Ui),
) -> DialogResult {
    let p = palette(ui);
    let mut result = DialogResult::Open;
    Frame::new()
        .fill(p.surface_raised)
        .stroke(Stroke::new(tokens::stroke::HAIRLINE, p.border_strong))
        .corner_radius(tokens::radius::LG as u8)
        .shadow(ui.visuals().window_shadow)
        .inner_margin(Margin::same(tokens::space::S6 as i8))
        .show(ui, |ui| {
            ui.set_width(width);
            ui.label(RichText::new(title).text_style(text::title()));
            ui.add_space(tokens::space::S5);
            body(ui);
            ui.add_space(tokens::space::S6);
            let footer = egui::vec2(ui.available_width(), tokens::control_h::MD);
            ui.allocate_ui_with_layout(footer, egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = tokens::space::S4;
                let ok = ui.add_enabled(ok_enabled, Button::new(ok_label).variant(ButtonVariant::Primary).min_width(80.0));
                let cancel = ui.add(Button::new("Cancel").min_width(80.0));
                if ok.clicked() {
                    result = DialogResult::Ok;
                }
                if cancel.clicked() {
                    result = DialogResult::Cancel;
                }
            });
        });
    let (enter, esc) = ui.input(|i| (i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::Escape)));
    if esc {
        result = DialogResult::Cancel;
    } else if enter && ok_enabled && result == DialogResult::Open {
        result = DialogResult::Ok;
    }
    result
}
