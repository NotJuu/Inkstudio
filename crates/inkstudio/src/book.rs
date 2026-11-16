//! A work (book) made of pages. Each page owns its document, history, panels and balloons.

use ink_anim::Timeline;
use ink_comic::{Balloon, Fonts, PanelSet, PanelTemplate};
use ink_core::{Document, History, Layer, LayerId, Rgba, UndoEntry};
#[allow(unused_imports)]
use ink_vector::Pt as _Pt;
use ink_vector::Pt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SheetKind {
    Illustration,
    MangaPage,
    Book,
    Webtoon,
    Animation,
    ConceptArt,
}

impl SheetKind {
    pub const ALL: [SheetKind; 6] = [
        SheetKind::Illustration,
        SheetKind::ConceptArt,
        SheetKind::MangaPage,
        SheetKind::Book,
        SheetKind::Webtoon,
        SheetKind::Animation,
    ];
    pub fn name(self) -> &'static str {
        match self {
            SheetKind::Illustration => "Illustration",
            SheetKind::MangaPage => "Manga page",
            SheetKind::Book => "Multi-page book",
            SheetKind::Webtoon => "Webtoon",
            SheetKind::Animation => "Animation",
            SheetKind::ConceptArt => "Concept art",
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
    /// Books: start with cover, title page, credits, afterword and back cover.
    #[serde(default)]
    pub front_matter: bool,
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
            front_matter: false,
        }
    }
}

impl SheetSettings {
    fn px(&self, mm: f32) -> f32 {
        mm * self.dpi as f32 / 25.4
    }

    /// Canvas size in pixels (trim + bleed on every side).
    pub fn canvas_px(&self) -> (u32, u32) {
        let bleed = if self.kind.has_print_guides() {
            self.bleed_mm
        } else {
            0.0
        };
        let w = self.px(self.trim_mm.0 + 2.0 * bleed).round().max(1.0) as u32;
        let h = self.px(self.trim_mm.1 + 2.0 * bleed).round().max(1.0) as u32;
        (w, h)
    }

    /// Trim rectangle in canvas pixels (lo, hi).
    pub fn trim_rect(&self) -> (Pt, Pt) {
        let b = if self.kind.has_print_guides() {
            self.px(self.bleed_mm)
        } else {
            0.0
        };
        let (w, h) = self.canvas_px();
        (Pt::new(b, b), Pt::new(w as f32 - b, h as f32 - b))
    }

    /// Safe area (inner border): where panels and text go.
    pub fn safe_rect(&self) -> (Pt, Pt) {
        let (lo, hi) = self.trim_rect();
        let s = if self.kind.has_print_guides() {
            self.px(self.safe_mm)
        } else {
            0.0
        };
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
    Objects {
        panels: PanelSet,
        balloons: Vec<Balloon>,
    },
}

pub struct Page {
    pub doc: Document,
    pub history: History,
    steps: Vec<Step>,
    redo_steps: Vec<Step>,
    /// Edits since the last save (window title shows •).
    pub dirty: bool,
    /// Counts edits; lets a background save tell whether the page changed meanwhile.
    pub generation: u64,
    pub panels: PanelSet,
    pub panel_layer: LayerId,
    pub balloons: Vec<Balloon>,
    pub balloon_layer: LayerId,
    pub ink_layer: LayerId,
    pub timeline: Option<Timeline>,
    pub frame: usize,
    pub thumb_dirty: bool,
    pub(crate) next_balloon: u64,
    pub role: PageRole,
    /// The page's ruler (perspective, symmetry, …) and whether strokes snap to it.
    pub ruler: Option<ink_vector::Ruler>,
    pub ruler_snap: bool,
}

/// What a page is in a book. Only story pages get page numbers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum PageRole {
    #[default]
    Story,
    Cover,
    Title,
    Credits,
    Contents,
    Afterword,
    BackCover,
}

