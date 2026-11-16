//! The design-system check page: every primitive, in every state that can be shown statically.
//! Rendered headless by `examples/design_gallery.rs` into `replica/design/gallery-*.png`.

use crate::icons;
use crate::theme::{palette, text, tokens};
use crate::widgets::*;
use egui::{Color32, RichText, Ui};

pub struct GalleryState {
    pub tool: usize,
    pub size: f32,
    pub opacity: f32,
    pub size_pressure: bool,
    pub opacity_pressure: bool,
    pub tab: usize,
    pub width_um: i64,
    pub bleed_um: i64,
    pub blend: usize,
    pub check: bool,
}

impl Default for GalleryState {
    fn default() -> Self {
        Self {
            tool: 1,
            size: 12.0,
            opacity: 100.0,
            size_pressure: true,
            opacity_pressure: false,
            tab: 0,
            width_um: 182_000,
            bleed_um: 3_000,
            blend: 0,
            check: true,
        }
    }
}

fn section(ui: &mut Ui, title: &str, body: impl FnOnce(&mut Ui)) {
    let p = palette(ui);
    ui.add_space(tokens::space::S5);
    ui.label(
        RichText::new(title)
            .text_style(text::label())
            .color(p.text_muted),
    );
    ui.add_space(tokens::space::S2);
    body(ui);
}

fn layer(name: &str, kind: LayerKind) -> LayerRowState {
    LayerRowState {
        name: name.into(),
        kind,
        depth: 0,
        visible: true,
        locked: false,
        clipped: false,
        has_mask: false,
        reference: false,
        draft: false,
        selected: false,
        opacity: 255,
        blend: "Normal",
        thumb: None,
    }
}

