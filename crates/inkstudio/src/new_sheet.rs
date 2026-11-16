//! "New sheet" dialog (S02): kind, paper format, resolution, bleed and safe area, color mode,
//! pages and binding, animation settings, panel template — with a live preview.

use crate::book::{Binding, ColorMode, SheetKind, SheetSettings};
use egui::{Color32, Rect, RichText, Stroke, Ui, pos2, vec2};
use ink_comic::{PanelSet, PanelTemplate};
use ink_core::Rgba;
use ink_ui::theme::{palette, text, tokens};
use ink_ui::widgets::{Button, ButtonVariant};
use ink_vector::Pt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    Mm,
    Px,
    In,
}

impl Unit {
    fn name(self) -> &'static str {
        match self {
            Unit::Mm => "mm",
            Unit::Px => "px",
            Unit::In => "in",
        }
    }
}

/// Paper formats: (name, width, height, unit). Pixel formats carry their own DPI.
pub const FORMATS: &[(&str, f32, f32, Unit)] = &[
    ("B5 (JIS) – manga magazine", 182.0, 257.0, Unit::Mm),
    ("B4 (JIS) – manga manuscript", 257.0, 364.0, Unit::Mm),
    ("A4", 210.0, 297.0, Unit::Mm),
    ("A5 – tankobon / doujinshi", 148.0, 210.0, Unit::Mm),
    ("B6 – small paperback", 128.0, 182.0, Unit::Mm),
    ("US comic (6.625 × 10.25 in)", 168.3, 260.4, Unit::Mm),
    ("US Letter", 215.9, 279.4, Unit::Mm),
    ("Square 210 mm", 210.0, 210.0, Unit::Mm),
    ("Webtoon strip 800 × 12800 px", 800.0, 12800.0, Unit::Px),
    ("Full HD 1920 × 1080 px", 1920.0, 1080.0, Unit::Px),
    ("HD 1280 × 720 px", 1280.0, 720.0, Unit::Px),
    ("Social post 1080 × 1350 px", 1080.0, 1350.0, Unit::Px),
    ("Custom", 0.0, 0.0, Unit::Mm),
];

pub const DPIS: &[u32] = &[72, 96, 150, 300, 350, 600, 1200];

pub struct NewSheet {
    pub s: SheetSettings,
    pub format: usize,
    pub unit: Unit,
    pub landscape: bool,
    pub cancelable: bool,
}

pub enum Outcome {
    Open,
    Create(SheetSettings),
    Cancel,
}

impl NewSheet {
    pub fn new(cancelable: bool) -> Self {
        let mut d = Self { s: SheetSettings::default(), format: 0, unit: Unit::Mm, landscape: false, cancelable };
        d.set_kind(SheetKind::MangaPage);
        d
    }

    /// Kind-specific defaults (the user can change all of them afterwards).
    pub fn set_kind(&mut self, k: SheetKind) {
        self.s.kind = k;
        let (fmt, dpi, mode, template, unit) = match k {
            SheetKind::Illustration => (2, 350, ColorMode::Color, None, Unit::Mm),
            SheetKind::MangaPage => (0, 600, ColorMode::Mono, Some(PanelTemplate::Manga5), Unit::Mm),
            SheetKind::Book => (0, 600, ColorMode::Mono, Some(PanelTemplate::Manga5), Unit::Mm),
            SheetKind::Webtoon => (8, 72, ColorMode::Color, None, Unit::Px),
            SheetKind::Animation => (10, 96, ColorMode::Color, None, Unit::Px),
        };
        self.s.dpi = dpi;
        self.s.color_mode = mode;
        self.s.template = template;
        self.unit = unit;
        if k == SheetKind::Book && self.s.pages < 2 {
            self.s.pages = 8;
        }
        self.set_format(fmt);
    }

    fn set_format(&mut self, i: usize) {
        self.format = i;
        let (_, w, h, u) = FORMATS[i];
        if w > 0.0 {
            let (w, h) = if u == Unit::Px { (w * 25.4 / self.s.dpi as f32, h * 25.4 / self.s.dpi as f32) } else { (w, h) };
            self.s.trim_mm = if self.landscape { (h.max(w), h.min(w)) } else { (w, h) };
            if u == Unit::Px {
                self.s.trim_mm = (w, h);
            }
        }
    }

