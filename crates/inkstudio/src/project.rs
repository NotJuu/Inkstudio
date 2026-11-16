//! Save and open books as `.inks` project files.

use crate::book::{Book, Page, SheetSettings};
use ink_anim::Timeline;
use ink_comic::{Balloon, PanelSet};
use ink_core::{History, LayerId};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// App data stored next to each page's pixels.
#[derive(Serialize, Deserialize)]
struct PageData {
    panels: PanelSet,
    balloons: Vec<Balloon>,
    timeline: Option<Timeline>,
    frame: usize,
    panel_layer: LayerId,
    balloon_layer: LayerId,
    ink_layer: LayerId,
}

#[derive(Serialize, Deserialize)]
struct WorkData {
    settings: SheetSettings,
    current: usize,
}

/// Everything needed to write a book, detached from it (documents share tiles with the
/// live book, so taking a snapshot is cheap and the write can run on another thread).
pub struct Snapshot {
    work: String,
    pages: Vec<(String, ink_core::Document)>,
    /// `Book::generation()` at snapshot time.
    pub generation: u64,
}

pub fn snapshot(book: &Book) -> Result<Snapshot, ink_io::Error> {
    let work = serde_json::to_string(&WorkData { settings: book.settings.clone(), current: book.current })?;
    let pages = book
        .pages
        .iter()
        .map(|p| {
            let json = serde_json::to_string(&PageData {
                panels: p.panels.clone(),
                balloons: p.balloons.clone(),
                timeline: p.timeline.clone(),
                frame: p.frame,
                panel_layer: p.panel_layer,
                balloon_layer: p.balloon_layer,
                ink_layer: p.ink_layer,
            })?;
            Ok((json, p.doc.clone()))
        })
        .collect::<Result<_, ink_io::Error>>()?;
    Ok(Snapshot { work, pages, generation: book.generation() })
}

impl Snapshot {
    pub fn write(&self, path: &Path) -> Result<(), ink_io::Error> {
        let pages: Vec<ink_io::PageOut> = self.pages.iter().map(|(json, doc)| ink_io::PageOut { json: json.clone(), doc }).collect();
        ink_io::save(path, &self.work, &pages, env!("CARGO_PKG_VERSION"))
    }
}

/// After a successful write of `snap` to `path`: remember the path and, if nothing changed
/// since the snapshot, mark the book clean.
pub fn mark_saved(book: &mut Book, path: &Path, generation: u64) {
    book.path = Some(path.to_path_buf());
    if book.generation() == generation {
        book.structure_dirty = false;
        for p in &mut book.pages {
            p.dirty = false;
        }
    }
}

pub fn save(book: &mut Book, path: &Path) -> Result<(), ink_io::Error> {
    let snap = snapshot(book)?;
    snap.write(path)?;
    mark_saved(book, path, snap.generation);
    Ok(())
}

pub fn open(path: &Path) -> Result<Book, ink_io::Error> {
    let proj = ink_io::load(path)?;
    let work: WorkData = serde_json::from_str(&proj.work_json)?;
    let mut pages = Vec::new();
    for pin in proj.pages {
        let d: PageData = serde_json::from_str(&pin.json)?;
        let mut page = Page::from_parts(pin.doc, d.panels, d.balloons, d.timeline, d.panel_layer, d.balloon_layer, d.ink_layer);
        page.frame = d.frame;
        if page.timeline.is_some() {
            page.set_frame(d.frame);
        }
        pages.push(page);
    }
    if pages.is_empty() {
        return Err(ink_io::Error::Corrupt("no pages".into()));
    }
    let current = work.current.min(pages.len() - 1);
    Ok(Book { settings: work.settings, pages, current, path: Some(path.to_path_buf()), structure_dirty: false, structure_generation: 0, recovery_id: crate::book::new_recovery_id() })
}

impl Page {
    /// Rebuild a page from loaded parts (history starts empty).
    pub fn from_parts(doc: ink_core::Document, panels: PanelSet, balloons: Vec<Balloon>, timeline: Option<Timeline>, panel_layer: LayerId, balloon_layer: LayerId, ink_layer: LayerId) -> Self {
        let next_balloon = balloons.iter().map(|b| b.id).max().unwrap_or(0);
        let mut p = Page::new_empty(doc, panel_layer, balloon_layer, ink_layer);
        p.panels = panels;
        p.balloons = balloons;
        p.timeline = timeline;
        p.next_balloon = next_balloon;
        p.history = History::default();
        p
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::book::SheetKind;
    use ink_comic::{BalloonStyle, Fonts, TextDir};
    use ink_core::Rgba;
    use ink_vector::Pt;

    #[test]
    fn book_roundtrip_with_balloons_panels_and_animation() {
        let dir = std::env::temp_dir().join(format!("inkstudio-proj-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let fonts = Fonts::system();

        let s = SheetSettings { kind: SheetKind::Book, pages: 2, dpi: 100, name: "vol".into(), ..Default::default() };
        let mut b = Book::new(s);
        {
            let p = b.page_mut();
            let id = p.new_balloon_id();
            p.balloons.push(Balloon::new(id, BalloonStyle::Shout, Pt::new(200.0, 200.0), 100, "やった！", TextDir::Vertical));
            p.render_balloons(&fonts, None);
            let ink = p.ink_layer;
            p.doc.layer_mut(ink).unwrap().raster.set(10, 10, Rgba::BLACK);
            p.dirty = true;
        }
        b.current = 1;
        assert!(b.is_dirty());
        let path = dir.join("vol.inks");
        save(&mut b, &path).unwrap();
        assert!(!b.is_dirty());

        let back = open(&path).unwrap();
        assert_eq!(back.pages.len(), 2);
        assert_eq!(back.current, 1);
        assert_eq!(back.settings.name, "vol");
        let p0 = &back.pages[0];
        assert_eq!(p0.balloons.len(), 1);
        assert_eq!(p0.balloons[0].text, "やった！");
        assert_eq!(p0.panels.panels.len(), b.pages[0].panels.panels.len());
        assert_eq!(p0.doc.layer(p0.ink_layer).unwrap().raster.get(10, 10), Rgba::BLACK);
        assert!(p0.doc.layer(p0.balloon_layer).unwrap().raster.tile_count() > 0, "rendered balloons saved");

        let s = SheetSettings { kind: SheetKind::Animation, trim_mm: (80.0, 60.0), dpi: 96, frames: 10, ..Default::default() };
        let mut a = Book::new(s);
        {
            let p = a.page_mut();
            let tl = p.timeline.as_mut().unwrap();
            tl.new_cel(&mut p.doc, 0, 5);
            p.set_frame(6);
        }
        let path = dir.join("anim.inks");
        save(&mut a, &path).unwrap();
        let back = open(&path).unwrap();
        let p = back.page();
        let tl = p.timeline.as_ref().unwrap();
        assert_eq!(tl.frames, 10);
        assert_eq!(tl.tracks[0].cels.len(), 2);
        assert_eq!(p.frame, 6);
        assert_eq!(tl.cel_at(0, 6), Some(tl.tracks[0].cels[1]));
    }
}