impl PageRole {
    pub const SPECIAL: [PageRole; 6] = [
        PageRole::Cover,
        PageRole::Title,
        PageRole::Credits,
        PageRole::Contents,
        PageRole::Afterword,
        PageRole::BackCover,
    ];
    pub fn name(self) -> &'static str {
        match self {
            PageRole::Story => "Story page",
            PageRole::Cover => "Cover",
            PageRole::Title => "Title page",
            PageRole::Credits => "Credits",
            PageRole::Contents => "Contents",
            PageRole::Afterword => "Afterword",
            PageRole::BackCover => "Back cover",
        }
    }
    /// Where a special page goes: front matter in this order, end matter at the back.
    fn rank(self) -> u8 {
        match self {
            PageRole::Cover => 0,
            PageRole::Title => 1,
            PageRole::Credits => 2,
            PageRole::Contents => 3,
            PageRole::Story => 4,
            PageRole::Afterword => 5,
            PageRole::BackCover => 6,
        }
    }
}

/// Book metadata shown on the cover, title, credits and back cover pages.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct BookInfo {
    pub title: String,
    pub subtitle: String,
    pub author: String,
    pub artist: String,
    /// (role, name), e.g. ("Lettering", "Ana").
    pub credits: Vec<(String, String)>,
    pub publisher: String,
    pub year: String,
    pub description: String,
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
            generation: 0,
            panels,
            panel_layer,
            balloons: Vec::new(),
            balloon_layer,
            ink_layer,
            timeline,
            frame: 0,
            thumb_dirty: true,
            next_balloon: 0,
            role: PageRole::Story,
            ruler: None,
            ruler_snap: true,
        };
        page.render_panels();
        page
    }

    /// A page around an existing document (loading).
    pub(crate) fn new_empty(
        doc: Document,
        panel_layer: LayerId,
        balloon_layer: LayerId,
        ink_layer: LayerId,
    ) -> Self {
        Self {
            doc,
            history: History::default(),
            steps: Vec::new(),
            redo_steps: Vec::new(),
            dirty: false,
            generation: 0,
            panels: PanelSet::default(),
            panel_layer,
            balloons: Vec::new(),
            balloon_layer,
            ink_layer,
            timeline: None,
            frame: 0,
            thumb_dirty: true,
            next_balloon: 0,
            role: PageRole::Story,
            ruler: None,
            ruler_snap: true,
        }
    }

    /// Mark the page edited.
    pub fn touch(&mut self) {
        self.dirty = true;
        self.generation += 1;
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
        let list: Vec<Balloon> = self
            .balloons
            .iter()
            .filter(|b| Some(b.id) != skip)
            .cloned()
            .collect();
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
        self.touch();
    }

    /// Record the comic objects *before* an edit.
    pub fn push_objects(&mut self) {
        self.steps.push(Step::Objects {
            panels: self.panels.clone(),
            balloons: self.balloons.clone(),
        });
        self.redo_steps.clear();
        self.touch();
    }

    pub fn undo(&mut self, fonts: &Fonts) -> bool {
        let Some(step) = self.steps.pop() else {
            return false;
        };
        let inverse = match step {
            Step::Doc => {
                self.history.undo(&mut self.doc);
                Step::Doc
            }
            Step::Objects { panels, balloons } => {
                let cur = Step::Objects {
                    panels: std::mem::replace(&mut self.panels, panels),
                    balloons: std::mem::replace(&mut self.balloons, balloons),
                };
                self.render_panels();
                self.render_balloons(fonts, None);
                cur
            }
        };
        self.redo_steps.push(inverse);
        self.thumb_dirty = true;
        self.touch();
        true
    }

    pub fn redo(&mut self, fonts: &Fonts) -> bool {
        let Some(step) = self.redo_steps.pop() else {
            return false;
        };
        let inverse = match step {
            Step::Doc => {
                self.history.redo(&mut self.doc);
                Step::Doc
            }
            Step::Objects { panels, balloons } => {
                let cur = Step::Objects {
                    panels: std::mem::replace(&mut self.panels, panels),
                    balloons: std::mem::replace(&mut self.balloons, balloons),
                };
                self.render_panels();
                self.render_balloons(fonts, None);
                cur
            }
        };
        self.steps.push(inverse);
        self.thumb_dirty = true;
        self.touch();
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
    pub structure_generation: u64,
    /// Identifies this book's crash-recovery copy.
    pub recovery_id: u64,
    pub info: BookInfo,
}

