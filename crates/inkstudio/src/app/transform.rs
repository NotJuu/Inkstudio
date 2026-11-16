//! Transform tool and image import. A transform lifts the layer's pixels into a buffer, shows
//! them as a live textured quad (move, scale, rotate, flip), and resamples them back into the
//! layer on Apply — one undo step. Imported pictures start as a transform on a new layer.

use super::*;
use ink_core::tile::{TILE, TileKey};
use ink_core::{Document, Tile};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Handle {
    Move,
    Rotate,
    /// Corner with local signs (±1, ±1).
    Corner(f32, f32),
    /// Edge: (±1, 0) or (0, ±1).
    Edge(f32, f32),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub center: Pt,
    /// Scale factors (> 0).
    pub sx: f32,
    pub sy: f32,
    /// Radians.
    pub angle: f32,
    pub flip_x: bool,
    pub flip_y: bool,
}

impl Placement {
    fn rot(&self, v: Pt) -> Pt {
        let (s, c) = self.angle.sin_cos();
        Pt::new(v.x * c - v.y * s, v.x * s + v.y * c)
    }
    fn unrot(&self, v: Pt) -> Pt {
        let (s, c) = self.angle.sin_cos();
        Pt::new(v.x * c + v.y * s, -v.x * s + v.y * c)
    }
    /// Source pixel (relative to the source center) → canvas.
    fn forward(&self, u: Pt) -> Pt {
        let fx = if self.flip_x { -1.0 } else { 1.0 };
        let fy = if self.flip_y { -1.0 } else { 1.0 };
        self.center
            .add(self.rot(Pt::new(u.x * self.sx * fx, u.y * self.sy * fy)))
    }
    /// Canvas → source (relative to the source center).
    fn inverse(&self, p: Pt) -> Pt {
        let fx = if self.flip_x { -1.0 } else { 1.0 };
        let fy = if self.flip_y { -1.0 } else { 1.0 };
        let v = self.unrot(p.sub(self.center));
        Pt::new(v.x / self.sx * fx, v.y / self.sy * fy)
    }
}

pub struct Transform {
    pub layer: LayerId,
    page: usize,
    /// Premultiplied source pixels.
    src: Arc<Vec<Rgba>>,
    sw: usize,
    sh: usize,
    /// The layer's tiles before the transform (restored on cancel, kept for undo).
    orig: Vec<(TileKey, Option<Arc<Tile>>)>,
    pub place: Placement,
    start: Placement,
    pub keep_aspect: bool,
    pub smooth: bool,
    drag: Option<(Handle, Pt, Placement)>,
    tex: Option<egui::TextureHandle>,
    /// Name for the undo entry ("Transform" / "Import image").
    label: &'static str,
}

impl Transform {
    fn half(&self) -> (f32, f32) {
        (self.sw as f32 / 2.0, self.sh as f32 / 2.0)
    }

    /// Corners in canvas coordinates (top-left, top-right, bottom-right, bottom-left of the source).
    pub fn corners(&self) -> [Pt; 4] {
        let (hw, hh) = self.half();
        [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
            .map(|(x, y)| self.place.forward(Pt::new(x * hw, y * hh)))
    }

    pub fn size(&self) -> (usize, usize) {
        (self.sw, self.sh)
    }

    pub fn page(&self) -> usize {
        self.page
    }
}

/// Pixels of `layer` inside their bounding box. None if the layer is empty.
fn lift(doc: &Document, layer: LayerId) -> Option<(Vec<Rgba>, usize, usize, i32, i32)> {
    let l = doc.layer(layer)?;
    let (mut x0, mut y0, mut x1, mut y1) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
    for k in l.raster.keys() {
        let t = l.raster.tile(*k)?;
        for ly in 0..TILE {
            for lx in 0..TILE {
                if t.px[ly * TILE + lx].a() != 0 {
                    let (x, y) = (k.0 * TILE as i32 + lx as i32, k.1 * TILE as i32 + ly as i32);
                    x0 = x0.min(x);
                    y0 = y0.min(y);
                    x1 = x1.max(x);
                    y1 = y1.max(y);
                }
            }
        }
    }
    if x1 < x0 {
        return None;
    }
    let (w, h) = ((x1 - x0 + 1) as usize, (y1 - y0 + 1) as usize);
    Some((l.raster.read_rect(x0, y0, w, h), w, h, x0, y0))
}

/// Halve an image (2×2 box filter).
fn halve(px: &[Rgba], w: usize, h: usize) -> (Vec<Rgba>, usize, usize) {
    let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
    let mut out = Vec::with_capacity(nw * nh);
    for y in 0..nh {
        for x in 0..nw {
            let mut acc = [0u32; 4];
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let c = px[(y * 2 + dy).min(h - 1) * w + (x * 2 + dx).min(w - 1)].0;
                for i in 0..4 {
                    acc[i] += c[i] as u32;
                }
            }
            out.push(Rgba([
                (acc[0] / 4) as u8,
                (acc[1] / 4) as u8,
                (acc[2] / 4) as u8,
                (acc[3] / 4) as u8,
            ]));
        }
    }
    (out, nw, nh)
}

