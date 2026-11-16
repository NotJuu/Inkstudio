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

pub fn save(book: &mut Book, path: &Path) -> Result<(), ink_io::Error> {
    let work = serde_json::to_string(&WorkData { settings: book.settings.clone(), current: book.current })?;
    let datas: Vec<String> = book
        .pages
        .iter()
        .map(|p| {
            serde_json::to_string(&PageData {
                panels: p.panels.clone(),
                balloons: p.balloons.clone(),
                timeline: p.timeline.clone(),
                frame: p.frame,
                panel_layer: p.panel_layer,
                balloon_layer: p.balloon_layer,
                ink_layer: p.ink_layer,
            })
        })
        .collect::<Result<_, _>>()?;
    let pages: Vec<ink_io::PageOut> = book.pages.iter().zip(datas).map(|(p, json)| ink_io::PageOut { json, doc: &p.doc }).collect();
    ink_io::save(path, &work, &pages, env!("CARGO_PKG_VERSION"))?;
    book.path = Some(path.to_path_buf());
    book.structure_dirty = false;
    for p in &mut book.pages {
        p.dirty = false;
    }
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
    Ok(Book { settings: work.settings, pages, current, path: Some(path.to_path_buf()), structure_dirty: false })
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
