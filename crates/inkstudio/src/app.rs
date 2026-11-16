//! The Inkstudio window: menus, tool column, palettes, canvas, page strip, timeline.

use crate::book::{Book, ColorMode, SheetKind, SheetSettings};
use crate::canvas::{TileCache, View};
use crate::new_sheet::{NewSheet, Outcome};
use crate::pen::{PenEvent, PenInput, PenSample};
use egui::{Align2, Color32, Key, Pos2, Rect, RichText, Stroke, Ui, pos2, vec2};
use ink_comic::{Balloon, BalloonHit, BalloonStyle, Fonts, PanelTemplate, TailStyle, TextDir};
use ink_core::{blend::BlendMode, Layer, LayerId, Rgba, UndoEntry};
use ink_raster::fill::{FillOptions, Refer};
use ink_raster::presets::{catalog, Category, Preset};
use ink_raster::{Sample, Stroke as BrushStroke};
use ink_ui::icons;
use ink_ui::theme::{self, icon_font, palette, text, tokens};
use ink_ui::widgets::*;
use ink_vector::Pt;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Brush,
    Eraser,
    Fill,
    Panel,
    Balloon,
    Hand,
}

impl Tool {
    const ALL: [(Tool, &'static str, &'static str, Key); 6] = [
        (Tool::Brush, icons::PEN_NIB, "Brush (B)", Key::B),
        (Tool::Eraser, icons::ERASER, "Eraser (E)", Key::E),
        (Tool::Fill, icons::PAINT_BUCKET, "Fill (G)", Key::G),
        (Tool::Panel, icons::SQUARES_FOUR, "Panel frames (F)", Key::F),
        (Tool::Balloon, icons::CHAT_CIRCLE, "Balloons & text (T)", Key::T),
        (Tool::Hand, icons::HAND, "Hand (H / hold Space)", Key::H),
    ];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelMode {
    Rectangle,
    Divide,
    Select,
}

#[derive(Clone, Debug)]
struct BalloonDrag {
    id: u64,
    hit: BalloonHit,
    start: Pt,
    orig: Balloon,
    moved: bool,
}

pub struct InkApp {
    pub book: Option<Book>,
    new_sheet: Option<NewSheet>,
    pub fonts: Fonts,
    pub view: View,
    fit_pending: bool,
    cache: TileCache,
    pub pen: PenInput,
    pub tool: Tool,
    presets: Vec<Preset>,
    brush_id: &'static str,
    eraser_id: &'static str,
    /// Size override per preset id (px at the document's DPI).
    sizes: HashMap<&'static str, f32>,
    stabilization: HashMap<&'static str, u32>,
    pub color: Hsv,
    pub sub_color: Rgba,
    pub active_layer: Option<LayerId>,
    stroke: Option<BrushStroke>,
    fill: FillOptions,
    panel_mode: PanelMode,
    panel_drag: Option<(Pt, Pt)>,
    selected_panel: Option<usize>,
    pub balloon_style: BalloonStyle,
    balloon_dir: TextDir,
    balloon_pt: f32,
    pub selected_balloon: Option<u64>,
    balloon_drag: Option<BalloonDrag>,
    text_snapshot: bool,
    focus_text: bool,
    playing: bool,
    play_accum: f32,
    toasts: Vec<(ToastKind, String, f64)>,
    hover_canvas: Option<Pt>,
    thumbs: HashMap<usize, (egui::TextureHandle, u64)>,
    thumb_gen: u64,
    pub last_export: Option<std::path::PathBuf>,
    /// Export folder override (otherwise $INKSTUDIO_EXPORT_DIR or ~/Pictures/Inkstudio).
    pub export_to: Option<std::path::PathBuf>,
    color_tab: usize,
    hinted_balloon: bool,
}

fn to_rgba(c: Color32) -> Rgba {
    Rgba::from_straight(c.r(), c.g(), c.b(), 255)
}

impl InkApp {
    pub fn new(ctx: &egui::Context) -> Self {
        theme::apply(ctx, theme::Mode::Dark);
        Self {
            book: None,
            new_sheet: Some(NewSheet::new(false)),
            fonts: Fonts::system(),
            view: View::default(),
            fit_pending: true,
            cache: TileCache::default(),
            pen: PenInput::default(),
            tool: Tool::Brush,
            presets: catalog(),
            brush_id: "g_pen",
            eraser_id: "eraser_hard",
            sizes: HashMap::new(),
            stabilization: HashMap::new(),
            color: Hsv { h: 0.0, s: 0.0, v: 0.07 },
            sub_color: Rgba::WHITE,
            active_layer: None,
            stroke: None,
            fill: FillOptions { refer: Refer::References, tolerance: 40, gap: 4, expand: 2 },
            panel_mode: PanelMode::Rectangle,
            panel_drag: None,
            selected_panel: None,
            balloon_style: BalloonStyle::Oval,
            balloon_dir: TextDir::Vertical,
            balloon_pt: 10.0,
            selected_balloon: None,
            balloon_drag: None,
            text_snapshot: false,
            focus_text: false,
            playing: false,
            play_accum: 0.0,
            toasts: Vec::new(),
            hover_canvas: None,
            thumbs: HashMap::new(),
            thumb_gen: 0,
            last_export: None,
            export_to: None,
            color_tab: 0,
            hinted_balloon: false,
        }
    }

    /// Open a book directly (used by tests and screenshots).
    pub fn open(&mut self, s: SheetSettings) {
        let book = Book::new(s);
        self.active_layer = Some(book.page().ink_layer);
        if book.page().timeline.is_some() {
            self.active_layer = book.page().timeline.as_ref().and_then(|t| t.cel_at(0, 0));
        }
        self.book = Some(book);
        self.new_sheet = None;
        self.fit_pending = true;
        self.cache.clear();
        self.thumbs.clear();
        self.selected_balloon = None;
        self.selected_panel = None;
    }

    /// Select a brush or eraser preset by id (also switches to the matching tool).
    pub fn select_brush(&mut self, id: &str) {
        if let Some(p) = self.presets.iter().find(|p| p.id == id) {
            if p.category == Category::Eraser {
                self.eraser_id = p.id;
                self.tool = Tool::Eraser;
            } else {
                self.brush_id = p.id;
                self.tool = Tool::Brush;
            }
        }
    }

    fn toast(&mut self, kind: ToastKind, msg: impl Into<String>, now: f64) {
        self.toasts.push((kind, msg.into(), now));
    }

    fn preset(&self, id: &str) -> &Preset {
        self.presets.iter().find(|p| p.id == id).unwrap_or(&self.presets[0])
    }

    fn current_preset_id(&self) -> &'static str {
        if self.tool == Tool::Eraser { self.eraser_id } else { self.brush_id }
    }

    /// Preset size scaled from its 600 dpi definition to this document.
    fn size_of(&self, id: &'static str) -> f32 {
        let dpi = self.book.as_ref().map_or(600, |b| b.settings.dpi) as f32;
        *self.sizes.get(id).unwrap_or(&(self.preset(id).params.size * dpi / 600.0))
    }

    fn mm_per_px(&self) -> f32 {
        25.4 / self.book.as_ref().map_or(600, |b| b.settings.dpi) as f32
    }

    // =========================================================================================
    // Top level
    // =========================================================================================

    pub fn ui(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let now = ctx.input(|i| i.time);
        self.shortcuts(&ctx, now);
        let p = palette(ui);

        egui::Panel::top("menu").exact_size(tokens::layout::MENU_BAR_H).frame(egui::Frame::new().fill(p.bg).inner_margin(egui::Margin::symmetric(4, 0))).show(ui, |ui| {
            self.menu_bar(ui, now);
        });
        egui::Panel::top("options").exact_size(tokens::layout::COMMAND_BAR_H + 4.0).frame(egui::Frame::new().fill(p.surface).inner_margin(egui::Margin::symmetric(8, 0))).show(ui, |ui| {
            ui.horizontal_centered(|ui| self.tool_options(ui));
        });
        egui::Panel::bottom("status").exact_size(tokens::layout::STATUS_BAR_H).frame(egui::Frame::new().fill(p.bg).inner_margin(egui::Margin::symmetric(10, 0))).show(ui, |ui| {
            self.status_bar(ui);
        });
        let has_timeline = self.book.as_ref().is_some_and(|b| b.page().timeline.is_some());
        let is_book = self.book.as_ref().is_some_and(|b| b.settings.kind == SheetKind::Book || b.pages.len() > 1);
        if has_timeline {
            egui::Panel::bottom("timeline").exact_size(96.0).frame(egui::Frame::new().fill(p.surface).inner_margin(egui::Margin::symmetric(8, 6))).show(ui, |ui| self.timeline_panel(ui, now));
        } else if is_book {
            egui::Panel::bottom("pages").exact_size(168.0).frame(egui::Frame::new().fill(p.surface).inner_margin(egui::Margin::symmetric(8, 6))).show(ui, |ui| self.pages_panel(ui));
        }
        egui::Panel::left("tools").exact_size(tokens::layout::TOOL_COLUMN_W + 4.0).resizable(false).frame(egui::Frame::new().fill(p.surface).inner_margin(egui::Margin::symmetric(4, 6))).show(ui, |ui| {
            self.tool_column(ui);
        });
        egui::Panel::left("left-dock").exact_size(250.0).resizable(false).frame(egui::Frame::new().fill(p.surface)).show(ui, |ui| {
            ui.set_max_width(ui.available_width());
            egui::ScrollArea::vertical().id_salt("left-scroll").show(ui, |ui| self.left_dock(ui, now));
        });
        egui::Panel::right("right-dock").exact_size(280.0).resizable(false).frame(egui::Frame::new().fill(p.surface)).show(ui, |ui| {
            ui.set_max_width(ui.available_width());
            self.right_dock(ui);
        });
        egui::CentralPanel::default().frame(egui::Frame::new().fill(p.canvas_surround)).show(ui, |ui| {
            self.canvas(ui, now);
        });

        // New sheet dialog.
        if let Some(dlg) = &mut self.new_sheet {
            let mut outcome = Outcome::Open;
            egui::Modal::new(egui::Id::new("new-sheet")).frame(egui::Frame::new().fill(p.surface_raised).stroke(Stroke::new(1.0, p.border_strong)).corner_radius(tokens::radius::LG as u8).inner_margin(20)).show(&ctx, |ui| {
                outcome = dlg.show(ui);
            });
            match outcome {
                Outcome::Create(s) => self.open(s),
                Outcome::Cancel => self.new_sheet = None,
                Outcome::Open => {}
            }
        }
        self.show_toasts(&ctx, now);
    }

