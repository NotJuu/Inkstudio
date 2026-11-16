//! The Inkstudio window: menus, tool column, dockable palettes, canvas, files.

use crate::book::{Book, ColorMode, SheetKind, SheetSettings};
use crate::canvas::{TileCache, View};
use crate::files::{self, FileDialog};
use crate::new_sheet::{NewSheet, Outcome};
use crate::pen::{PenEvent, PenInput, PenSample};
use crate::settings::Settings;
use egui::{Align2, Color32, Key, Pos2, Rect, RichText, Stroke, Ui, ViewportCommand, pos2, vec2};
use egui_dock::{DockArea, DockState};
use ink_comic::{Balloon, BalloonHit, BalloonStyle, Fonts, PanelTemplate, TailStyle, TextDir};
use ink_core::{Layer, LayerId, Rgba, UndoEntry, blend::BlendMode};
use ink_raster::fill::{FillOptions, Refer};
use ink_raster::presets::{Category, Preset, catalog};
use ink_raster::{Sample, Stroke as BrushStroke};
use ink_ui::i18n::{self, Lang};
use ink_ui::icons;
use ink_ui::theme::{self, palette, text, tokens};
use ink_ui::tr;
use ink_ui::widgets::*;
use ink_vector::Pt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

mod adjust;
mod book_ui;
mod coloring;
mod reader;
mod rulers;
mod timeline;
mod transform;

pub use coloring::GradientMode;
pub use rulers::RulerKind;

/// Replace `{key}` placeholders in a translated template.
fn trf(template: &str, args: &[(&str, String)]) -> String {
    let mut s = tr(template).to_string();
    for (k, v) in args {
        s = s.replace(&format!("{{{k}}}"), v);
    }
    s
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Brush,
    Eraser,
    Fill,
    LassoFill,
    Gradient,
    Eyedropper,
    Ruler,
    Transform,
    Panel,
    Balloon,
    Hand,
    Rotate,
}

impl Tool {
    const ALL: [(Tool, &'static str, &'static str, Key); 12] = [
        (Tool::Brush, icons::PEN_NIB, "Brush (B)", Key::B),
        (Tool::Eraser, icons::ERASER, "Eraser (E)", Key::E),
        (Tool::Fill, icons::PAINT_BUCKET, "Fill (G)", Key::G),
        (Tool::LassoFill, icons::LASSO, "Lasso fill (L)", Key::L),
        (Tool::Gradient, icons::GRADIENT, "Gradient (D)", Key::D),
        (
            Tool::Eyedropper,
            icons::EYEDROPPER,
            "Eyedropper (I / hold Alt)",
            Key::I,
        ),
        (
            Tool::Ruler,
            icons::RULER,
            "Perspective & symmetry rulers (U)",
            Key::U,
        ),
        (
            Tool::Transform,
            icons::ARROWS_OUT_CARDINAL,
            "Move & transform (V / Ctrl+T)",
            Key::V,
        ),
        (Tool::Panel, icons::SQUARES_FOUR, "Panel frames (F)", Key::F),
        (
            Tool::Balloon,
            icons::CHAT_CIRCLE,
            "Balloons & text (T)",
            Key::T,
        ),
        (Tool::Hand, icons::HAND, "Hand (H / hold Space)", Key::H),
        (
            Tool::Rotate,
            icons::ARROWS_CLOCKWISE,
            "Rotate view (R + drag)",
            Key::R,
        ),
    ];
}

/// Dockable palettes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Pane {
    Canvas,
    SubTool,
    ToolProperty,
    Color,
    Layers,
    Navigator,
    Pages,
    Timeline,
}

impl Pane {
    pub const PALETTES: [Pane; 7] = [
        Pane::SubTool,
        Pane::ToolProperty,
        Pane::Color,
        Pane::Layers,
        Pane::Navigator,
        Pane::Pages,
        Pane::Timeline,
    ];
    pub fn title(self) -> &'static str {
        match self {
            Pane::Canvas => "Canvas",
            Pane::SubTool => "Sub tool",
            Pane::ToolProperty => "Tool property",
            Pane::Color => "Color",
            Pane::Layers => "Layer",
            Pane::Navigator => "Navigator",
            Pane::Pages => "Pages",
            Pane::Timeline => "Timeline",
        }
    }
}