/// Resample `src` placed by `place` into new tiles covering the transformed area.
pub fn render(
    src: &[Rgba],
    sw: usize,
    sh: usize,
    place: &Placement,
    smooth: bool,
    canvas: (u32, u32),
) -> Vec<(TileKey, Tile)> {
    use rayon::prelude::*;
    // Pre-shrink for strong downscaling so bilinear sampling doesn't alias.
    let shrink = 1.0 / place.sx.min(place.sy).max(1e-4);
    let mut levels = 0;
    let (mut buf, mut w, mut h) = (src.to_vec(), sw, sh);
    let mut level_scale = 1.0;
    while smooth && shrink / level_scale >= 2.0 && w > 1 && h > 1 {
        (buf, w, h) = halve(&buf, w, h);
        level_scale *= 2.0;
        levels += 1;
    }
    let _ = levels;
    let k = (sw as f32 / w as f32, sh as f32 / h as f32);
    let (hw, hh) = (sw as f32 / 2.0, sh as f32 / 2.0);
    let corners =
        [(-hw, -hh), (hw, -hh), (hw, hh), (-hw, hh)].map(|(x, y)| place.forward(Pt::new(x, y)));
    let x0 = corners
        .iter()
        .map(|c| c.x)
        .fold(f32::MAX, f32::min)
        .floor()
        .max(0.0) as i32;
    let y0 = corners
        .iter()
        .map(|c| c.y)
        .fold(f32::MAX, f32::min)
        .floor()
        .max(0.0) as i32;
    let x1 = (corners.iter().map(|c| c.x).fold(f32::MIN, f32::max).ceil() as i32)
        .min(canvas.0 as i32 - 1);
    let y1 = (corners.iter().map(|c| c.y).fold(f32::MIN, f32::max).ceil() as i32)
        .min(canvas.1 as i32 - 1);
    if x1 < x0 || y1 < y0 {
        return Vec::new();
    }
    let keys: Vec<TileKey> = ink_core::tile::keys_in(x0, y0, x1, y1).collect();
    let sample = |u: f32, v: f32| -> Rgba {
        // u, v in level pixel coordinates (pixel centers at .5).
        if smooth {
            let (fx, fy) = (u - 0.5, v - 0.5);
            let (ix, iy) = (fx.floor(), fy.floor());
            let (tx, ty) = (fx - ix, fy - iy);
            let get = |x: i32, y: i32| -> [f32; 4] {
                if x < 0 || y < 0 || x >= w as i32 || y >= h as i32 {
                    [0.0; 4]
                } else {
                    buf[y as usize * w + x as usize].0.map(|c| c as f32)
                }
            };
            let (a, b, c, d) = (
                get(ix as i32, iy as i32),
                get(ix as i32 + 1, iy as i32),
                get(ix as i32, iy as i32 + 1),
                get(ix as i32 + 1, iy as i32 + 1),
            );
            let mut o = [0u8; 4];
            for i in 0..4 {
                let top = a[i] + (b[i] - a[i]) * tx;
                let bot = c[i] + (d[i] - c[i]) * tx;
                o[i] = (top + (bot - top) * ty).round().clamp(0.0, 255.0) as u8;
            }
            o[0] = o[0].min(o[3]);
            o[1] = o[1].min(o[3]);
            o[2] = o[2].min(o[3]);
            Rgba(o)
        } else {
            let (x, y) = (u.floor() as i32, v.floor() as i32);
            if x < 0 || y < 0 || x >= w as i32 || y >= h as i32 {
                Rgba::TRANSPARENT
            } else {
                buf[y as usize * w + x as usize]
            }
        }
    };
    keys.par_iter()
        .filter_map(|&key| {
            let mut t = Tile::empty();
            let mut any = false;
            for ly in 0..TILE {
                let y = key.1 * TILE as i32 + ly as i32;
                if y < y0 || y > y1 {
                    continue;
                }
                for lx in 0..TILE {
                    let x = key.0 * TILE as i32 + lx as i32;
                    if x < x0 || x > x1 {
                        continue;
                    }
                    let s = place.inverse(Pt::new(x as f32 + 0.5, y as f32 + 0.5));
                    let (u, v) = ((s.x + hw) / k.0, (s.y + hh) / k.1);
                    let c = sample(u, v);
                    if c.a() != 0 {
                        t.px[ly * TILE + lx] = c;
                        any = true;
                    }
                }
            }
            any.then_some((key, t))
        })
        .collect()
}