/// A fresh, practically unique id for recovery files.
pub fn new_recovery_id() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    (t ^ ((std::process::id() as u64) << 40))
        .wrapping_add(N.fetch_add(1, Ordering::Relaxed) * 0x9E37_79B9)
}

impl Book {
    pub fn new(settings: SheetSettings) -> Self {
        let n = if settings.kind == SheetKind::Book {
            settings.pages.max(1)
        } else {
            1
        };
        let pages = (0..n).map(|_| Page::new(&settings)).collect();
        let info = BookInfo {
            title: settings.name.clone(),
            ..Default::default()
        };
        Self {
            settings,
            pages,
            current: 0,
            path: None,
            structure_dirty: false,
            structure_generation: 0,
            recovery_id: new_recovery_id(),
            info,
        }
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
        self.structure_generation += 1;
    }

    /// Page number shown for story pages (1-based), None for special pages.
    pub fn page_number(&self, i: usize) -> Option<usize> {
        if self.pages.get(i)?.role != PageRole::Story {
            return None;
        }
        Some(
            self.pages[..=i]
                .iter()
                .filter(|p| p.role == PageRole::Story)
                .count(),
        )
    }

    /// Short label for a page in the strip: number for story pages, role name otherwise.
    pub fn page_label(&self, i: usize) -> String {
        match self.page_number(i) {
            Some(n) => n.to_string(),
            None => ink_ui::tr(self.pages[i].role.name()).to_string(),
        }
    }

    /// Insert a special page at its natural place (front matter before the story, end matter
    /// after it). Replaces nothing; returns the new page's index.
    pub fn add_special(&mut self, role: PageRole, fonts: &Fonts) -> usize {
        let mut s = self.settings.clone();
        s.template = None;
        let mut page = Page::new(&s);
        page.role = role;
        let at = self
            .pages
            .iter()
            .position(|p| p.role.rank() > role.rank())
            .unwrap_or(self.pages.len());
        self.pages.insert(at, page);
        self.structure_dirty = true;
        self.structure_generation += 1;
        self.fill_special(at, fonts);
        at
    }

