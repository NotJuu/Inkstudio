use super::IconButton;
use crate::icons;
use crate::theme::{palette, text, tokens};
use egui::{DragValue, Response, RichText, Slider, Ui};

/// Labelled slider with a numeric field and an optional pen-pressure toggle.
/// Used for brush size, opacity, density, stabilization (S05), tone settings (S14), filters (S16).
///
/// Keyboard: Tab focuses the slider; arrow keys step; the numeric field accepts typing.
/// `pressure` is `Some(&mut bool)` when the parameter can follow pen pressure.
pub fn slider_field(
    ui: &mut Ui,
    label: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    suffix: &str,
    pressure: Option<&mut bool>,
) -> Response {
    let p = palette(ui);
    ui.horizontal(|ui| {
        ui.set_min_height(tokens::control_h::SM);
        ui.add_sized([72.0, tokens::control_h::SM], egui::Label::new(RichText::new(label).color(p.text_muted)).truncate());
        let slider = ui.add(
            Slider::new(value, range.clone())
                .show_value(false)
                .clamping(egui::SliderClamping::Always)
                .text(""),
        );
        let slider = slider.on_hover_text(label);
        let field = ui.add(
            DragValue::new(value)
                .range(range)
                .suffix(suffix)
                .speed(0.5)
                .max_decimals(1),
        );
        if let Some(on) = pressure {
            let tip = if *on { "Pen pressure on" } else { "Pen pressure off" };
            if ui.add(IconButton::new(icons::PEN_NIB, tip).selected(*on).size(tokens::control_h::SM)).clicked() {
                *on = !*on;
            }
        }
        slider | field
    })
    .inner
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    Px,
    Mm,
    In,
    Pt,
}

impl Unit {
    pub fn suffix(self) -> &'static str {
        match self {
            Unit::Px => " px",
            Unit::Mm => " mm",
            Unit::In => " in",
            Unit::Pt => " pt",
        }
    }
}

/// Numeric field with a unit suffix, for canvas and page sizes (S02, S20, S24).
/// The value is always stored in micrometres; display converts with `dpi` so
/// switching units never accumulates rounding error.
pub fn unit_field(ui: &mut Ui, label: &str, value_um: &mut i64, unit: Unit, dpi: f32, error: Option<&str>) -> Response {
    let p = palette(ui);
    let per_unit_um = match unit {
        Unit::Px => 25_400.0 / dpi as f64,
        Unit::Mm => 1000.0,
        Unit::In => 25_400.0,
        Unit::Pt => 25_400.0 / 72.0,
    };
    let mut shown = *value_um as f64 / per_unit_um;
    let resp = ui
        .vertical(|ui| {
            ui.label(RichText::new(label).text_style(text::caption()).color(p.text_muted));
            let r = ui.add(
                DragValue::new(&mut shown)
                    .suffix(unit.suffix())
                    .range(0.0..=f64::MAX)
                    .max_decimals(if unit == Unit::Px { 0 } else { 2 }),
            );
            if let Some(msg) = error {
                ui.label(RichText::new(msg).text_style(text::caption()).color(p.danger));
            }
            r
        })
        .inner;
    if resp.changed() {
        *value_um = (shown * per_unit_um).round() as i64;
    }
    resp
}