/// Decode a picture file into premultiplied pixels.
pub fn decode_image(bytes: &[u8]) -> Result<(Vec<Rgba>, usize, usize), String> {
    let img = image::load_from_memory(bytes)
        .map_err(|e| e.to_string())?
        .to_rgba8();
    let (w, h) = (img.width() as usize, img.height() as usize);
    if w == 0 || h == 0 {
        return Err("empty image".into());
    }
    let px = img
        .pixels()
        .map(|p| Rgba::from_straight(p[0], p[1], p[2], p[3]))
        .collect();
    Ok((px, w, h))
}

impl InkApp {
    /// Start transforming the selected layer.
    pub fn begin_transform(&mut self, now: f64) -> bool {
        if self.transform.is_some() {
            return true;
        }
        let Some(target) = self.target_layer(now) else {
            return false;
        };
        let b = self.book.as_mut().unwrap();
        let page_idx = b.current;
        let page = b.page_mut();
        let Some((px, w, h, x0, y0)) = lift(&page.doc, target) else {
            self.toast(
                ToastKind::Info,
                tr("This layer is empty: nothing to transform."),
                now,
            );
            return false;
        };
        let l = page.doc.layer_mut(target).unwrap();
        let keys: Vec<TileKey> = l.raster.keys().copied().collect();
        let orig: Vec<(TileKey, Option<Arc<Tile>>)> = keys
            .iter()
            .map(|k| (*k, l.raster.tile(*k).cloned()))
            .collect();
        for k in &keys {
            l.raster.put_tile(*k, None);
        }
        page.doc.mark_dirty(keys);
        let place = Placement {
            center: Pt::new(x0 as f32 + w as f32 / 2.0, y0 as f32 + h as f32 / 2.0),
            sx: 1.0,
            sy: 1.0,
            angle: 0.0,
            flip_x: false,
            flip_y: false,
        };
        self.transform = Some(Transform {
            layer: target,
            page: page_idx,
            src: Arc::new(px),
            sw: w,
            sh: h,
            orig,
            place,
            start: place,
            keep_aspect: true,
            smooth: true,
            drag: None,
            tex: None,
            label: "Transform",
        });
        self.tool = Tool::Transform;
        true
    }

    /// Put a decoded picture on a new layer above the selected one, fitted into the page, and
    /// start transforming it.
    pub fn import_pixels(&mut self, name: &str, px: Vec<Rgba>, w: usize, h: usize, at: Option<Pt>) {
        self.commit_transform();
        let Some(b) = self.book.as_mut() else { return };
        let page_idx = b.current;
        let page = b.page_mut();
        let (cw, ch) = page.canvas();
        let base = self
            .active_layer
            .filter(|id| page.doc.layer(*id).is_some() && !page.is_object_layer(*id));
        let idx = base.and_then(|id| page.doc.index_of(id)).map_or_else(
            || page.doc.index_of(page.ink_layer).map_or(0, |i| i + 1),
            |i| i + 1,
        );
        let snap = page.doc.snapshot_layers();
        let id = page.doc.add_layer(Layer::new(name), idx);
        page.push_doc(UndoEntry::Layers {
            label: "New layer".into(),
            layers: snap,
        });
        self.active_layer = Some(id);
        let fit = (cw as f32 * 0.9 / w as f32)
            .min(ch as f32 * 0.9 / h as f32)
            .min(1.0);
        let center = at.unwrap_or(Pt::new(cw as f32 / 2.0, ch as f32 / 2.0));
        let place = Placement {
            center,
            sx: fit,
            sy: fit,
            angle: 0.0,
            flip_x: false,
            flip_y: false,
        };
        self.transform = Some(Transform {
            layer: id,
            page: page_idx,
            src: Arc::new(px),
            sw: w,
            sh: h,
            orig: Vec::new(),
            place,
            start: place,
            keep_aspect: true,
            smooth: true,
            drag: None,
            tex: None,
            label: "Import image",
        });
        self.tool = Tool::Transform;
    }