pub fn show(ui: &mut Ui, s: &mut GalleryState) {
    let p = palette(ui);
    ui.label(RichText::new("Inkstudio design system").text_style(text::heading()));
    ui.label(
        RichText::new("Every primitive and state. Colors and sizes come from tokens.json.")
            .color(p.text_muted),
    );

    ui.columns(3, |cols| {
        // ---- Column 1: buttons, tools, fields --------------------------------------------
        let ui = &mut cols[0];
        section(ui, "Buttons · variants", |ui| {
            ui.horizontal(|ui| {
                ui.add(Button::primary("Export"));
                ui.add(Button::new("Cancel"));
                ui.add(Button::new("Reset").variant(ButtonVariant::Ghost));
                ui.add(Button::new("Delete page").variant(ButtonVariant::Danger));
            });
        });
        section(ui, "Buttons · sizes, icon, loading, disabled", |ui| {
            ui.horizontal(|ui| {
                ui.add(Button::new("Small").size(ButtonSize::Sm));
                ui.add(Button::new("Medium"));
                ui.add(Button::primary("Large").size(ButtonSize::Lg));
            });
            ui.horizontal(|ui| {
                ui.add(Button::new("New page").icon(icons::PLUS));
                ui.add(Button::primary("Exporting").loading(true).min_width(96.0));
                ui.add_enabled(false, Button::primary("Disabled"));
                ui.add_enabled(false, Button::new("Disabled"));
            });
        });
        section(ui, "Tool column · icon toggles (focus ring on Pen)", |ui| {
            ui.horizontal(|ui| {
                let tools = [
                    (icons::HAND, "Hand"),
                    (icons::PEN_NIB, "Pen"),
                    (icons::PENCIL_SIMPLE, "Pencil"),
                    (icons::PAINT_BRUSH, "Brush"),
                    (icons::ERASER, "Eraser"),
                    (icons::PAINT_BUCKET, "Fill"),
                    (icons::SELECTION, "Select"),
                    (icons::SQUARES_FOUR, "Frame border"),
                    (icons::CHAT_CIRCLE, "Balloon"),
                    (icons::RULER, "Ruler"),
                ];
                for (i, (icon, name)) in tools.iter().enumerate() {
                    let r = ui.add(IconButton::new(icon, *name).selected(s.tool == i));
                    if i == 1 {
                        r.request_focus();
                    }
                    if r.clicked() {
                        s.tool = i;
                    }
                }
            });
        });
        section(ui, "Slider fields · pressure toggles", |ui| {
            slider_field(
                ui,
                "Brush size",
                &mut s.size,
                0.5..=500.0,
                " px",
                Some(&mut s.size_pressure),
            );
            slider_field(
                ui,
                "Opacity",
                &mut s.opacity,
                0.0..=100.0,
                " %",
                Some(&mut s.opacity_pressure),
            );
            ui.add_enabled_ui(false, |ui| {
                let mut v = 40.0;
                slider_field(ui, "Disabled", &mut v, 0.0..=100.0, " %", None);
            });
        });
        section(ui, "Unit fields · dropdown · checkbox", |ui| {
            ui.horizontal(|ui| {
                unit_field(ui, "Trim width", &mut s.width_um, Unit::Mm, 600.0, None);
                unit_field(ui, "Bleed", &mut s.bleed_um, Unit::Mm, 600.0, None);
                let mut bad = 0i64;
                unit_field(
                    ui,
                    "Height",
                    &mut bad,
                    Unit::Mm,
                    600.0,
                    Some("Must be more than 0"),
                );
            });
            ui.horizontal(|ui| {
                let modes = ["Normal", "Multiply", "Screen", "Overlay"];
                egui::ComboBox::from_id_salt("blend")
                    .selected_text(modes[s.blend])
                    .show_ui(ui, |ui| {
                        for (i, m) in modes.iter().enumerate() {
                            ui.selectable_value(&mut s.blend, i, *m);
                        }
                    });
                ui.checkbox(&mut s.check, "Refer to other layers");
            });
            let mut t = String::from("Chapter 3");
            ui.add_sized(
                [200.0, tokens::control_h::SM],
                egui::TextEdit::singleline(&mut t),
            );
        });

        // ---- Column 2: palettes ---------------------------------------------------------
        let ui = &mut cols[1];
        section(ui, "Palette header · tabs", |ui| {
            palette_header(ui, "Color");
            tab_strip(ui, &["Wheel", "Sliders", "Sets", "History"], &mut s.tab);
        });
        section(ui, "Swatches · main/sub pair", |ui| {
            ui.horizontal(|ui| {
                color_pair(ui, Color32::from_rgb(24, 24, 28), Color32::WHITE);
                ui.add_space(8.0);
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        for (i, c) in [
                            Color32::BLACK,
                            Color32::WHITE,
                            Color32::from_rgb(230, 76, 60),
                            Color32::from_rgb(241, 196, 15),
                            Color32::from_rgb(46, 204, 113),
                            Color32::from_rgb(52, 152, 219),
                            Color32::from_rgb(155, 89, 182),
                            Color32::TRANSPARENT,
                        ]
                        .into_iter()
                        .enumerate()
                        {
                            swatch(ui, c, 20.0, i == 0);
                        }
                    });
                });
            });
        });
        section(ui, "Layer rows · states", |ui| {
            palette_header(ui, "Layer");
            let mut a = layer("Balloons", LayerKind::Balloon);
            a.depth = 0;
            layer_row(ui, &a);
            let mut f = layer("Panel 2", LayerKind::Frame);
            f.has_mask = true;
            layer_row(ui, &f);
            let mut ink = layer("Ink", LayerKind::Vector);
            ink.selected = true;
            ink.depth = 1;
            layer_row(ui, &ink);
            let mut tone = layer("Tone 60 lpi 20%", LayerKind::Tone);
            tone.clipped = true;
            tone.depth = 1;
            tone.blend = "Multiply";
            layer_row(ui, &tone);
            let mut sk = layer("Sketch", LayerKind::Raster);
            sk.visible = false;
            sk.locked = true;
            sk.draft = true;
            sk.opacity = 102;
            sk.depth = 1;
            layer_row(ui, &sk);
            let mut paper = layer("Paper", LayerKind::Paper);
            paper.locked = true;
            paper.reference = true;
            layer_row(ui, &paper);
        });
        section(ui, "List rows · sub tools", |ui| {
            list_row(ui, Some(icons::PEN_NIB), "G-pen", true);
            list_row(ui, Some(icons::PEN_NIB), "Mapping pen", false);
            list_row(ui, Some(icons::PENCIL_SIMPLE), "Rough pencil", false);
        });

        // ---- Column 3: dialogs, feedback, timeline, overlays ----------------------------
        let ui = &mut cols[2];
        section(ui, "Dialog", |ui| {
            dialog_frame(ui, "New tone", "Create", true, 300.0, |ui| {
                let mut lpi = 60.0;
                let mut density = 20.0;
                slider_field(ui, "Lines/inch", &mut lpi, 5.0..=300.0, "", None);
                slider_field(ui, "Density", &mut density, 0.0..=100.0, " %", None);
            });
        });
        section(ui, "Progress · toasts", |ui| {
            progress_bar(ui, "Exporting page 7 of 24", Some(0.29), true);
            ui.add_space(6.0);
            toast(ui, ToastKind::Success, "Saved chapter-03.inks", None);
            ui.add_space(4.0);
            toast(
                ui,
                ToastKind::Error,
                "Couldn't export: disk is full",
                Some("Retry"),
            );
            ui.add_space(4.0);
            toast(
                ui,
                ToastKind::Warning,
                "Font \"Noto Serif JP\" is missing",
                Some("Replace"),
            );
        });
        section(ui, "Timeline track", |ui| {
            use CelCell::*;
            let cells = [
                Start(1),
                Hold,
                Hold,
                Start(2),
                Hold,
                Start(3),
                Hold,
                Hold,
                Hold,
                Blank,
                Start(4),
                Hold,
            ];
            timeline_track(ui, "Character", &cells, 4, true);
            timeline_track(
                ui,
                "Background",
                &[
                    Start(1),
                    Hold,
                    Hold,
                    Hold,
                    Hold,
                    Hold,
                    Hold,
                    Hold,
                    Hold,
                    Hold,
                    Hold,
                    Hold,
                ],
                4,
                false,
            );
        });
        section(ui, "Canvas overlays on black / white art", |ui| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(300.0, 90.0), egui::Sense::hover());
            let half = rect.width() / 2.0;
            let left = egui::Rect::from_min_size(rect.min, egui::vec2(half, rect.height()));
            let right = egui::Rect::from_min_size(
                rect.min + egui::vec2(half, 0.0),
                egui::vec2(half, rect.height()),
            );
            ui.painter().rect_filled(left, 0, Color32::BLACK);
            ui.painter().rect_filled(right, 0, Color32::WHITE);
            let a = rect.left_center() + egui::vec2(16.0, 20.0);
            let b = rect.right_center() + egui::vec2(-16.0, -20.0);
            dual_stroke_line(ui.painter(), a, b, p.overlay_ruler);
            dual_stroke_line(
                ui.painter(),
                rect.left_top() + egui::vec2(16.0, 12.0),
                rect.right_top() + egui::vec2(-16.0, 12.0),
                p.overlay_frame_preview,
            );
            handle(ui, a, false);
            handle(ui, b, true);
            handle(ui, rect.center(), false);
        });
    });
}