/// The default arrangement: tools left, color/navigator/layers right, pages + timeline below.
pub fn default_dock() -> DockState<Pane> {
    crate::layouts::builtin(crate::layouts::STANDARD).expect("standard layout")
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

/// Page strip context-menu commands.
#[derive(Clone, Copy)]
enum PageAction {
    Info,
    Read,
    Special(crate::book::PageRole),
}

/// What to do once unsaved changes are dealt with.
#[derive(Clone, Debug, PartialEq)]
enum Pending {
    New,
    Open(Option<PathBuf>),
    Quit,
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
    sizes: HashMap<&'static str, f32>,
    opacity: HashMap<&'static str, f32>,
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
    thumbs: HashMap<usize, egui::TextureHandle>,
    nav_thumb: Option<(egui::TextureHandle, usize)>,
    thumb_gen: u64,
    pub last_export: Option<PathBuf>,
    pub export_to: Option<PathBuf>,
    pub color_tab: usize,
    hinted_balloon: bool,
    // ---- shell ----
    pub dock: DockState<Pane>,
    pub settings: Settings,
    pub config_dir: PathBuf,
    settings_dirty: bool,
    dock_saved: String,
    frame_count: u64,
    file_dialog: Option<(FileDialog, Option<Pending>)>,
    confirm: Option<Pending>,
    allow_close: bool,
    quit_requested: bool,
    title: String,
    pub canvas_rect: Rect,
    rotate_drag: Option<f32>,
    /// Name of the attached tablet backend (macOS / Linux), for the status bar.
    pub tablet_name: Option<&'static str>,
    // ---- autosave ----
    autosave_job: Option<AutosaveJob>,
    frame_time: f64,
    last_autosave: f64,
    /// UI time of the last finished autosave (status bar).
    pub autosaved_at: Option<f64>,
    /// Recovery copies found at start-up, offered in a dialog.
    pub recovery: Vec<crate::recovery::Entry>,
    /// Name being typed in the "Save layout as" dialog.
    pub layout_name: Option<String>,
    // ---- rulers ----
    ruler_drag: Option<usize>,
    ruler_snap: Option<ink_vector::RulerSnap>,
    pub ruler_new: rulers::RulerKind,
    // ---- coloring ----
    lasso: Vec<Pt>,
    gradient: Option<(Pt, Pt)>,
    pub gradient_mode: coloring::GradientMode,
    pub color_set: usize,
    /// Display the canvas in greys to check values (concept art).
    pub value_check: bool,
    /// Brush tool temporarily picking colors while Alt is held.
    alt_pick: bool,
    alt_held: bool,
    // ---- book ----
    pub book_info: Option<crate::book::BookInfo>,
    pub reader: Option<reader::Reader>,
    // ---- animation ----
    pub anim_track: usize,
    rename_track: Option<(usize, String)>,
    // ---- layers & images ----
    pub transform: Option<transform::Transform>,
    pub adjust: Option<adjust::AdjustDialog>,
    pub rename_layer: Option<(LayerId, String)>,
    shift_held: bool,
    opacity_drag: Option<Vec<(LayerId, Layer)>>,
}

/// An autosave running on a background thread.
struct AutosaveJob {
    handle: std::thread::JoinHandle<Result<(), String>>,
    generation: u64,
    project: Option<PathBuf>,
    recovery_id: u64,
}

fn to_rgba(c: Color32) -> Rgba {
    Rgba::from_straight(c.r(), c.g(), c.b(), 255)
}

impl InkApp {
    pub fn new(ctx: &egui::Context) -> Self {
        Self::with_config(ctx, crate::settings::config_dir())
    }

    pub fn with_config(ctx: &egui::Context, config_dir: PathBuf) -> Self {
        theme::apply(ctx, theme::Mode::Dark);
        let settings = Settings::load_from(&config_dir);
        let lang = settings
            .lang
            .as_deref()
            .and_then(Lang::from_code)
            .unwrap_or_else(i18n::system_lang);
        i18n::set_lang(lang);
        let mut settings = settings;
        // Older settings kept a single layout; it becomes the saved state of "standard".
        if let Some(old) = settings.dock_ron.take() {
            settings
                .layouts
                .entry(crate::layouts::STANDARD.into())
                .or_insert(old);
        }
        let dock = load_layout(&settings, &settings.active_layout.clone());
        let recovery = crate::recovery::list(&config_dir);
        let mut pen = PenInput::default();
        pen.simulate = settings.simulate_pressure;
        Self {
            book: None,
            new_sheet: if recovery.is_empty() {
                Some(NewSheet::new(false))
            } else {
                None
            },
            fonts: Fonts::system(),
            view: View::default(),
            fit_pending: true,
            cache: TileCache::default(),
            pen,
            tool: Tool::Brush,
            presets: catalog(),
            brush_id: "g_pen",
            eraser_id: "eraser_hard",
            sizes: HashMap::new(),
            opacity: HashMap::new(),
            stabilization: HashMap::new(),
            color: Hsv {
                h: 0.0,
                s: 0.0,
                v: 0.07,
            },
            sub_color: Rgba::WHITE,
            active_layer: None,
            stroke: None,
            fill: FillOptions {
                refer: Refer::References,
                tolerance: 40,
                gap: 4,
                expand: 2,
            },
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
            nav_thumb: None,
            thumb_gen: 0,
            last_export: None,
            export_to: None,
            color_tab: 0,
            hinted_balloon: false,
            dock_saved: ron::to_string(&dock).unwrap_or_default(),
            dock,
            settings,
            config_dir,
            settings_dirty: false,
            frame_count: 0,
            file_dialog: None,
            confirm: None,
            allow_close: false,
            quit_requested: false,
            title: String::new(),
            canvas_rect: Rect::NOTHING,
            rotate_drag: None,
            tablet_name: None,
            autosave_job: None,
            frame_time: 0.0,
            last_autosave: 0.0,
            autosaved_at: None,
            recovery,
            layout_name: None,
            ruler_drag: None,
            ruler_snap: None,
            ruler_new: rulers::RulerKind::TwoPoint,
            lasso: Vec::new(),
            gradient: None,
            gradient_mode: coloring::GradientMode::MainToSub,
            color_set: 0,
            value_check: false,
            alt_pick: false,
            alt_held: false,
            book_info: None,
            reader: None,
            anim_track: 0,
            rename_track: None,
            transform: None,
            adjust: None,
            rename_layer: None,
            shift_held: false,
            opacity_drag: None,
        }
    }

    /// Open a new book (dialog result, tests, screenshots).
    pub fn open(&mut self, s: SheetSettings) {
        let front = s.front_matter && s.kind == SheetKind::Book;
        let mut book = Book::new(s);
        if front {
            for role in book_ui::COMPLETE {
                book.add_special(role, &self.fonts);
            }
            book.current = 0;
            book.structure_dirty = false;
        }
        self.install(book);
    }

    fn install(&mut self, book: Book) {
        self.active_layer = Some(book.page().ink_layer);
        if let Some(tl) = &book.page().timeline {
            self.active_layer = tl.cel_at(0, book.page().frame).or(self.active_layer);
        }
        let kind = book.settings.kind;
        self.book = Some(book);
        self.new_sheet = None;
        self.last_autosave = self.frame_time;
        if self.settings.auto_layout {
            let id = self
                .settings
                .kind_layouts
                .get(crate::layouts::kind_key(kind))
                .cloned()
                .unwrap_or_else(|| crate::layouts::for_kind(kind).to_string());
            if id != self.settings.active_layout && self.layout_exists(&id) {
                self.switch_layout(&id);
            }
        }
        self.fit_pending = true;
        self.cache.clear();
        self.thumbs.clear();
        self.nav_thumb = None;
        self.selected_balloon = None;
        self.selected_panel = None;
        self.playing = false;
    }

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

    pub fn set_lang(&mut self, l: Lang) {
        i18n::set_lang(l);
        self.settings.lang = Some(l.code().into());
        self.settings_dirty = true;
    }

    fn toast(&mut self, kind: ToastKind, msg: impl Into<String>, now: f64) {
        self.toasts.push((kind, msg.into(), now));
    }

    fn preset(&self, id: &str) -> &Preset {
        self.presets
            .iter()
            .find(|p| p.id == id)
            .unwrap_or(&self.presets[0])
    }

    fn current_preset_id(&self) -> &'static str {
        if self.tool == Tool::Eraser {
            self.eraser_id
        } else {
            self.brush_id
        }
    }

    fn size_of(&self, id: &'static str) -> f32 {
        let dpi = self.book.as_ref().map_or(600, |b| b.settings.dpi) as f32;
        *self
            .sizes
            .get(id)
            .unwrap_or(&(self.preset(id).params.size * dpi / 600.0))
    }

    fn mm_per_px(&self) -> f32 {
        25.4 / self.book.as_ref().map_or(600, |b| b.settings.dpi) as f32
    }

    pub fn is_dirty(&self) -> bool {
        self.book.as_ref().is_some_and(|b| b.is_dirty())
    }

    fn book_name(&self) -> String {
        match &self.book {
            Some(b) => b
                .path
                .as_ref()
                .and_then(|p| p.file_stem())
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| b.settings.name.clone()),
            None => String::new(),
        }
    }

    // =========================================================================================
    // Top level
    // =========================================================================================

    pub fn ui(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let now = ctx.input(|i| i.time);
        self.frame_time = now;
        self.frame_count += 1;
        self.handle_close_request(&ctx);
        self.shortcuts(&ctx, now);
        // Switching tool or page finishes a transform, as in other painting apps.
        if let Some(t) = &self.transform {
            if self.tool != Tool::Transform
                || self.book.as_ref().is_some_and(|b| b.current != t.page())
            {
                self.commit_transform();
            }
        }
        self.handle_dropped_files(&ctx, now);
        self.autosave_tick(now);
        self.update_title(&ctx);
        let p = palette(ui);

        egui::Panel::top("menu")
            .exact_size(tokens::layout::MENU_BAR_H)
            .frame(
                egui::Frame::new()
                    .fill(p.bg)
                    .inner_margin(egui::Margin::symmetric(4, 0)),
            )
            .show(ui, |ui| {
                self.menu_bar(ui, now);
            });
        egui::Panel::top("options")
            .exact_size(tokens::layout::COMMAND_BAR_H + 4.0)
            .frame(
                egui::Frame::new()
                    .fill(p.surface)
                    .inner_margin(egui::Margin::symmetric(8, 0)),
            )
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| self.tool_options(ui));
            });
        egui::Panel::bottom("status")
            .exact_size(tokens::layout::STATUS_BAR_H)
            .frame(
                egui::Frame::new()
                    .fill(p.bg)
                    .inner_margin(egui::Margin::symmetric(10, 0)),
            )
            .show(ui, |ui| {
                self.status_bar(ui);
            });
        egui::Panel::left("tools")
            .exact_size(tokens::layout::TOOL_COLUMN_W + 4.0)
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(p.surface)
                    .inner_margin(egui::Margin::symmetric(4, 6)),
            )
            .show(ui, |ui| {
                self.tool_column(ui);
            });
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(p.bg))
            .show(ui, |ui| {
                let mut dock = std::mem::replace(&mut self.dock, DockState::new(vec![]));
                let mut style = egui_dock::Style::from_egui(ui.style());
                style.tab_bar.bg_fill = p.bg;
                style.tab_bar.height = tokens::layout::PALETTE_TAB_H;
                style.separator.width = 3.0;
                style.separator.color_idle = p.border;
                style.separator.color_hovered = p.focus_ring;
                style.tab.tab_body.bg_fill = p.surface;
                style.tab.tab_body.inner_margin = egui::Margin::ZERO;
                DockArea::new(&mut dock)
                    .style(style)
                    .show_leaf_collapse_buttons(false)
                    .show_inside(ui, &mut Viewer { app: self, now });
                self.dock = dock;
            });

        // Modal dialogs, one at a time.
        if let Some(dlg) = &mut self.new_sheet {
            let mut outcome = Outcome::Open;
            egui::Modal::new(egui::Id::new("new-sheet"))
                .frame(modal_frame(p))
                .show(&ctx, |ui| {
                    outcome = dlg.show(ui);
                });
            match outcome {
                Outcome::Create(s) => self.open(s),
                Outcome::Cancel => self.new_sheet = None,
                Outcome::Open => {}
            }
        } else if let Some((dlg, _)) = &mut self.file_dialog {
            let recent = self.settings.recent.clone();
            let mut outcome = files::Outcome::Open;
            egui::Modal::new(egui::Id::new("file-dialog"))
                .frame(modal_frame(p))
                .show(&ctx, |ui| {
                    outcome = dlg.show(ui, &recent);
                });
            match outcome {
                files::Outcome::Done(path) => {
                    let (dlg, pending) = self.file_dialog.take().unwrap();
                    self.settings.last_dir = Some(dlg.dir.clone());
                    match dlg.mode {
                        files::Mode::Open => self.open_file(&path, now),
                        files::Mode::Image => self.import_image_file(&path, None, now),
                        files::Mode::Save => {
                            if self.save_to(&path, now) {
                                if let Some(p) = pending {
                                    self.proceed(p, now);
                                }
                            }
                        }
                    }
                }
                files::Outcome::Cancel => self.file_dialog = None,
                files::Outcome::Open => {}
            }
        } else if self.layout_name.is_some() {
            self.layout_name_dialog(&ctx);
        } else if self.book_info.is_some() {
            self.book_info_dialog(&ctx);
        } else if self.adjust.is_some() {
            self.adjust_dialog(&ctx);
        } else if self.rename_layer.is_some() {
            self.rename_layer_dialog(&ctx);
        } else if !self.recovery.is_empty() {
            self.recovery_dialog(&ctx, now);
        } else if let Some(pending) = self.confirm.clone() {
            self.unsaved_prompt(&ctx, pending, now);
        }
        if self.reader.is_some() {
            self.reader_ui(&ctx);
        }
        self.show_toasts(&ctx, now);
        self.persist_settings();
    }

    fn handle_close_request(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close && self.is_dirty() {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            self.confirm = Some(Pending::Quit);
        }
        if self.quit_requested {
            self.quit_requested = false;
            ctx.send_viewport_cmd(ViewportCommand::Close);
        }
    }

    fn update_title(&mut self, ctx: &egui::Context) {
        let t = match &self.book {
            Some(_) => format!(
                "{}{} — Inkstudio",
                self.book_name(),
                if self.is_dirty() { " •" } else { "" }
            ),
            None => "Inkstudio".into(),
        };
        if t != self.title {
            ctx.send_viewport_cmd(ViewportCommand::Title(t.clone()));
            self.title = t;
        }
    }

    fn persist_settings(&mut self) {
        // The dock layout changes by dragging; compare now and then, not every frame.
        if self.frame_count % 30 == 0 {
            self.store_layout();
        }
        if self.settings_dirty {
            self.settings.simulate_pressure = self.pen.simulate;
            let _ = self.settings.save_to(&self.config_dir);
            self.settings_dirty = false;
        }
    }

    // ---- files ---------------------------------------------------------------------------------

    fn start_dir(&self) -> PathBuf {
        self.book
            .as_ref()
            .and_then(|b| b.path.as_ref())
            .and_then(|p| p.parent().map(|d| d.to_path_buf()))
            .or_else(|| self.settings.last_dir.clone())
            .filter(|d| d.is_dir())
            .unwrap_or_else(crate::settings::home_dir)
    }

    /// Run `p` now, or ask about unsaved changes first.
    fn request(&mut self, p: Pending, now: f64) {
        if self.is_dirty() {
            self.confirm = Some(p);
        } else {
            self.proceed(p, now);
        }
    }

    fn proceed(&mut self, p: Pending, now: f64) {
        self.confirm = None;
        match p {
            Pending::New => self.new_sheet = Some(NewSheet::new(self.book.is_some())),
            Pending::Open(Some(path)) => self.open_file(&path, now),
            Pending::Open(None) => {
                self.file_dialog = Some((
                    FileDialog::new(files::Mode::Open, self.start_dir(), ""),
                    None,
                ))
            }
            Pending::Quit => {
                self.allow_close = true;
                self.quit_requested = true;
            }
        }
    }

    /// Save to the book's file, or ask for a name. Returns true if saved.
    pub fn save(&mut self, now: f64) -> bool {
        let path = self.book.as_ref().and_then(|b| b.path.clone());
        match path {
            Some(p) => self.save_to(&p, now),
            None => {
                self.save_as(None);
                false
            }
        }
    }

    fn save_as(&mut self, pending: Option<Pending>) {
        let name = self.book_name();
        self.file_dialog = Some((
            FileDialog::new(files::Mode::Save, self.start_dir(), &format!("{name}.inks")),
            pending,
        ));
    }

    pub fn save_to(&mut self, path: &std::path::Path, now: f64) -> bool {
        self.commit_transform();
        self.close_adjust(true);
        let Some(book) = &mut self.book else {
            return false;
        };
        match crate::project::save(book, path) {
            Ok(()) => {
                self.settings.add_recent(path);
                self.settings_dirty = true;
                let f = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                self.toast(ToastKind::Success, trf("Saved {f}", &[("f", f)]), now);
                if let Some(b) = &self.book {
                    crate::recovery::remove(&self.config_dir, b.recovery_id);
                }
                self.last_autosave = now;
                true
            }
            Err(e) => {
                self.toast(
                    ToastKind::Error,
                    trf("Couldn't save: {e}", &[("e", e.to_string())]),
                    now,
                );
                false
            }
        }
    }

    pub fn open_file(&mut self, path: &std::path::Path, now: f64) {
        match crate::project::open(path) {
            Ok(book) => {
                self.install(book);
                self.settings.add_recent(path);
                self.settings_dirty = true;
                let f = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                self.toast(ToastKind::Success, trf("Opened {f}", &[("f", f)]), now);
            }
            Err(e) => self.toast(
                ToastKind::Error,
                trf("Couldn't open: {e}", &[("e", e.to_string())]),
                now,
            ),
        }
    }

    fn unsaved_prompt(&mut self, ctx: &egui::Context, pending: Pending, now: f64) {
        let p = theme::palette_ctx(ctx);
        let name = self.book_name();
        let mut choice = 0;
        egui::Modal::new(egui::Id::new("unsaved"))
            .frame(modal_frame(p))
            .show(ctx, |ui| {
                ui.set_width(440.0);
                ui.label(
                    RichText::new(trf("Save changes to “{f}”?", &[("f", name.clone())]))
                        .text_style(text::title()),
                );
                ui.add_space(6.0);
                ui.label(
                    RichText::new(tr("Your changes will be lost if you don't save them."))
                        .color(p.text_muted),
                );
                ui.add_space(14.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    if ui
                        .add(
                            Button::new(tr("Save"))
                                .variant(ButtonVariant::Primary)
                                .min_width(90.0),
                        )
                        .clicked()
                    {
                        choice = 1;
                    }
                    if ui
                        .add(
                            Button::new(tr("Don't save"))
                                .variant(ButtonVariant::Danger)
                                .min_width(90.0),
                        )
                        .clicked()
                    {
                        choice = 2;
                    }
                    if ui.add(Button::new(tr("Cancel")).min_width(80.0)).clicked() {
                        choice = 3;
                    }
                });
            });
        match choice {
            1 => {
                self.confirm = None;
                if self.book.as_ref().and_then(|b| b.path.as_ref()).is_some() {
                    if self.save(now) {
                        self.proceed(pending, now);
                    }
                } else {
                    self.save_as(Some(pending));
                }
            }
            2 => {
                if let Some(b) = &self.book {
                    crate::recovery::remove(&self.config_dir, b.recovery_id);
                }
                self.proceed(pending, now)
            }
            3 => self.confirm = None,
            _ => {}
        }
    }

    // ---- workspace layouts -------------------------------------------------------------------------

    fn layout_exists(&self, id: &str) -> bool {
        crate::layouts::is_builtin(id)
            || self
                .settings
                .custom_layouts
                .iter()
                .any(|n| crate::layouts::custom_id(n) == id)
    }

    /// Remember the current dock state as the active layout's state.
    fn store_layout(&mut self) {
        let s = ron::to_string(&self.dock).unwrap_or_default();
        if s != self.dock_saved {
            self.settings
                .layouts
                .insert(self.settings.active_layout.clone(), s.clone());
            self.dock_saved = s;
            self.settings_dirty = true;
        }
    }

    /// Switch to another layout; the current one keeps its arrangement.
    pub fn switch_layout(&mut self, id: &str) {
        self.store_layout();
        self.dock = load_layout(&self.settings, id);
        self.settings.active_layout = id.to_string();
        self.dock_saved = ron::to_string(&self.dock).unwrap_or_default();
        if let Some(b) = &self.book {
            self.settings.kind_layouts.insert(
                crate::layouts::kind_key(b.settings.kind).into(),
                id.to_string(),
            );
        }
        self.settings_dirty = true;
    }

    /// Save the current arrangement as a new user layout and make it active.
    pub fn save_layout_as(&mut self, name: &str) -> bool {
        let name = name.trim();
        if name.is_empty() {
            return false;
        }
        let id = crate::layouts::custom_id(name);
        if !self.settings.custom_layouts.iter().any(|n| n == name) {
            self.settings.custom_layouts.push(name.to_string());
        }
        self.settings
            .layouts
            .insert(id.clone(), ron::to_string(&self.dock).unwrap_or_default());
        self.settings.active_layout = id;
        self.dock_saved = ron::to_string(&self.dock).unwrap_or_default();
        self.settings_dirty = true;
        true
    }

    pub fn delete_layout(&mut self, name: &str) {
        let id = crate::layouts::custom_id(name);
        self.settings.custom_layouts.retain(|n| n != name);
        self.settings.layouts.remove(&id);
        self.settings.kind_layouts.retain(|_, v| *v != id);
        if self.settings.active_layout == id {
            self.settings.active_layout = crate::layouts::STANDARD.into();
            self.dock = load_layout(&self.settings, crate::layouts::STANDARD);
            self.dock_saved = ron::to_string(&self.dock).unwrap_or_default();
        }
        self.settings_dirty = true;
    }

    /// Restore the active built-in layout's default arrangement.
    pub fn reset_layout(&mut self) {
        let id = self.settings.active_layout.clone();
        if let Some(d) = crate::layouts::builtin(&id) {
            self.settings.layouts.remove(&id);
            self.dock = d;
            self.dock_saved = ron::to_string(&self.dock).unwrap_or_default();
            self.settings_dirty = true;
        }
    }

    fn layouts_menu(&mut self, ui: &mut Ui) {
        let p = palette(ui);
        ui.label(RichText::new(tr("Layouts")).color(p.text_muted));
        let active = self.settings.active_layout.clone();
        for (id, name) in crate::layouts::BUILTIN {
            if ui.radio(active == id, tr(name)).clicked() {
                self.switch_layout(id);
                ui.close();
            }
        }
        for name in self.settings.custom_layouts.clone() {
            let id = crate::layouts::custom_id(&name);
            if ui.radio(active == id, &name).clicked() {
                self.switch_layout(&id);
                ui.close();
            }
        }
        ui.separator();
        if ui.button(tr("Save layout as…")).clicked() {
            self.layout_name = Some(String::new());
            ui.close();
        }
        let builtin = crate::layouts::is_builtin(&active);
        if ui
            .add_enabled(builtin, egui::Button::new(tr("Reset layout")))
            .clicked()
        {
            self.reset_layout();
            ui.close();
        }
        if !self.settings.custom_layouts.is_empty() {
            ui.menu_button(tr("Delete layout"), |ui| {
                for name in self.settings.custom_layouts.clone() {
                    if ui.button(&name).clicked() {
                        self.delete_layout(&name);
                        ui.close();
                    }
                }
            });
        }
        if ui
            .checkbox(
                &mut self.settings.auto_layout,
                tr("Switch layout by sheet type"),
            )
            .changed()
        {
            self.settings_dirty = true;
        }
    }

    fn layout_name_dialog(&mut self, ctx: &egui::Context) {
        let p = theme::palette_ctx(ctx);
        let Some(name) = &mut self.layout_name else {
            return;
        };
        let mut done = None;
        egui::Modal::new(egui::Id::new("layout-name"))
            .frame(modal_frame(p))
            .show(ctx, |ui| {
                ui.set_width(360.0);
                ui.label(RichText::new(tr("Save layout as…")).text_style(text::title()));
                ui.add_space(8.0);
                let r = ui.add(
                    egui::TextEdit::singleline(name)
                        .hint_text(tr("Layout name"))
                        .desired_width(f32::INFINITY),
                );
                if !r.has_focus() && name.is_empty() {
                    r.request_focus();
                }
                ui.add_space(12.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    if ui
                        .add_enabled(
                            !name.trim().is_empty(),
                            Button::new(tr("Save"))
                                .variant(ButtonVariant::Primary)
                                .min_width(80.0),
                        )
                        .clicked()
                    {
                        done = Some(true);
                    }
                    if ui.add(Button::new(tr("Cancel")).min_width(80.0)).clicked() {
                        done = Some(false);
                    }
                });
                if ui.input(|i| i.key_pressed(Key::Enter)) && !name.trim().is_empty() {
                    done = Some(true);
                }
                if ui.input(|i| i.key_pressed(Key::Escape)) {
                    done = Some(false);
                }
            });
        match done {
            Some(true) => {
                let n = self.layout_name.take().unwrap_or_default();
                self.save_layout_as(&n);
            }
            Some(false) => self.layout_name = None,
            None => {}
        }
    }

    // ---- autosave & recovery -------------------------------------------------------------------

    /// Finish a running autosave; start a new one when the interval has passed.
    fn autosave_tick(&mut self, now: f64) {
        if self.transform.is_some() || self.adjust.is_some() {
            return; // the layer is lifted out; wait until the edit is finished
        }
        if self
            .autosave_job
            .as_ref()
            .is_some_and(|j| j.handle.is_finished())
        {
            let job = self.autosave_job.take().unwrap();
            match job.handle.join() {
                Ok(Ok(())) => {
                    self.autosaved_at = Some(now);
                    if let (Some(path), Some(book)) = (&job.project, &mut self.book) {
                        if book.recovery_id == job.recovery_id {
                            crate::project::mark_saved(book, path, job.generation);
                            if !book.is_dirty() {
                                crate::recovery::remove(&self.config_dir, job.recovery_id);
                            }
                        }
                    }
                }
                Ok(Err(e)) => self.toast(
                    ToastKind::Error,
                    trf("Autosave failed: {e}", &[("e", e)]),
                    now,
                ),
                Err(_) => self.toast(
                    ToastKind::Error,
                    trf("Autosave failed: {e}", &[("e", "panic".into())]),
                    now,
                ),
            }
        }
        let secs = self.settings.autosave_secs;
        let busy =
            self.autosave_job.is_some() || self.stroke.is_some() || self.balloon_drag.is_some();
        let Some(book) = &self.book else { return };
        if secs == 0 || busy || !book.is_dirty() || now - self.last_autosave < secs as f64 {
            return;
        }
        self.last_autosave = now;
        // Snapshot on the UI thread (cheap: tiles are shared), write on a worker thread.
        let snap = match crate::project::snapshot(book) {
            Ok(s) => s,
            Err(e) => {
                self.toast(
                    ToastKind::Error,
                    trf("Autosave failed: {e}", &[("e", e.to_string())]),
                    now,
                );
                return;
            }
        };
        let generation = snap.generation;
        let note = crate::recovery::Note {
            id: book.recovery_id,
            name: self.book_name(),
            original: book.path.clone(),
            saved_at: crate::recovery::now_secs(),
        };
        let project = if self.settings.autosave_project {
            book.path.clone()
        } else {
            None
        };
        let config = self.config_dir.clone();
        let target = project.clone();
        let recovery_id = note.id;
        let handle = std::thread::Builder::new()
            .name("autosave".into())
            .spawn(move || {
                let rec = crate::recovery::file(&config, note.id);
                std::fs::create_dir_all(crate::recovery::dir(&config))
                    .map_err(|e| e.to_string())?;
                snap.write(&rec).map_err(|e| e.to_string())?;
                crate::recovery::write_note(&config, &note).map_err(|e| e.to_string())?;
                if let Some(p) = target {
                    snap.write(&p).map_err(|e| e.to_string())?;
                }
                Ok(())
            });
        match handle {
            Ok(handle) => {
                self.autosave_job = Some(AutosaveJob {
                    handle,
                    generation,
                    project,
                    recovery_id,
                })
            }
            Err(e) => self.toast(
                ToastKind::Error,
                trf("Autosave failed: {e}", &[("e", e.to_string())]),
                now,
            ),
        }
    }

    /// Wait for a running autosave (tests and quitting).
    pub fn finish_autosave(&mut self, now: f64) {
        if let Some(job) = &self.autosave_job {
            while !job.handle.is_finished() {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        }
        let interval = self.settings.autosave_secs;
        self.settings.autosave_secs = 0;
        self.autosave_tick(now);
        self.settings.autosave_secs = interval;
    }

    /// Restore a recovery copy as an unsaved book.
    pub fn recover(&mut self, i: usize, now: f64) {
        if i >= self.recovery.len() {
            return;
        }
        let e = self.recovery.remove(i);
        match crate::project::open(&e.file) {
            Ok(mut book) => {
                book.path = e.note.original.clone();
                book.recovery_id = e.note.id;
                book.structure_dirty = true;
                book.structure_generation += 1;
                if book.path.is_none() {
                    book.settings.name = e.note.name.clone();
                }
                self.install(book);
                self.toast(
                    ToastKind::Success,
                    trf("Recovered “{f}”", &[("f", e.note.name)]),
                    now,
                );
            }
            Err(err) => self.toast(
                ToastKind::Error,
                trf("Couldn't open: {e}", &[("e", err.to_string())]),
                now,
            ),
        }
    }

    fn recovery_dialog(&mut self, ctx: &egui::Context, now: f64) {
        let p = theme::palette_ctx(ctx);
        let mut action: Option<(usize, bool)> = None;
        let mut later = false;
        let secs_now = crate::recovery::now_secs();
        egui::Modal::new(egui::Id::new("recovery")).frame(modal_frame(p)).show(ctx, |ui| {
            ui.set_width(520.0);
            ui.label(RichText::new(tr("Recover unsaved work?")).text_style(text::title()));
            ui.add_space(4.0);
            ui.label(RichText::new(tr("Inkstudio was closed while these had unsaved changes. Autosave kept a copy.")).color(p.text_muted));
            ui.add_space(10.0);
            for (i, e) in self.recovery.iter().enumerate() {
                egui::Frame::new().fill(p.surface_sunken).corner_radius(3).inner_margin(8).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(RichText::new(&e.note.name).text_style(text::body_strong()));
                            let mins = secs_now.saturating_sub(e.note.saved_at) / 60;
                            let place = e.note.original.as_ref().map_or_else(|| tr("never saved").to_string(), |p| p.display().to_string());
                            let when = if mins == 0 { tr("Autosaved just now").to_string() } else { trf("Autosaved {n} min ago", &[("n", mins.to_string())]) };
                            ui.label(RichText::new(format!("{when} · {place}")).text_style(text::caption()).color(p.text_muted));
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.add(Button::new(tr("Recover")).variant(ButtonVariant::Primary).size(ButtonSize::Sm)).clicked() {
                                action = Some((i, true));
                            }
                            if ui.add(Button::new(tr("Discard")).size(ButtonSize::Sm)).clicked() {
                                action = Some((i, false));
                            }
                        });
                    });
                });
                ui.add_space(4.0);
            }
            ui.add_space(8.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(Button::new(tr("Later")).min_width(80.0)).on_hover_text(tr("Keep the copies and ask again next time")).clicked() {
                    later = true;
                }
            });
        });
        match action {
            Some((i, true)) => self.recover(i, now),
            Some((i, false)) => {
                let e = self.recovery.remove(i);
                crate::recovery::remove(&self.config_dir, e.note.id);
            }
            None => {}
        }
        if later {
            self.recovery.clear();
        }
        if self.recovery.is_empty() && self.book.is_none() && self.new_sheet.is_none() {
            self.new_sheet = Some(NewSheet::new(false));
        }
    }

    // ---- keyboard ------------------------------------------------------------------------------

    fn shortcuts(&mut self, ctx: &egui::Context, now: f64) {
        if self.new_sheet.is_some()
            || self.file_dialog.is_some()
            || self.confirm.is_some()
            || self.layout_name.is_some()
            || !self.recovery.is_empty()
            || self.book_info.is_some()
            || self.reader.is_some()
            || self.adjust.is_some()
            || self.rename_layer.is_some()
        {
            return;
        }
        if self.transform.is_some() && !ctx.egui_wants_keyboard_input() {
            let (enter, esc) = ctx.input_mut(|i| {
                (
                    i.consume_key(egui::Modifiers::NONE, Key::Enter),
                    i.consume_key(egui::Modifiers::NONE, Key::Escape),
                )
            });
            if enter {
                self.commit_transform();
            } else if esc {
                self.cancel_transform();
            }
        }
        if self.book.is_some() && !ctx.egui_wants_keyboard_input() {
            let cmd = egui::Modifiers::COMMAND;
            let (t, u, imp, dup) = ctx.input_mut(|i| {
                (
                    i.consume_key(cmd, Key::T),
                    i.consume_key(cmd, Key::U),
                    i.consume_key(cmd | egui::Modifiers::SHIFT, Key::I),
                    i.consume_key(cmd, Key::J),
                )
            });
            if t {
                self.begin_transform(now);
            }
            if u {
                self.open_adjust(now);
            }
            if imp {
                self.import_image_dialog();
            }
            if dup {
                self.duplicate_layer();
            }
        }
        if self.book.is_some() && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::F5)) {
            self.open_reader();
            return;
        }
        let cmd = egui::Modifiers::COMMAND;
        // File commands work even while typing (e.g. saving while editing balloon text).
        let (new, open, save, save_as) = ctx.input_mut(|i| {
            (
                i.consume_key(cmd, Key::N),
                i.consume_key(cmd, Key::O),
                i.consume_key(cmd | egui::Modifiers::SHIFT, Key::S),
                i.consume_key(cmd, Key::S),
            )
        });
        let (save_as, save) = (save, save_as); // consume Shift+S first so plain Ctrl+S doesn't swallow it
        if ctx.egui_wants_keyboard_input() {
            self.file_shortcuts(new, open, save, save_as, now);
            return;
        }
        let (undo, redo, fit) = ctx.input_mut(|i| {
            (
                i.consume_key(cmd, Key::Z),
                i.consume_key(cmd | egui::Modifiers::SHIFT, Key::Z) || i.consume_key(cmd, Key::Y),
                i.consume_key(cmd, Key::Num0),
            )
        });
        if redo {
            self.redo();
        } else if undo {
            self.undo();
        }
        self.file_shortcuts(new, open, save, save_as, now);
        self.view_shortcuts(ctx, fit, now);
    }

    fn file_shortcuts(&mut self, new: bool, open: bool, save: bool, save_as: bool, now: f64) {
        if new {
            self.request(Pending::New, now);
        }
        if open {
            self.request(Pending::Open(None), now);
        }
        if save_as && self.book.is_some() {
            self.save_as(None);
        } else if save && self.book.is_some() {
            self.save(now);
        }
    }

    fn view_shortcuts(&mut self, ctx: &egui::Context, fit: bool, now: f64) {
        if fit {
            self.fit_pending = true;
        }
        let c = self.canvas_rect.center();
        let (zin, zout, one, rl, rr, r0, flip, bigger, smaller, del, swap, play) = ctx.input(|i| {
            let plain = i.modifiers.is_none();
            (
                i.key_pressed(Key::Plus) || i.key_pressed(Key::Equals),
                i.key_pressed(Key::Minus),
                i.key_pressed(Key::Num1) && plain,
                i.key_pressed(Key::Num4) && plain,
                i.key_pressed(Key::Num6) && plain,
                i.key_pressed(Key::Num5) && plain,
                i.key_pressed(Key::M) && plain,
                i.key_pressed(Key::CloseBracket),
                i.key_pressed(Key::OpenBracket),
                i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace),
                i.key_pressed(Key::X) && plain,
                i.key_pressed(Key::Enter) && plain,
            )
        });
        if self.book.is_some() && self.canvas_rect.is_positive() {
            if zin {
                self.view.zoom_at(c, 1.25);
            }
            if zout {
                self.view.zoom_at(c, 0.8);
            }
            if one {
                self.view.set_zoom(c, 1.0);
            }
            if rl {
                self.view.rotate_at(c, -15f32.to_radians());
            }
            if rr {
                self.view.rotate_at(c, 15f32.to_radians());
            }
            if r0 {
                self.view.set_angle(c, 0.0);
            }
            if flip {
                self.view.toggle_flip(c);
            }
            // Arrow keys move the view (Shift = faster).
            let (d, fast) = ctx.input(|i| {
                (
                    vec2(
                        (i.key_pressed(Key::ArrowLeft) as i32
                            - i.key_pressed(Key::ArrowRight) as i32) as f32,
                        (i.key_pressed(Key::ArrowUp) as i32 - i.key_pressed(Key::ArrowDown) as i32)
                            as f32,
                    ),
                    i.modifiers.shift,
                )
            });
            if d != egui::Vec2::ZERO {
                self.view.pan(d * if fast { 240.0 } else { 60.0 });
            }
        }
        let id = self.current_preset_id();
        if bigger || smaller {
            let s = self.size_of(id);
            self.sizes.insert(
                id,
                (s * if bigger { 1.15 } else { 1.0 / 1.15 }).clamp(0.5, 2000.0),
            );
        }
        if swap {
            self.swap_colors();
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, Key::V)) {
            self.value_check = !self.value_check;
        }
        if play
            && self
                .book
                .as_ref()
                .is_some_and(|b| b.page().timeline.is_some())
        {
            self.playing = !self.playing;
        }
        if del {
            self.delete_selection(now);
        }
        for (t, _, _, key) in Tool::ALL {
            // R is "hold to rotate", not a tool switch.
            if t != Tool::Rotate && ctx.input(|i| i.key_pressed(key) && i.modifiers.is_none()) {
                self.tool = t;
            }
        }
    }

    fn swap_colors(&mut self) {
        let main = self.color.to_color32();
        let s = self.sub_color.to_straight();
        self.color = hsv_from(Color32::from_rgb(s[0], s[1], s[2]));
        self.sub_color = to_rgba(main);
    }

    pub fn undo(&mut self) {
        if self.transform.is_some() {
            self.cancel_transform();
            return;
        }
        if let Some(b) = &mut self.book {
            let p = &mut b.pages[b.current];
            p.undo(&self.fonts);
        }
    }
    pub fn redo(&mut self) {
        self.commit_transform();
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
                    self.toast(ToastKind::Info, tr("Panel deleted"), now);
                }
            }
        }
    }

    // =========================================================================================
    // Menu, options, status
    // =========================================================================================

    fn menu_bar(&mut self, ui: &mut Ui, now: f64) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button(tr("File"), |ui| {
                if ui.button(format!("{}  (Ctrl+N)", tr("New sheet…"))).clicked() {
                    self.request(Pending::New, now);
                    ui.close();
                }
                if ui.button(format!("{}  (Ctrl+O)", tr("Open…"))).clicked() {
                    self.request(Pending::Open(None), now);
                    ui.close();
                }
                ui.menu_button(tr("Autosave"), |ui| {
                    for (secs, label) in [(0u32, tr("Off")), (60, tr("Every minute")), (180, tr("Every 3 minutes")), (300, tr("Every 5 minutes")), (600, tr("Every 10 minutes"))] {
                        if ui.radio(self.settings.autosave_secs == secs, label).clicked() {
                            self.settings.autosave_secs = secs;
                            self.settings_dirty = true;
                        }
                    }
                    ui.separator();
                    if ui.checkbox(&mut self.settings.autosave_project, tr("Also save the project file")).changed() {
                        self.settings_dirty = true;
                    }
                    ui.label(RichText::new(tr("A recovery copy is always kept until you save, so work survives a crash.")).text_style(text::caption()).color(palette(ui).text_muted));
                });
                ui.menu_button(tr("Open recent"), |ui| {
                    if self.settings.recent.is_empty() {
                        ui.label(RichText::new(tr("No recent files")).color(palette(ui).text_muted));
                    }
                    for r in self.settings.recent.clone() {
                        let name = r.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                        if ui.button(name).on_hover_text(r.display().to_string()).clicked() {
                            self.request(Pending::Open(Some(r)), now);
                            ui.close();
                        }
                    }
                });
                ui.separator();
                let has = self.book.is_some();
                if ui.add_enabled(has, egui::Button::new(format!("{}  (Ctrl+S)", tr("Save")))).clicked() {
                    self.save(now);
                    ui.close();
                }
                if ui.add_enabled(has, egui::Button::new(format!("{}  (Ctrl+Shift+S)", tr("Save as…")))).clicked() {
                    self.save_as(None);
                    ui.close();
                }
                ui.separator();
                if ui.add_enabled(has, egui::Button::new(format!("{}  (Ctrl+Shift+I)", tr("Import image…")))).clicked() {
                    self.import_image_dialog();
                    ui.close();
                }
                ui.separator();
                if ui.add_enabled(has, egui::Button::new(tr("Export page as PNG"))).clicked() {
                    self.export_pages(false, now);
                    ui.close();
                }
                let multi = self.book.as_ref().is_some_and(|b| b.pages.len() > 1);
                if ui.add_enabled(multi, egui::Button::new(tr("Export all pages as PNG"))).clicked() {
                    self.export_pages(true, now);
                    ui.close();
                }
                let anim = self.book.as_ref().is_some_and(|b| b.page().timeline.is_some());
                if ui.add_enabled(anim, egui::Button::new(tr("Export animation as GIF"))).clicked() {
                    self.export_gif(now);
                    ui.close();
                }
                ui.separator();
                if ui.button(tr("Quit")).clicked() {
                    self.request(Pending::Quit, now);
                    ui.close();
                }
            });
            ui.menu_button(tr("Edit"), |ui| {
                let (cu, cr) = self.book.as_ref().map_or((false, false), |b| (b.page().can_undo(), b.page().can_redo()));
                if ui.add_enabled(cu, egui::Button::new(format!("{}  (Ctrl+Z)", tr("Undo")))).clicked() {
                    self.undo();
                    ui.close();
                }
                if ui.add_enabled(cr, egui::Button::new(format!("{}  (Ctrl+Shift+Z)", tr("Redo")))).clicked() {
                    self.redo();
                    ui.close();
                }
            });
            ui.menu_button(tr("View"), |ui| {
                let c = self.canvas_rect.center();
                if ui.button(format!("{}  (Ctrl+0)", tr("Fit page"))).clicked() {
                    self.fit_pending = true;
                    ui.close();
                }
                if ui.button(format!("{}  (1)", tr("Actual pixels (100%)"))).clicked() {
                    self.view.set_zoom(c, 1.0);
                    ui.close();
                }
                if ui.button(format!("{}  (+)", tr("Zoom in"))).clicked() {
                    self.view.zoom_at(c, 1.25);
                }
                if ui.button(format!("{}  (−)", tr("Zoom out"))).clicked() {
                    self.view.zoom_at(c, 0.8);
                }
                ui.separator();
                if ui.button(format!("{}  (4)", tr("Rotate left 15°"))).clicked() {
                    self.view.rotate_at(c, -15f32.to_radians());
                }
                if ui.button(format!("{}  (6)", tr("Rotate right 15°"))).clicked() {
                    self.view.rotate_at(c, 15f32.to_radians());
                }
                if ui.button(format!("{}  (5)", tr("Reset rotation"))).clicked() {
                    self.view.set_angle(c, 0.0);
                }
                let mut flip = self.view.flip;
                if ui.checkbox(&mut flip, format!("{}  (M)", tr("Flip horizontally"))).changed() {
                    self.view.toggle_flip(c);
                }
                ui.checkbox(&mut self.value_check, format!("{}  (Shift+V)", tr("Value check (grayscale)")));
                ui.separator();
                if ui.checkbox(&mut self.pen.simulate, tr("Simulate pressure for mouse")).changed() {
                    self.settings_dirty = true;
                }
            });
            ui.menu_button(tr("Page"), |ui| {
                let Some(b) = &mut self.book else { return };
                if ui.button(tr("Add page after this")).clicked() {
                    let at = b.current + 1;
                    b.add_page(at);
                    b.current = at;
                    self.cache.clear();
                    self.thumbs.clear();
                    ui.close();
                }
                if ui.add_enabled(b.pages.len() > 1, egui::Button::new(tr("Delete this page"))).clicked() {
                    let c = b.current;
                    b.delete_page(c);
                    self.cache.clear();
                    self.thumbs.clear();
                    ui.close();
                }
                ui.separator();
                ui.label(RichText::new(tr("Replace panels with template")).color(palette(ui).text_muted));
                for t in PanelTemplate::ALL {
                    if ui.button(tr(t.name())).clicked() {
                        let (lo, hi) = b.settings.safe_rect();
                        let page = b.page_mut();
                        page.push_objects();
                        page.panels.apply_template(t, lo, hi);
                        page.render_panels();
                        ui.close();
                    }
                }
            });
            ui.menu_button(tr("Layer"), |ui| self.layer_menu(ui, now));
            ui.menu_button(tr("Book"), |ui| self.book_menu(ui));
            ui.menu_button(tr("Animation"), |ui| {
                let anim = self.book.as_ref().is_some_and(|b| b.page().timeline.is_some());
                let label = if self.playing { tr("Pause") } else { tr("Play") };
                if ui.add_enabled(anim, egui::Button::new(format!("{label}  (Enter)"))).clicked() {
                    self.playing = !self.playing;
                    ui.close();
                }
                if !anim {
                    ui.label(RichText::new(tr("Create an Animation sheet to use the timeline.")).color(palette(ui).text_muted));
                }
            });
            ui.menu_button(tr("Window"), |ui| {
                ui.label(RichText::new(tr("Show palettes")).color(palette(ui).text_muted));
                for pane in Pane::PALETTES {
                    let mut shown = self.dock.find_tab(&pane).is_some();
                    if ui.checkbox(&mut shown, tr(pane.title())).changed() {
                        if shown {
                            self.dock.push_to_focused_leaf(pane);
                        } else if let Some(path) = self.dock.find_tab(&pane) {
                            self.dock.remove_tab(path);
                        }
                    }
                }
                ui.separator();
                self.layouts_menu(ui);
            });
            ui.menu_button(format!("{} / Language", tr("Language")), |ui| {
                for l in Lang::ALL {
                    if ui.radio(i18n::lang() == l, l.native_name()).clicked() {
                        self.set_lang(l);
                        ui.close();
                    }
                }
            });
        });
    }

    fn tool_options(&mut self, ui: &mut Ui) {
        let p = palette(ui);
        let now = ui.input(|i| i.time);
        ui.spacing_mut().item_spacing.x = 6.0;
        let (cu, cr) = self.book.as_ref().map_or((false, false), |b| {
            (b.page().can_undo(), b.page().can_redo())
        });
        if ui
            .add_enabled(
                cu,
                IconButton::new(
                    icons::ARROW_COUNTER_CLOCKWISE,
                    format!("{} (Ctrl+Z)", tr("Undo")),
                )
                .size(28.0),
            )
            .clicked()
        {
            self.undo();
        }
        if ui
            .add_enabled(
                cr,
                IconButton::new(
                    icons::ARROW_CLOCKWISE,
                    format!("{} (Ctrl+Shift+Z)", tr("Redo")),
                )
                .size(28.0),
            )
            .clicked()
        {
            self.redo();
        }
        if ui
            .add_enabled(
                self.book.is_some(),
                IconButton::new(icons::FLOPPY_DISK, format!("{} (Ctrl+S)", tr("Save"))).size(28.0),
            )
            .clicked()
        {
            self.save(now);
        }
        ui.separator();
        match self.tool {
            Tool::Brush | Tool::Eraser => {
                let id = self.current_preset_id();
                let name = tr(&self.preset(id).params.name).to_string();
                ui.label(RichText::new(name).text_style(text::body_strong()));
                let mut size = self.size_of(id);
                ui.label(RichText::new(tr("Size")).color(p.text_muted));
                if ui
                    .add(
                        egui::Slider::new(&mut size, 0.5..=600.0)
                            .logarithmic(true)
                            .suffix(" px")
                            .max_decimals(1),
                    )
                    .changed()
                {
                    self.sizes.insert(id, size);
                }
                ui.label(
                    RichText::new(format!("{:.2} mm", size * self.mm_per_px())).color(p.text_muted),
                );
                let mut op = *self
                    .opacity
                    .get(id)
                    .unwrap_or(&(self.preset(id).params.opacity * 100.0));
                ui.label(RichText::new(tr("Opacity")).color(p.text_muted));
                if ui
                    .add(
                        egui::Slider::new(&mut op, 1.0..=100.0)
                            .suffix(" %")
                            .max_decimals(0),
                    )
                    .changed()
                {
                    self.opacity.insert(id, op);
                }
            }
            Tool::Balloon => {
                ui.label(RichText::new(tr("Balloon")).text_style(text::body_strong()));
                ui.label(RichText::new(tr(self.balloon_style.name())).color(p.text_muted));
                ui.label(
                    RichText::new(tr(
                        "Click empty canvas to place · drag body / tail tip / corners",
                    ))
                    .color(p.text_muted),
                );
            }
            Tool::Panel => {
                ui.label(RichText::new(tr("Panels")).text_style(text::body_strong()));
                ui.selectable_value(
                    &mut self.panel_mode,
                    PanelMode::Rectangle,
                    tr("Draw rectangle"),
                );
                ui.selectable_value(
                    &mut self.panel_mode,
                    PanelMode::Divide,
                    tr("Divide (drag a cut)"),
                );
                ui.selectable_value(&mut self.panel_mode, PanelMode::Select, tr("Select"));
            }
            Tool::Fill => {
                ui.label(RichText::new(tr("Fill")).text_style(text::body_strong()));
                ui.label(RichText::new(tr("Close gap")).color(p.text_muted));
                ui.add(egui::Slider::new(&mut self.fill.gap, 0..=40).suffix(" px"));
            }
            Tool::Hand => {
                ui.label(RichText::new(tr("Drag to pan · scroll to zoom")).color(p.text_muted));
            }
            Tool::LassoFill => {
                ui.label(RichText::new(tr("Lasso fill")).text_style(text::body_strong()));
                ui.label(RichText::new(tr("Draw around an area to fill it")).color(p.text_muted));
            }
            Tool::Gradient => {
                ui.label(RichText::new(tr("Gradient")).text_style(text::body_strong()));
                ui.selectable_value(
                    &mut self.gradient_mode,
                    coloring::GradientMode::MainToSub,
                    tr("Main → sub"),
                );
                ui.selectable_value(
                    &mut self.gradient_mode,
                    coloring::GradientMode::MainToTransparent,
                    tr("Main → transparent"),
                );
            }
            Tool::Eyedropper => {
                ui.label(RichText::new(tr("Eyedropper")).text_style(text::body_strong()));
                ui.label(
                    RichText::new(tr("Click to pick · Alt + click with any painting tool"))
                        .color(p.text_muted),
                );
            }
            Tool::Transform => {
                ui.label(RichText::new(tr("Transform")).text_style(text::body_strong()));
                if self.transform.is_some() {
                    if ui
                        .add(
                            Button::new(tr("Apply (Enter)"))
                                .variant(ButtonVariant::Primary)
                                .size(ButtonSize::Sm),
                        )
                        .clicked()
                    {
                        self.commit_transform();
                    }
                    if ui
                        .add(Button::new(tr("Cancel (Esc)")).size(ButtonSize::Sm))
                        .clicked()
                    {
                        self.cancel_transform();
                    }
                } else {
                    ui.label(
                        RichText::new(tr("Click the canvas to move the selected layer"))
                            .color(p.text_muted),
                    );
                }
            }
            Tool::Ruler => {
                ui.label(RichText::new(tr("Ruler")).text_style(text::body_strong()));
                if let Some(b) = &mut self.book {
                    let page = b.page_mut();
                    if let Some(k) = page.ruler.as_ref().and_then(rulers::RulerKind::of) {
                        ui.label(RichText::new(tr(k.name())).color(p.text_muted));
                        if ui.checkbox(&mut page.ruler_snap, tr("Snap")).changed() {
                            page.touch();
                        }
                        if ui
                            .add(Button::new(tr("Remove ruler")).size(ButtonSize::Sm))
                            .clicked()
                        {
                            page.ruler = None;
                            page.touch();
                        }
                    } else {
                        ui.label(
                            RichText::new(tr(
                                "Pick a ruler in the Sub tool palette, then click the page",
                            ))
                            .color(p.text_muted),
                        );
                    }
                }
            }
            Tool::Rotate => {
                ui.label(
                    RichText::new(tr("Drag to rotate around the view center")).color(p.text_muted),
                );
                let c = self.canvas_rect.center();
                if ui
                    .add(Button::new(tr("Reset rotation")).size(ButtonSize::Sm))
                    .clicked()
                {
                    self.view.set_angle(c, 0.0);
                }
            }
        }
    }

    fn status_bar(&mut self, ui: &mut Ui) {
        let p = palette(ui);
        ui.horizontal_centered(|ui| {
            let st = |t: String| {
                RichText::new(t)
                    .text_style(text::caption())
                    .color(p.text_muted)
            };
            ui.label(st(trf(
                "Zoom {z}%",
                &[("z", format!("{:.1}", self.view.zoom * 100.0))],
            )));
            ui.label(st(trf(
                "Rotation {r}°",
                &[("r", format!("{:.0}", self.view.angle.to_degrees()))],
            )));
            if self.view.flip {
                ui.label(st(tr("Mirrored").into()));
            }
            if let Some(b) = &self.book {
                ui.label(st(format!(
                    "{} × {} px · {} dpi · {}",
                    b.page().doc.width,
                    b.page().doc.height,
                    b.settings.dpi,
                    tr(b.settings.color_mode.name())
                )));
                if b.pages.len() > 1 {
                    ui.label(st(format!(
                        "{} {} / {}",
                        tr("Page"),
                        b.current + 1,
                        b.pages.len()
                    )));
                }
                if let Some(tl) = &b.page().timeline {
                    ui.label(st(trf(
                        "Frame {a} / {b}",
                        &[
                            ("a", (b.page().frame + 1).to_string()),
                            ("b", tl.frames.to_string()),
                        ],
                    )));
                }
                if b.is_dirty() {
                    ui.label(
                        RichText::new(format!("• {}", tr("Unsaved")))
                            .text_style(text::caption())
                            .color(p.warning),
                    );
                }
            }
            if let Some(c) = self.hover_canvas {
                ui.label(st(format!("{:.0}, {:.0}", c.x, c.y)));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some(t) = self.autosaved_at {
                    let ago = (ui.input(|i| i.time) - t).max(0.0);
                    let label = if ago < 60.0 {
                        tr("Autosaved just now").to_string()
                    } else {
                        trf(
                            "Autosaved {n} min ago",
                            &[("n", format!("{:.0}", ago / 60.0))],
                        )
                    };
                    ui.label(st(label));
                }
                let src = if self.pen.saw_pen {
                    trf(
                        "Pen pressure {p}",
                        &[("p", format!("{:.2}", self.pen.last_pressure))],
                    )
                } else if let Some(name) = self.tablet_name {
                    trf("Tablet ready ({b})", &[("b", name.to_string())])
                } else if self.pen.simulate {
                    tr("Mouse (simulated pressure)").into()
                } else {
                    tr("Mouse").into()
                };
                ui.label(st(src));
            });
        });
    }

    fn show_toasts(&mut self, ctx: &egui::Context, now: f64) {
        self.toasts
            .retain(|t| now - t.2 < 5.0 || t.0 == ToastKind::Error && now - t.2 < 12.0);
        if self.toasts.is_empty() {
            return;
        }
        egui::Area::new(egui::Id::new("toasts"))
            .anchor(Align2::CENTER_BOTTOM, vec2(0.0, -40.0))
            .show(ctx, |ui| {
                for (k, m, _) in self.toasts.clone() {
                    toast(ui, k, &m, None);
                    ui.add_space(4.0);
                }
            });
        ctx.request_repaint_after(std::time::Duration::from_millis(500));
    }

    fn tool_column(&mut self, ui: &mut Ui) {
        ui.spacing_mut().item_spacing.y = 2.0;
        for (t, icon, name, _) in Tool::ALL {
            if ui
                .add(IconButton::new(icon, tr(name)).selected(self.tool == t))
                .clicked()
            {
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
            ui.painter().rect(
                sub,
                1,
                Color32::from_rgb(s[0], s[1], s[2]),
                Stroke::new(1.0, p.border_strong),
                egui::StrokeKind::Inside,
            );
            ui.painter().rect(
                main,
                1,
                self.color.to_color32(),
                Stroke::new(1.0, p.border_strong),
                egui::StrokeKind::Inside,
            );
            if resp.on_hover_text(tr("Swap main/sub color (X)")).clicked() {
                self.swap_colors();
            }
        });
    }

    // =========================================================================================
    // Palettes (dock tabs)
    // =========================================================================================

    fn sub_tool_pane(&mut self, ui: &mut Ui) {
        let p = palette(ui);
        egui::ScrollArea::vertical().id_salt("subtool").show(ui, |ui| {
            egui::Frame::new().inner_margin(egui::Margin::symmetric(4, 4)).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 1.0;
                match self.tool {
                    Tool::Brush | Tool::Eraser => {
                        for cat in Category::ALL {
                            if (cat == Category::Eraser) != (self.tool == Tool::Eraser) {
                                continue;
                            }
                            ui.add_space(4.0);
                            ui.label(RichText::new(tr(cat.name())).text_style(text::caption()).color(p.text_muted));
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
                                if list_row(ui, Some(icon), tr(&preset.params.name), selected).on_hover_text(tr(preset.use_for)).clicked() {
                                    if self.tool == Tool::Eraser {
                                        self.eraser_id = preset.id;
                                    } else {
                                        self.brush_id = preset.id;
                                    }
                                }
                            }
                        }
                    }
                    Tool::Balloon => self.balloon_styles(ui),
                    Tool::Panel => {
                        ui.spacing_mut().item_spacing.y = 4.0;
                        ui.label(RichText::new(tr("Replaces the panels on this page (undoable).")).text_style(text::caption()).color(p.text_muted));
                        for t in PanelTemplate::ALL {
                            if ui.add(Button::new(tr(t.name())).min_width(200.0)).clicked() {
                                if let Some(b) = &mut self.book {
                                    let (lo, hi) = b.settings.safe_rect();
                                    let page = b.page_mut();
                                    page.push_objects();
                                    page.panels.apply_template(t, lo, hi);
                                    page.render_panels();
                                }
                            }
                        }
                    }
                    Tool::Ruler => self.ruler_panel(ui),
                    Tool::Transform => self.transform_actions(ui),
                    Tool::Fill | Tool::LassoFill | Tool::Gradient | Tool::Eyedropper | Tool::Hand | Tool::Rotate => {
                        let msg = match self.tool {
                            Tool::Fill => tr("Click inside line art. The color goes onto the selected layer; the shape is read from reference layers (Ink, Panels)."),
                            Tool::LassoFill => tr("Draw around an area: it is filled with the main color when you lift the pen."),
                            Tool::Gradient => tr("Drag across the page: main color at the start, sub color (or transparent) at the end. Use it on a clipped layer to shade."),
                            Tool::Eyedropper => tr("Click the canvas to pick a color. With any painting tool, hold Alt and click."),
                            _ => tr("Drag to move the view. Scroll to pan, Ctrl+scroll or pinch to zoom, R+drag to rotate."),
                        };
                        ui.label(RichText::new(msg).color(p.text_muted));
                    }
                }
            });
        });
    }

    fn balloon_styles(&mut self, ui: &mut Ui) {
        let p = palette(ui);
        let w = ((ui.available_width() - 6.0) / 2.0).clamp(80.0, 140.0);
        egui::Grid::new("balloon-styles")
            .spacing([4.0, 4.0])
            .show(ui, |ui| {
                for (i, style) in BalloonStyle::ALL.into_iter().enumerate() {
                    let selected = self.balloon_style == style;
                    let (r, resp) = ui.allocate_exact_size(vec2(w, 64.0), egui::Sense::click());
                    let fill = if selected {
                        p.accent_subtle
                    } else if resp.hovered() {
                        p.surface_hover
                    } else {
                        p.surface_sunken
                    };
                    ui.painter().rect(
                        r,
                        3,
                        fill,
                        Stroke::new(1.0, if selected { p.focus_ring } else { p.border }),
                        egui::StrokeKind::Inside,
                    );
                    draw_style_icon(
                        ui.painter(),
                        Rect::from_min_size(r.min + vec2(8.0, 6.0), vec2(w - 16.0, 38.0)),
                        style,
                    );
                    ui.painter().text(
                        pos2(r.center().x, r.bottom() - 9.0),
                        Align2::CENTER_CENTER,
                        tr(style.name()),
                        egui::FontId::proportional(11.0),
                        p.text,
                    );
                    if resp.clicked() {
                        self.balloon_style = style;
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
    }

    fn tool_property_pane(&mut self, ui: &mut Ui) {
        let p = palette(ui);
        egui::ScrollArea::vertical().id_salt("toolprop").show(ui, |ui| {
            egui::Frame::new().inner_margin(egui::Margin::symmetric(8, 6)).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                ui.spacing_mut().slider_width = (ui.available_width() - 200.0).max(40.0);
                match self.tool {
                    Tool::Brush | Tool::Eraser => {
                        let id = self.current_preset_id();
                        let preset = self.preset(id).clone();
                        ui.label(RichText::new(tr(&preset.params.name)).text_style(text::body_strong()));
                        ui.label(RichText::new(tr(preset.use_for)).text_style(text::caption()).color(p.text_muted));
                        let mut size = self.size_of(id);
                        if slider_field(ui, tr("Size"), &mut size, 0.5..=600.0, " px", None).changed() {
                            self.sizes.insert(id, size);
                        }
                        let mut op = *self.opacity.get(id).unwrap_or(&(preset.params.opacity * 100.0));
                        if slider_field(ui, tr("Opacity"), &mut op, 1.0..=100.0, " %", None).changed() {
                            self.opacity.insert(id, op);
                        }
                        let mut st = *self.stabilization.get(id).unwrap_or(&preset.params.stabilization) as f32;
                        if slider_field(ui, tr("Stabilization"), &mut st, 0.0..=100.0, "", None).changed() {
                            self.stabilization.insert(id, st as u32);
                        }
                    }
                    Tool::Fill => {
                        egui::ComboBox::from_label(tr("Refer to"))
                            .selected_text(match self.fill.refer {
                                Refer::Target => tr("This layer"),
                                Refer::References => tr("Reference layers"),
                                Refer::AllLayers => tr("All layers"),
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut self.fill.refer, Refer::Target, tr("This layer"));
                                ui.selectable_value(&mut self.fill.refer, Refer::References, tr("Reference layers"));
                                ui.selectable_value(&mut self.fill.refer, Refer::AllLayers, tr("All layers"));
                            });
                        let mut g = self.fill.gap as f32;
                        if slider_field(ui, tr("Close gap"), &mut g, 0.0..=40.0, " px", None).changed() {
                            self.fill.gap = g as u32;
                        }
                        let mut t = self.fill.tolerance as f32;
                        if slider_field(ui, tr("Tolerance"), &mut t, 0.0..=255.0, "", None).changed() {
                            self.fill.tolerance = t as u8;
                        }
                        let mut e = self.fill.expand as f32;
                        if slider_field(ui, tr("Expand"), &mut e, 0.0..=10.0, " px", None).changed() {
                            self.fill.expand = e as u32;
                        }
                        self.coloring_actions(ui);
                    }
                    Tool::Panel => {
                        ui.horizontal_wrapped(|ui| {
                            ui.selectable_value(&mut self.panel_mode, PanelMode::Rectangle, tr("Draw rectangle"));
                            ui.selectable_value(&mut self.panel_mode, PanelMode::Divide, tr("Divide (drag a cut)"));
                            ui.selectable_value(&mut self.panel_mode, PanelMode::Select, tr("Select"));
                        });
                        if let Some(b) = &mut self.book {
                            let mm = 25.4 / b.settings.dpi as f32;
                            let page = b.page_mut();
                            let mut border = page.panels.border * mm;
                            let mut gh = page.panels.gutter.0 * mm;
                            let mut gv = page.panels.gutter.1 * mm;
                            ui.horizontal(|ui| {
                                ui.label(tr("Border"));
                                if ui.add(egui::DragValue::new(&mut border).range(0.05..=5.0).speed(0.01).suffix(" mm")).changed() {
                                    page.panels.border = border / mm;
                                    page.render_panels();
                                    page.touch();
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.label(tr("Gutter ↕"));
                                ui.add(egui::DragValue::new(&mut gh).range(0.0..=30.0).speed(0.1).suffix(" mm"));
                                ui.label("↔");
                                ui.add(egui::DragValue::new(&mut gv).range(0.0..=30.0).speed(0.1).suffix(" mm"));
                            });
                            page.panels.gutter = (gh / mm, gv / mm);
                        }
                        ui.label(RichText::new(tr("Rectangle: drag on the page.\nDivide: drag a line across a panel.\nSelect + Delete removes a panel.")).text_style(text::caption()).color(p.text_muted));
                    }
                    Tool::Balloon => self.balloon_text(ui),
                    Tool::Ruler => self.ruler_props(ui),
                    Tool::Transform => self.transform_props(ui),
                    Tool::Gradient => {
                        ui.radio_value(&mut self.gradient_mode, coloring::GradientMode::MainToSub, tr("Main color → sub color"));
                        ui.radio_value(&mut self.gradient_mode, coloring::GradientMode::MainToTransparent, tr("Main color → transparent"));
                        self.coloring_actions(ui);
                    }
                    Tool::LassoFill | Tool::Eyedropper => {
                        ui.label(RichText::new(tr("Uses the main color. Alt + click picks a color with any painting tool.")).color(p.text_muted));
                        self.coloring_actions(ui);
                    }
                    Tool::Hand | Tool::Rotate => {
                        ui.label(RichText::new(tr("Drag to move the view. Scroll to pan, Ctrl+scroll or pinch to zoom, R+drag to rotate.")).color(p.text_muted));
                    }
                }
            });
        });
    }

    /// Text editor of the selected balloon (the balloon tool's property palette).
    fn balloon_text(&mut self, ui: &mut Ui) {
        let p = palette(ui);
        ui.horizontal(|ui| {
            ui.label(tr("Size of new balloons"));
            ui.add(
                egui::DragValue::new(&mut self.balloon_pt)
                    .range(4.0..=72.0)
                    .speed(0.2)
                    .suffix(" pt"),
            );
        });
        ui.horizontal(|ui| {
            ui.selectable_value(
                &mut self.balloon_dir,
                TextDir::Vertical,
                format!("縦 {}", tr("Vertical")),
            );
            ui.selectable_value(&mut self.balloon_dir, TextDir::Horizontal, tr("Horizontal"));
        });
        ui.separator();
        let Some(b) = &mut self.book else { return };
        let page = b.page_mut();
        let Some(id) = self.selected_balloon else {
            ui.label(
                RichText::new(tr(
                    "Click on the page to place a balloon, or click a balloon to edit it.",
                ))
                .color(p.text_muted),
            );
            self.text_snapshot = false;
            return;
        };
        let Some(idx) = page.balloons.iter().position(|x| x.id == id) else {
            self.selected_balloon = None;
            return;
        };
        let mut bl = page.balloons[idx].clone();
        ui.label(RichText::new(tr("Text")).text_style(text::body_strong()));
        let resp = ui.add(
            egui::TextEdit::multiline(&mut bl.text)
                .desired_rows(4)
                .desired_width(f32::INFINITY)
                .hint_text(tr("Type the dialogue…")),
        );
        if self.focus_text {
            resp.request_focus();
            self.focus_text = false;
        }
        if resp.gained_focus() {
            self.text_snapshot = false;
        }
        let mut changed = resp.changed();
        ui.horizontal(|ui| {
            changed |= ui
                .selectable_value(
                    &mut bl.dir,
                    TextDir::Vertical,
                    format!("縦 {}", tr("Vertical")),
                )
                .changed();
            changed |= ui
                .selectable_value(&mut bl.dir, TextDir::Horizontal, tr("Horizontal"))
                .changed();
        });
        ui.horizontal(|ui| {
            let dpi = page.doc.dpi as f32;
            let mut pt = bl.font_px * 72.0 / dpi;
            ui.label(tr("Font size"));
            if ui
                .add(
                    egui::DragValue::new(&mut pt)
                        .range(4.0..=96.0)
                        .speed(0.2)
                        .suffix(" pt"),
                )
                .changed()
            {
                bl.font_px = pt * dpi / 72.0;
                changed = true;
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label(tr("Tail"));
            for (t, name) in [
                (TailStyle::Pointed, "Pointed"),
                (TailStyle::Curved, "Curved"),
                (TailStyle::Bubbles, "Bubbles"),
                (TailStyle::None, "None"),
            ] {
                changed |= ui.selectable_value(&mut bl.tail, t, tr(name)).changed();
            }
        });
        changed |= ui
            .checkbox(&mut bl.auto_fit, tr("Fit balloon to text"))
            .changed();
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
            page.touch();
        }
        ui.horizontal(|ui| {
            if ui
                .add(Button::new(tr("Bring to front")).size(ButtonSize::Sm))
                .clicked()
            {
                page.push_objects();
                let x = page.balloons.remove(idx);
                page.balloons.push(x);
                page.render_balloons(&self.fonts, None);
            }
            if ui
                .add(
                    Button::new(tr("Delete"))
                        .variant(ButtonVariant::Danger)
                        .size(ButtonSize::Sm),
                )
                .clicked()
            {
                page.push_objects();
                page.balloons.remove(idx);
                page.render_balloons(&self.fonts, None);
                self.selected_balloon = None;
            }
        });
        if self.fonts.is_empty() {
            ui.label(RichText::new(tr("No font found: text will not render.")).color(p.warning));
        }
    }

    fn layers_pane(&mut self, ui: &mut Ui) {
        let mut layer_action: Option<u8> = None;
        self.layers_pane_inner(ui, &mut layer_action);
        let now = ui.input(|i| i.time);
        match layer_action {
            Some(1) => self.duplicate_layer(),
            Some(2) => self.merge_down(now),
            Some(3) => self.open_adjust(now),
            Some(4) => {
                self.begin_transform(now);
            }
            Some(5) => {
                if let (Some(id), Some(b)) = (self.active_layer, &self.book) {
                    if let Some(l) = b.page().doc.layer(id) {
                        self.rename_layer = Some((id, l.props.name.clone()));
                    }
                }
            }
            Some(6) => self.import_image_dialog(),
            _ => {}
        }
    }

    fn layers_pane_inner(&mut self, ui: &mut Ui, layer_action: &mut Option<u8>) {
        let Some(b) = &mut self.book else { return };
        let page = &mut b.pages[b.current];
        let cel_ids: Vec<LayerId> = page
            .timeline
            .as_ref()
            .map(|t| t.tracks.iter().flat_map(|t| t.cels.clone()).collect())
            .unwrap_or_default();
        let active = self.active_layer;
        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(6, 4))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                if let Some(id) = active {
                    if let Some(l) = page.doc.layer(id) {
                        let mut props = l.props.clone();
                        let mut changed = false;
                        ui.horizontal(|ui| {
                            egui::ComboBox::from_id_salt("blend")
                                .width(130.0)
                                .selected_text(tr(props.blend.name()))
                                .show_ui(ui, |ui| {
                                    for m in BlendMode::ALL {
                                        changed |= ui
                                            .selectable_value(&mut props.blend, m, tr(m.name()))
                                            .changed();
                                    }
                                });
                        });
                        // Opacity: slider while dragging, one undo step when released.
                        ui.horizontal(|ui| {
                            ui.label(tr("Opacity"));
                            ui.spacing_mut().slider_width = (ui.available_width() - 70.0).max(60.0);
                            let mut op = props.opacity as f32 / 2.55;
                            let r = ui.add(
                                egui::Slider::new(&mut op, 0.0..=100.0)
                                    .suffix(" %")
                                    .max_decimals(0),
                            );
                            if r.changed() {
                                if self.opacity_drag.is_none() {
                                    self.opacity_drag = Some(page.doc.snapshot_layers());
                                }
                                if let Some(l) = page.doc.layer_mut(id) {
                                    l.props.opacity = (op * 2.55).round() as u8;
                                }
                                page.doc.mark_all_dirty();
                                page.touch();
                            }
                            if !r.dragged() && !r.has_focus() {
                                if let Some(snap) = self.opacity_drag.take() {
                                    page.push_doc(UndoEntry::Layers {
                                        label: "Layer opacity".into(),
                                        layers: snap,
                                    });
                                }
                            }
                        });
                        if page
                            .doc
                            .layer(id)
                            .is_some_and(|l| l.props.opacity != props.opacity)
                        {
                            props.opacity = page.doc.layer(id).unwrap().props.opacity;
                        }
                        ui.horizontal_wrapped(|ui| {
                            changed |= ui.checkbox(&mut props.clip_below, tr("Clip")).changed();
                            changed |= ui.checkbox(&mut props.lock_alpha, tr("Lock α")).changed();
                            changed |= ui.checkbox(&mut props.reference, tr("Ref")).changed();
                            changed |= ui.checkbox(&mut props.draft, tr("Draft")).changed();
                        });
                        if changed {
                            let snap = page.doc.snapshot_layers();
                            page.doc.layer_mut(id).unwrap().props = props;
                            page.doc.mark_all_dirty();
                            page.push_doc(UndoEntry::Layers {
                                label: "Layer properties".into(),
                                layers: snap,
                            });
                        }
                    }
                }
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 1.0;
                    if ui
                        .add(IconButton::new(icons::FILE_PLUS, tr("New layer above")).size(24.0))
                        .clicked()
                    {
                        let snap = page.doc.snapshot_layers();
                        let at = active
                            .and_then(|a| page.doc.index_of(a))
                            .map_or(page.doc.layers().len(), |i| i + 1);
                        let n = page.doc.layers().len() + 1;
                        let id = page
                            .doc
                            .add_layer(Layer::new(trf("Layer {n}", &[("n", n.to_string())])), at);
                        page.push_doc(UndoEntry::Layers {
                            label: "New layer".into(),
                            layers: snap,
                        });
                        self.active_layer = Some(id);
                    }
                    let can_delete = active.is_some_and(|a| {
                        !page.is_object_layer(a)
                            && page.doc.layers().len() > 1
                            && !cel_ids.contains(&a)
                    });
                    if ui
                        .add_enabled(
                            can_delete,
                            IconButton::new(icons::TRASH, tr("Delete layer")).size(24.0),
                        )
                        .clicked()
                    {
                        let snap = page.doc.snapshot_layers();
                        page.doc.remove_layer(active.unwrap());
                        page.push_doc(UndoEntry::Layers {
                            label: "Delete layer".into(),
                            layers: snap,
                        });
                        self.active_layer = page.doc.layers().last().map(|l| l.0);
                    }
                    if ui
                        .add_enabled(
                            can_delete,
                            IconButton::new(icons::COPY, tr("Duplicate layer (Ctrl+J)")).size(24.0),
                        )
                        .clicked()
                    {
                        *layer_action = Some(1);
                    }
                    if ui
                        .add_enabled(
                            can_delete,
                            IconButton::new(icons::ARROW_LINE_DOWN, tr("Merge down")).size(24.0),
                        )
                        .clicked()
                    {
                        *layer_action = Some(2);
                    }
                    if ui
                        .add_enabled(
                            can_delete,
                            IconButton::new(
                                icons::SLIDERS_HORIZONTAL,
                                tr("Adjust colors… (Ctrl+U)"),
                            )
                            .size(24.0),
                        )
                        .clicked()
                    {
                        *layer_action = Some(3);
                    }
                    if ui
                        .add_enabled(
                            can_delete,
                            IconButton::new(
                                icons::ARROWS_OUT_CARDINAL,
                                tr("Move & transform (V / Ctrl+T)"),
                            )
                            .size(24.0),
                        )
                        .clicked()
                    {
                        *layer_action = Some(4);
                    }
                    if ui
                        .add_enabled(
                            active.is_some_and(|a| !page.is_object_layer(a)),
                            IconButton::new(
                                icons::PENCIL_SIMPLE,
                                tr("Rename layer… (double-click)"),
                            )
                            .size(24.0),
                        )
                        .clicked()
                    {
                        *layer_action = Some(5);
                    }
                    if ui
                        .add(IconButton::new(icons::IMAGE, tr("Import image…")).size(24.0))
                        .clicked()
                    {
                        *layer_action = Some(6);
                    }
                    for (icon, label, dir) in [
                        (icons::ARROW_UP, "Move layer up", 1i32),
                        (icons::ARROW_DOWN, "Move layer down", -1),
                    ] {
                        if ui
                            .add(IconButton::new(icon, tr(label)).size(24.0))
                            .clicked()
                        {
                            if let Some(a) = active {
                                let i = page.doc.index_of(a).unwrap_or(0) as i32;
                                let j =
                                    (i + dir).clamp(0, page.doc.layers().len() as i32 - 1) as usize;
                                let snap = page.doc.snapshot_layers();
                                page.doc.move_layer(a, j);
                                page.push_doc(UndoEntry::Layers {
                                    label: "Move layer".into(),
                                    layers: snap,
                                });
                            }
                        }
                    }
                });
            });
        egui::ScrollArea::vertical()
            .id_salt("layers")
            .show(ui, |ui| {
                let layers: Vec<(LayerId, ink_core::LayerProps)> = page
                    .doc
                    .layers()
                    .iter()
                    .rev()
                    .map(|(id, l)| (*id, l.props.clone()))
                    .collect();
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
                        name: tr(&props.name).to_string(),
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
                        blend: blend_label(props.blend),
                        thumb: None,
                    };
                    match layer_row(ui, &st) {
                        LayerRowEvent::Select => self.active_layer = Some(id),
                        LayerRowEvent::ToggleVisible => {
                            if let Some(l) = page.doc.layer_mut(id) {
                                l.props.visible = !l.props.visible;
                            }
                            page.doc.mark_all_dirty();
                            page.touch();
                        }
                        LayerRowEvent::Rename => {
                            self.active_layer = Some(id);
                            if !page.is_object_layer(id) {
                                self.rename_layer = Some((id, props.name.clone()));
                            }
                        }
                        LayerRowEvent::ToggleLock => {
                            if let Some(l) = page.doc.layer_mut(id) {
                                l.props.locked = !l.props.locked;
                            }
                            page.touch();
                        }
                        _ => {}
                    }
                }
            });
    }

    fn navigator_pane(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let p = palette(ui);
        let Some(b) = &mut self.book else { return };
        let idx = b.current;
        let page = &mut b.pages[idx];
        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(8, 6))
            .show(ui, |ui| {
                let avail = ui.available_width().max(60.0);
                let stale = self.nav_thumb.as_ref().is_none_or(|(_, i)| *i != idx)
                    || (self.stroke.is_none() && self.frame_count % 20 == 0 && page.thumb_dirty);
                if stale {
                    let img = thumbnail(&page.doc, 180.0);
                    self.thumb_gen += 1;
                    let h = ctx.load_texture(
                        format!("nav{}", self.thumb_gen),
                        img,
                        egui::TextureOptions::LINEAR,
                    );
                    self.nav_thumb = Some((h, idx));
                }
                let (w, h) = (page.doc.width as f32, page.doc.height as f32);
                let box_h = (avail * h / w).min(220.0);
                let k = (avail / w).min(box_h / h);
                let (r, resp) =
                    ui.allocate_exact_size(vec2(avail, box_h), egui::Sense::click_and_drag());
                ui.painter().rect_filled(r, 2, p.canvas_surround);
                let img_rect = Rect::from_center_size(r.center(), vec2(w * k, h * k));
                if let Some((t, _)) = &self.nav_thumb {
                    ui.painter().image(
                        t.id(),
                        img_rect,
                        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                        Color32::WHITE,
                    );
                }
                // Visible area of the canvas as a polygon (correct when rotated and mirrored).
                if self.canvas_rect.is_positive() {
                    let cr = self.canvas_rect;
                    let pts: Vec<Pos2> = [
                        cr.left_top(),
                        cr.right_top(),
                        cr.right_bottom(),
                        cr.left_bottom(),
                    ]
                    .iter()
                    .map(|q| {
                        let c = self.view.to_canvas(*q);
                        pos2(img_rect.min.x + c.0 * k, img_rect.min.y + c.1 * k)
                    })
                    .collect();
                    ui.painter().with_clip_rect(r).add(egui::Shape::closed_line(
                        pts,
                        Stroke::new(1.5, p.overlay_trim),
                    ));
                }
                if let Some(ptr) = resp.interact_pointer_pos() {
                    if resp.dragged() || resp.clicked() {
                        let cx = (ptr.x - img_rect.min.x) / k;
                        let cy = (ptr.y - img_rect.min.y) / k;
                        let at = self.view.to_screen(cx, cy);
                        self.view.pan(self.canvas_rect.center() - at);
                    }
                }
                ui.label(
                    RichText::new(tr("Click or drag in the preview to move the view."))
                        .text_style(text::caption())
                        .color(p.text_muted),
                );
                let c = self.canvas_rect.center();
                ui.spacing_mut().slider_width = (avail - 110.0).max(60.0);
                ui.horizontal(|ui| {
                    ui.label(tr("Zoom"));
                    let mut z = self.view.zoom * 100.0;
                    if ui
                        .add(
                            egui::Slider::new(&mut z, 1.0..=3200.0)
                                .logarithmic(true)
                                .suffix(" %")
                                .max_decimals(1),
                        )
                        .changed()
                    {
                        self.view.set_zoom(c, z / 100.0);
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(tr("Rotation"));
                    let mut deg = self.view.angle.to_degrees();
                    if ui
                        .add(
                            egui::Slider::new(&mut deg, -180.0..=180.0)
                                .suffix("°")
                                .max_decimals(0),
                        )
                        .changed()
                    {
                        self.view.set_angle(c, deg.to_radians());
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    if ui
                        .add(
                            IconButton::new(
                                icons::ARROW_COUNTER_CLOCKWISE,
                                format!("{} (4)", tr("Rotate left 15°")),
                            )
                            .size(26.0),
                        )
                        .clicked()
                    {
                        self.view.rotate_at(c, -15f32.to_radians());
                    }
                    if ui
                        .add(
                            IconButton::new(
                                icons::ARROW_CLOCKWISE,
                                format!("{} (6)", tr("Rotate right 15°")),
                            )
                            .size(26.0),
                        )
                        .clicked()
                    {
                        self.view.rotate_at(c, 15f32.to_radians());
                    }
                    if ui
                        .add(
                            IconButton::new(
                                icons::FLIP_HORIZONTAL,
                                format!("{} (M)", tr("Flip horizontally")),
                            )
                            .selected(self.view.flip)
                            .size(26.0),
                        )
                        .clicked()
                    {
                        self.view.toggle_flip(c);
                    }
                    if ui
                        .add(Button::new(tr("Reset")).size(ButtonSize::Sm))
                        .on_hover_text(format!("{} (5)", tr("Reset rotation")))
                        .clicked()
                    {
                        self.view.set_angle(c, 0.0);
                        if self.view.flip {
                            self.view.toggle_flip(c);
                        }
                    }
                    if ui
                        .add(Button::new(tr("Fit")).size(ButtonSize::Sm))
                        .on_hover_text("Ctrl+0")
                        .clicked()
                    {
                        self.fit_pending = true;
                    }
                    if ui
                        .add(Button::new("100%").size(ButtonSize::Sm))
                        .on_hover_text("1")
                        .clicked()
                    {
                        self.view.set_zoom(c, 1.0);
                    }
                });
            });
    }

    fn pages_pane(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let mut page_action = None;
        let p = palette(ui);
        let Some(b) = &mut self.book else { return };
        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(8, 6))
            .show(ui, |ui| {
                let rtl = b.settings.binding == crate::book::Binding::Right;
                ui.horizontal(|ui| {
                    ui.set_min_height(28.0);
                    ui.label(
                        RichText::new(tr(if rtl {
                            "right binding: page 1 is on the right"
                        } else {
                            "left binding"
                        }))
                        .color(p.text_muted),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add_enabled(
                                b.pages.len() > 1,
                                Button::new(tr("Delete")).size(ButtonSize::Sm),
                            )
                            .clicked()
                        {
                            let c = b.current;
                            b.delete_page(c);
                            self.cache.clear();
                            self.thumbs.clear();
                        }
                        if ui
                            .add(
                                Button::new(tr("Read"))
                                    .icon(icons::BOOK_OPEN)
                                    .size(ButtonSize::Sm),
                            )
                            .on_hover_text(tr("Read as manga (F5)"))
                            .clicked()
                        {
                            page_action = Some(PageAction::Read);
                        }
                        if ui
                            .add(
                                Button::new(tr("Add page"))
                                    .icon(icons::PLUS)
                                    .size(ButtonSize::Sm),
                            )
                            .clicked()
                        {
                            let at = b.current + 1;
                            b.add_page(at);
                            b.current = at;
                            self.cache.clear();
                            self.thumbs.clear();
                        }
                    });
                });
                if b.pages.len() == 1 && b.settings.kind != SheetKind::Book {
                    ui.label(
                        RichText::new(tr(
                            "This sheet has one page. Create a multi-page book to manage pages.",
                        ))
                        .text_style(text::caption())
                        .color(p.text_muted),
                    );
                }
                if self.stroke.is_none() {
                    let stale = (0..b.pages.len())
                        .find(|i| b.pages[*i].thumb_dirty || !self.thumbs.contains_key(i));
                    if let Some(i) = stale {
                        let d = &b.pages[i].doc;
                        // Fit into an 84 × 90 box so portrait pages leave room for the label.
                        let img =
                            thumbnail(d, 84f32.min(90.0 * d.width as f32 / d.height.max(1) as f32));
                        self.thumb_gen += 1;
                        let h = ctx.load_texture(
                            format!("thumb{}-{}", i, self.thumb_gen),
                            img,
                            egui::TextureOptions::LINEAR,
                        );
                        self.thumbs.insert(i, h);
                        b.pages[i].thumb_dirty = false;
                        ctx.request_repaint();
                    }
                }
                let mut switch = None;
                let mut mv = None;
                let mut menu_action = None;
                egui::ScrollArea::horizontal()
                    .id_salt("pages")
                    .show(ui, |ui| {
                        let layout = if rtl {
                            egui::Layout::right_to_left(egui::Align::Min)
                        } else {
                            egui::Layout::left_to_right(egui::Align::Min)
                        };
                        ui.with_layout(layout, |ui| {
                            for i in 0..b.pages.len() {
                                let (r, resp) = ui.allocate_exact_size(
                                    vec2(96.0, 112.0),
                                    egui::Sense::click_and_drag(),
                                );
                                let sel = i == b.current;
                                ui.painter().rect_filled(
                                    r,
                                    2,
                                    if sel {
                                        p.accent_subtle
                                    } else {
                                        p.surface_sunken
                                    },
                                );
                                if let Some(h) = self.thumbs.get(&i) {
                                    let img = Rect::from_center_size(
                                        pos2(r.center().x, r.top() + 48.0),
                                        h.size_vec2(),
                                    );
                                    ui.painter().image(
                                        h.id(),
                                        img,
                                        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                                        Color32::WHITE,
                                    );
                                    ui.painter().rect_stroke(
                                        img,
                                        0,
                                        Stroke::new(1.0, p.border_strong),
                                        egui::StrokeKind::Outside,
                                    );
                                }
                                let dirty = if b.pages[i].dirty { " •" } else { "" };
                                let special = b.pages[i].role != crate::book::PageRole::Story;
                                ui.painter().text(
                                    pos2(r.center().x, r.bottom() - 8.0),
                                    Align2::CENTER_CENTER,
                                    format!("{}{dirty}", b.page_label(i)),
                                    egui::FontId::proportional(11.0),
                                    if special { p.accent } else { p.text },
                                );
                                if sel {
                                    ui.painter().rect_stroke(
                                        r,
                                        2,
                                        Stroke::new(2.0, p.focus_ring),
                                        egui::StrokeKind::Inside,
                                    );
                                }
                                let resp = resp.on_hover_text(trf(
                                    "Page {n} · drag ← → to reorder",
                                    &[("n", b.page_label(i))],
                                ));
                                resp.context_menu(|ui| {
                                    if ui
                                        .button(tr("Book info (title, author, credits)…"))
                                        .clicked()
                                    {
                                        menu_action = Some(PageAction::Info);
                                        ui.close();
                                    }
                                    if ui.button(tr("Read as manga")).clicked() {
                                        switch = Some(i);
                                        menu_action = Some(PageAction::Read);
                                        ui.close();
                                    }
                                    ui.menu_button(tr("Insert special page"), |ui| {
                                        for role in crate::book::PageRole::SPECIAL {
                                            if ui.button(tr(role.name())).clicked() {
                                                menu_action = Some(PageAction::Special(role));
                                                ui.close();
                                            }
                                        }
                                    });
                                });
                                if resp.clicked() {
                                    switch = Some(i);
                                }
                                if resp.drag_stopped() {
                                    if let Some(d) = resp.total_drag_delta() {
                                        if d.x.abs() > 60.0 {
                                            let forward = if rtl { d.x < 0.0 } else { d.x > 0.0 };
                                            mv = Some((
                                                i,
                                                forward,
                                                (d.x.abs() / 100.0).round().max(1.0) as usize,
                                            ));
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
                if let Some((i, fwd, n)) = mv {
                    let to = if fwd {
                        (i + n).min(b.pages.len() - 1)
                    } else {
                        i.saturating_sub(n)
                    };
                    if to != i {
                        b.move_page(i, to);
                        self.thumbs.clear();
                        self.cache.clear();
                    }
                }
                page_action = menu_action;
            });
        match page_action {
            Some(PageAction::Info) => self.open_book_info(),
            Some(PageAction::Read) => self.open_reader(),
            Some(PageAction::Special(r)) => self.insert_special(r),
            None => {}
        }
    }

    // =========================================================================================
    // Canvas
    // =========================================================================================

    fn canvas_pane(&mut self, ui: &mut Ui, now: f64) {
        let p = palette(ui);
        let rect = ui.available_rect_before_wrap();
        let resp = ui.allocate_rect(rect, egui::Sense::click_and_drag());
        ui.painter().rect_filled(rect, 0, p.canvas_surround);
        self.canvas_rect = rect;
        let Some(book) = &mut self.book else {
            if self.new_sheet.is_none() {
                ui.painter().text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    tr("Create a sheet to start (Ctrl+N)"),
                    egui::FontId::proportional(16.0),
                    p.text_muted,
                );
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
        let (scroll, zoom_delta, rotation_delta, space, r_key, pointer) = ui.input(|i| {
            (
                i.smooth_scroll_delta,
                i.zoom_delta(),
                i.multi_touch().map_or(0.0, |t| t.rotation_delta),
                i.key_down(Key::Space),
                i.key_down(Key::R),
                i.pointer.hover_pos(),
            )
        });
        if resp.hovered() {
            if let Some(ptr) = pointer {
                // Ctrl/Cmd + wheel and pinch arrive as zoom_delta; plain wheel / two-finger scroll pans.
                if zoom_delta != 1.0 {
                    self.view.zoom_at(ptr, zoom_delta);
                }
                if rotation_delta != 0.0 {
                    self.view.rotate_at(ptr, rotation_delta);
                }
                if scroll != egui::Vec2::ZERO {
                    self.view.pan(scroll);
                }
            }
        }
        let middle = ui.input(|i| i.pointer.middle_down());
        let typing = ui.ctx().egui_wants_keyboard_input();
        let rotating = self.tool == Tool::Rotate || (r_key && !typing);
        let hand = !rotating && (self.tool == Tool::Hand || (space && !typing) || middle);
        if hand && resp.dragged() {
            self.view.pan(resp.drag_delta());
        }
        if rotating {
            let c = rect.center();
            match resp.interact_pointer_pos().filter(|_| resp.dragged()) {
                Some(ptr) => {
                    let a = (ptr - c).angle();
                    if let Some(prev) = self.rotate_drag {
                        self.view.rotate_at(c, crate::canvas::wrap_angle(a - prev));
                    }
                    self.rotate_drag = Some(a);
                }
                None => self.rotate_drag = None,
            }
            if resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::AllScroll);
            }
        } else {
            self.rotate_drag = None;
        }
        self.hover_canvas = pointer.filter(|p| rect.contains(*p)).map(|p| {
            let c = self.view.to_canvas(p);
            Pt::new(c.0, c.1)
        });

        // ---- pen / tool input ------------------------------------------------------------------
        self.alt_held = ui.input(|i| i.modifiers.alt);
        if self.cache.gray != self.value_check {
            self.cache.gray = self.value_check;
            self.cache.clear();
        }
        self.shift_held = ui.input(|i| i.modifiers.shift);
        if !hand
            && !rotating
            && self.new_sheet.is_none()
            && self.file_dialog.is_none()
            && self.confirm.is_none()
            && self.adjust.is_none()
            && self.rename_layer.is_none()
            && self.book_info.is_none()
        {
            let events = ui.input(|i| i.events.clone());
            let blocked = ui.ctx().is_pointer_over_egui() && !resp.hovered() && !resp.dragged();
            let pen_events = self
                .pen
                .process(&events, now, |pos| rect.contains(pos) && !blocked);
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
        let v = self.view;
        let page_quad = v.quad(0.0, 0.0, w as f32, h as f32);
        painter.add(egui::Shape::convex_polygon(
            page_quad.iter().map(|q| *q + vec2(3.0, 4.0)).collect(),
            Color32::from_black_alpha(80),
            Stroke::NONE,
        ));
        if page.doc.paper.a() == 0 {
            painter.add(egui::Shape::convex_polygon(
                page_quad.to_vec(),
                Color32::from_gray(205),
                Stroke::NONE,
            ));
        }
        self.cache
            .paint(ui.ctx(), &painter, &page.doc, &v, rect, 96);

        if book.settings.kind.has_print_guides() {
            let (tlo, thi) = book.settings.trim_rect();
            let (slo, shi) = book.settings.safe_rect();
            for (lo, hi, c) in [(tlo, thi, p.overlay_trim), (slo, shi, p.overlay_safe)] {
                dashed_quad(&painter, v.quad(lo.x, lo.y, hi.x, hi.y), c);
            }
        }
        let page = &book.pages[page_idx];
        match self.tool {
            Tool::Panel => {
                for (i, pn) in page.panels.panels.iter().enumerate() {
                    let pts: Vec<Pos2> = pn.poly.iter().map(|q| v.to_screen(q.x, q.y)).collect();
                    let sel = Some(i) == self.selected_panel;
                    painter.add(egui::Shape::closed_line(
                        pts,
                        Stroke::new(
                            if sel { 2.5 } else { 1.0 },
                            if sel {
                                p.focus_ring
                            } else {
                                p.overlay_frame_preview
                            },
                        ),
                    ));
                }
                if let Some((a, b2)) = self.panel_drag {
                    match self.panel_mode {
                        PanelMode::Divide => dual_stroke_line(
                            &painter,
                            v.to_screen(a.x, a.y),
                            v.to_screen(b2.x, b2.y),
                            p.overlay_frame_preview,
                        ),
                        _ => dashed_quad(
                            &painter,
                            v.quad(a.x.min(b2.x), a.y.min(b2.y), a.x.max(b2.x), a.y.max(b2.y)),
                            p.overlay_frame_preview,
                        ),
                    }
                }
            }
            Tool::Balloon => {
                for bl in &page.balloons {
                    let selected = Some(bl.id) == self.selected_balloon;
                    let dragging = self.balloon_drag.as_ref().is_some_and(|d| d.id == bl.id);
                    if dragging {
                        let pts: Vec<Pos2> =
                            bl.outline().iter().map(|q| v.to_screen(q.x, q.y)).collect();
                        painter.add(egui::Shape::closed_line(
                            pts,
                            Stroke::new(2.0, p.overlay_frame_preview),
                        ));
                        if let Some(t) = bl.tail_polygon() {
                            let pts: Vec<Pos2> = t.iter().map(|q| v.to_screen(q.x, q.y)).collect();
                            painter.add(egui::Shape::closed_line(
                                pts,
                                Stroke::new(2.0, p.overlay_frame_preview),
                            ));
                        }
                        for (c, r) in bl.tail_bubbles() {
                            painter.circle_stroke(
                                v.to_screen(c.x, c.y),
                                r * v.zoom,
                                Stroke::new(2.0, p.overlay_frame_preview),
                            );
                        }
                    }
                    if selected || dragging {
                        let (c, hh) = (bl.center, bl.half);
                        let q = v.quad(c.x - hh.x, c.y - hh.y, c.x + hh.x, c.y + hh.y);
                        dashed_quad(&painter, q, p.focus_ring);
                        for corner in q {
                            handle(ui, corner, false);
                        }
                        if bl.tail != TailStyle::None {
                            painter.circle(
                                v.to_screen(bl.tail_tip.x, bl.tail_tip.y),
                                6.0,
                                p.overlay_handle,
                                Stroke::new(1.5, p.focus_ring),
                            );
                        }
                    }
                }
            }
            Tool::Brush | Tool::Eraser if !hand && !rotating => {
                if let Some(ptr) = pointer.filter(|p| rect.contains(*p)) {
                    let r = (self.size_of(self.current_preset_id()) * 0.5 * v.zoom).max(1.5);
                    painter.circle_stroke(ptr, r, Stroke::new(2.5, Color32::from_white_alpha(140)));
                    painter.circle_stroke(ptr, r, Stroke::new(1.0, Color32::from_black_alpha(200)));
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
                }
            }
            Tool::Fill => ui.ctx().set_cursor_icon(egui::CursorIcon::Cell),
            Tool::LassoFill => {
                if self.lasso.len() > 1 {
                    let pts: Vec<Pos2> = self.lasso.iter().map(|q| v.to_screen(q.x, q.y)).collect();
                    let c = self.color.to_color32();
                    painter.add(egui::Shape::convex_polygon(
                        pts.clone(),
                        Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), 90),
                        Stroke::NONE,
                    ));
                    painter.add(egui::Shape::closed_line(
                        pts.clone(),
                        Stroke::new(2.5, Color32::from_white_alpha(160)),
                    ));
                    painter.add(egui::Shape::closed_line(
                        pts,
                        Stroke::new(1.0, Color32::from_black_alpha(220)),
                    ));
                }
                ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
            }
            Tool::Gradient => {
                if let Some((a, b2)) = self.gradient {
                    let (sa, sb) = (v.to_screen(a.x, a.y), v.to_screen(b2.x, b2.y));
                    dual_stroke_line(&painter, sa, sb, p.overlay_frame_preview);
                    painter.circle(
                        sa,
                        5.0,
                        self.color.to_color32(),
                        Stroke::new(1.5, Color32::WHITE),
                    );
                    let e = self.sub_color.to_straight();
                    painter.circle(
                        sb,
                        5.0,
                        Color32::from_rgb(e[0], e[1], e[2]),
                        Stroke::new(1.5, Color32::WHITE),
                    );
                }
                ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
            }
            Tool::Eyedropper => ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair),
            _ => {}
        }
        self.paint_ruler(&painter, pointer.filter(|p| rect.contains(*p)));
        let ctx2 = ui.ctx().clone();
        self.paint_transform(&ctx2, &painter, pointer.filter(|p| rect.contains(*p)));
        if self.transform.is_some() {
            ui.ctx().request_repaint();
        }
        if self.value_check {
            painter.text(
                rect.left_top() + vec2(10.0, 10.0),
                Align2::LEFT_TOP,
                tr("Value check (grayscale view)"),
                egui::FontId::proportional(12.0),
                p.warning,
            );
        }
        if hand && resp.hovered() {
            ui.ctx().set_cursor_icon(if resp.dragged() {
                egui::CursorIcon::Grabbing
            } else {
                egui::CursorIcon::Grab
            });
        }
        if self.stroke.is_some() {
            ui.ctx().request_repaint();
        }
    }

    fn tool_event(&mut self, ev: PenEvent, now: f64) {
        let to_pt = |view: &View, s: &PenSample| {
            let c = view.to_canvas(s.pos);
            Pt::new(c.0, c.1)
        };
        match self.tool {
            Tool::Brush | Tool::Eraser | Tool::Fill | Tool::LassoFill | Tool::Gradient
                if self.alt_pick || matches!(ev, PenEvent::Down(_)) && self.alt_down() =>
            {
                // Alt + click picks a color with any painting tool.
                if let PenEvent::Down(_) = ev {
                    self.alt_pick = true;
                }
                if let PenEvent::Up(_) = ev {
                    self.alt_pick = false;
                }
                self.eyedropper_event(ev);
            }
            Tool::Brush | Tool::Eraser => self.brush_event(ev, now),
            Tool::Fill => {
                if let PenEvent::Down(s) = ev {
                    let pt = to_pt(&self.view, &s);
                    let color = to_rgba(self.color.to_color32());
                    let Some(target) = self.target_layer(now) else {
                        return;
                    };
                    let book = self.book.as_mut().unwrap();
                    let page = book.page_mut();
                    match ink_raster::fill::fill(
                        &mut page.doc,
                        target,
                        pt.x as i32,
                        pt.y as i32,
                        color,
                        &self.fill,
                    ) {
                        Some(e) => {
                            page.push_doc(e);
                            self.remember_color();
                        }
                        None => self.toasts.push((
                            ToastKind::Info,
                            tr("Nothing to fill there").into(),
                            now,
                        )),
                    }
                }
            }
            Tool::Panel => {
                let mode = self.panel_mode;
                let pt = match ev {
                    PenEvent::Down(s) | PenEvent::Move(s) | PenEvent::Up(s) => {
                        to_pt(&self.view, &s)
                    }
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
                        let Some((a, b)) = self.panel_drag.take() else {
                            return;
                        };
                        let small = a.sub(b).len() * self.view.zoom < 6.0;
                        if small || mode == PanelMode::Select {
                            self.selected_panel = page.panels.hit(b);
                        } else if mode == PanelMode::Rectangle {
                            page.push_objects();
                            page.panels.add_rect(a, b);
                            self.selected_panel = Some(page.panels.panels.len() - 1);
                            page.render_panels();
                        } else {
                            match page.panels.hit(a.lerp(b, 0.5)) {
                                Some(i) => {
                                    page.push_objects();
                                    if page.panels.divide(i, a, b) {
                                        page.render_panels();
                                    }
                                }
                                None => self.toasts.push((
                                    ToastKind::Info,
                                    tr("Drag the cut across a panel").into(),
                                    now,
                                )),
                            }
                        }
                    }
                }
            }
            Tool::Balloon => self.balloon_event(ev, now),
            Tool::LassoFill => self.lasso_event(ev, now),
            Tool::Gradient => self.gradient_event(ev, now),
            Tool::Eyedropper => self.eyedropper_event(ev),
            Tool::Ruler => self.ruler_event(ev),
            Tool::Transform => self.transform_event(ev, now),
            Tool::Hand | Tool::Rotate => {}
        }
    }

    fn target_layer(&mut self, now: f64) -> Option<LayerId> {
        let book = self.book.as_mut()?;
        let page = book.page_mut();
        let mut target = self
            .active_layer
            .filter(|id| page.doc.layer(*id).is_some())
            .unwrap_or(page.ink_layer);
        if let Some(tl) = page.timeline.as_mut() {
            if let Some(ti) = tl.tracks.iter().position(|t| t.cels.contains(&target)) {
                let frame = page.frame;
                self.anim_track = ti;
                target = tl.drawing_cel(&mut page.doc, ti, frame)?;
                tl.apply(&mut page.doc, frame);
                self.active_layer = Some(target);
            }
        }
        if page.is_object_layer(target) {
            self.toasts.push((
                ToastKind::Warning,
                tr("This layer holds panels or balloons. Select a drawing layer.").into(),
                now,
            ));
            return None;
        }
        if page.doc.layer(target).is_some_and(|l| l.props.locked) {
            self.toasts
                .push((ToastKind::Warning, tr("The layer is locked.").into(), now));
            return None;
        }
        Some(target)
    }

    /// Alt (Option) held: color pick with painting tools, freehand with rulers.
    fn alt_down(&self) -> bool {
        self.alt_held
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
                let Some(target) = self.target_layer(now) else {
                    return;
                };
                let id = self.current_preset_id();
                let mut params = self.preset(id).params.clone();
                let size = self.size_of(id);
                let k = size / params.size.max(1e-3);
                params.size = size;
                params.taper_start *= k;
                params.taper_end *= k;
                if let Some(op) = self.opacity.get(id) {
                    params.opacity = op / 100.0;
                }
                if let Some(st) = self.stabilization.get(id) {
                    params.stabilization = *st;
                }
                let color = to_rgba(self.color.to_color32());
                let mut smp = sample(&self.view, &s);
                let mut st = {
                    let page = self.book.as_mut().unwrap().page_mut();
                    BrushStroke::begin(&page.doc, target, params, color)
                };
                let alt = self.alt_down();
                self.begin_ruler_stroke(&mut st, alt);
                self.snap_sample(&mut smp);
                let page = self.book.as_mut().unwrap().page_mut();
                st.add(&mut page.doc, smp);
                self.stroke = Some(st);
                if self.tool == Tool::Brush {
                    self.remember_color();
                }
            }
            PenEvent::Move(s) => {
                let mut smp = sample(&self.view, &s);
                self.snap_sample(&mut smp);
                if let (Some(st), Some(book)) = (&mut self.stroke, &mut self.book) {
                    st.add(&mut book.pages[book.current].doc, smp);
                }
            }
            PenEvent::Up(s) => {
                let mut smp = sample(&self.view, &s);
                self.snap_sample(&mut smp);
                self.ruler_snap = None;
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
        let Some(book) = self.book.as_mut() else {
            return;
        };
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
                let mut order: Vec<usize> = (0..page.balloons.len()).rev().collect();
                if let Some(sel) = self
                    .selected_balloon
                    .and_then(|id| page.balloons.iter().position(|b| b.id == id))
                {
                    order.retain(|i| *i != sel);
                    order.insert(0, sel);
                }
                let hit = order
                    .into_iter()
                    .find_map(|i| page.balloons[i].hit(pt, slop).map(|h| (i, h)));
                match hit {
                    Some((i, h)) => {
                        let bl = page.balloons[i].clone();
                        self.selected_balloon = Some(bl.id);
                        self.balloon_style = bl.style;
                        page.render_balloons(&self.fonts, Some(bl.id));
                        self.balloon_drag = Some(BalloonDrag {
                            id: bl.id,
                            hit: h,
                            start: pt,
                            orig: bl,
                            moved: false,
                        });
                    }
                    None => {
                        page.push_objects();
                        let id = page.new_balloon_id();
                        let placeholder = if self.balloon_dir == TextDir::Vertical {
                            "セリフ"
                        } else {
                            tr("Text")
                        };
                        let mut bl = Balloon::new(
                            id,
                            self.balloon_style,
                            pt,
                            dpi,
                            placeholder,
                            self.balloon_dir,
                        );
                        bl.font_px = self.balloon_pt * dpi as f32 / 72.0;
                        bl.fit_to_text(&self.fonts);
                        bl.tail_tip = Pt::new(pt.x - bl.half.x * 0.4, pt.y + bl.half.y * 1.6);
                        page.balloons.push(bl);
                        page.render_balloons(&self.fonts, None);
                        self.selected_balloon = Some(id);
                        self.focus_text = true;
                        self.text_snapshot = true;
                        if !self.hinted_balloon {
                            self.hinted_balloon = true;
                            self.toasts.push((
                                ToastKind::Info,
                                tr("Balloon placed. Type the text in the Text palette.").into(),
                                now,
                            ));
                        }
                    }
                }
            }
            1 => {
                if let Some(d) = &mut self.balloon_drag {
                    if !d.moved && pt.sub(d.start).len() * self.view.zoom > 3.0 {
                        d.moved = true;
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

    fn export_dir(&self) -> PathBuf {
        if let Some(d) = &self.export_to {
            return d.clone();
        }
        if let Ok(d) = std::env::var("INKSTUDIO_EXPORT_DIR") {
            return d.into();
        }
        crate::settings::home_dir()
            .join("Pictures")
            .join("Inkstudio")
    }

    pub fn export_pages(&mut self, all: bool, now: f64) {
        let dir = self.export_dir();
        let name = sanitize(&self.book_name());
        let Some(book) = &self.book else { return };
        if let Err(e) = std::fs::create_dir_all(&dir) {
            self.toast(
                ToastKind::Error,
                trf("Export failed: {e}", &[("e", e.to_string())]),
                now,
            );
            return;
        }
        let mode = book.settings.color_mode;
        let range: Vec<usize> = if all {
            (0..book.pages.len()).collect()
        } else {
            vec![book.current]
        };
        let mut last = None;
        for i in range {
            let pg = &book.pages[i];
            let rgba = pg.doc.flatten_rgba8();
            let path = dir.join(format!("{name}-p{:02}.png", i + 1));
            if let Err(e) = write_png(&path, pg.doc.width, pg.doc.height, &rgba, mode) {
                self.toast(
                    ToastKind::Error,
                    trf("Export failed: {e}", &[("e", e)]),
                    now,
                );
                return;
            }
            last = Some(path);
        }
        if let Some(p) = last {
            self.toast(
                ToastKind::Success,
                trf("Exported to {f}", &[("f", p.display().to_string())]),
                now,
            );
            self.last_export = Some(p);
        }
    }

    pub fn export_gif(&mut self, now: f64) {
        let dir = self.export_dir();
        let name = sanitize(&self.book_name());
        let Some(book) = &mut self.book else { return };
        let page = book.page_mut();
        let Some(tl) = page.timeline.clone() else {
            return;
        };
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join(format!("{name}.gif"));
        let scale = ((page.doc.width.max(page.doc.height) as f32 / 1280.0).ceil() as u32).max(1);
        let res = std::fs::File::create(&path)
            .map_err(|e| e.to_string())
            .and_then(|f| {
                ink_anim::export_gif(&mut page.doc, &tl, scale, std::io::BufWriter::new(f))
            });
        let frame = page.frame;
        page.set_frame(frame);
        match res {
            Ok(()) => {
                self.toast(
                    ToastKind::Success,
                    trf("Exported to {f}", &[("f", path.display().to_string())]),
                    now,
                );
                self.last_export = Some(path);
            }
            Err(e) => self.toast(
                ToastKind::Error,
                trf("Export failed: {e}", &[("e", e)]),
                now,
            ),
        }
    }
}

/// Dock tab rendering.
struct Viewer<'a> {
    app: &'a mut InkApp,
    now: f64,
}

impl egui_dock::TabViewer for Viewer<'_> {
    type Tab = Pane;

    fn id(&mut self, tab: &mut Pane) -> egui::Id {
        egui::Id::new(("pane", *tab))
    }

    fn title(&mut self, tab: &mut Pane) -> egui::WidgetText {
        let title = match tab {
            Pane::Canvas => match &self.app.book {
                None => tr("Canvas").to_string(),
                Some(_) => format!(
                    "{}{}",
                    self.app.book_name(),
                    if self.app.is_dirty() { " •" } else { "" }
                ),
            },
            Pane::SubTool => {
                let t = match self.app.tool {
                    Tool::Brush => tr("Brushes"),
                    Tool::Eraser => tr("Eraser"),
                    Tool::Fill => tr("Fill"),
                    Tool::LassoFill => tr("Lasso fill"),
                    Tool::Gradient => tr("Gradient"),
                    Tool::Eyedropper => tr("Eyedropper"),
                    Tool::Ruler => tr("Rulers"),
                    Tool::Transform => tr("Transform"),
                    Tool::Panel => tr("Panel templates"),
                    Tool::Balloon => tr("Balloon styles"),
                    Tool::Hand | Tool::Rotate => tr("Tool"),
                };
                format!("{} · {t}", tr("Sub tool"))
            }
            other => tr(other.title()).to_string(),
        };
        title.into()
    }

    fn ui(&mut self, ui: &mut Ui, tab: &mut Pane) {
        match tab {
            Pane::Canvas => self.app.canvas_pane(ui, self.now),
            Pane::SubTool => self.app.sub_tool_pane(ui),
            Pane::ToolProperty => self.app.tool_property_pane(ui),
            Pane::Color => self.app.color_pane(ui),
            Pane::Layers => self.app.layers_pane(ui),
            Pane::Navigator => self.app.navigator_pane(ui),
            Pane::Pages => self.app.pages_pane(ui),
            Pane::Timeline => self.app.timeline_pane(ui),
        }
    }

    fn is_closeable(&self, tab: &Pane) -> bool {
        *tab != Pane::Canvas
    }

    fn allowed_in_windows(&self, tab: &mut Pane) -> bool {
        *tab != Pane::Canvas
    }

    fn clear_background(&self, tab: &Pane) -> bool {
        *tab != Pane::Canvas
    }

    fn scroll_bars(&self, _tab: &Pane) -> [bool; 2] {
        [false, false]
    }
}

/// Dock state of a layout: its saved arrangement, else the built-in default, else Standard.
fn load_layout(settings: &Settings, id: &str) -> DockState<Pane> {
    settings
        .layouts
        .get(id)
        .and_then(|s| ron::from_str::<DockState<Pane>>(s).ok())
        .filter(|d| d.find_tab(&Pane::Canvas).is_some())
        .or_else(|| crate::layouts::builtin(id))
        .unwrap_or_else(default_dock)
}

fn modal_frame(p: &theme::Palette) -> egui::Frame {
    egui::Frame::new()
        .fill(p.surface_raised)
        .stroke(Stroke::new(1.0, p.border_strong))
        .corner_radius(tokens::radius::LG as u8)
        .inner_margin(20)
}

/// Translated blend-mode name (`tr` keeps the `'static` lifetime of its input).
fn blend_label(m: BlendMode) -> &'static str {
    tr(m.name())
}

pub fn hsv_from(c: Color32) -> Hsv {
    let (r, g, b) = (
        c.r() as f32 / 255.0,
        c.g() as f32 / 255.0,
        c.b() as f32 / 255.0,
    );
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
    Hsv {
        h,
        s: if max == 0.0 { 0.0 } else { d / max },
        v: max,
    }
}

fn sanitize(s: &str) -> String {
    let t: String = s
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if t.is_empty() { "inkstudio".into() } else { t }
}

fn dashed_quad(painter: &egui::Painter, q: [Pos2; 4], c: Color32) {
    let pts = [q[0], q[1], q[2], q[3], q[0]];
    for w in pts.windows(2) {
        painter.add(egui::Shape::dashed_line(
            &[w[0], w[1]],
            Stroke::new(1.0, c),
            6.0,
            4.0,
        ));
    }
}

/// Small preview of a balloon style for the picker.
fn draw_style_icon(painter: &egui::Painter, r: Rect, style: BalloonStyle) {
    let mut b = Balloon::new(1, style, Pt::new(0.0, 0.0), 72, "", TextDir::Horizontal);
    b.half = Pt::new(1.0, 0.62);
    b.tail_tip = Pt::new(-0.55, 1.25);
    b.font_px = 0.3;
    let (c, k) = (
        r.center() - vec2(0.0, 4.0),
        (r.width() / 2.6).min(r.height() / 2.4),
    );
    let map = |q: &Pt| pos2(c.x + q.x * k, c.y + q.y * k);
    let ink = Color32::from_gray(25);
    let line = Stroke::new(1.4, ink);
    if style == BalloonStyle::TextOnly {
        painter.text(
            c,
            Align2::CENTER_CENTER,
            "Aa あ",
            egui::FontId::proportional(16.0),
            Color32::from_gray(220),
        );
        return;
    }
    let body: Vec<Pos2> = b.outline().iter().map(map).collect();
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
        painter.add(egui::Shape::convex_polygon(tp, Color32::WHITE, line));
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
    let (tw, th) = (
        (doc.width as f32 * k).round().max(1.0) as usize,
        (doc.height as f32 * k).round().max(1.0) as usize,
    );
    let step = (1.0 / k).max(1.0);
    let mut cache: HashMap<(i32, i32), ink_core::Tile> = HashMap::new();
    let mut px = Vec::with_capacity(tw * th);
    for y in 0..th {
        for x in 0..tw {
            let (cx, cy) = (
                ((x as f32 + 0.5) * step) as i32,
                ((y as f32 + 0.5) * step) as i32,
            );
            let (key, lx, ly) =
                ink_core::tile::split(cx.min(doc.width as i32 - 1), cy.min(doc.height as i32 - 1));
            let t = cache
                .entry(key)
                .or_insert_with(|| doc.composite_tile(key, true));
            let c = t.get(lx, ly).to_straight();
            px.push(Color32::from_rgba_unmultiplied(c[0], c[1], c[2], 255));
        }
    }
    egui::ColorImage {
        size: [tw, th],
        source_size: vec2(tw as f32, th as f32),
        pixels: px,
    }
}

fn write_png(
    path: &std::path::Path,
    w: u32,
    h: u32,
    rgba: &[u8],
    mode: ColorMode,
) -> Result<(), String> {
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
            rgba.chunks_exact(4)
                .map(|c| ((c[0] as u32 * 54 + c[1] as u32 * 183 + c[2] as u32 * 19) >> 8) as u8)
                .collect()
        }
        ColorMode::Mono => {
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
    enc.write_header()
        .map_err(|e| e.to_string())?
        .write_image_data(&data)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hsv_roundtrip() {
        for c in [
            Color32::from_rgb(200, 30, 90),
            Color32::from_rgb(10, 200, 40),
            Color32::WHITE,
            Color32::BLACK,
        ] {
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
        let rgba: Vec<u8> = (0..16 * 4)
            .flat_map(|i| {
                if i % 2 == 0 {
                    [0, 0, 0, 255]
                } else {
                    [255, 255, 255, 255]
                }
            })
            .collect();
        write_png(&path, 16, 4, &rgba, ColorMode::Mono).unwrap();
        let dec = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(&path).unwrap()));
        let r = dec.read_info().unwrap();
        assert_eq!(r.info().bit_depth, png::BitDepth::One);
    }

    #[test]
    fn default_dock_has_every_pane() {
        let d = default_dock();
        assert!(d.find_tab(&Pane::Canvas).is_some());
        for p in Pane::PALETTES {
            assert!(d.find_tab(&p).is_some(), "{p:?}");
        }
        let back: DockState<Pane> = ron::from_str(&ron::to_string(&d).unwrap()).unwrap();
        assert!(back.find_tab(&Pane::Timeline).is_some());
    }

    #[test]
    fn layouts_keep_their_own_arrangement_and_survive_restart() {
        let cfg = std::env::temp_dir().join(format!("inkstudio-layouts-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&cfg);
        let ctx = egui::Context::default();
        let mut app = InkApp::with_config(&ctx, cfg.clone());
        app.switch_layout("animation");
        assert!(app.dock.find_tab(&Pane::Timeline).is_some());
        // Close the color palette in "animation" only.
        let path = app.dock.find_tab(&Pane::Color).unwrap();
        app.dock.remove_tab(path);
        app.switch_layout("manga");
        assert!(
            app.dock.find_tab(&Pane::Color).is_some(),
            "manga is untouched"
        );
        assert!(app.dock.find_tab(&Pane::Timeline).is_none());
        app.switch_layout("animation");
        assert!(
            app.dock.find_tab(&Pane::Color).is_none(),
            "animation remembers its change"
        );
        app.reset_layout();
        assert!(
            app.dock.find_tab(&Pane::Color).is_some(),
            "reset restores the default"
        );
        // A user layout.
        let path = app.dock.find_tab(&Pane::Navigator).unwrap();
        app.dock.remove_tab(path);
        assert!(app.save_layout_as("Mein Layout"));
        app.persist_settings();
        let app2 = InkApp::with_config(&ctx, cfg.clone());
        assert_eq!(app2.settings.active_layout, "custom:Mein Layout");
        assert!(app2.dock.find_tab(&Pane::Navigator).is_none());
        // Opening an animation sheet switches to the animation layout.
        let mut app3 = InkApp::with_config(&ctx, cfg.clone());
        app3.open(SheetSettings {
            kind: SheetKind::Animation,
            trim_mm: (50.0, 30.0),
            dpi: 72,
            ..Default::default()
        });
        assert_eq!(app3.settings.active_layout, "animation");
        app3.delete_layout("Mein Layout");
        assert!(app3.settings.custom_layouts.is_empty());
    }

    /// Every `tr("…")` literal in the UI sources and every name passed through tr() at
    /// runtime has a German translation.
    #[test]
    fn every_ui_string_is_translated() {
        let literals = |src: &str| -> Vec<String> {
            let mut out = Vec::new();
            let mut rest = src;
            while let Some(i) = rest.find("tr(\"") {
                let s = &rest[i + 4..];
                let b = s.as_bytes();
                let mut end = 0;
                while end < b.len() && !(b[end] == b'"' && (end == 0 || b[end - 1] != b'\\')) {
                    end += 1;
                }
                out.push(s[..end].replace("\\n", "\n"));
                rest = &s[end..];
            }
            out
        };
        let mut missing = Vec::new();
        for src in [
            include_str!("app.rs"),
            include_str!("new_sheet.rs"),
            include_str!("files.rs"),
            include_str!("app/rulers.rs"),
            include_str!("app/coloring.rs"),
            include_str!("app/book_ui.rs"),
            include_str!("app/reader.rs"),
            include_str!("app/timeline.rs"),
            include_str!("app/transform.rs"),
            include_str!("app/adjust.rs"),
            include_str!("book.rs"),
        ] {
            let src = &src[..src.find("#[cfg(test)]").unwrap_or(src.len())];
            missing.extend(
                literals(src)
                    .into_iter()
                    .filter(|s| !ink_ui::i18n::has_de(s)),
            );
        }
        let names = Tool::ALL
            .iter()
            .map(|t| t.2)
            .chain(Pane::PALETTES.iter().map(|p| p.title()))
            .chain(BalloonStyle::ALL.iter().map(|b| b.name()))
            .chain(PanelTemplate::ALL.iter().map(|t| t.name()))
            .chain(Category::ALL.iter().map(|c| c.name()))
            .chain(BlendMode::ALL.iter().map(|m| m.name()))
            .chain(SheetKind::ALL.iter().map(|k| k.name()))
            .chain(
                [ColorMode::Color, ColorMode::Gray, ColorMode::Mono]
                    .iter()
                    .map(|m| m.name()),
            )
            .chain(crate::new_sheet::FORMATS.iter().map(|f| f.0))
            .chain(crate::layouts::BUILTIN.iter().map(|l| l.1))
            .chain(rulers::RulerKind::ALL.iter().map(|k| k.name()))
            .chain(crate::book::PageRole::SPECIAL.iter().map(|r| r.name()))
            .map(String::from)
            .chain(
                crate::settings::default_color_sets()
                    .into_iter()
                    .map(|s| s.0),
            )
            .chain(
                catalog()
                    .into_iter()
                    .flat_map(|p| [p.params.name, p.use_for.to_string()]),
            );
        missing.extend(names.filter(|s| {
            !ink_ui::i18n::has_de(s)
                && !s
                    .chars()
                    .all(|c| !c.is_alphabetic() || c.is_ascii_uppercase() || c.is_ascii_digit())
        }));
        missing.sort();
        missing.dedup();
        assert!(
            missing.is_empty(),
            "missing German translations: {missing:#?}"
        );
    }
}
