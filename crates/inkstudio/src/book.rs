//! A work (book) made of pages. Each page owns its document, history, panels and balloons.

use ink_anim::Timeline;
use ink_comic::{Balloon, Fonts, PanelSet, PanelTemplate};
use ink_core::{Document, History, Layer, LayerId, Rgba, UndoEntry};
use ink_vector::Pt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SheetKind {
    Illustration,
    MangaPage,
    Book,
    Webtoon,
    Animation,
}

impl SheetKind {
    pub const ALL: [SheetKind; 5] = [SheetKind::Illustration, SheetKind::MangaPage, SheetKind::Book, SheetKind::Webtoon, SheetKind::Animation];
    pub fn name(self) -> &'static str {
        match self {
            SheetKind::Illustration => "Illustration",
            SheetKind::MangaPage => "Manga page",
            SheetKind::Book => "Multi-page book",
            SheetKind::Webtoon => "Webtoon",
            SheetKind::Animation => "Animation",
        }
    }
    pub fn has_print_guides(self) -> bool {
        matches!(self, SheetKind::MangaPage | SheetKind::Book)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ColorMode {
    Color,
    Gray,
    Mono,
}

impl ColorMode {
    pub fn name(self) -> &'static str {
        match self {
            ColorMode::Color => "Color",
            ColorMode::Gray => "Grayscale",
            ColorMode::Mono => "Monochrome (1-bit)",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Binding {
    /// Japanese manga: read right to left, binding on the right.
    Right,
    Left,
}

/// Everything the "New sheet" dialog decides.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct SheetSettings {
    pub name: String,
    pub kind: SheetKind,
    /// Finished (trim) size in mm. For pixel-based kinds this is derived from px and dpi.
    pub trim_mm: (f32, f32),
    pub dpi: u32,
    pub bleed_mm: f32,
    pub safe_mm: f32,
    pub paper: Rgba,
    pub color_mode: ColorMode,
    pub pages: usize,
    pub binding: Binding,
    pub fps: f32,
    pub frames: usize,
    pub template: Option<PanelTemplate>,
}

impl Default for SheetSettings {
    fn default() -> Self {
        Self {
            name: "Untitled".into(),
            kind: SheetKind::MangaPage,
            trim_mm: (182.0, 257.0), // B5 (JIS), the common manga magazine size
            dpi: 600,
            bleed_mm: 3.0,
            safe_mm: 15.0,
            paper: Rgba::WHITE,
            color_mode: ColorMode::Mono,
            pages: 1,
            binding: Binding::Right,
            fps: 12.0,
            frames: 24,
            template: Some(PanelTemplate::Manga5),
        }
    }
}

impl SheetSettings {
    fn px(&self, mm: f32) -> f32 {
        mm * self.dpi as f32 / 25.4
    }

    /// Canvas size in pixels (trim + bleed on every side).
    pub fn canvas_px(&self) -> (u32, u32) {
        let bleed = if self.kind.has_print_guides() { self.bleed_mm } else { 0.0 };
        let w = self.px(self.trim_mm.0 + 2.0 * bleed).round().max(1.0) as u32;
        let h = self.px(self.trim_mm.1 + 2.0 * bleed).round().max(1.0) as u32;
        (w, h)
    }

    /// Trim rectangle in canvas pixels (lo, hi).
    pub fn trim_rect(&self) -> (Pt, Pt) {
        let b = if self.kind.has_print_guides() { self.px(self.bleed_mm) } else { 0.0 };
        let (w, h) = self.canvas_px();
        (Pt::new(b, b), Pt::new(w as f32 - b, h as f32 - b))
    }

    /// Safe area (inner border): where panels and text go.
    pub fn safe_rect(&self) -> (Pt, Pt) {
        let (lo, hi) = self.trim_rect();
        let s = if self.kind.has_print_guides() { self.px(self.safe_mm) } else { 0.0 };
        (Pt::new(lo.x + s, lo.y + s), Pt::new(hi.x - s, hi.y - s))
    }

    pub fn megapixels(&self) -> f32 {
        let (w, h) = self.canvas_px();
        w as f32 * h as f32 / 1e6
    }
}

/// One undoable step on a page: pixels/layers (document history) or comic objects.
enum Step {
    Doc,
    Objects { panels: PanelSet, balloons: Vec<Balloon> },
}

pub struct Page {
    pub doc: Document,
    pub history: History,
    steps: Vec<Step>,
    redo_steps: Vec<Step>,
    /// Edits since the last save (window title shows •).
    pub dirty: bool,
    pub panels: PanelSet,
    pub panel_layer: LayerId,
    pub balloons: Vec<Balloon>,
    pub balloon_layer: LayerId,
    pub ink_layer: LayerId,
    pub timeline: Option<Timeline>,
    pub frame: usize,
    pub thumb_dirty: bool,
    pub(crate) next_balloon: u64,
}

impl Page {
    pub fn new(s: &SheetSettings) -> Self {
        let (w, h) = s.canvas_px();
        let mut doc = Document::new(w, h, s.dpi);
        doc.paper = s.paper;
        let mut sketch = Layer::new("Sketch");
        sketch.props.draft = true;
        sketch.props.opacity = 140;
        doc.push_layer(sketch);
        let mut ink = Layer::new("Ink");
        ink.props.reference = true;
        let ink_layer = doc.push_layer(ink);
        let mut timeline = None;
        if s.kind == SheetKind::Animation {
            let mut tl = Timeline::new(s.fps, s.frames);
            let t = tl.add_track("Cel");
            tl.new_cel(&mut doc, t, 0);
            timeline = Some(tl);
        }
        let mut panels_l = Layer::new("Panels");
        panels_l.props.reference = true;
        let panel_layer = doc.push_layer(panels_l);
        let balloon_layer = doc.push_layer(Layer::new("Balloons & text"));
        let mut panels = PanelSet::for_dpi(s.dpi);
        if let (Some(t), true) = (s.template, s.kind.has_print_guides()) {
            let (lo, hi) = s.safe_rect();
            panels.apply_template(t, lo, hi);
        }
        let mut page = Self {
            doc,
            history: History::default(),
            steps: Vec::new(),
            redo_steps: Vec::new(),
            dirty: false,
            panels,
            panel_layer,
            balloons: Vec::new(),
            balloon_layer,
            ink_layer,
            timeline,
            frame: 0,
            thumb_dirty: true,
            next_balloon: 0,
        };
        page.render_panels();
        page
    }

    /// A page around an existing document (loading).
    pub(crate) fn new_empty(doc: Document, panel_layer: LayerId, balloon_layer: LayerId, ink_layer: LayerId) -> Self {
        Self {
            doc,
            history: History::default(),
            steps: Vec::new(),
            redo_steps: Vec::new(),
            dirty: false,
            panels: PanelSet::default(),
            panel_layer,
            balloons: Vec::new(),
            balloon_layer,
            ink_layer,
            timeline: None,
            frame: 0,
            thumb_dirty: true,
            next_balloon: 0,
        }
    }

    pub fn canvas(&self) -> (u32, u32) {
        (self.doc.width, self.doc.height)
    }

    pub fn is_object_layer(&self, id: LayerId) -> bool {
        id == self.panel_layer || id == self.balloon_layer
    }

    pub fn render_panels(&mut self) {
        let r = self.panels.render(self.canvas());
        if let Some(l) = self.doc.layer_mut(self.panel_layer) {
            l.raster = r;
        }
        self.doc.mark_all_dirty();
        self.thumb_dirty = true;
    }

    /// Re-render balloons; `skip` hides one (while it is being dragged as a preview).
    pub fn render_balloons(&mut self, fonts: &Fonts, skip: Option<u64>) {
        let list: Vec<Balloon> = self.balloons.iter().filter(|b| Some(b.id) != skip).cloned().collect();
        let r = ink_comic::balloon::render_all(&list, fonts, self.canvas());
        if let Some(l) = self.doc.layer_mut(self.balloon_layer) {
            l.raster = r;
        }
        self.doc.mark_all_dirty();
        self.thumb_dirty = true;
    }

    pub fn new_balloon_id(&mut self) -> u64 {
        self.next_balloon += 1;
        self.next_balloon
    }

    // ---- undo ------------------------------------------------------------------------------

    pub fn push_doc(&mut self, e: UndoEntry) {
        self.history.push(e);
        self.steps.push(Step::Doc);
        self.redo_steps.clear();
        self.thumb_dirty = true;
        self.dirty = true;
    }

    /// Record the comic objects *before* an edit.
    pub fn push_objects(&mut self) {
        self.steps.push(Step::Objects { panels: self.panels.clone(), balloons: self.balloons.clone() });
        self.redo_steps.clear();
        self.dirty = true;
    }

    pub fn undo(&mut self, fonts: &Fonts) -> bool {
        let Some(step) = self.steps.pop() else { return false };
        let inverse = match step {
            Step::Doc => {
                self.history.undo(&mut self.doc);
                Step::Doc
            }
            Step::Objects { panels, balloons } => {
                let cur = Step::Objects { panels: std::mem::replace(&mut self.panels, panels), balloons: std::mem::replace(&mut self.balloons, balloons) };
                self.render_panels();
                self.render_balloons(fonts, None);
                cur
            }
        };
        self.redo_steps.push(inverse);
        self.thumb_dirty = true;
        self.dirty = true;
        true
    }

    pub fn redo(&mut self, fonts: &Fonts) -> bool {
        let Some(step) = self.redo_steps.pop() else { return false };
        let inverse = match step {
            Step::Doc => {
                self.history.redo(&mut self.doc);
                Step::Doc
            }
            Step::Objects { panels, balloons } => {
                let cur = Step::Objects { panels: std::mem::replace(&mut self.panels, panels), balloons: std::mem::replace(&mut self.balloons, balloons) };
                self.render_panels();
                self.render_balloons(fonts, None);
                cur
            }
        };
        self.steps.push(inverse);
        self.thumb_dirty = true;
        self.dirty = true;
        true
    }

    pub fn can_undo(&self) -> bool {
        !self.steps.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo_steps.is_empty()
    }

    /// Show the cels of `frame` (animation pages).
    pub fn set_frame(&mut self, frame: usize) {
        if let Some(tl) = &self.timeline {
            self.frame = frame.min(tl.frames - 1);
            tl.apply(&mut self.doc, self.frame);
        }
    }
}

pub struct Book {
    pub settings: SheetSettings,
    pub pages: Vec<Page>,
    pub current: usize,
    /// Where the book was last saved / opened from.
    pub path: Option<std::path::PathBuf>,
    /// Page-level structure changed (pages added, removed, moved) since the last save.
    pub structure_dirty: bool,
}

impl Book {
    pub fn new(settings: SheetSettings) -> Self {
        let n = if settings.kind == SheetKind::Book { settings.pages.max(1) } else { 1 };
        let pages = (0..n).map(|_| Page::new(&settings)).collect();
        Self { settings, pages, current: 0, path: None, structure_dirty: false }
    }

    pub fn page(&self) -> &Page {
        &self.pages[self.current]
    }
    pub fn page_mut(&mut self) -> &mut Page {
        &mut self.pages[self.current]
    }

    pub fn add_page(&mut self, at: usize) {
        let mut s = self.settings.clone();
        s.template = self.settings.template;
        self.pages.insert(at.min(self.pages.len()), Page::new(&s));
        self.structure_dirty = true;
    }

    /// Unsaved changes anywhere in the book.
    pub fn is_dirty(&self) -> bool {
        self.structure_dirty || self.pages.iter().any(|p| p.dirty)
    }

    pub fn delete_page(&mut self, i: usize) {
        if self.pages.len() > 1 && i < self.pages.len() {
            self.pages.remove(i);
            self.current = self.current.min(self.pages.len() - 1);
            self.structure_dirty = true;
        }
    }

    pub fn move_page(&mut self, from: usize, to: usize) {
        if from < self.pages.len() && to < self.pages.len() && from != to {
            let p = self.pages.remove(from);
            self.pages.insert(to, p);
            self.current = to;
            self.structure_dirty = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn b5_manga_page_at_600_dpi() {
        let s = SheetSettings::default();
        let (w, h) = s.canvas_px();
        // 188 × 263 mm with bleed at 600 dpi.
        assert_eq!((w, h), (4441, 6213));
        let (lo, hi) = s.safe_rect();
        assert!(lo.x > 400.0 && hi.y < h as f32 - 400.0);
        let page = Page::new(&s);
        assert_eq!(page.panels.panels.len(), 5);
        assert_eq!(page.doc.layers().len(), 4);
    }

    #[test]
    fn book_pages_and_object_undo() {
        let s = SheetSettings { kind: SheetKind::Book, pages: 3, dpi: 150, template: Some(PanelTemplate::TwoRows), ..Default::default() };
        let mut b = Book::new(s);
        assert_eq!(b.pages.len(), 3);
        b.add_page(1);
        b.move_page(1, 3);
        b.delete_page(0);
        assert_eq!(b.pages.len(), 3);
        let fonts = Fonts::from_bytes(Vec::new());
        let p = b.page_mut();
        p.push_objects();
        p.panels.panels.clear();
        p.render_panels();
        assert!(p.undo(&fonts));
        assert_eq!(p.panels.panels.len(), 2);
        assert!(p.redo(&fonts));
        assert!(p.panels.panels.is_empty());
    }

    #[test]
    fn animation_page_has_a_cel() {
        let s = SheetSettings { kind: SheetKind::Animation, trim_mm: (100.0, 60.0), dpi: 96, ..Default::default() };
        let mut p = Page::new(&s);
        let tl = p.timeline.as_ref().unwrap();
        assert_eq!(tl.frames, 24);
        assert!(tl.cel_at(0, 0).is_some());
        p.set_frame(5);
        assert_eq!(p.frame, 5);
    }
}
