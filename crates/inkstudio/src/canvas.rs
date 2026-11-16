//! Canvas view: pan/zoom transform and a GPU tile cache with levels of detail.
//!
//! Only visible tiles are uploaded. Zoomed out, tiles are downsampled on the CPU (LOD 1–3:
//! 128, 64, 32 px), so a whole 600 dpi page costs a few MB of texture memory instead of 200.
//! Dirty tiles (from strokes, fills, undo) replace their texture in place.

use egui::{Color32, ColorImage, Pos2, Rect, TextureHandle, TextureOptions, Vec2, pos2, vec2};
use ink_core::tile::{TileKey, TILE};
use ink_core::{Document, LayerId, Rgba, Tile};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    /// Screen position of canvas pixel (0, 0).
    pub origin: Pos2,
    /// Screen points per canvas pixel.
    pub zoom: f32,
}

impl Default for View {
    fn default() -> Self {
        Self { origin: Pos2::ZERO, zoom: 0.1 }
    }
}

impl View {
    pub fn to_screen(&self, x: f32, y: f32) -> Pos2 {
        pos2(self.origin.x + x * self.zoom, self.origin.y + y * self.zoom)
    }
    pub fn to_canvas(&self, p: Pos2) -> (f32, f32) {
        ((p.x - self.origin.x) / self.zoom, (p.y - self.origin.y) / self.zoom)
    }
    /// Fit a w×h canvas into `area` with a margin.
    pub fn fit(area: Rect, w: u32, h: u32) -> Self {
        let zoom = ((area.width() - 60.0) / w as f32).min((area.height() - 60.0) / h as f32).max(0.01);
        let size = vec2(w as f32 * zoom, h as f32 * zoom);
        Self { origin: area.center() - size / 2.0, zoom }
    }
    /// Zoom by `factor` keeping the canvas point under `anchor` fixed.
    pub fn zoom_at(&mut self, anchor: Pos2, factor: f32) {
        let new = (self.zoom * factor).clamp(0.01, 32.0);
        let k = new / self.zoom;
        self.origin = anchor + (self.origin - anchor) * k;
        self.zoom = new;
    }
    pub fn pan(&mut self, d: Vec2) {
        self.origin += d;
    }
    /// Level of detail for the current zoom: 0 = full tiles, 3 = 1/8.
    pub fn lod(&self) -> u8 {
        let mut lod = 0u8;
        let mut z = self.zoom;
        while z < 0.5 && lod < 3 {
            z *= 2.0;
            lod += 1;
        }
        lod
    }
}

/// Uploaded tile textures, keyed by (tile, lod).
#[derive(Default)]
pub struct TileCache {
    tex: HashMap<(TileKey, u8), TextureHandle>,
    stale: HashSet<TileKey>,
    /// Onion-skin cels to tint into the display (animation).
    pub onion: Vec<(LayerId, Rgba, f32)>,
    pub uploads_last_frame: usize,
}

fn downsample(t: &Tile, lod: u8) -> ColorImage {
    let s = 1usize << lod;
    let n = TILE / s;
    let mut px = Vec::with_capacity(n * n);
    for y in 0..n {
        for x in 0..n {
            let mut acc = [0u32; 4];
            for dy in 0..s {
                for dx in 0..s {
                    let c = t.px[(y * s + dy) * TILE + x * s + dx].0;
                    for i in 0..4 {
                        acc[i] += c[i] as u32;
                    }
                }
            }
            let d = (s * s) as u32;
            px.push(Color32::from_rgba_premultiplied((acc[0] / d) as u8, (acc[1] / d) as u8, (acc[2] / d) as u8, (acc[3] / d) as u8));
        }
    }
    ColorImage { size: [n, n], source_size: vec2(n as f32, n as f32), pixels: px }
}

impl TileCache {
    pub fn clear(&mut self) {
        self.tex.clear();
        self.stale.clear();
    }

    /// Mark tiles changed in the document since the last frame.
    pub fn invalidate(&mut self, keys: impl IntoIterator<Item = TileKey>) {
        self.stale.extend(keys);
    }