    /// (Re)write the text of a special page from the book info.
    pub fn fill_special(&mut self, i: usize, fonts: &Fonts) {
        let info = self.info.clone();
        let (lo, hi) = self.settings.safe_rect();
        let dpi = self.settings.dpi as f32;
        let story_pages: Vec<usize> = (0..self.pages.len())
            .filter(|j| self.pages[*j].role == PageRole::Story)
            .collect();
        let page = &mut self.pages[i];
        let role = page.role;
        let pt = |p: f32| p * dpi / 72.0;
        let w = hi.x - lo.x;
        let h = hi.y - lo.y;
        let cx = lo.x + w / 2.0;
        let or = |s: &str, d: &str| {
            if s.trim().is_empty() {
                d.to_string()
            } else {
                s.to_string()
            }
        };
        let title = or(&info.title, ink_ui::tr("Title"));
        let author = or(&info.author, ink_ui::tr("Author"));
        let mut blocks: Vec<(f32, String, f32)> = Vec::new(); // (y fraction, text, size pt)
        match role {
            PageRole::Cover => {
                blocks.push((0.22, title.clone(), 40.0));
                if !info.subtitle.is_empty() {
                    blocks.push((0.33, info.subtitle.clone(), 16.0));
                }
                blocks.push((0.88, author.clone(), 18.0));
            }
            PageRole::Title => {
                blocks.push((0.40, title.clone(), 30.0));
                if !info.subtitle.is_empty() {
                    blocks.push((0.48, info.subtitle.clone(), 14.0));
                }
                blocks.push((0.60, author.clone(), 14.0));
            }
            PageRole::Credits => {
                let mut lines = vec![format!("{title}"), String::new()];
                let story = if info.artist.trim().is_empty() || info.artist == info.author {
                    format!("{}: {author}", ink_ui::tr("Story & art"))
                } else {
                    format!(
                        "{}: {author}\n{}: {}",
                        ink_ui::tr("Story"),
                        ink_ui::tr("Art"),
                        info.artist
                    )
                };
                lines.push(story);
                for (r, n) in &info.credits {
                    lines.push(format!("{r}: {n}"));
                }
                lines.push(String::new());
                let year = or(&info.year, "2026");
                lines.push(format!("© {year} {author}"));
                if !info.publisher.is_empty() {
                    lines.push(info.publisher.clone());
                }
                lines.push(ink_ui::tr("Made with Inkstudio").into());
                blocks.push((0.70, lines.join("\n"), 9.0));
            }
            PageRole::Contents => {
                blocks.push((0.14, ink_ui::tr("Contents").into(), 22.0));
                let mut lines = Vec::new();
                for (c, j) in story_pages
                    .iter()
                    .step_by(story_pages.len().div_ceil(6).max(1))
                    .enumerate()
                {
                    lines.push(format!(
                        "{} {} …… {}",
                        ink_ui::tr("Chapter"),
                        c + 1,
                        story_pages.iter().position(|x| x == j).unwrap_or(0) + 1
                    ));
                }
                if lines.is_empty() {
                    lines.push(format!("{} 1 …… 1", ink_ui::tr("Chapter")));
                }
                blocks.push((0.45, lines.join("\n"), 12.0));
            }
            PageRole::Afterword => {
                blocks.push((0.14, ink_ui::tr("Afterword").into(), 22.0));
                blocks.push((
                    0.40,
                    ink_ui::tr("Thank you for reading {title}!\nWrite your afterword here.")
                        .replace("{title}", &title),
                    11.0,
                ));
            }
            PageRole::BackCover => {
                blocks.push((
                    0.30,
                    or(
                        &info.description,
                        ink_ui::tr("A short description of the story."),
                    ),
                    12.0,
                ));
                let line = [info.publisher.as_str(), info.year.as_str()]
                    .iter()
                    .filter(|s| !s.is_empty())
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(" · ");
                if !line.is_empty() {
                    blocks.push((0.90, line, 10.0));
                }
            }
            PageRole::Story => {}
        }
        page.balloons.clear();
        for (fy, text, size) in blocks {
            let id = page.new_balloon_id();
            let mut b = ink_comic::Balloon::new(
                id,
                ink_comic::BalloonStyle::TextOnly,
                ink_vector::Pt::new(cx, lo.y + h * fy),
                self.settings.dpi,
                &text,
                ink_comic::TextDir::Horizontal,
            );
            b.font_px = pt(size);
            b.fit_to_text(fonts);
            page.balloons.push(b);
        }
        page.render_balloons(fonts, None);
        page.touch();
    }

    /// Unsaved changes anywhere in the book.
    pub fn is_dirty(&self) -> bool {
        self.structure_dirty || self.pages.iter().any(|p| p.dirty)
    }

    /// Total edit count (changes whenever anything is edited).
    pub fn generation(&self) -> u64 {
        self.structure_generation + self.pages.iter().map(|p| p.generation).sum::<u64>()
    }

    pub fn delete_page(&mut self, i: usize) {
        if self.pages.len() > 1 && i < self.pages.len() {
            self.pages.remove(i);
            self.current = self.current.min(self.pages.len() - 1);
            self.structure_dirty = true;
            self.structure_generation += 1;
        }
    }

    pub fn move_page(&mut self, from: usize, to: usize) {
        if from < self.pages.len() && to < self.pages.len() && from != to {
            let p = self.pages.remove(from);
            self.pages.insert(to, p);
            self.current = to;
            self.structure_dirty = true;
            self.structure_generation += 1;
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
        let s = SheetSettings {
            kind: SheetKind::Book,
            pages: 3,
            dpi: 150,
            template: Some(PanelTemplate::TwoRows),
            ..Default::default()
        };
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
        let s = SheetSettings {
            kind: SheetKind::Animation,
            trim_mm: (100.0, 60.0),
            dpi: 96,
            ..Default::default()
        };
        let mut p = Page::new(&s);
        let tl = p.timeline.as_ref().unwrap();
        assert_eq!(tl.frames, 24);
        assert!(tl.cel_at(0, 0).is_some());
        p.set_frame(5);
        assert_eq!(p.frame, 5);
    }
}