    pub fn import_image_file(&mut self, path: &std::path::Path, at: Option<Pt>, now: f64) {
        if self.book.is_none() {
            self.toast(ToastKind::Info, tr("Create or open a sheet first."), now);
            return;
        }
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| tr("Image").into());
        match std::fs::read(path)
            .map_err(|e| e.to_string())
            .and_then(|b| decode_image(&b))
        {
            Ok((px, w, h)) => {
                self.import_pixels(&name, px, w, h, at);
                self.toast(
                    ToastKind::Success,
                    trf(
                        "Imported {f} ({w} × {h} px). Adjust it, then press Enter.",
                        &[("f", name), ("w", w.to_string()), ("h", h.to_string())],
                    ),
                    now,
                );
            }
            Err(e) => self.toast(
                ToastKind::Error,
                trf("Couldn't import the image: {e}", &[("e", e)]),
                now,
            ),
        }
    }

    pub fn import_image_dialog(&mut self) {
        if self.book.is_none() {
            return;
        }
        self.file_dialog = Some((
            crate::files::FileDialog::new(crate::files::Mode::Image, self.start_dir(), ""),
            None,
        ));
    }

    /// Files dropped on the window: pictures are imported where they were dropped,
    /// .inks projects are opened.
    pub(super) fn handle_dropped_files(&mut self, ctx: &egui::Context, now: f64) {
        let (dropped, hovering) =
            ctx.input(|i| (i.raw.dropped_files.clone(), !i.raw.hovered_files.is_empty()));
        if hovering {
            let p = theme::palette_ctx(ctx);
            let screen = ctx.content_rect();
            egui::Area::new(egui::Id::new("drop-hint"))
                .order(egui::Order::Foreground)
                .fixed_pos(screen.min)
                .show(ctx, |ui| {
                    let (r, _) = ui.allocate_exact_size(screen.size(), egui::Sense::hover());
                    ui.painter()
                        .rect_filled(r, 0, Color32::from_black_alpha(120));
                    ui.painter().rect_stroke(
                        r.shrink(12.0),
                        8,
                        Stroke::new(3.0, p.focus_ring),
                        egui::StrokeKind::Inside,
                    );
                    ui.painter().text(
                        r.center(),
                        Align2::CENTER_CENTER,
                        tr("Drop images to import them as layers"),
                        egui::FontId::proportional(22.0),
                        Color32::WHITE,
                    );
                });
        }
        let at = self.hover_canvas;
        for f in dropped {
            let path = f.path().to_path_buf();
            if crate::files::is_image(&path) {
                if path.is_file() {
                    self.import_image_file(&path, at, now);
                } else if self.book.is_some() {
                    // No file on disk (e.g. dragged from a browser): read the bytes instead.
                    let name = path
                        .file_stem()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_else(|| tr("Image").into());
                    match f.bytes().and_then(|b| decode_image(&b)) {
                        Ok((px, w, h)) => self.import_pixels(&name, px, w, h, at),
                        Err(e) => self.toast(
                            ToastKind::Error,
                            trf("Couldn't import the image: {e}", &[("e", e)]),
                            now,
                        ),
                    }
                }
            } else if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case(ink_io::EXTENSION))
            {
                self.request(Pending::Open(Some(path)), now);
            }
        }
    }

    /// Sub tool palette for the transform tool.
    pub(super) fn transform_actions(&mut self, ui: &mut Ui) {
        let p = palette(ui);
        let now = ui.input(|i| i.time);
        ui.spacing_mut().item_spacing.y = 6.0;
        let w = ui.available_width().min(240.0);
        if ui
            .add(
                Button::new(tr("Import image…"))
                    .icon(icons::IMAGE)
                    .min_width(w),
            )
            .clicked()
        {
            self.import_image_dialog();
        }
        if ui
            .add_enabled(
                self.transform.is_none(),
                Button::new(tr("Transform selected layer"))
                    .icon(icons::ARROWS_OUT)
                    .min_width(w),
            )
            .clicked()
        {
            self.begin_transform(now);
        }
        ui.label(
            RichText::new(tr(
                "You can also drop image files (PNG, JPEG, WebP, BMP, GIF, TIFF) onto the window.",
            ))
            .text_style(text::caption())
            .color(p.text_muted),
        );
    }

    /// Bake the transform into the layer (one undo step).
    pub fn commit_transform(&mut self) {
        let Some(t) = self.transform.take() else {
            return;
        };
        let Some(b) = self.book.as_mut() else { return };
        let Some(page) = b.pages.get_mut(t.page) else {
            return;
        };
        let canvas = page.canvas();
        let tiles = render(&t.src, t.sw, t.sh, &t.place, t.smooth, canvas);
        let Some(l) = page.doc.layer_mut(t.layer) else {
            return;
        };
        let mut undo: HashMap<TileKey, Option<Arc<Tile>>> = t.orig.into_iter().collect();
        let mut keys: Vec<TileKey> = undo.keys().copied().collect();
        for (k, tile) in tiles {
            undo.entry(k).or_insert(None);
            l.raster.put_tile(k, Some(Arc::new(tile)));
            keys.push(k);
        }
        page.doc.mark_dirty(keys);
        page.push_doc(UndoEntry::Tiles {
            label: t.label.into(),
            layer: t.layer,
            tiles: undo.into_iter().collect(),
        });
    }

    /// Drop the transform and put the layer back as it was.
    pub fn cancel_transform(&mut self) {
        let Some(t) = self.transform.take() else {
            return;
        };
        let Some(b) = self.book.as_mut() else { return };
        let Some(page) = b.pages.get_mut(t.page) else {
            return;
        };
        if t.label == "Import image" {
            // Undo the new layer as well.
            page.undo(&self.fonts);
            return;
        }
        if let Some(l) = page.doc.layer_mut(t.layer) {
            let keys: Vec<TileKey> = t.orig.iter().map(|(k, _)| *k).collect();
            for (k, tile) in t.orig {
                l.raster.put_tile(k, tile);
            }
            page.doc.mark_dirty(keys);
        }
    }

    pub(super) fn transform_event(&mut self, ev: PenEvent, now: f64) {
        if self.transform.is_none() {
            if let PenEvent::Down(_) = ev {
                if !self.begin_transform(now) {
                    return;
                }
            } else {
                return;
            }
        }
        let view = self.view;
        let shift = self.shift_held;
        let t = self.transform.as_mut().unwrap();
        let (s, phase) = match ev {
            PenEvent::Down(s) => (s, 0),
            PenEvent::Move(s) => (s, 1),
            PenEvent::Up(s) => (s, 2),
        };
        let c = view.to_canvas(s.pos);
        let pt = Pt::new(c.0, c.1);
        match phase {
            0 => {
                let h = hit(t, &view, s.pos);
                t.drag = Some((h, pt, t.place));
            }
            1 => {
                let Some((h, start, p0)) = t.drag else { return };
                let (hw, hh) = t.half();
                let mut p = p0;
                match h {
                    Handle::Move => p.center = p0.center.add(pt.sub(start)),
                    Handle::Rotate => {
                        let a0 = start.sub(p0.center);
                        let a1 = pt.sub(p0.center);
                        let mut a = p0.angle + (a1.y.atan2(a1.x) - a0.y.atan2(a0.x));
                        if shift {
                            let step = 15f32.to_radians();
                            a = (a / step).round() * step;
                        }
                        p.angle = a;
                    }
                    Handle::Corner(cx, cy) => {
                        let fx = if p0.flip_x { -1.0 } else { 1.0 };
                        let fy = if p0.flip_y { -1.0 } else { 1.0 };
                        // The opposite corner stays where it is.
                        let o = p0.forward(Pt::new(-cx * hw, -cy * hh));
                        let d = p0.unrot(pt.sub(o));
                        let (dx, dy) = (d.x * cx * fx, d.y * cy * fy);
                        if t.keep_aspect != shift {
                            let s0 = (dx * hw * 2.0 + dy * hh * 2.0) / (4.0 * (hw * hw + hh * hh));
                            let s = s0.max(1.0 / (2.0 * hw.max(hh)));
                            p.sx = s;
                            p.sy = s;
                        } else {
                            p.sx = (dx / (2.0 * hw)).max(1.0 / (2.0 * hw));
                            p.sy = (dy / (2.0 * hh)).max(1.0 / (2.0 * hh));
                        }
                        p.center = o.add(p0.rot(Pt::new(cx * fx * hw * p.sx, cy * fy * hh * p.sy)));
                    }
                    Handle::Edge(ex, ey) => {
                        let fx = if p0.flip_x { -1.0 } else { 1.0 };
                        let fy = if p0.flip_y { -1.0 } else { 1.0 };
                        let o = p0.forward(Pt::new(-ex * hw, -ey * hh));
                        let d = p0.unrot(pt.sub(o));
                        if ex != 0.0 {
                            p.sx = (d.x * ex * fx / (2.0 * hw)).max(1.0 / (2.0 * hw));
                        } else {
                            p.sy = (d.y * ey * fy / (2.0 * hh)).max(1.0 / (2.0 * hh));
                        }
                        p.center = o.add(p0.rot(Pt::new(ex * fx * hw * p.sx, ey * fy * hh * p.sy)));
                    }
                }
                t.place = p;
            }
            _ => t.drag = None,
        }
    }

    /// Live preview: the lifted pixels as a textured quad, plus the frame and handles.
    pub(super) fn paint_transform(
        &mut self,
        ctx: &egui::Context,
        painter: &egui::Painter,
        pointer: Option<Pos2>,
    ) {
        let Some(t) = self.transform.as_mut() else {
            return;
        };
        if self.book.as_ref().is_none_or(|b| b.current != t.page) {
            return;
        }
        if t.tex.is_none() {
            let (mut px, mut w, mut h) = ((*t.src).clone(), t.sw, t.sh);
            while w.max(h) > 2048 {
                (px, w, h) = halve(&px, w, h);
            }
            let img = egui::ColorImage {
                size: [w, h],
                source_size: vec2(w as f32, h as f32),
                pixels: px
                    .iter()
                    .map(|c| Color32::from_rgba_premultiplied(c.0[0], c.0[1], c.0[2], c.0[3]))
                    .collect(),
            };
            t.tex = Some(ctx.load_texture(
                "transform",
                img,
                if t.smooth {
                    egui::TextureOptions::LINEAR
                } else {
                    egui::TextureOptions::NEAREST
                },
            ));
        }
        let v = self.view;
        let q = t.corners().map(|c| v.to_screen(c.x, c.y));
        let opacity = self
            .book
            .as_ref()
            .and_then(|b| b.pages[t.page].doc.layer(t.layer))
            .map_or(255, |l| l.props.opacity);
        if let Some(tex) = &t.tex {
            let mut mesh = egui::epaint::Mesh::with_texture(tex.id());
            let tint = Color32::from_white_alpha(opacity);
            for (p, uv) in q.iter().zip([
                pos2(0.0, 0.0),
                pos2(1.0, 0.0),
                pos2(1.0, 1.0),
                pos2(0.0, 1.0),
            ]) {
                mesh.vertices.push(egui::epaint::Vertex {
                    pos: *p,
                    uv,
                    color: tint,
                });
            }
            mesh.add_triangle(0, 1, 2);
            mesh.add_triangle(0, 2, 3);
            painter.add(egui::Shape::mesh(mesh));
        }
        let accent = Color32::from_rgb(70, 150, 255);
        painter.add(egui::Shape::closed_line(
            q.to_vec(),
            Stroke::new(2.5, Color32::from_white_alpha(150)),
        ));
        painter.add(egui::Shape::closed_line(
            q.to_vec(),
            Stroke::new(1.2, accent),
        ));
        let hovered = pointer.map(|p| hit(t, &v, p));
        for (pos, h) in handle_positions(t, &v) {
            let big = hovered == Some(h);
            match h {
                Handle::Rotate => {
                    let top = q[0].lerp(q[1], 0.5);
                    painter.line_segment([top, pos], Stroke::new(1.2, accent));
                    painter.circle(
                        pos,
                        if big { 7.0 } else { 5.5 },
                        Color32::WHITE,
                        Stroke::new(1.5, accent),
                    );
                }
                _ => {
                    let r = Rect::from_center_size(
                        pos,
                        vec2(if big { 11.0 } else { 9.0 }, if big { 11.0 } else { 9.0 }),
                    );
                    painter.rect(
                        r,
                        1,
                        Color32::WHITE,
                        Stroke::new(1.5, accent),
                        egui::StrokeKind::Middle,
                    );
                }
            }
        }
        let cursor = match hovered {
            Some(Handle::Move) => egui::CursorIcon::Move,
            Some(Handle::Rotate) => egui::CursorIcon::Alias,
            Some(Handle::Corner(x, y)) => {
                if (x * y > 0.0) != (t.place.flip_x != t.place.flip_y) {
                    egui::CursorIcon::ResizeNwSe
                } else {
                    egui::CursorIcon::ResizeNeSw
                }
            }
            Some(Handle::Edge(x, _)) => {
                if x != 0.0 {
                    egui::CursorIcon::ResizeHorizontal
                } else {
                    egui::CursorIcon::ResizeVertical
                }
            }
            None => egui::CursorIcon::Default,
        };
        ctx.set_cursor_icon(cursor);
    }

    /// Numbers and buttons for the transform (Tool property palette).
    pub(super) fn transform_props(&mut self, ui: &mut Ui) {
        let p = palette(ui);
        let now = ui.input(|i| i.time);
        let Some(t) = self.transform.as_mut() else {
            ui.label(RichText::new(tr("Click the canvas to transform the selected layer, or import an image (File → Import image…, or drop a file on the window).")).color(p.text_muted));
            if ui
                .add(
                    Button::new(tr("Transform selected layer"))
                        .icon(icons::ARROWS_OUT)
                        .size(ButtonSize::Sm),
                )
                .clicked()
            {
                self.begin_transform(now);
            }
            return;
        };
        let mut pl = t.place;
        egui::Grid::new("transform-grid")
            .num_columns(2)
            .spacing([8.0, 6.0])
            .show(ui, |ui| {
                ui.label("X");
                ui.add(
                    egui::DragValue::new(&mut pl.center.x)
                        .speed(1.0)
                        .suffix(" px")
                        .max_decimals(0),
                );
                ui.end_row();
                ui.label("Y");
                ui.add(
                    egui::DragValue::new(&mut pl.center.y)
                        .speed(1.0)
                        .suffix(" px")
                        .max_decimals(0),
                );
                ui.end_row();
                ui.label(tr("Width"));
                let mut w = pl.sx * 100.0;
                if ui
                    .add(
                        egui::DragValue::new(&mut w)
                            .range(0.5..=2000.0)
                            .speed(0.5)
                            .suffix(" %")
                            .max_decimals(1),
                    )
                    .changed()
                {
                    let k = w / 100.0 / pl.sx;
                    pl.sx = w / 100.0;
                    if t.keep_aspect {
                        pl.sy *= k;
                    }
                }
                ui.end_row();
                ui.label(tr("Height"));
                let mut h = pl.sy * 100.0;
                if ui
                    .add(
                        egui::DragValue::new(&mut h)
                            .range(0.5..=2000.0)
                            .speed(0.5)
                            .suffix(" %")
                            .max_decimals(1),
                    )
                    .changed()
                {
                    let k = h / 100.0 / pl.sy;
                    pl.sy = h / 100.0;
                    if t.keep_aspect {
                        pl.sx *= k;
                    }
                }
                ui.end_row();
                ui.label(tr("Angle"));
                let mut a = pl.angle.to_degrees();
                if ui
                    .add(
                        egui::DragValue::new(&mut a)
                            .speed(0.5)
                            .suffix("°")
                            .max_decimals(1),
                    )
                    .changed()
                {
                    pl.angle = a.to_radians();
                }
                ui.end_row();
            });
        ui.checkbox(&mut t.keep_aspect, tr("Keep proportions (Shift toggles)"));
        if ui
            .checkbox(&mut t.smooth, tr("Smooth (off for crisp 1-bit art)"))
            .changed()
        {
            t.tex = None;
        }
        ui.horizontal(|ui| {
            if ui
                .add(IconButton::new(icons::FLIP_HORIZONTAL, tr("Flip horizontally")).size(26.0))
                .clicked()
            {
                pl.flip_x = !pl.flip_x;
            }
            if ui
                .add(IconButton::new(icons::FLIP_VERTICAL, tr("Flip vertically")).size(26.0))
                .clicked()
            {
                pl.flip_y = !pl.flip_y;
            }
            if ui
                .add(IconButton::new(icons::ARROWS_CLOCKWISE, tr("Rotate 90°")).size(26.0))
                .clicked()
            {
                pl.angle += std::f32::consts::FRAC_PI_2;
            }
            if ui
                .add(Button::new(tr("Reset")).size(ButtonSize::Sm))
                .clicked()
            {
                pl = t.start;
            }
        });
        t.place = pl;
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui
                .add(Button::new(tr("Apply (Enter)")).variant(ButtonVariant::Primary))
                .clicked()
            {
                self.commit_transform();
            } else if ui.add(Button::new(tr("Cancel (Esc)"))).clicked() {
                self.cancel_transform();
            }
        });
        ui.label(RichText::new(tr("Drag inside to move, corners to scale, the round handle (or outside) to rotate. Shift: free scaling / 15° steps.")).text_style(text::caption()).color(p.text_muted));
    }
}

