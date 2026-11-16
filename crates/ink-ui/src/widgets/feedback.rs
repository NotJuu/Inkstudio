use super::{Button, ButtonSize, ButtonVariant};
use crate::icons;
use crate::theme::{palette, text, tokens};
use egui::{Color32, CornerRadius, Frame, Margin, ProgressBar, Response, RichText, Stroke, Ui};

/// Determinate or indeterminate progress, with optional Cancel (S16 filters, S24 export).
/// Returns true if Cancel was clicked. Screen readers get "<label>, NN%".
pub fn progress_bar(ui: &mut Ui, label: &str, fraction: Option<f32>, cancelable: bool) -> bool {
    let p = palette(ui);
    let mut cancel = false;
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(label).color(p.text_muted));
            if let Some(f) = fraction {
                ui.label(RichText::new(format!("{:.0}%", f * 100.0)).color(p.text_muted));
            }
        });
        ui.horizontal(|ui| {
            let bar = ProgressBar::new(fraction.unwrap_or(0.0))
                .fill(p.accent)
                .desired_height(6.0)
                .desired_width(ui.available_width() - if cancelable { 72.0 } else { 0.0 })
                .animate(fraction.is_none());
            ui.add(bar);
            if cancelable && ui.add(Button::new("Cancel").size(ButtonSize::Sm)).clicked() {
                cancel = true;
            }
        });
    });
    cancel
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Success,
    Warning,
    Error,
}

/// Non-blocking status message (S03). Shown bottom-centre by the shell, 4 s auto-dismiss
/// (errors stay until dismissed). Announced to screen readers as a live region by the shell.
/// The kind is shown with an icon as well as color.
pub fn toast(ui: &mut Ui, kind: ToastKind, message: &str, action: Option<&str>) -> Option<Response> {
    let p = palette(ui);
    let (icon, color) = match kind {
        ToastKind::Info => (icons::INFO, p.accent_text),
        ToastKind::Success => (icons::CHECK_CIRCLE, p.success),
        ToastKind::Warning => (icons::WARNING, p.warning),
        ToastKind::Error => (icons::WARNING_OCTAGON, p.danger),
    };
    let mut out = None;
    Frame::new()
        .fill(p.surface_raised)
        .stroke(Stroke::new(1.0, p.border_strong))
        .corner_radius(CornerRadius::same(tokens::radius::LG as u8))
        .shadow(ui.visuals().popup_shadow)
        .inner_margin(Margin::symmetric(tokens::space::S5 as i8, tokens::space::S4 as i8))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(icon).font(crate::theme::icon_font(tokens::icon::MD)).color(color));
                ui.label(RichText::new(message).text_style(text::body()).color(p.text));
                if let Some(a) = action {
                    out = Some(ui.add(Button::new(a).variant(ButtonVariant::Ghost).size(ButtonSize::Sm)));
                }
            });
        });
    let _ = Color32::TRANSPARENT;
    out
}