    /// Draw the visible part of the page. Uploads at most `budget` tiles per frame
    /// (the rest keep their old texture or show the lower LOD) so the UI never stalls.
    pub fn paint(&mut self, ctx: &egui::Context, painter: &egui::Painter, doc: &Document, view: &View, clip: Rect, budget: usize) {
        let lod = view.lod();
        let (tw, th) = doc.tiles_wh();
        let (cx0, cy0) = view.to_canvas(clip.min);
        let (cx1, cy1) = view.to_canvas(clip.max);
        let t = TILE as f32;
        let tx0 = ((cx0 / t).floor().max(0.0)) as i32;
        let ty0 = ((cy0 / t).floor().max(0.0)) as i32;
        let tx1 = ((cx1 / t).ceil() as i32).min(tw as i32);
        let ty1 = ((cy1 / t).ceil() as i32).min(th as i32);
        let opts = if view.zoom >= 2.0 { TextureOptions::NEAREST } else { TextureOptions::LINEAR };
        let mut uploads = 0;
        // Collect jobs first so compositing runs in parallel.
        let mut jobs: Vec<TileKey> = Vec::new();
        for ty in ty0..ty1 {
            for tx in tx0..tx1 {
                let k = (tx, ty);
                let have = self.tex.contains_key(&(k, lod));
                if (!have || self.stale.contains(&k)) && jobs.len() < budget {
                    jobs.push(k);
                }
            }
        }
        if !jobs.is_empty() {
            use rayon::prelude::*;
            let onion = &self.onion;
            let imgs: Vec<(TileKey, ColorImage)> = jobs
                .par_iter()
                .map(|&k| {
                    let tile = if onion.is_empty() { doc.composite_tile(k, true) } else { ink_anim::composite_with_onion(doc, k, onion) };
                    (k, downsample(&tile, lod))
                })
                .collect();
            for (k, img) in imgs {
                uploads += 1;
                // A stale tile is stale at every LOD; drop the other levels.
                if self.stale.remove(&k) {
                    for l in 0..4u8 {
                        if l != lod {
                            self.tex.remove(&(k, l));
                        }
                    }
                }
                match self.tex.get_mut(&(k, lod)) {
                    Some(h) => h.set(img, opts),
                    None => {
                        let h = ctx.load_texture(format!("tile{}_{}_{}", k.0, k.1, lod), img, opts);
                        self.tex.insert((k, lod), h);
                    }
                }
            }
        }
        self.uploads_last_frame = uploads;
        let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
        for ty in ty0..ty1 {
            for tx in tx0..tx1 {
                let k = (tx, ty);
                // Fall back to any available LOD while the right one is uploading.
                let h = self.tex.get(&(k, lod)).or_else(|| (0..4u8).find_map(|l| self.tex.get(&(k, l))));
                let Some(h) = h else { continue };
                let x0 = (tx as f32 * t).min(doc.width as f32);
                let y0 = (ty as f32 * t).min(doc.height as f32);
                let x1 = ((tx + 1) as f32 * t).min(doc.width as f32);
                let y1 = ((ty + 1) as f32 * t).min(doc.height as f32);
                // Partial edge tiles: crop the UV to the canvas.
                let uv = Rect::from_min_max(uv.min, pos2((x1 - x0) / t, (y1 - y0) / t));
                let r = Rect::from_min_max(view.to_screen(x0, y0), view.to_screen(x1, y1));
                painter.image(h.id(), r, uv, Color32::WHITE);
            }
        }
        if self.stale.iter().any(|k| k.0 >= tx0 && k.0 < tx1 && k.1 >= ty0 && k.1 < ty1) || jobs.len() >= budget {
            ctx.request_repaint();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_keeps_anchor_fixed_and_lod_follows_zoom() {
        let mut v = View { origin: pos2(100.0, 50.0), zoom: 1.0 };
        let anchor = pos2(300.0, 200.0);
        let before = v.to_canvas(anchor);
        v.zoom_at(anchor, 0.25);
        let after = v.to_canvas(anchor);
        assert!((before.0 - after.0).abs() < 1e-3 && (before.1 - after.1).abs() < 1e-3);
        assert_eq!(v.lod(), 1);
        v.zoom = 0.05;
        assert_eq!(v.lod(), 3);
        v.zoom = 3.0;
        assert_eq!(v.lod(), 0);
    }

    #[test]
    fn fit_centers_page() {
        let v = View::fit(Rect::from_min_size(Pos2::ZERO, vec2(1000.0, 800.0)), 4000, 6000);
        let c = v.to_screen(2000.0, 3000.0);
        assert!((c.x - 500.0).abs() < 1.0 && (c.y - 400.0).abs() < 1.0);
    }
}