    fn to_unit(&self, mm: f32) -> f32 {
        match self.unit {
            Unit::Mm => mm,
            Unit::In => mm / 25.4,
            Unit::Px => (mm * self.s.dpi as f32 / 25.4).round(),
        }
    }
    fn from_unit(&self, v: f32) -> f32 {
        match self.unit {
            Unit::Mm => v,
            Unit::In => v * 25.4,
            Unit::Px => v * 25.4 / self.s.dpi as f32,
        }
    }

    pub fn show(&mut self, ui: &mut Ui) -> Outcome {
        let p = palette(ui);
        let mut out = Outcome::Open;
        ui.label(RichText::new("New sheet").text_style(text::title()));
        ui.add_space(tokens::space::S4);
        // ---- kind -----------------------------------------------------------------------------
        ui.horizontal(|ui| {
            for k in SheetKind::ALL {
                if ui.selectable_label(self.s.kind == k, k.name()).clicked() {
                    self.set_kind(k);
                }
            }
        });
        ui.add_space(tokens::space::S5);
        ui.horizontal_top(|ui| {
            // ---- settings column ------------------------------------------------------------
            ui.vertical(|ui| {
                ui.set_width(380.0);
                egui::Grid::new("sheet-grid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                    ui.label("Name");
                    ui.add(egui::TextEdit::singleline(&mut self.s.name).desired_width(220.0));
                    ui.end_row();

                    ui.label("Format");
                    let cur = FORMATS[self.format].0;
                    let mut pick = None;
                    egui::ComboBox::from_id_salt("format").width(220.0).selected_text(cur).show_ui(ui, |ui| {
                        for (i, f) in FORMATS.iter().enumerate() {
                            if ui.selectable_label(self.format == i, f.0).clicked() {
                                pick = Some(i);
                            }
                        }
                    });
                    if let Some(i) = pick {
                        self.set_format(i);
                    }
                    ui.end_row();

                    ui.label("Size");
                    ui.horizontal(|ui| {
                        let mut w = self.to_unit(self.s.trim_mm.0);
                        let mut h = self.to_unit(self.s.trim_mm.1);
                        let dec = if self.unit == Unit::Px { 0 } else { 1 };
                        let cw = ui.add(egui::DragValue::new(&mut w).range(1.0..=f32::MAX).max_decimals(dec).speed(1.0));
                        ui.label("×");
                        let ch = ui.add(egui::DragValue::new(&mut h).range(1.0..=f32::MAX).max_decimals(dec).speed(1.0));
                        if cw.changed() || ch.changed() {
                            self.s.trim_mm = (self.from_unit(w), self.from_unit(h));
                            self.format = FORMATS.len() - 1;
                        }
                        egui::ComboBox::from_id_salt("unit").width(56.0).selected_text(self.unit.name()).show_ui(ui, |ui| {
                            for u in [Unit::Mm, Unit::Px, Unit::In] {
                                ui.selectable_value(&mut self.unit, u, u.name());
                            }
                        });
                    });
                    ui.end_row();

                    ui.label("Orientation");
                    ui.horizontal(|ui| {
                        let (w, h) = self.s.trim_mm;
                        if ui.selectable_label(w <= h, "Portrait").clicked() && w > h {
                            self.s.trim_mm = (h, w);
                        }
                        if ui.selectable_label(w > h, "Landscape").clicked() && w <= h {
                            self.s.trim_mm = (h, w);
                        }
                        self.landscape = self.s.trim_mm.0 > self.s.trim_mm.1;
                    });
                    ui.end_row();

                    ui.label("Resolution");
                    let px_before = (self.to_unit(self.s.trim_mm.0), self.to_unit(self.s.trim_mm.1));
                    let dpi_before = self.s.dpi;
                    ui.horizontal(|ui| {
                        egui::ComboBox::from_id_salt("dpi").width(90.0).selected_text(format!("{} dpi", self.s.dpi)).show_ui(ui, |ui| {
                            for d in DPIS {
                                ui.selectable_value(&mut self.s.dpi, *d, format!("{d} dpi"));
                            }
                        });
                        ui.add(egui::DragValue::new(&mut self.s.dpi).range(36..=2400).suffix(" dpi"));
                    });
                    if self.unit == Unit::Px && self.s.dpi != dpi_before {
                        // Pixel-based sheets keep their pixel size; the physical size follows the DPI.
                        self.s.trim_mm = (self.from_unit(px_before.0), self.from_unit(px_before.1));
                    }
                    ui.end_row();

                    if self.s.kind.has_print_guides() {
                        ui.label("Bleed");
                        ui.add(egui::DragValue::new(&mut self.s.bleed_mm).range(0.0..=20.0).suffix(" mm").speed(0.1));
                        ui.end_row();
                        ui.label("Safe area margin");
                        ui.add(egui::DragValue::new(&mut self.s.safe_mm).range(0.0..=60.0).suffix(" mm").speed(0.2));
                        ui.end_row();
                    }

                    ui.label("Color mode");
                    egui::ComboBox::from_id_salt("mode").width(220.0).selected_text(self.s.color_mode.name()).show_ui(ui, |ui| {
                        for m in [ColorMode::Color, ColorMode::Gray, ColorMode::Mono] {
                            ui.selectable_value(&mut self.s.color_mode, m, m.name());
                        }
                    });
                    ui.end_row();

                    ui.label("Paper");
                    ui.horizontal(|ui| {
                        if ui.selectable_label(self.s.paper.a() == 255, "White").clicked() {
                            self.s.paper = Rgba::WHITE;
                        }
                        if ui.selectable_label(self.s.paper.a() == 0, "Transparent").clicked() {
                            self.s.paper = Rgba::TRANSPARENT;
                        }
                    });
                    ui.end_row();

                    if self.s.kind == SheetKind::Book {
                        ui.label("Pages");
                        ui.add(egui::DragValue::new(&mut self.s.pages).range(1..=500));
                        ui.end_row();
                        ui.label("Binding");
                        ui.horizontal(|ui| {
                            ui.selectable_value(&mut self.s.binding, Binding::Right, "Right (manga, read ←)");
                            ui.selectable_value(&mut self.s.binding, Binding::Left, "Left (read →)");
                        });
                        ui.end_row();
                    }

                    if self.s.kind.has_print_guides() {
                        ui.label("Panels");
                        let name = self.s.template.map_or("None", |t| t.name());
                        egui::ComboBox::from_id_salt("tpl").width(220.0).selected_text(name).show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.s.template, None, "None");
                            for t in PanelTemplate::ALL {
                                ui.selectable_value(&mut self.s.template, Some(t), t.name());
                            }
                        });
                        ui.end_row();
                    }

                    if self.s.kind == SheetKind::Animation {
                        ui.label("Frame rate");
                        ui.horizontal(|ui| {
                            for f in [8.0, 12.0, 24.0, 30.0] {
                                ui.selectable_value(&mut self.s.fps, f, format!("{f}"));
                            }
                            ui.add(egui::DragValue::new(&mut self.s.fps).range(1.0..=60.0).suffix(" fps"));
                        });
                        ui.end_row();
                        ui.label("Length");
                        ui.horizontal(|ui| {
                            ui.add(egui::DragValue::new(&mut self.s.frames).range(1..=9999).suffix(" frames"));
                            ui.label(RichText::new(format!("= {:.2} s", self.s.frames as f32 / self.s.fps)).color(p.text_muted));
                        });
                        ui.end_row();
                    }
                });
            });
            ui.add_space(tokens::space::S6);
            // ---- preview ----------------------------------------------------------------------
            ui.vertical(|ui| {
                self.preview(ui);
                let (w, h) = self.s.canvas_px();
                let mp = self.s.megapixels();
                let pages = if self.s.kind == SheetKind::Book { self.s.pages } else { 1 };
                ui.label(RichText::new(format!("{w} × {h} px · {mp:.1} MP")).color(p.text_muted));
                ui.label(RichText::new(format!("{:.1} × {:.1} mm finished · {} dpi", self.s.trim_mm.0, self.s.trim_mm.1, self.s.dpi)).color(p.text_muted));
                if pages > 1 {
                    ui.label(RichText::new(format!("{pages} pages")).color(p.text_muted));
                }
                if mp > 120.0 {
                    ui.label(RichText::new("Very large canvas: drawing stays fast, but exports take longer.").color(p.warning));
                }
            });
        });
        ui.add_space(tokens::space::S6);
        let valid = self.s.trim_mm.0 > 0.0 && self.s.trim_mm.1 > 0.0 && self.s.canvas_px().0 <= 30000 && self.s.canvas_px().1 <= 60000;
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = tokens::space::S4;
            if ui.add_enabled(valid, Button::new("Create").variant(ButtonVariant::Primary).min_width(96.0)).clicked() {
                out = Outcome::Create(self.s.clone());
            }
            if self.cancelable && ui.add(Button::new("Cancel").min_width(80.0)).clicked() {
                out = Outcome::Cancel;
            }
        });
        if ui.input(|i| i.key_pressed(egui::Key::Enter)) && valid {
            out = Outcome::Create(self.s.clone());
        }
        if self.cancelable && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            out = Outcome::Cancel;
        }
        out
    }

    /// Page preview: canvas (with bleed), trim, safe area and the panel template.
    fn preview(&self, ui: &mut Ui) {
        let p = palette(ui);
        let box_size = vec2(240.0, 300.0);
        let (rect, _) = ui.allocate_exact_size(box_size, egui::Sense::hover());
        ui.painter().rect_filled(rect, 2, p.canvas_surround);
        let (w, h) = self.s.canvas_px();
        let k = ((box_size.x - 30.0) / w as f32).min((box_size.y - 30.0) / h as f32);
        let size = vec2(w as f32 * k, h as f32 * k);
        let page = Rect::from_center_size(rect.center(), size);
        let to = |pt: Pt| pos2(page.min.x + pt.x * k, page.min.y + pt.y * k);
        ui.painter().rect_filled(page.translate(vec2(2.0, 3.0)), 0, Color32::from_black_alpha(80));
        let paper = if self.s.paper.a() == 0 { Color32::from_gray(200) } else { Color32::WHITE };
        ui.painter().rect_filled(page, 0, paper);
        if self.s.kind.has_print_guides() {
            let (tlo, thi) = self.s.trim_rect();
            let (slo, shi) = self.s.safe_rect();
            ui.painter().rect_stroke(Rect::from_min_max(to(tlo), to(thi)), 0, Stroke::new(1.0, p.overlay_trim), egui::StrokeKind::Middle);
            ui.painter().rect_stroke(Rect::from_min_max(to(slo), to(shi)), 0, Stroke::new(1.0, p.overlay_safe), egui::StrokeKind::Middle);
            if let Some(t) = self.s.template {
                let mut set = PanelSet::for_dpi(self.s.dpi);
                set.apply_template(t, slo, shi);
                for pn in &set.panels {
                    let pts: Vec<egui::Pos2> = pn.poly.iter().map(|q| to(*q)).collect();
                    ui.painter().add(egui::Shape::closed_line(pts, Stroke::new(1.2, Color32::from_gray(30))));
                }
            }
        }
        if self.s.kind == SheetKind::Webtoon {
            ui.painter().text(page.center(), egui::Align2::CENTER_CENTER, "scroll ↓", egui::FontId::proportional(11.0), Color32::from_gray(120));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_set_sensible_defaults() {
        let mut d = NewSheet::new(true);
        d.set_kind(SheetKind::Webtoon);
        assert_eq!(d.s.canvas_px(), (800, 12800));
        d.set_kind(SheetKind::Animation);
        assert_eq!(d.s.canvas_px(), (1280, 720));
        d.set_kind(SheetKind::Book);
        assert!(d.s.pages >= 2);
        assert_eq!(d.s.dpi, 600);
    }

    #[test]
    fn unit_roundtrip() {
        let mut d = NewSheet::new(true);
        d.unit = Unit::Px;
        let px = d.to_unit(182.0);
        assert!((d.from_unit(px) - 182.0).abs() < 0.05);
        d.unit = Unit::In;
        assert!((d.to_unit(25.4) - 1.0).abs() < 1e-6);
    }
}