    fn shortcuts(&mut self, ctx: &egui::Context, now: f64) {
        if ctx.egui_wants_keyboard_input() || self.new_sheet.is_some() {
            return;
        }
        let (undo, redo, new, fit, zoom_in, zoom_out, bigger, smaller, del, swap, play) = ctx.input_mut(|i| {
            let cmd = egui::Modifiers::COMMAND;
            (
                i.consume_key(cmd, Key::Z),
                i.consume_key(cmd | egui::Modifiers::SHIFT, Key::Z) || i.consume_key(cmd, Key::Y),
                i.consume_key(cmd, Key::N),
                i.consume_key(cmd, Key::Num0),
                i.consume_key(cmd, Key::Plus) || i.consume_key(cmd, Key::Equals),
                i.consume_key(cmd, Key::Minus),
                i.key_pressed(Key::CloseBracket),
                i.key_pressed(Key::OpenBracket),
                i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace),
                i.key_pressed(Key::X) && i.modifiers.is_none(),
                i.key_pressed(Key::Enter) && i.modifiers.is_none(),
            )
        });
        if redo {
            self.redo();
        } else if undo {
            self.undo();
        }
        if new {
            self.new_sheet = Some(NewSheet::new(self.book.is_some()));
        }
        if fit {
            self.fit_pending = true;
        }
        if zoom_in || zoom_out {
            let c = ctx.content_rect().center();
            self.view.zoom_at(c, if zoom_in { 1.25 } else { 0.8 });
        }
        let id = self.current_preset_id();
        if bigger || smaller {
            let s = self.size_of(id);
            self.sizes.insert(id, (s * if bigger { 1.15 } else { 1.0 / 1.15 }).clamp(0.5, 2000.0));
        }
        if swap {
            let main = self.color.to_color32();
            let sub = self.sub_color.to_straight();
            self.sub_color = to_rgba(main);
            self.color = hsv_from(Color32::from_rgb(sub[0], sub[1], sub[2]));
        }
        if play && self.book.as_ref().is_some_and(|b| b.page().timeline.is_some()) {
            self.playing = !self.playing;
        }
        if del {
            self.delete_selection(now);
        }
        for (t, _, _, key) in Tool::ALL {
            if ctx.input(|i| i.key_pressed(key) && i.modifiers.is_none()) {
                self.tool = t;
            }
        }
    }

    pub fn undo(&mut self) {
        if let Some(b) = &mut self.book {
            let p = &mut b.pages[b.current];
            p.undo(&self.fonts);
        }
    }
    pub fn redo(&mut self) {
        if let Some(b) = &mut self.book {
            let p = &mut b.pages[b.current];
            p.redo(&self.fonts);
        }
    }

    fn delete_selection(&mut self, now: f64) {
        let Some(b) = &mut self.book else { return };
        let page = b.page_mut();
        if self.tool == Tool::Balloon {
            if let Some(id) = self.selected_balloon.take() {
                page.push_objects();
                page.balloons.retain(|x| x.id != id);
                page.render_balloons(&self.fonts, None);
            }
        } else if self.tool == Tool::Panel {
            if let Some(i) = self.selected_panel.take() {
                if i < page.panels.panels.len() {
                    page.push_objects();
                    page.panels.panels.remove(i);
                    page.render_panels();
                    self.toast(ToastKind::Info, "Panel deleted", now);
                }
            }
        }
    }

    // =========================================================================================
    // Menu, options, status
    // =========================================================================================

    fn menu_bar(&mut self, ui: &mut Ui, now: f64) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("New sheet…  (Ctrl+N)").clicked() {
                    self.new_sheet = Some(NewSheet::new(self.book.is_some()));
                    ui.close();
                }
                ui.separator();
                let has = self.book.is_some();
                if ui.add_enabled(has, egui::Button::new("Export page as PNG")).clicked() {
                    self.export_pages(false, now);
                    ui.close();
                }
                let multi = self.book.as_ref().is_some_and(|b| b.pages.len() > 1);
                if ui.add_enabled(multi, egui::Button::new("Export all pages as PNG")).clicked() {
                    self.export_pages(true, now);
                    ui.close();
                }
                let anim = self.book.as_ref().is_some_and(|b| b.page().timeline.is_some());
                if ui.add_enabled(anim, egui::Button::new("Export animation as GIF")).clicked() {
                    self.export_gif(now);
                    ui.close();
                }
            });
            ui.menu_button("Edit", |ui| {
                let (cu, cr) = self.book.as_ref().map_or((false, false), |b| (b.page().can_undo(), b.page().can_redo()));
                if ui.add_enabled(cu, egui::Button::new("Undo  (Ctrl+Z)")).clicked() {
                    self.undo();
                    ui.close();
                }
                if ui.add_enabled(cr, egui::Button::new("Redo  (Ctrl+Shift+Z)")).clicked() {
                    self.redo();
                    ui.close();
                }
            });
            ui.menu_button("View", |ui| {
                if ui.button("Fit page  (Ctrl+0)").clicked() {
                    self.fit_pending = true;
                    ui.close();
                }
                if ui.button("Actual pixels (100%)").clicked() {
                    let c = ui.ctx().content_rect().center();
                    let k = 1.0 / self.view.zoom;
                    self.view.zoom_at(c, k);
                    ui.close();
                }
                ui.checkbox(&mut self.pen.simulate, "Simulate pressure for mouse");
            });
            ui.menu_button("Page", |ui| {
                let Some(b) = &mut self.book else { return };
                if ui.button("Add page after this").clicked() {
                    let at = b.current + 1;
                    b.add_page(at);
                    b.current = at;
                    self.cache.clear();
                    self.thumbs.clear();
                    ui.close();
                }
                if ui.add_enabled(b.pages.len() > 1, egui::Button::new("Delete this page")).clicked() {
                    let c = b.current;
                    b.delete_page(c);
                    self.cache.clear();
                    self.thumbs.clear();
                    ui.close();
                }
                ui.separator();
                ui.label(RichText::new("Replace panels with template").color(palette(ui).text_muted));
                for t in PanelTemplate::ALL {
                    if ui.button(t.name()).clicked() {
                        let (lo, hi) = b.settings.safe_rect();
                        let page = b.page_mut();
                        page.push_objects();
                        page.panels.apply_template(t, lo, hi);
                        page.render_panels();
                        ui.close();
                    }
                }
            });
            ui.menu_button("Animation", |ui| {
                let anim = self.book.as_ref().is_some_and(|b| b.page().timeline.is_some());
                if ui.add_enabled(anim, egui::Button::new(if self.playing { "Pause  (Enter)" } else { "Play  (Enter)" })).clicked() {
                    self.playing = !self.playing;
                    ui.close();
                }
                if !anim {
                    ui.label(RichText::new("Create an Animation sheet to use the timeline.").color(palette(ui).text_muted));
                }
            });
        });
    }

    fn tool_options(&mut self, ui: &mut Ui) {
        let p = palette(ui);
        ui.spacing_mut().item_spacing.x = 6.0;
        let (cu, cr) = self.book.as_ref().map_or((false, false), |b| (b.page().can_undo(), b.page().can_redo()));
        if ui.add_enabled(cu, IconButton::new(icons::ARROW_COUNTER_CLOCKWISE, "Undo (Ctrl+Z)").size(28.0)).clicked() {
            self.undo();
        }
        if ui.add_enabled(cr, IconButton::new(icons::ARROW_CLOCKWISE, "Redo (Ctrl+Shift+Z)").size(28.0)).clicked() {
            self.redo();
        }
        ui.separator();
        match self.tool {
            Tool::Brush | Tool::Eraser => {
                let id = self.current_preset_id();
                let name = self.preset(id).params.name.clone();
                ui.label(RichText::new(name).text_style(text::body_strong()));
                let mut size = self.size_of(id);
                ui.label(RichText::new("Size").color(p.text_muted));
                if ui.add(egui::Slider::new(&mut size, 0.5..=600.0).logarithmic(true).suffix(" px").max_decimals(1)).changed() {
                    self.sizes.insert(id, size);
                }
                ui.label(RichText::new(format!("{:.2} mm", size * self.mm_per_px())).color(p.text_muted));
                let mut st = *self.stabilization.get(id).unwrap_or(&self.preset(id).params.stabilization);
                ui.label(RichText::new("Stabilization").color(p.text_muted));
                if ui.add(egui::Slider::new(&mut st, 0..=100)).changed() {
                    self.stabilization.insert(id, st);
                }
            }
            Tool::Fill => {
                ui.label(RichText::new("Fill").text_style(text::body_strong()));
                egui::ComboBox::from_id_salt("refer").selected_text(match self.fill.refer {
                    Refer::Target => "This layer",
                    Refer::References => "Reference layers",
                    Refer::AllLayers => "All layers",
                }).show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.fill.refer, Refer::Target, "This layer");
                    ui.selectable_value(&mut self.fill.refer, Refer::References, "Reference layers");
                    ui.selectable_value(&mut self.fill.refer, Refer::AllLayers, "All layers");
                });
                ui.label(RichText::new("Close gap").color(p.text_muted));
                ui.add(egui::Slider::new(&mut self.fill.gap, 0..=40).suffix(" px"));
                ui.label(RichText::new("Tolerance").color(p.text_muted));
                ui.add(egui::Slider::new(&mut self.fill.tolerance, 0..=255));
                ui.label(RichText::new("Expand").color(p.text_muted));
                ui.add(egui::Slider::new(&mut self.fill.expand, 0..=10).suffix(" px"));
            }
            Tool::Panel => {
                ui.label(RichText::new("Panels").text_style(text::body_strong()));
                ui.selectable_value(&mut self.panel_mode, PanelMode::Rectangle, "Draw rectangle");
                ui.selectable_value(&mut self.panel_mode, PanelMode::Divide, "Divide (drag a cut)");
                ui.selectable_value(&mut self.panel_mode, PanelMode::Select, "Select");
                if let Some(b) = &mut self.book {
                    let mm = 25.4 / b.settings.dpi as f32;
                    let page = b.page_mut();
                    let mut border = page.panels.border * mm;
                    let mut gh = page.panels.gutter.0 * mm;
                    let mut gv = page.panels.gutter.1 * mm;
                    ui.label(RichText::new("Border").color(p.text_muted));
                    let c1 = ui.add(egui::DragValue::new(&mut border).range(0.05..=5.0).speed(0.01).suffix(" mm")).changed();
                    ui.label(RichText::new("Gutter ↕").color(p.text_muted));
                    ui.add(egui::DragValue::new(&mut gh).range(0.0..=30.0).speed(0.1).suffix(" mm"));
                    ui.label(RichText::new("↔").color(p.text_muted));
                    ui.add(egui::DragValue::new(&mut gv).range(0.0..=30.0).speed(0.1).suffix(" mm"));
                    page.panels.gutter = (gh / mm, gv / mm);
                    if c1 {
                        page.panels.border = border / mm;
                        page.render_panels();
                    }
                }
            }
            Tool::Balloon => {
                ui.label(RichText::new("Balloon").text_style(text::body_strong()));
                ui.label(RichText::new(self.balloon_style.name()).color(p.text_muted));
                ui.selectable_value(&mut self.balloon_dir, TextDir::Vertical, "縦 Vertical");
                ui.selectable_value(&mut self.balloon_dir, TextDir::Horizontal, "Horizontal");
                ui.label(RichText::new("Size").color(p.text_muted));
                ui.add(egui::DragValue::new(&mut self.balloon_pt).range(4.0..=72.0).speed(0.2).suffix(" pt"));
                ui.label(RichText::new("Click empty canvas to place · drag body / tail tip / corners").color(p.text_muted));
            }
            Tool::Hand => {
                ui.label(RichText::new("Drag to pan · scroll to zoom").color(p.text_muted));
            }
        }
    }

    fn status_bar(&mut self, ui: &mut Ui) {
        let p = palette(ui);
        ui.horizontal_centered(|ui| {
            let st = |t: String| RichText::new(t).text_style(text::caption()).color(p.text_muted);
            ui.label(st(format!("Zoom {:.1}%", self.view.zoom * 100.0)));
            if let Some(b) = &self.book {
                ui.label(st(format!("{} × {} px · {} dpi · {}", b.page().doc.width, b.page().doc.height, b.settings.dpi, b.settings.color_mode.name())));
                if b.pages.len() > 1 {
                    ui.label(st(format!("Page {} / {}", b.current + 1, b.pages.len())));
                }
                if let Some(tl) = &b.page().timeline {
                    ui.label(st(format!("Frame {} / {} · {} fps", b.page().frame + 1, tl.frames, tl.fps)));
                }
            }
            if let Some(c) = self.hover_canvas {
                ui.label(st(format!("{:.0}, {:.0}", c.x, c.y)));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let src = if self.pen.saw_pen { format!("Pen pressure {:.2}", self.pen.last_pressure) } else if self.pen.simulate { "Mouse (simulated pressure)".into() } else { "Mouse".into() };
                ui.label(st(src));
                ui.label(st(format!("{} GPU tile uploads", self.cache.uploads_last_frame)));
            });
        });
    }

    fn show_toasts(&mut self, ctx: &egui::Context, now: f64) {
        self.toasts.retain(|t| now - t.2 < 5.0 || t.0 == ToastKind::Error && now - t.2 < 12.0);
        if self.toasts.is_empty() {
            return;
        }
        egui::Area::new(egui::Id::new("toasts")).anchor(Align2::CENTER_BOTTOM, vec2(0.0, -40.0)).show(ctx, |ui| {
            for (k, m, _) in self.toasts.clone() {
                toast(ui, k, &m, None);
                ui.add_space(4.0);
            }
        });
        ctx.request_repaint_after(std::time::Duration::from_millis(500));
    }

    // =========================================================================================
    // Docks
    // =========================================================================================

    fn tool_column(&mut self, ui: &mut Ui) {
        ui.spacing_mut().item_spacing.y = 2.0;
        for (t, icon, name, _) in Tool::ALL {
            if ui.add(IconButton::new(icon, name).selected(self.tool == t)).clicked() {
                self.tool = t;
            }
        }
        ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
            ui.add_space(4.0);
            let (r, resp) = ui.allocate_exact_size(vec2(32.0, 32.0), egui::Sense::click());
            let sub = Rect::from_min_size(r.min + vec2(12.0, 12.0), vec2(20.0, 20.0));
            let main = Rect::from_min_size(r.min, vec2(20.0, 20.0));
            let p = palette(ui);
            let s = self.sub_color.to_straight();
            ui.painter().rect(sub, 1, Color32::from_rgb(s[0], s[1], s[2]), Stroke::new(1.0, p.border_strong), egui::StrokeKind::Inside);
            ui.painter().rect(main, 1, self.color.to_color32(), Stroke::new(1.0, p.border_strong), egui::StrokeKind::Inside);
            if resp.on_hover_text("Swap main/sub color (X)").clicked() {
                let main = self.color.to_color32();
                self.color = hsv_from(Color32::from_rgb(s[0], s[1], s[2]));
                self.sub_color = to_rgba(main);
            }
        });
    }

    fn left_dock(&mut self, ui: &mut Ui, now: f64) {
        ui.spacing_mut().item_spacing.y = 0.0;
        match self.tool {
            Tool::Brush | Tool::Eraser => {
                palette_header(ui, if self.tool == Tool::Eraser { "Sub tool · Eraser" } else { "Sub tool · Brushes" });
                egui::Frame::new().inner_margin(egui::Margin::symmetric(4, 4)).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 1.0;
                    for cat in Category::ALL {
                        let erasers = cat == Category::Eraser;
                        if erasers != (self.tool == Tool::Eraser) {
                            continue;
                        }
                        let p = palette(ui);
                        ui.add_space(4.0);
                        ui.label(RichText::new(cat.name()).text_style(text::caption()).color(p.text_muted));
                        for preset in self.presets.iter().filter(|p| p.category == cat) {
                            let selected = self.current_preset_id() == preset.id;
                            let icon = match cat {
                                Category::Ink => icons::PEN_NIB,
                                Category::Pencil => icons::PENCIL_SIMPLE,
                                Category::Paint => icons::PAINT_BRUSH,
                                Category::Airbrush => icons::SPRAY_BOTTLE,
                                Category::Eraser => icons::ERASER,
                                Category::Effect => icons::LIGHTNING,
                                Category::Decoration => icons::SPARKLE,
                                Category::Tone => icons::DOTS_NINE,
                            };
                            if list_row(ui, Some(icon), &preset.params.name, selected).on_hover_text(preset.use_for).clicked() {
                                if self.tool == Tool::Eraser {
                                    self.eraser_id = preset.id;
                                } else {
                                    self.brush_id = preset.id;
                                }
                            }
                        }
                    }
                });
            }
            Tool::Balloon => self.balloon_dock(ui, now),
            Tool::Panel => {
                palette_header(ui, "Panel templates");
                egui::Frame::new().inner_margin(egui::Margin::symmetric(8, 6)).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 4.0;
                    let p = palette(ui);
                    ui.label(RichText::new("Replaces the panels on this page (undoable).").text_style(text::caption()).color(p.text_muted));
                    for t in PanelTemplate::ALL {
                        if ui.add(Button::new(t.name()).min_width(220.0)).clicked() {
                            if let Some(b) = &mut self.book {
                                let (lo, hi) = b.settings.safe_rect();
                                let page = b.page_mut();
                                page.push_objects();
                                page.panels.apply_template(t, lo, hi);
                                page.render_panels();
                            }
                        }
                    }
                    ui.add_space(6.0);
                    ui.label(RichText::new("Rectangle: drag on the page.\nDivide: drag a line across a panel.\nSelect + Delete removes a panel.").text_style(text::caption()).color(p.text_muted));
                });
            }
            Tool::Fill | Tool::Hand => {
                palette_header(ui, "Tool");
                egui::Frame::new().inner_margin(8).show(ui, |ui| {
                    let p = palette(ui);
                    let msg = if self.tool == Tool::Fill {
                        "Click inside line art. The color goes onto the selected layer; the shape is read from reference layers (Ink, Panels)."
                    } else {
                        "Drag to move the view. Scroll to zoom, Ctrl+0 to fit."
                    };
                    ui.label(RichText::new(msg).color(p.text_muted));
                });
            }
        }
    }

    /// Balloon style picker + text editor (the balloon tool's own palette).
    fn balloon_dock(&mut self, ui: &mut Ui, now: f64) {
        let p = palette(ui);
        palette_header(ui, "Balloon styles");
        egui::Frame::new().inner_margin(egui::Margin::symmetric(6, 6)).show(ui, |ui| {
            egui::Grid::new("balloon-styles").spacing([4.0, 4.0]).show(ui, |ui| {
                for (i, style) in BalloonStyle::ALL.into_iter().enumerate() {
                    let selected = self.balloon_style == style;
                    let (r, resp) = ui.allocate_exact_size(vec2(115.0, 64.0), egui::Sense::click());
                    let fill = if selected { p.accent_subtle } else if resp.hovered() { p.surface_hover } else { p.surface_sunken };
                    ui.painter().rect(r, 3, fill, Stroke::new(1.0, if selected { p.focus_ring } else { p.border }), egui::StrokeKind::Inside);
                    draw_style_icon(ui.painter(), Rect::from_min_size(r.min + vec2(8.0, 6.0), vec2(99.0, 38.0)), style);
                    ui.painter().text(pos2(r.center().x, r.bottom() - 9.0), Align2::CENTER_CENTER, style.name(), egui::FontId::proportional(11.0), p.text);
                    let resp = resp.on_hover_text(format!("{} balloon", style.name()));
                    if resp.clicked() {
                        self.balloon_style = style;
                        // Restyle the selected balloon too.
                        if let (Some(id), Some(b)) = (self.selected_balloon, &mut self.book) {
                            let page = b.page_mut();
                            if page.balloons.iter().any(|x| x.id == id) {
                                page.push_objects();
                                if let Some(bl) = page.balloons.iter_mut().find(|x| x.id == id) {
                                    bl.style = style;
                                    bl.tail = style.default_tail();
                                    if bl.auto_fit {
                                        bl.fit_to_text(&self.fonts);
                                    }
                                }
                                page.render_balloons(&self.fonts, None);
                            }
                        }
                    }
                    if i % 2 == 1 {
                        ui.end_row();
                    }
                }
            });
        });

        palette_header(ui, "Text");
        egui::Frame::new().inner_margin(egui::Margin::symmetric(8, 6)).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 6.0;
            let Some(b) = &mut self.book else { return };
            let page = b.page_mut();
            let Some(id) = self.selected_balloon else {
                ui.label(RichText::new("Click on the page to place a balloon, or click a balloon to edit it.").color(p.text_muted));
                self.text_snapshot = false;
                return;
            };
            let Some(idx) = page.balloons.iter().position(|x| x.id == id) else {
                self.selected_balloon = None;
                return;
            };
            let mut bl = page.balloons[idx].clone();
            let te = egui::TextEdit::multiline(&mut bl.text).desired_rows(4).desired_width(f32::INFINITY).hint_text("Type the dialogue…");
            let resp = ui.add(te);
            if self.focus_text {
                resp.request_focus();
                self.focus_text = false;
            }
            if resp.gained_focus() {
                self.text_snapshot = false;
            }
            let mut changed = resp.changed();
            ui.horizontal(|ui| {
                changed |= ui.selectable_value(&mut bl.dir, TextDir::Vertical, "縦 Vertical").changed();
                changed |= ui.selectable_value(&mut bl.dir, TextDir::Horizontal, "Horizontal").changed();
            });
            ui.horizontal(|ui| {
                let dpi = b_dpi(page);
                let mut pt = bl.font_px * 72.0 / dpi;
                ui.label("Font size");
                if ui.add(egui::DragValue::new(&mut pt).range(4.0..=96.0).speed(0.2).suffix(" pt")).changed() {
                    bl.font_px = pt * dpi / 72.0;
                    changed = true;
                }
            });
            ui.horizontal_wrapped(|ui| {
                ui.label("Tail");
                for (t, name) in [(TailStyle::Pointed, "Pointed"), (TailStyle::Curved, "Curved"), (TailStyle::Bubbles, "Bubbles"), (TailStyle::None, "None")] {
                    changed |= ui.selectable_value(&mut bl.tail, t, name).changed();
                }
            });
            changed |= ui.checkbox(&mut bl.auto_fit, "Fit balloon to text").changed();
            if changed {
                if !self.text_snapshot {
                    page.push_objects();
                    self.text_snapshot = true;
                }
                if bl.auto_fit {
                    bl.fit_to_text(&self.fonts);
                }
                page.balloons[idx] = bl;
                page.render_balloons(&self.fonts, None);
            }
            ui.horizontal(|ui| {
                if ui.add(Button::new("Bring to front").size(ButtonSize::Sm)).clicked() {
                    page.push_objects();
                    let x = page.balloons.remove(idx);
                    page.balloons.push(x);
                    page.render_balloons(&self.fonts, None);
                }
                if ui.add(Button::new("Delete").variant(ButtonVariant::Danger).size(ButtonSize::Sm)).clicked() {
                    page.push_objects();
                    page.balloons.remove(idx);
                    page.render_balloons(&self.fonts, None);
                    self.selected_balloon = None;
                }
            });
            if self.fonts.is_empty() {
                ui.label(RichText::new("No font found: text will not render.").color(p.warning));
            }
        });
        let _ = now;
    }

    fn right_dock(&mut self, ui: &mut Ui) {
        ui.spacing_mut().item_spacing.y = 0.0;
        palette_header(ui, "Color");
        egui::Frame::new().inner_margin(egui::Margin::symmetric(8, 6)).show(ui, |ui| {
            tab_strip(ui, &["Wheel", "Set"], &mut self.color_tab);
            ui.add_space(6.0);
            if self.color_tab == 0 {
                ui.horizontal(|ui| {
                    color_wheel(ui, &mut self.color, 150.0);
                    ui.add_space(4.0);
                    let s = self.sub_color.to_straight();
                    let (m, sub, swap) = color_pair(ui, self.color.to_color32(), Color32::from_rgb(s[0], s[1], s[2]));
                    let _ = (m, sub);
                    if swap {
                        let main = self.color.to_color32();
                        self.color = hsv_from(Color32::from_rgb(s[0], s[1], s[2]));
                        self.sub_color = to_rgba(main);
                    }
                });
            }
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(3.0, 3.0);
                let set = [
                    Color32::BLACK, Color32::from_gray(64), Color32::from_gray(128), Color32::from_gray(192), Color32::WHITE,
                    Color32::from_rgb(120, 170, 230), Color32::from_rgb(214, 70, 60), Color32::from_rgb(232, 170, 60), Color32::from_rgb(70, 150, 90), Color32::from_rgb(250, 220, 200),
                ];
                for c in set {
                    if swatch(ui, c, 20.0, self.color.to_color32() == c).clicked() {
                        self.color = hsv_from(c);
                    }
                }
            });
        });
        palette_header(ui, "Layer");
        self.layers_palette(ui);
    }

    fn layers_palette(&mut self, ui: &mut Ui) {
        let Some(b) = &mut self.book else { return };
        let page = &mut b.pages[b.current];
        let cel_ids: Vec<LayerId> = page.timeline.as_ref().map(|t| t.tracks.iter().flat_map(|t| t.cels.clone()).collect()).unwrap_or_default();
        let active = self.active_layer;
        egui::Frame::new().inner_margin(egui::Margin::symmetric(6, 4)).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            if let Some(id) = active {
                if let Some(l) = page.doc.layer(id) {
                    let mut props = l.props.clone();
                    let mut changed = false;
                    ui.horizontal(|ui| {
                        egui::ComboBox::from_id_salt("blend").width(110.0).selected_text(props.blend.name()).show_ui(ui, |ui| {
                            for m in BlendMode::ALL {
                                changed |= ui.selectable_value(&mut props.blend, m, m.name()).changed();
                            }
                        });
                        let mut op = props.opacity as f32 / 2.55;
                        if ui.add(egui::DragValue::new(&mut op).range(0.0..=100.0).suffix(" %").max_decimals(0)).changed() {
                            props.opacity = (op * 2.55).round() as u8;
                            changed = true;
                        }
                    });
                    ui.horizontal(|ui| {
                        changed |= ui.checkbox(&mut props.clip_below, "Clip").changed();
                        changed |= ui.checkbox(&mut props.lock_alpha, "Lock α").changed();
                        changed |= ui.checkbox(&mut props.reference, "Ref").changed();
                        changed |= ui.checkbox(&mut props.draft, "Draft").changed();
                    });
                    if changed {
                        let snap = page.doc.snapshot_layers();
                        page.doc.layer_mut(id).unwrap().props = props;
                        page.doc.mark_all_dirty();
                        page.push_doc(UndoEntry::Layers { label: "Layer properties".into(), layers: snap });
                    }
                }
            }
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 1.0;
                if ui.add(IconButton::new(icons::FILE_PLUS, "New layer above").size(24.0)).clicked() {
                    let snap = page.doc.snapshot_layers();
                    let at = active.and_then(|a| page.doc.index_of(a)).map_or(page.doc.layers().len(), |i| i + 1);
                    let n = page.doc.layers().len() + 1;
                    let id = page.doc.add_layer(Layer::new(format!("Layer {n}")), at);
                    page.push_doc(UndoEntry::Layers { label: "New layer".into(), layers: snap });
                    self.active_layer = Some(id);
                }
                let can_delete = active.is_some_and(|a| !page.is_object_layer(a) && page.doc.layers().len() > 1 && !cel_ids.contains(&a));
                if ui.add_enabled(can_delete, IconButton::new(icons::TRASH, "Delete layer").size(24.0)).clicked() {
                    let snap = page.doc.snapshot_layers();
                    page.doc.remove_layer(active.unwrap());
                    page.push_doc(UndoEntry::Layers { label: "Delete layer".into(), layers: snap });
                    self.active_layer = page.doc.layers().last().map(|l| l.0);
                }
                for (icon, label, dir) in [(icons::ARROW_UP, "Move layer up", 1i32), (icons::ARROW_DOWN, "Move layer down", -1)] {
                    if ui.add(IconButton::new(icon, label).size(24.0)).clicked() {
                        if let Some(a) = active {
                            let i = page.doc.index_of(a).unwrap_or(0) as i32;
                            let j = (i + dir).clamp(0, page.doc.layers().len() as i32 - 1) as usize;
                            let snap = page.doc.snapshot_layers();
                            page.doc.move_layer(a, j);
                            page.push_doc(UndoEntry::Layers { label: "Move layer".into(), layers: snap });
                        }
                    }
                }
            });
        });
        egui::ScrollArea::vertical().id_salt("layers").show(ui, |ui| {
            let layers: Vec<(LayerId, ink_core::LayerProps)> = page.doc.layers().iter().rev().map(|(id, l)| (*id, l.props.clone())).collect();
            for (id, props) in layers {
                let kind = if id == page.panel_layer {
                    LayerKind::Frame
                } else if id == page.balloon_layer {
                    LayerKind::Balloon
                } else if cel_ids.contains(&id) {
                    LayerKind::Animation
                } else if props.draft {
                    LayerKind::Raster
                } else {
                    LayerKind::Vector
                };
                let st = LayerRowState {
                    name: props.name.clone(),
                    kind,
                    depth: 0,
                    visible: props.visible,
                    locked: props.locked,
                    clipped: props.clip_below,
                    has_mask: false,
                    reference: props.reference,
                    draft: props.draft,
                    selected: Some(id) == self.active_layer,
                    opacity: props.opacity,
                    blend: props.blend.name(),
                    thumb: None,
                };
                match layer_row(ui, &st) {
                    LayerRowEvent::Select => self.active_layer = Some(id),
                    LayerRowEvent::ToggleVisible => {
                        if let Some(l) = page.doc.layer_mut(id) {
                            l.props.visible = !l.props.visible;
                        }
                        page.doc.mark_all_dirty();
                    }
                    LayerRowEvent::ToggleLock => {
                        if let Some(l) = page.doc.layer_mut(id) {
                            l.props.locked = !l.props.locked;
                        }
                    }
                    _ => {}
                }
            }
        });
    }

    fn pages_panel(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let p = palette(ui);
        let Some(b) = &mut self.book else { return };
        ui.horizontal(|ui| {
            ui.set_min_height(28.0);
            ui.label(RichText::new("Pages").text_style(text::body_strong()));
            let rtl = b.settings.binding == crate::book::Binding::Right;
            ui.label(RichText::new(if rtl { "right binding: page 1 is on the right" } else { "left binding" }).color(p.text_muted));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add_enabled(b.pages.len() > 1, Button::new("Delete").size(ButtonSize::Sm)).clicked() {
                    let c = b.current;
                    b.delete_page(c);
                    self.cache.clear();
                    self.thumbs.clear();
                }
                if ui.add(Button::new("Add page").icon(icons::PLUS).size(ButtonSize::Sm)).clicked() {
                    let at = b.current + 1;
                    b.add_page(at);
                    b.current = at;
                    self.cache.clear();
                    self.thumbs.clear();
                }
            });
        });
        // Thumbnails: regenerate one stale page per frame while the pen is up.
        if self.stroke.is_none() {
            let stale = (0..b.pages.len()).find(|i| b.pages[*i].thumb_dirty || !self.thumbs.contains_key(i));
            if let Some(i) = stale {
                let img = thumbnail(&b.pages[i].doc, 84.0);
                self.thumb_gen += 1;
                let h = ctx.load_texture(format!("thumb{}-{}", i, self.thumb_gen), img, egui::TextureOptions::LINEAR);
                self.thumbs.insert(i, (h, self.thumb_gen));
                b.pages[i].thumb_dirty = false;
                ctx.request_repaint();
            }
        }
        let rtl = b.settings.binding == crate::book::Binding::Right;
        let mut switch = None;
        let mut mv = None;
        egui::ScrollArea::horizontal().id_salt("pages").show(ui, |ui| {
            let layout = if rtl { egui::Layout::right_to_left(egui::Align::Min) } else { egui::Layout::left_to_right(egui::Align::Min) };
            ui.with_layout(layout, |ui| {
                for i in 0..b.pages.len() {
                    let (r, resp) = ui.allocate_exact_size(vec2(96.0, 112.0), egui::Sense::click());
                    let sel = i == b.current;
                    ui.painter().rect_filled(r, 2, if sel { p.accent_subtle } else { p.surface_sunken });
                    if let Some((h, _)) = self.thumbs.get(&i) {
                        let sz = h.size_vec2();
                        let img = Rect::from_center_size(pos2(r.center().x, r.top() + 48.0), sz);
                        ui.painter().image(h.id(), img, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                        ui.painter().rect_stroke(img, 0, Stroke::new(1.0, p.border_strong), egui::StrokeKind::Outside);
                    }
                    ui.painter().text(pos2(r.center().x, r.bottom() - 8.0), Align2::CENTER_CENTER, format!("{}", i + 1), egui::FontId::proportional(11.0), p.text);
                    if sel {
                        ui.painter().rect_stroke(r, 2, Stroke::new(2.0, p.focus_ring), egui::StrokeKind::Inside);
                    }
                    let resp = resp.on_hover_text(format!("Page {} · drag ← → to reorder", i + 1));
                    if resp.clicked() {
                        switch = Some(i);
                    }
                    if resp.dragged() {
                        let dx = resp.drag_delta().x;
                        if dx.abs() > 0.0 {
                            let forward = if rtl { dx < 0.0 } else { dx > 0.0 };
                            if resp.total_drag_delta().is_some_and(|d| d.x.abs() > 60.0) {
                                mv = Some((i, forward));
                            }
                        }
                    }
                }
            });
        });
        if let Some(i) = switch {
            if i != b.current {
                b.current = i;
                self.cache.clear();
                self.selected_balloon = None;
                self.selected_panel = None;
                self.active_layer = Some(b.page().ink_layer);
            }
        }
        if let Some((i, fwd)) = mv {
            let to = if fwd { (i + 1).min(b.pages.len() - 1) } else { i.saturating_sub(1) };
            if to != i {
                b.move_page(i, to);
                self.thumbs.clear();
                self.cache.clear();
            }
        }
    }

    fn timeline_panel(&mut self, ui: &mut Ui, _now: f64) {
        let p = palette(ui);
        let dt = ui.input(|i| i.stable_dt).min(0.1);
        let Some(b) = &mut self.book else { return };
        let page = &mut b.pages[b.current];
        let Some(tl) = page.timeline.as_mut() else { return };
        let mut frame = page.frame;
        let mut new_cel = false;
        let mut blank = false;
        ui.horizontal(|ui| {
            if ui.add(IconButton::new(icons::SKIP_BACK, "First frame").size(26.0)).clicked() {
                frame = 0;
            }
            if ui.add(IconButton::new(icons::CARET_LEFT, "Previous frame (,)").size(26.0)).clicked() {
                frame = frame.saturating_sub(1);
            }
            let play_icon = if self.playing { icons::PAUSE } else { icons::PLAY };
            if ui.add(IconButton::new(play_icon, "Play / pause (Enter)").selected(self.playing).size(26.0)).clicked() {
                self.playing = !self.playing;
            }
            if ui.add(IconButton::new(icons::CARET_RIGHT, "Next frame (.)").size(26.0)).clicked() {
                frame = (frame + 1).min(tl.frames - 1);
            }
            ui.separator();
            ui.label(RichText::new(format!("Frame {} / {}", frame + 1, tl.frames)).text_style(text::body_strong()));
            ui.add(egui::DragValue::new(&mut tl.fps).range(1.0..=60.0).suffix(" fps"));
            ui.add(egui::DragValue::new(&mut tl.frames).range(1..=9999).suffix(" frames"));
            ui.separator();
            if ui.add(Button::new("New cel here").icon(icons::PLUS).size(ButtonSize::Sm)).clicked() {
                new_cel = true;
            }
            if ui.add(Button::new("Blank").size(ButtonSize::Sm)).on_hover_text("Show nothing from this frame on").clicked() {
                blank = true;
            }
            ui.checkbox(&mut tl.onion.enabled, "Onion skin");
            ui.checkbox(&mut tl.looping, "Loop");
        });
        // Frame cells.
        let cells: Vec<CelCell> = (0..tl.frames)
            .map(|f| {
                if tl.is_key(0, f) {
                    match tl.cel_index(0, f) {
                        Some(i) => CelCell::Start(i as u16 + 1),
                        None => CelCell::Blank,
                    }
                } else if tl.cel_index(0, f).is_some() {
                    CelCell::Hold
                } else {
                    CelCell::Blank
                }
            })
            .collect();
        egui::ScrollArea::horizontal().id_salt("tl").show(ui, |ui| {
            let name = tl.tracks.first().map_or("Cel".to_string(), |t| t.name.clone());
            if let Some(f) = timeline_track(ui, &name, &cells, frame, true) {
                frame = f.min(tl.frames - 1);
                self.playing = false;
            }
        });
        if ui.input(|i| i.key_pressed(Key::Comma)) && !ui.ctx().egui_wants_keyboard_input() {
            frame = frame.saturating_sub(1);
        }
        if ui.input(|i| i.key_pressed(Key::Period)) && !ui.ctx().egui_wants_keyboard_input() {
            frame = (frame + 1).min(tl.frames - 1);
        }
        if new_cel {
            if let Some(id) = tl.new_cel(&mut page.doc, 0, frame) {
                self.active_layer = Some(id);
            }
        }
        if blank {
            tl.set_key(0, frame, None);
        }
        if self.playing {
            self.play_accum += dt;
            let d = tl.frame_duration();
            while self.play_accum >= d {
                self.play_accum -= d;
                frame = tl.next_frame(frame);
            }
            ui.ctx().request_repaint();
        }
        frame = frame.min(tl.frames.saturating_sub(1));
        if frame != page.frame || new_cel || blank {
            page.set_frame(frame);
            if let Some(c) = page.timeline.as_ref().and_then(|t| t.cel_at(0, frame)) {
                self.active_layer = Some(c);
            }
        }
        let _ = p;
    }

    // =========================================================================================
    // Canvas
    // =========================================================================================

    fn canvas(&mut self, ui: &mut Ui, now: f64) {
        let p = palette(ui);
        let rect = ui.max_rect();
        let resp = ui.allocate_rect(rect, egui::Sense::click_and_drag());
        let Some(book) = &mut self.book else {
            if self.new_sheet.is_none() {
                ui.painter().text(rect.center(), Align2::CENTER_CENTER, "Create a sheet to start (Ctrl+N)", egui::FontId::proportional(16.0), p.text_muted);
            }
            return;
        };
        let page_idx = book.current;
        let (w, h) = book.pages[page_idx].canvas();
        if self.fit_pending {
            self.view = View::fit(rect, w, h);
            self.fit_pending = false;
        }
        // ---- navigation ------------------------------------------------------------------------
        let (scroll, zoom_delta, space, pointer) = ui.input(|i| (i.smooth_scroll_delta, i.zoom_delta(), i.key_down(Key::Space), i.pointer.hover_pos()));
        if resp.hovered() {
            if let Some(ptr) = pointer {
                if zoom_delta != 1.0 {
                    self.view.zoom_at(ptr, zoom_delta);
                } else if scroll.y != 0.0 {
                    self.view.zoom_at(ptr, (scroll.y * 0.0025).exp());
                }
                if scroll.x != 0.0 {
                    self.view.pan(vec2(scroll.x, 0.0));
                }
            }
        }
        let middle = ui.input(|i| i.pointer.middle_down());
        let hand = self.tool == Tool::Hand || space || middle;
        if hand && resp.dragged() {
            self.view.pan(resp.drag_delta());
        }
        self.hover_canvas = pointer.filter(|p| rect.contains(*p)).map(|p| {
            let c = self.view.to_canvas(p);
            Pt::new(c.0, c.1)
        });

        // ---- pen / tool input ------------------------------------------------------------------
        if !hand && self.new_sheet.is_none() {
            let events = ui.input(|i| i.events.clone());
            let blocked = ui.ctx().is_pointer_over_egui() && !resp.hovered() && !resp.dragged();
            let pen_events = self.pen.process(&events, now, |pos| rect.contains(pos) && !blocked);
            for ev in pen_events {
                self.tool_event(ev, now);
            }
        }

        // ---- paint -----------------------------------------------------------------------------
        let book = self.book.as_mut().unwrap();
        let page = &mut book.pages[page_idx];
        let dirty = page.doc.take_dirty();
        self.cache.invalidate(dirty);
        let onion = match &page.timeline {
            Some(tl) if !self.playing => tl.onion_layers(page.frame),
            _ => Vec::new(),
        };
        if onion != self.cache.onion {
            self.cache.onion = onion;
            page.doc.mark_all_dirty();
            let d = page.doc.take_dirty();
            self.cache.invalidate(d);
        }
        let painter = ui.painter_at(rect);
        let page_rect = Rect::from_min_max(self.view.to_screen(0.0, 0.0), self.view.to_screen(w as f32, h as f32));
        painter.rect_filled(page_rect.translate(vec2(3.0, 4.0)), 0, Color32::from_black_alpha(80));
        if page.doc.paper.a() == 0 {
            painter.rect_filled(page_rect, 0, Color32::from_gray(205));
        }
        self.cache.paint(ui.ctx(), &painter, &page.doc, &self.view, rect, 96);

        // Print guides.
        if book.settings.kind.has_print_guides() {
            let (tlo, thi) = book.settings.trim_rect();
            let (slo, shi) = book.settings.safe_rect();
            for (lo, hi, c) in [(tlo, thi, p.overlay_trim), (slo, shi, p.overlay_safe)] {
                let r = Rect::from_min_max(self.view.to_screen(lo.x, lo.y), self.view.to_screen(hi.x, hi.y));
                dashed_rect(&painter, r, c);
            }
        }
        let page = &book.pages[page_idx];
        // Tool overlays.
        match self.tool {
            Tool::Panel => {
                for (i, pn) in page.panels.panels.iter().enumerate() {
                    let pts: Vec<Pos2> = pn.poly.iter().map(|q| self.view.to_screen(q.x, q.y)).collect();
                    let sel = Some(i) == self.selected_panel;
                    painter.add(egui::Shape::closed_line(pts.clone(), Stroke::new(if sel { 2.5 } else { 1.0 }, if sel { p.focus_ring } else { p.overlay_frame_preview })));
                }
                if let Some((a, b2)) = self.panel_drag {
                    let (sa, sb) = (self.view.to_screen(a.x, a.y), self.view.to_screen(b2.x, b2.y));
                    match self.panel_mode {
                        PanelMode::Divide => dual_stroke_line(&painter, sa, sb, p.overlay_frame_preview),
                        _ => dashed_rect(&painter, Rect::from_two_pos(sa, sb), p.overlay_frame_preview),
                    }
                }
            }
            Tool::Balloon => {
                let zoom = self.view.zoom;
                for bl in &page.balloons {
                    let selected = Some(bl.id) == self.selected_balloon;
                    let dragging = self.balloon_drag.as_ref().is_some_and(|d| d.id == bl.id);
                    if dragging {
                        // Live preview while dragging (the raster updates on release).
                        let pts: Vec<Pos2> = bl.outline().iter().map(|q| self.view.to_screen(q.x, q.y)).collect();
                        painter.add(egui::Shape::closed_line(pts, Stroke::new(2.0, p.overlay_frame_preview)));
                        if let Some(t) = bl.tail_polygon() {
                            let pts: Vec<Pos2> = t.iter().map(|q| self.view.to_screen(q.x, q.y)).collect();
                            painter.add(egui::Shape::closed_line(pts, Stroke::new(2.0, p.overlay_frame_preview)));
                        }
                        for (c, r) in bl.tail_bubbles() {
                            painter.circle_stroke(self.view.to_screen(c.x, c.y), r * zoom, Stroke::new(2.0, p.overlay_frame_preview));
                        }
                    }
                    if selected || dragging {
                        let (c, hh) = (bl.center, bl.half);
                        let r = Rect::from_min_max(self.view.to_screen(c.x - hh.x, c.y - hh.y), self.view.to_screen(c.x + hh.x, c.y + hh.y));
                        dashed_rect(&painter, r, p.focus_ring);
                        for corner in [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom()] {
                            handle(ui, corner, false);
                        }
                        if bl.tail != TailStyle::None {
                            let t = self.view.to_screen(bl.tail_tip.x, bl.tail_tip.y);
                            painter.circle(t, 6.0, p.overlay_handle, Stroke::new(1.5, p.focus_ring));
                        }
                    }
                }
            }
            Tool::Brush | Tool::Eraser => {
                if let Some(ptr) = pointer.filter(|p| rect.contains(*p)) {
                    let r = (self.size_of(self.current_preset_id()) * 0.5 * self.view.zoom).max(1.5);
                    painter.circle_stroke(ptr, r, Stroke::new(2.5, Color32::from_white_alpha(140)));
                    painter.circle_stroke(ptr, r, Stroke::new(1.0, Color32::from_black_alpha(200)));
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
                }
            }
            Tool::Hand => ui.ctx().set_cursor_icon(egui::CursorIcon::Grab),
            Tool::Fill => ui.ctx().set_cursor_icon(egui::CursorIcon::Cell),
        }
        if self.stroke.is_some() {
            ui.ctx().request_repaint();
        }
        let _ = icon_font;
    }

    fn tool_event(&mut self, ev: PenEvent, now: f64) {
        let to_pt = |view: &View, s: &PenSample| {
            let c = view.to_canvas(s.pos);
            Pt::new(c.0, c.1)
        };
        match self.tool {
            Tool::Brush | Tool::Eraser => self.brush_event(ev, now),
            Tool::Fill => {
                if let PenEvent::Down(s) = ev {
                    let pt = to_pt(&self.view, &s);
                    let color = to_rgba(self.color.to_color32());
                    let Some(target) = self.target_layer(now) else { return };
                    let book = self.book.as_mut().unwrap();
                    let page = book.page_mut();
                    match ink_raster::fill::fill(&mut page.doc, target, pt.x as i32, pt.y as i32, color, &self.fill) {
                        Some(e) => page.push_doc(e),
                        None => self.toasts.push((ToastKind::Info, "Nothing to fill there".into(), now)),
                    }
                }
            }
            Tool::Panel => {
                let mode = self.panel_mode;
                let pt = match ev {
                    PenEvent::Down(s) | PenEvent::Move(s) | PenEvent::Up(s) => to_pt(&self.view, &s),
                };
                let book = self.book.as_mut().unwrap();
                let page = book.page_mut();
                match ev {
                    PenEvent::Down(_) => self.panel_drag = Some((pt, pt)),
                    PenEvent::Move(_) => {
                        if let Some(d) = &mut self.panel_drag {
                            d.1 = pt;
                        }
                    }
                    PenEvent::Up(_) => {
                        let Some((a, b)) = self.panel_drag.take() else { return };
                        let small = a.sub(b).len() * self.view.zoom < 6.0;
                        if small || mode == PanelMode::Select {
                            self.selected_panel = page.panels.hit(b);
                        } else if mode == PanelMode::Rectangle {
                            page.push_objects();
                            page.panels.add_rect(a, b);
                            self.selected_panel = Some(page.panels.panels.len() - 1);
                            page.render_panels();
                        } else {
                            let mid = a.lerp(b, 0.5);
                            match page.panels.hit(mid) {
                                Some(i) => {
                                    page.push_objects();
                                    if page.panels.divide(i, a, b) {
                                        page.render_panels();
                                    }
                                }
                                None => self.toasts.push((ToastKind::Info, "Drag the cut across a panel".into(), now)),
                            }
                        }
                    }
                }
            }
            Tool::Balloon => self.balloon_event(ev, now),
            Tool::Hand => {}
        }
    }

    /// The layer a paint tool writes to (cel for animation pages). None if not paintable.
    fn target_layer(&mut self, now: f64) -> Option<LayerId> {
        let book = self.book.as_mut()?;
        let page = book.page_mut();
        let mut target = self.active_layer.filter(|id| page.doc.layer(*id).is_some()).unwrap_or(page.ink_layer);
        if let Some(tl) = page.timeline.as_mut() {
            let is_cel = tl.tracks.iter().any(|t| t.cels.contains(&target));
            if is_cel {
                let frame = page.frame;
                target = tl.drawing_cel(&mut page.doc, 0, frame)?;
                tl.apply(&mut page.doc, frame);
                self.active_layer = Some(target);
            }
        }
        if page.is_object_layer(target) {
            self.toasts.push((ToastKind::Warning, "This layer holds panels or balloons. Select a drawing layer.".into(), now));
            return None;
        }
        if page.doc.layer(target).is_some_and(|l| l.props.locked) {
            self.toasts.push((ToastKind::Warning, "The layer is locked.".into(), now));
            return None;
        }
        Some(target)
    }

    fn brush_event(&mut self, ev: PenEvent, now: f64) {
        let sample = |view: &View, s: &PenSample| {
            let c = view.to_canvas(s.pos);
            let mut smp = Sample::new(c.0, c.1, s.pressure);
            smp.time = s.time;
            smp
        };
        match ev {
            PenEvent::Down(s) => {
                let Some(target) = self.target_layer(now) else { return };
                let id = self.current_preset_id();
                let mut params = self.preset(id).params.clone();
                let dpi = self.book.as_ref().unwrap().settings.dpi as f32;
                let size = self.size_of(id);
                let k = size / params.size.max(1e-3);
                params.size = size;
                // Taper lengths scale with the brush; other px settings with the DPI.
                params.taper_start *= k;
                params.taper_end *= k;
                let _ = dpi;
                if let Some(st) = self.stabilization.get(id) {
                    params.stabilization = *st;
                }
                let color = to_rgba(self.color.to_color32());
                let smp = sample(&self.view, &s);
                let book = self.book.as_mut().unwrap();
                let page = book.page_mut();
                let mut st = BrushStroke::begin(&page.doc, target, params, color);
                st.add(&mut page.doc, smp);
                self.stroke = Some(st);
            }
            PenEvent::Move(s) => {
                let smp = sample(&self.view, &s);
                if let (Some(st), Some(book)) = (&mut self.stroke, &mut self.book) {
                    st.add(&mut book.pages[book.current].doc, smp);
                }
            }
            PenEvent::Up(s) => {
                let smp = sample(&self.view, &s);
                if let (Some(mut st), Some(book)) = (self.stroke.take(), &mut self.book) {
                    let page = book.page_mut();
                    st.add(&mut page.doc, smp);
                    let e = st.end(&mut page.doc);
                    page.push_doc(e);
                }
            }
        }
    }

    fn balloon_event(&mut self, ev: PenEvent, now: f64) {
        let Some(book) = self.book.as_mut() else { return };
        let dpi = book.settings.dpi;
        let page = book.page_mut();
        let (s, phase) = match ev {
            PenEvent::Down(s) => (s, 0),
            PenEvent::Move(s) => (s, 1),
            PenEvent::Up(s) => (s, 2),
        };
        let c = self.view.to_canvas(s.pos);
        let pt = Pt::new(c.0, c.1);
        let slop = 10.0 / self.view.zoom;
        match phase {
            0 => {
                // Topmost balloon under the pointer (selected one first, so its handles win).
                let mut order: Vec<usize> = (0..page.balloons.len()).rev().collect();
                if let Some(sel) = self.selected_balloon.and_then(|id| page.balloons.iter().position(|b| b.id == id)) {
                    order.retain(|i| *i != sel);
                    order.insert(0, sel);
                }
                let hit = order.into_iter().find_map(|i| page.balloons[i].hit(pt, slop).map(|h| (i, h)));
                match hit {
                    Some((i, h)) => {
                        let bl = page.balloons[i].clone();
                        self.selected_balloon = Some(bl.id);
                        self.balloon_style = bl.style;
                        page.render_balloons(&self.fonts, Some(bl.id));
                        self.balloon_drag = Some(BalloonDrag { id: bl.id, hit: h, start: pt, orig: bl, moved: false });
                    }
                    None => {
                        page.push_objects();
                        let id = page.new_balloon_id();
                        let mut bl = Balloon::new(id, self.balloon_style, pt, dpi, if self.balloon_dir == TextDir::Vertical { "セリフ" } else { "Text" }, self.balloon_dir);
                        bl.font_px = self.balloon_pt * dpi as f32 / 72.0;
                        bl.fit_to_text(&self.fonts);
                        let off = bl.half.y * 1.6;
                        bl.tail_tip = Pt::new(pt.x - bl.half.x * 0.4, pt.y + off);
                        page.balloons.push(bl);
                        page.render_balloons(&self.fonts, None);
                        self.selected_balloon = Some(id);
                        self.focus_text = true;
                        self.text_snapshot = true;
                        if !self.hinted_balloon {
                            self.hinted_balloon = true;
                            self.toasts.push((ToastKind::Info, "Balloon placed. Type the text in the Text palette.".into(), now));
                        }
                    }
                }
            }
            1 => {
                if let Some(d) = &mut self.balloon_drag {
                    if !d.moved && pt.sub(d.start).len() * self.view.zoom > 3.0 {
                        d.moved = true;
                        // Snapshot for undo (the state before this drag).
                        let mut before = page.balloons.clone();
                        if let Some(b) = before.iter_mut().find(|b| b.id == d.id) {
                            *b = d.orig.clone();
                        }
                        let cur = std::mem::replace(&mut page.balloons, before);
                        page.push_objects();
                        page.balloons = cur;
                    }
                    if let Some(bl) = page.balloons.iter_mut().find(|b| b.id == d.id) {
                        *bl = d.orig.clone();
                        match d.hit {
                            BalloonHit::Body => bl.translate(pt.sub(d.start)),
                            BalloonHit::Tail => bl.tail_tip = pt,
                            BalloonHit::Corner(k) => bl.drag_corner(k, pt),
                        }
                    }
                }
            }
            _ => {
                if self.balloon_drag.take().is_some() {
                    page.render_balloons(&self.fonts, None);
                }
            }
        }
    }

    // =========================================================================================
    // Export
    // =========================================================================================

    fn export_dir(&self) -> std::path::PathBuf {
        if let Some(d) = &self.export_to {
            return d.clone();
        }
        if let Ok(d) = std::env::var("INKSTUDIO_EXPORT_DIR") {
            return d.into();
        }
        let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"));
        match home {
            Some(h) => std::path::PathBuf::from(h).join("Pictures").join("Inkstudio"),
            None => std::path::PathBuf::from("."),
        }
    }

    pub fn export_pages(&mut self, all: bool, now: f64) {
        let dir = self.export_dir();
        let Some(book) = &self.book else { return };
        if let Err(e) = std::fs::create_dir_all(&dir) {
            self.toast(ToastKind::Error, format!("Couldn't create {}: {e}", dir.display()), now);
            return;
        }
        let name = sanitize(&book.settings.name);
        let mode = book.settings.color_mode;
        let range: Vec<usize> = if all { (0..book.pages.len()).collect() } else { vec![book.current] };
        let mut last = None;
        for i in range {
            let pg = &book.pages[i];
            let rgba = pg.doc.flatten_rgba8();
            let path = dir.join(format!("{name}-p{:02}.png", i + 1));
            if let Err(e) = write_png(&path, pg.doc.width, pg.doc.height, &rgba, mode) {
                self.toast(ToastKind::Error, format!("Export failed: {e}"), now);
                return;
            }
            last = Some(path);
        }
        if let Some(p) = last {
            self.toast(ToastKind::Success, format!("Exported to {}", p.display()), now);
            self.last_export = Some(p);
        }
    }

    pub fn export_gif(&mut self, now: f64) {
        let dir = self.export_dir();
        let Some(book) = &mut self.book else { return };
        let name = sanitize(&book.settings.name);
        let page = book.page_mut();
        let Some(tl) = page.timeline.clone() else { return };
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join(format!("{name}.gif"));
        let scale = ((page.doc.width.max(page.doc.height) as f32 / 1280.0).ceil() as u32).max(1);
        let res = std::fs::File::create(&path).map_err(|e| e.to_string()).and_then(|f| ink_anim::export_gif(&mut page.doc, &tl, scale, std::io::BufWriter::new(f)));
        let frame = page.frame;
        page.set_frame(frame);
        match res {
            Ok(()) => {
                self.toast(ToastKind::Success, format!("Exported {}", path.display()), now);
                self.last_export = Some(path);
            }
            Err(e) => self.toast(ToastKind::Error, format!("GIF export failed: {e}"), now),
        }
    }
}

