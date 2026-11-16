//! Canvas view: pan/zoom transform and a GPU tile cache with levels of detail.
//!
//! Only visible tiles are uploaded. Zoomed out, tiles are downsampled on the CPU (LOD 1–3:
//! 128, 64, 32 px), so a whole 600 dpi page costs a few MB of texture memory instead of 200.
//! Dirty tiles (from strokes, fills, undo) replace their texture in place.

use egui::{Color32, ColorImage, Pos2, Rect, TextureHandle, TextureOptions, Vec2, pos2, vec2};
use ink_core::tile::{TILE, TileKey};
use ink_core::{Document, LayerId, Rgba, Tile};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    /// Screen position of canvas pixel (0, 0).
    pub origin: Pos2,
    /// Screen points per canvas pixel.
    pub zoom: f32,
    /// Rotation in radians (clockwise on screen).
    pub angle: f32,
    /// Mirror the canvas horizontally (to check drawings, like flipping paper).
    pub flip: bool,
}

impl Default for View {
    fn default() -> Self {
        Self {
            origin: Pos2::ZERO,
            zoom: 0.1,
            angle: 0.0,
            flip: false,
        }
    }
}

impl View {
    pub fn to_screen(&self, x: f32, y: f32) -> Pos2 {
        let fx = if self.flip { -x } else { x };
        let (s, c) = self.angle.sin_cos();
        let (vx, vy) = (fx * self.zoom, y * self.zoom);
        pos2(
            self.origin.x + vx * c - vy * s,
            self.origin.y + vx * s + vy * c,
        )
    }
    pub fn to_canvas(&self, p: Pos2) -> (f32, f32) {
        let (vx, vy) = (p.x - self.origin.x, p.y - self.origin.y);
        let (s, c) = self.angle.sin_cos();
        let (x, y) = (
            (vx * c + vy * s) / self.zoom,
            (-vx * s + vy * c) / self.zoom,
        );
        (if self.flip { -x } else { x }, y)
    }
    /// Fit a w×h canvas into `area` with a margin (keeps rotation and flip at zero).
    pub fn fit(area: Rect, w: u32, h: u32) -> Self {
        let zoom = ((area.width() - 60.0) / w as f32)
            .min((area.height() - 60.0) / h as f32)
            .max(0.01);
        let size = vec2(w as f32 * zoom, h as f32 * zoom);
        Self {
            origin: area.center() - size / 2.0,
            zoom,
            angle: 0.0,
            flip: false,
        }
    }
    /// Apply `change`, then move the view so the canvas point under `anchor` stays under it.
    fn keep_anchor(&mut self, anchor: Pos2, change: impl FnOnce(&mut Self)) {
        let (cx, cy) = self.to_canvas(anchor);
        change(self);
        let now = self.to_screen(cx, cy);
        self.origin += anchor - now;
    }
    /// Zoom by `factor` around `anchor`.
    pub fn zoom_at(&mut self, anchor: Pos2, factor: f32) {
        self.keep_anchor(anchor, |v| v.zoom = (v.zoom * factor).clamp(0.01, 32.0));
    }
    pub fn set_zoom(&mut self, anchor: Pos2, zoom: f32) {
        self.keep_anchor(anchor, |v| v.zoom = zoom.clamp(0.01, 32.0));
    }
    /// Rotate by `delta` radians around `anchor`.
    pub fn rotate_at(&mut self, anchor: Pos2, delta: f32) {
        self.keep_anchor(anchor, |v| v.angle = wrap_angle(v.angle + delta));
    }
    pub fn set_angle(&mut self, anchor: Pos2, angle: f32) {
        self.keep_anchor(anchor, |v| v.angle = wrap_angle(angle));
    }
    /// Mirror around `anchor`.
    pub fn toggle_flip(&mut self, anchor: Pos2) {
        self.keep_anchor(anchor, |v| v.flip = !v.flip);
    }
    pub fn pan(&mut self, d: Vec2) {
        self.origin += d;
    }
    /// Screen corners of a canvas rectangle (for rotated outlines).
    pub fn quad(&self, x0: f32, y0: f32, x1: f32, y1: f32) -> [Pos2; 4] {
        [
            self.to_screen(x0, y0),
            self.to_screen(x1, y0),
            self.to_screen(x1, y1),
            self.to_screen(x0, y1),
        ]
    }
    /// Canvas-space bounding box of a screen rectangle.
    pub fn canvas_bounds(&self, r: Rect) -> (f32, f32, f32, f32) {
        let pts = [
            r.left_top(),
            r.right_top(),
            r.right_bottom(),
            r.left_bottom(),
        ]
        .map(|p| self.to_canvas(p));
        let xs = pts.map(|p| p.0);
        let ys = pts.map(|p| p.1);
        (
            xs.iter().cloned().fold(f32::MAX, f32::min),
            ys.iter().cloned().fold(f32::MAX, f32::min),
            xs.iter().cloned().fold(f32::MIN, f32::max),
            ys.iter().cloned().fold(f32::MIN, f32::max),
        )
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

pub fn wrap_angle(a: f32) -> f32 {
    let t = std::f32::consts::TAU;
    let mut a = a.rem_euclid(t);
    if a > std::f32::consts::PI {
        a -= t;
    }
    a
}

/// Uploaded tile textures, keyed by (tile, lod).
#[derive(Default)]
pub struct TileCache {
    tex: HashMap<(TileKey, u8), TextureHandle>,
    stale: HashSet<TileKey>,
    /// Onion-skin cels to tint into the display (animation).
    pub onion: Vec<(LayerId, Rgba, f32)>,
    pub uploads_last_frame: usize,
    /// Show luminance only (value check for painting).
    pub gray: bool,
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
            px.push(Color32::from_rgba_premultiplied(
                (acc[0] / d) as u8,
                (acc[1] / d) as u8,
                (acc[2] / d) as u8,
                (acc[3] / d) as u8,
            ));
        }
    }
    ColorImage {
        size: [n, n],
        source_size: vec2(n as f32, n as f32),
        pixels: px,
    }
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
    pub fn paint(
        &mut self,
        ctx: &egui::Context,
        painter: &egui::Painter,
        doc: &Document,
        view: &View,
        clip: Rect,
        budget: usize,
    ) {
        let lod = view.lod();
        let (tw, th) = doc.tiles_wh();
        let (cx0, cy0, cx1, cy1) = view.canvas_bounds(clip);
        let t = TILE as f32;
        let tx0 = ((cx0 / t).floor().max(0.0)) as i32;
        let ty0 = ((cy0 / t).floor().max(0.0)) as i32;
        let tx1 = ((cx1 / t).ceil() as i32).min(tw as i32);
        let ty1 = ((cy1 / t).ceil() as i32).min(th as i32);
        let opts = if view.zoom >= 2.0 {
            TextureOptions::NEAREST
        } else {
            TextureOptions::LINEAR
        };
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
            let gray = self.gray;
            let imgs: Vec<(TileKey, ColorImage)> = jobs
                .par_iter()
                .map(|&k| {
                    let tile = if onion.is_empty() {
                        doc.composite_tile(k, true)
                    } else {
                        ink_anim::composite_with_onion(doc, k, onion)
                    };
                    let mut img = downsample(&tile, lod);
                    if gray {
                        for px in &mut img.pixels {
                            let l =
                                ((px.r() as u32 * 54 + px.g() as u32 * 183 + px.b() as u32 * 19)
                                    >> 8) as u8;
                            *px = Color32::from_rgba_premultiplied(
                                l.min(px.a()),
                                l.min(px.a()),
                                l.min(px.a()),
                                px.a(),
                            );
                        }
                    }
                    (k, img)
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
        for ty in ty0..ty1 {
            for tx in tx0..tx1 {
                let k = (tx, ty);
                // Fall back to any available LOD while the right one is uploading.
                let h = self
                    .tex
                    .get(&(k, lod))
                    .or_else(|| (0..4u8).find_map(|l| self.tex.get(&(k, l))));
                let Some(h) = h else { continue };
                let x0 = (tx as f32 * t).min(doc.width as f32);
                let y0 = (ty as f32 * t).min(doc.height as f32);
                let x1 = ((tx + 1) as f32 * t).min(doc.width as f32);
                let y1 = ((ty + 1) as f32 * t).min(doc.height as f32);
                // Partial edge tiles: crop the UV to the canvas. Rotated views need real quads.
                let (u1, v1) = ((x1 - x0) / t, (y1 - y0) / t);
                let q = view.quad(x0, y0, x1, y1);
                let mut mesh = egui::epaint::Mesh::with_texture(h.id());
                for (p, uvp) in
                    q.iter()
                        .zip([pos2(0.0, 0.0), pos2(u1, 0.0), pos2(u1, v1), pos2(0.0, v1)])
                {
                    mesh.vertices.push(egui::epaint::Vertex {
                        pos: *p,
                        uv: uvp,
                        color: Color32::WHITE,
                    });
                }
                mesh.add_triangle(0, 1, 2);
                mesh.add_triangle(0, 2, 3);
                painter.add(egui::Shape::mesh(mesh));
            }
        }
        if self
            .stale
            .iter()
            .any(|k| k.0 >= tx0 && k.0 < tx1 && k.1 >= ty0 && k.1 < ty1)
            || jobs.len() >= budget
        {
            ctx.request_repaint();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_keeps_anchor_fixed_and_lod_follows_zoom() {
        let mut v = View {
            origin: pos2(100.0, 50.0),
            zoom: 1.0,
            ..Default::default()
        };
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
    fn rotation_and_flip_roundtrip_and_keep_anchor() {
        let mut v = View {
            origin: pos2(400.0, 300.0),
            zoom: 0.5,
            angle: 0.0,
            flip: false,
        };
        let anchor = pos2(500.0, 420.0);
        let before = v.to_canvas(anchor);
        v.rotate_at(anchor, 0.7);
        v.toggle_flip(anchor);
        v.zoom_at(anchor, 1.7);
        let after = v.to_canvas(anchor);
        assert!((before.0 - after.0).abs() < 1e-2 && (before.1 - after.1).abs() < 1e-2);
        for (x, y) in [(0.0, 0.0), (123.0, -45.0), (3000.0, 2000.0)] {
            let s = v.to_screen(x, y);
            let c = v.to_canvas(s);
            assert!((c.0 - x).abs() < 1e-2 && (c.1 - y).abs() < 1e-2);
        }
        assert!((wrap_angle(4.0 * std::f32::consts::PI + 0.1) - 0.1).abs() < 1e-5);
    }

    #[test]
    fn fit_centers_page() {
        let v = View::fit(
            Rect::from_min_size(Pos2::ZERO, vec2(1000.0, 800.0)),
            4000,
            6000,
        );
        let c = v.to_screen(2000.0, 3000.0);
        assert!((c.x - 500.0).abs() < 1.0 && (c.y - 400.0).abs() < 1.0);
    }
}