/// Screen positions of the handles.
fn handle_positions(t: &Transform, v: &View) -> Vec<(Pos2, Handle)> {
    let (hw, hh) = t.half();
    let mut out = Vec::new();
    let s = |x: f32, y: f32| {
        let c = t.place.forward(Pt::new(x * hw, y * hh));
        v.to_screen(c.x, c.y)
    };
    for (x, y) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        out.push((s(x, y), Handle::Corner(x, y)));
    }
    for (x, y) in [(0.0, -1.0), (1.0, 0.0), (0.0, 1.0), (-1.0, 0.0)] {
        out.push((s(x, y), Handle::Edge(x, y)));
    }
    // Rotation knob 28 px "above" the top edge, in screen space.
    let top = s(0.0, -1.0);
    let center = s(0.0, 0.0);
    let up = (top - center).normalized();
    let up = if up.is_finite() { up } else { vec2(0.0, -1.0) };
    out.push((top + up * 28.0, Handle::Rotate));
    out
}

fn hit(t: &Transform, v: &View, pos: Pos2) -> Handle {
    if let Some((_, h)) = handle_positions(t, v)
        .into_iter()
        .filter(|(p, _)| (*p - pos).length() < 10.0)
        .min_by(|a, b| (a.0 - pos).length().total_cmp(&(b.0 - pos).length()))
    {
        return h;
    }
    let c = v.to_canvas(pos);
    let u = t.place.inverse(Pt::new(c.0, c.1));
    let (hw, hh) = t.half();
    if u.x.abs() <= hw && u.y.abs() <= hh {
        Handle::Move
    } else {
        Handle::Rotate
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checker(w: usize, h: usize) -> Vec<Rgba> {
        (0..w * h)
            .map(|i| {
                if (i % w + i / w) % 2 == 0 {
                    Rgba::BLACK
                } else {
                    Rgba::WHITE
                }
            })
            .collect()
    }

    #[test]
    fn identity_render_copies_pixels() {
        let src = checker(10, 6);
        let place = Placement {
            center: Pt::new(105.0, 53.0),
            sx: 1.0,
            sy: 1.0,
            angle: 0.0,
            flip_x: false,
            flip_y: false,
        };
        let tiles = render(&src, 10, 6, &place, false, (512, 512));
        let get = |x: usize, y: usize| {
            tiles
                .iter()
                .find(|(k, _)| *k == (0, 0))
                .unwrap()
                .1
                .get(x, y)
        };
        assert_eq!(get(100, 50), Rgba::BLACK);
        assert_eq!(get(101, 50), Rgba::WHITE);
        assert_eq!(get(109, 55), Rgba::BLACK);
        assert_eq!(get(110, 50).a(), 0);
        // Inverse of forward is the identity.
        let p = Placement {
            center: Pt::new(3.0, 4.0),
            sx: 2.0,
            sy: 0.5,
            angle: 0.7,
            flip_x: true,
            flip_y: false,
        };
        let u = Pt::new(-3.0, 8.0);
        let back = p.inverse(p.forward(u));
        assert!(back.sub(u).len() < 1e-3);
    }

    #[test]
    fn half_scale_and_rotation_land_where_expected() {
        let src = vec![Rgba::BLACK; 200 * 100];
        let place = Placement {
            center: Pt::new(256.0, 256.0),
            sx: 0.5,
            sy: 0.5,
            angle: std::f32::consts::FRAC_PI_2,
            flip_x: false,
            flip_y: false,
        };
        let tiles = render(&src, 200, 100, &place, true, (1024, 1024));
        let get = |x: i32, y: i32| {
            let (k, lx, ly) = ink_core::tile::split(x, y);
            tiles
                .iter()
                .find(|(kk, _)| *kk == k)
                .map_or(Rgba::TRANSPARENT, |t| t.1.get(lx, ly))
        };
        // 200×100 at 50 % rotated 90° → 50 wide, 100 tall around the center.
        assert_eq!(get(256, 256 - 45).a(), 255);
        assert_eq!(get(256 - 20, 256).a(), 255);
        assert_eq!(get(256 - 35, 256).a(), 0);
        assert_eq!(get(256, 256 - 60).a(), 0);
    }

    #[test]
    fn decode_png() {
        let mut bytes = Vec::new();
        {
            let mut e = png::Encoder::new(&mut bytes, 2, 1);
            e.set_color(png::ColorType::Rgba);
            e.set_depth(png::BitDepth::Eight);
            e.write_header()
                .unwrap()
                .write_image_data(&[255, 0, 0, 255, 0, 0, 255, 128])
                .unwrap();
        }
        let (px, w, h) = decode_image(&bytes).unwrap();
        assert_eq!((w, h), (2, 1));
        assert_eq!(px[0], Rgba::from_straight(255, 0, 0, 255));
        assert_eq!(px[1].a(), 128);
    }
}