fn b_dpi(page: &crate::book::Page) -> f32 {
    page.doc.dpi as f32
}

pub fn hsv_from(c: Color32) -> Hsv {
    let (r, g, b) = (c.r() as f32 / 255.0, c.g() as f32 / 255.0, c.b() as f32 / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d == 0.0 {
        0.0
    } else if max == r {
        ((g - b) / d).rem_euclid(6.0) / 6.0
    } else if max == g {
        ((b - r) / d + 2.0) / 6.0
    } else {
        ((r - g) / d + 4.0) / 6.0
    };
    Hsv { h, s: if max == 0.0 { 0.0 } else { d / max }, v: max }
}

fn sanitize(s: &str) -> String {
    let t: String = s.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
    if t.is_empty() { "inkstudio".into() } else { t }
}

fn dashed_rect(painter: &egui::Painter, r: Rect, c: Color32) {
    let pts = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
    for w in pts.windows(2) {
        painter.add(egui::Shape::dashed_line(&[w[0], w[1]], Stroke::new(1.0, c), 6.0, 4.0));
    }
}

/// Small preview of a balloon style for the picker.
fn draw_style_icon(painter: &egui::Painter, r: Rect, style: BalloonStyle) {
    let mut b = Balloon::new(1, style, Pt::new(0.0, 0.0), 72, "", TextDir::Horizontal);
    b.half = Pt::new(1.0, 0.62);
    b.tail_tip = Pt::new(-0.55, 1.25);
    b.font_px = 0.3;
    let (c, k) = (r.center() - vec2(0.0, 4.0), (r.width() / 2.6).min(r.height() / 2.4));
    let map = |q: &Pt| pos2(c.x + q.x * k, c.y + q.y * k);
    let ink = Color32::from_gray(25);
    let line = Stroke::new(1.4, ink);
    if style == BalloonStyle::TextOnly {
        painter.text(c, Align2::CENTER_CENTER, "Aa あ", egui::FontId::proportional(16.0), Color32::from_gray(220));
        return;
    }
    let body: Vec<Pos2> = b.outline().iter().map(map).collect();
    // White fill: a fan from the center works for every star-shaped outline.
    let mut mesh = egui::epaint::Mesh::default();
    mesh.colored_vertex(c, Color32::WHITE);
    for q in &body {
        mesh.colored_vertex(*q, Color32::WHITE);
    }
    let n = body.len() as u32;
    for i in 0..n {
        mesh.add_triangle(0, 1 + i, 1 + (i + 1) % n);
    }
    if let Some(t) = b.tail_polygon() {
        let tp: Vec<Pos2> = t.iter().map(map).collect();
        painter.add(egui::Shape::convex_polygon(tp.clone(), Color32::WHITE, line));
    }
    painter.add(egui::Shape::mesh(mesh));
    if style == BalloonStyle::Whisper {
        let mut closed = body.clone();
        closed.push(body[0]);
        for s in egui::Shape::dashed_line(&closed, line, 4.0, 3.0) {
            painter.add(s);
        }
    } else {
        painter.add(egui::Shape::closed_line(body, line));
    }
    for (cc, rr) in b.tail_bubbles() {
        painter.circle(map(&cc), rr * k, Color32::WHITE, line);
    }
}

/// Downscaled page thumbnail (`width` px wide).
fn thumbnail(doc: &ink_core::Document, width: f32) -> egui::ColorImage {
    let k = width / doc.width as f32;
    let (tw, th) = ((doc.width as f32 * k).round().max(1.0) as usize, (doc.height as f32 * k).round().max(1.0) as usize);
    let step = (1.0 / k).max(1.0);
    // Composite only the tiles we sample, at a coarse grid.
    let mut cache: HashMap<(i32, i32), ink_core::Tile> = HashMap::new();
    let mut px = Vec::with_capacity(tw * th);
    for y in 0..th {
        for x in 0..tw {
            let (cx, cy) = (((x as f32 + 0.5) * step) as i32, ((y as f32 + 0.5) * step) as i32);
            let (key, lx, ly) = ink_core::tile::split(cx.min(doc.width as i32 - 1), cy.min(doc.height as i32 - 1));
            let t = cache.entry(key).or_insert_with(|| doc.composite_tile(key, true));
            let c = t.get(lx, ly).to_straight();
            px.push(Color32::from_rgba_unmultiplied(c[0], c[1], c[2], 255));
        }
    }
    egui::ColorImage { size: [tw, th], source_size: vec2(tw as f32, th as f32), pixels: px }
}

fn write_png(path: &std::path::Path, w: u32, h: u32, rgba: &[u8], mode: ColorMode) -> Result<(), String> {
    let f = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(f), w, h);
    let data: Vec<u8> = match mode {
        ColorMode::Color => {
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            rgba.to_vec()
        }
        ColorMode::Gray => {
            enc.set_color(png::ColorType::Grayscale);
            enc.set_depth(png::BitDepth::Eight);
            rgba.chunks_exact(4).map(|c| ((c[0] as u32 * 54 + c[1] as u32 * 183 + c[2] as u32 * 19) >> 8) as u8).collect()
        }
        ColorMode::Mono => {
            // 1-bit for print: threshold at 50% luminance.
            enc.set_color(png::ColorType::Grayscale);
            enc.set_depth(png::BitDepth::One);
            let row_bytes = (w as usize).div_ceil(8);
            let mut out = vec![0u8; row_bytes * h as usize];
            for y in 0..h as usize {
                for x in 0..w as usize {
                    let c = &rgba[(y * w as usize + x) * 4..];
                    let l = (c[0] as u32 * 54 + c[1] as u32 * 183 + c[2] as u32 * 19) >> 8;
                    if l >= 128 {
                        out[y * row_bytes + x / 8] |= 0x80 >> (x % 8);
                    }
                }
            }
            out
        }
    };
    enc.write_header().map_err(|e| e.to_string())?.write_image_data(&data).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hsv_roundtrip() {
        for c in [Color32::from_rgb(200, 30, 90), Color32::from_rgb(10, 200, 40), Color32::WHITE, Color32::BLACK] {
            let back = hsv_from(c).to_color32();
            for (a, b) in [(c.r(), back.r()), (c.g(), back.g()), (c.b(), back.b())] {
                assert!(a.abs_diff(b) <= 1, "{c:?} -> {back:?}");
            }
        }
    }

    #[test]
    fn mono_png_is_one_bit() {
        let dir = std::env::temp_dir().join("inkstudio-test-png");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("m.png");
        let rgba: Vec<u8> = (0..16 * 4).flat_map(|i| if i % 2 == 0 { [0, 0, 0, 255] } else { [255, 255, 255, 255] }).collect();
        write_png(&path, 16, 4, &rgba, ColorMode::Mono).unwrap();
        let dec = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(&path).unwrap()));
        let r = dec.read_info().unwrap();
        assert_eq!(r.info().bit_depth, png::BitDepth::One);
    }
}
