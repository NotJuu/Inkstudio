//! Brush engine: what makes inking feel good.
//!
//! * **Stabilization**: raw pen samples are smoothed with a moving window before dabs are placed;
//!   **post-correction** pulls the stroke end onto the last real pen position.
//! * **Pressure curves** for size and opacity, with a minimum size.
//! * **Taper** (Einspitzen/Ausspitzen): size ramps in at the start and out at the end.
//! * **Stroke buffer**: dabs write *coverage* into a per-stroke buffer (max or accumulate);
//!   the layer is recomposited from the pre-stroke tiles, so overlapping dabs never darken a
//!   stroke beyond its opacity, and undo is just the pre-stroke tiles.
//! * Only tiles touched since the last update are recomposited.

use crate::curve::Curve;
use crate::tip::{hash01, sdf, Grain, Scatter, TipShape};
use crate::tone::{Tone, ToneSettings};
use ink_core::tile::{TileKey, TILE, TILE_PX};
use ink_core::{Document, LayerId, Rgba, Tile, UndoEntry};
use std::collections::HashMap;
use std::sync::Arc;

/// One pen sample in canvas pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {
    pub x: f32,
    pub y: f32,
    /// 0..=1. Mice report 1.0.
    pub pressure: f32,
    /// Seconds since an arbitrary origin.
    pub time: f64,
    /// Extra size multiplier (vector strokes store their baked taper / width correction here).
    pub scale: f32,
}

impl Sample {
    pub fn new(x: f32, y: f32, pressure: f32) -> Self {
        Self { x, y, pressure, time: 0.0, scale: 1.0 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Coverage {
    /// Pens: a pixel takes the strongest dab that touched it. Uniform opacity along the stroke.
    Max,
    /// Airbrush / soft brushes: dabs build up.
    Accumulate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Paint {
    Color,
    Erase,
}

#[derive(Clone, Debug)]
pub struct BrushParams {
    pub name: String,
    /// Diameter at full pressure, in pixels.
    pub size: f32,
    /// Fraction of `size` at zero pressure (0..=1).
    pub min_size: f32,
    pub size_curve: Option<Curve>,
    /// 0..=1.
    pub opacity: f32,
    pub opacity_curve: Option<Curve>,
    /// Per-dab strength for Accumulate brushes, 0..=1.
    pub flow: f32,
    /// 1 = hard edge (1px anti-aliased), 0 = fully soft.
    pub hardness: f32,
    /// Dab spacing as a fraction of the current diameter.
    pub spacing: f32,
    pub coverage: Coverage,
    pub paint: Paint,
    /// Taper lengths in pixels (0 = off).
    pub taper_start: f32,
    pub taper_end: f32,
    /// 0..=100. Window of samples averaged.
    pub stabilization: u32,
    pub post_correction: bool,
    pub anti_alias: bool,
    // ---- tip ---------------------------------------------------------------------------
    pub tip: TipShape,
    /// 1 = round; < 1 squashes the tip (flat brushes, calligraphy nibs).
    pub roundness: f32,
    /// Tip rotation in degrees (relative to the stroke direction if `follow_direction`).
    pub angle_deg: f32,
    pub follow_direction: bool,
    // ---- texture and effects ----------------------------------------------------------------
    pub grain: Option<Grain>,
    pub scatter: Option<Scatter>,
    /// > 0: lines get thinner when drawn fast (brush pens); < 0: thicker.
    pub speed_size: f32,
    /// Dashed line: (dash, gap) in multiples of the brush diameter.
    pub dash: Option<(f32, f32)>,
    /// Paint (or erase) only where this screentone has dots: tone pens and tone scratching.
    pub pattern: Option<ToneSettings>,
}

impl BrushParams {
    /// Crisp inking pen: size follows pressure, both ends tapered, light stabilization.
    pub fn g_pen(size: f32) -> Self {
        Self {
            name: "G-pen".into(),
            size,
            min_size: 0.08,
            size_curve: Some(Curve::gamma(1.2)),
            opacity: 1.0,
            opacity_curve: None,
            flow: 1.0,
            hardness: 1.0,
            spacing: 0.08,
            coverage: Coverage::Max,
            paint: Paint::Color,
            taper_start: size * 2.0,
            taper_end: size * 3.0,
            stabilization: 12,
            post_correction: true,
            anti_alias: true,
            tip: TipShape::Round,
            roundness: 1.0,
            angle_deg: 0.0,
            follow_direction: false,
            grain: None,
            scatter: None,
            speed_size: 0.0,
            dash: None,
            pattern: None,
        }
    }

    /// Fine, even lines for details and hatching: steeper pressure curve, thinner minimum.
    pub fn mapping_pen(size: f32) -> Self {
        Self { name: "Mapping pen".into(), min_size: 0.15, size_curve: Some(Curve::gamma(0.8)), taper_start: size * 4.0, taper_end: size * 4.0, ..Self::g_pen(size) }
    }

    /// Sketching: pressure drives opacity, slightly soft edge.
    pub fn pencil(size: f32) -> Self {
        Self {
            name: "Pencil".into(),
            min_size: 0.6,
            opacity: 0.9,
            opacity_curve: Some(Curve::gamma(1.5)),
            hardness: 0.75,
            spacing: 0.12,
            taper_start: 0.0,
            taper_end: 0.0,
            stabilization: 4,
            ..Self::g_pen(size)
        }
    }

    pub fn airbrush(size: f32) -> Self {
        Self {
            name: "Airbrush".into(),
            min_size: 1.0,
            size_curve: None,
            opacity: 1.0,
            opacity_curve: Some(Curve::linear()),
            flow: 0.08,
            hardness: 0.0,
            spacing: 0.1,
            coverage: Coverage::Accumulate,
            taper_start: 0.0,
            taper_end: 0.0,
            stabilization: 0,
            ..Self::g_pen(size)
        }
    }

    pub fn eraser(size: f32) -> Self {
        Self { name: "Eraser".into(), min_size: 0.5, paint: Paint::Erase, taper_start: 0.0, taper_end: 0.0, stabilization: 0, ..Self::g_pen(size) }
    }

    fn radius_at(&self, pressure: f32) -> f32 {
        let p = self.size_curve.as_ref().map_or(1.0, |c| c.eval(pressure));
        0.5 * self.size * (self.min_size + (1.0 - self.min_size) * p)
    }

    fn alpha_at(&self, pressure: f32) -> f32 {
        self.opacity * self.opacity_curve.as_ref().map_or(1.0, |c| c.eval(pressure))
    }
}

/// Moving-average stabilizer (window grows with the stabilization level).
#[derive(Clone, Debug, Default)]
pub struct Stabilizer {
    window: usize,
    buf: std::collections::VecDeque<Sample>,
}

impl Stabilizer {
    pub fn new(level: u32) -> Self {
        Self { window: 1 + level as usize / 3, buf: Default::default() }
    }

    pub fn push(&mut self, s: Sample) -> Sample {
        self.buf.push_back(s);
        while self.buf.len() > self.window {
            self.buf.pop_front();
        }
        // Triangular weights: recent samples count more, so lag stays small for a given smoothness.
        let mut acc = (0.0f32, 0.0f32, 0.0f32);
        let mut wsum = 0.0f32;
        for (i, b) in self.buf.iter().enumerate() {
            let w = (i + 1) as f32;
            acc.0 += b.x * w;
            acc.1 += b.y * w;
            acc.2 += b.pressure * w;
            wsum += w;
        }
        Sample { x: acc.0 / wsum, y: acc.1 / wsum, pressure: acc.2 / wsum, time: s.time, scale: s.scale }
    }

    /// Samples that walk the smoothed position onto the last raw sample (post-correction).
    pub fn drain(&mut self) -> Vec<Sample> {
        let Some(&last) = self.buf.back() else { return Vec::new() };
        (1..self.window).map(|_| self.push(last)).collect()
    }
}

#[derive(Clone, Copy, Debug)]
struct Dab {
    x: f32,
    y: f32,
    radius: f32,
    alpha: f32,
    /// Arc length from the stroke start.
    dist: f32,
    /// Tip rotation in radians.
    angle: f32,
    /// Random seed for scatter (stable, so re-rendering a stroke gives identical pixels).
    seed: u32,
}

/// A smoothed point of the stroke path, with the radius and arc length the engine used.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PathPoint {
    pub sample: Sample,
    pub dist: f32,
}

/// Per-stroke coverage (0..=65535) in sparse tiles.
#[derive(Default)]
struct CoverageBuffer {
    tiles: HashMap<TileKey, Box<[u16]>>,
}

impl CoverageBuffer {
    fn tile(&mut self, k: TileKey) -> &mut [u16] {
        self.tiles.entry(k).or_insert_with(|| vec![0u16; TILE_PX].into_boxed_slice())
    }
}

/// An in-progress stroke on a raster layer.
pub struct Stroke {
    pub params: BrushParams,
    pub color: Rgba,
    pub layer: LayerId,
    stab: Stabilizer,
    last: Option<Sample>,
    /// Distance travelled since the last dab.
    carry: f32,
    length: f32,
    dabs: Vec<Dab>,
    cov: CoverageBuffer,
    /// Pre-stroke tiles: the composite source and the undo record.
    base: HashMap<TileKey, Option<Arc<Tile>>>,
    lock_alpha: bool,
    /// Tiles changed since the last flush, with the changed local rect (x0, y0, x1, y1).
    pending: HashMap<TileKey, [u16; 4]>,
    path: Vec<PathPoint>,
    tone: Option<Tone>,
    /// Current stroke direction (radians) and smoothed speed (px/s).
    dir: f32,
    speed: f32,
    seq: u32,
}

impl Stroke {
    pub fn begin(doc: &Document, layer: LayerId, params: BrushParams, color: Rgba) -> Self {
        let lock_alpha = doc.layer(layer).is_some_and(|l| l.props.lock_alpha);
        let tone = params.pattern.map(|t| Tone::new(t, doc.dpi as f32));
        Self {
            stab: Stabilizer::new(params.stabilization),
            params,
            color,
            layer,
            last: None,
            carry: 0.0,
            length: 0.0,
            dabs: Vec::new(),
            cov: CoverageBuffer::default(),
            base: HashMap::new(),
            lock_alpha,
            pending: HashMap::new(),
            path: Vec::new(),
            tone,
            dir: 0.0,
            speed: 0.0,
            seq: 0,
        }
    }

    /// The smoothed path so far (after stabilization), for vector layers.
    pub fn path(&self) -> &[PathPoint] {
        &self.path
    }

    /// Total arc length of the smoothed path.
    pub fn length(&self) -> f32 {
        self.length
    }

    /// Finish and also return the smoothed path (vector layers keep it as an editable stroke).
    pub fn end_with_path(mut self, doc: &mut Document) -> (UndoEntry, Vec<PathPoint>, f32) {
        if self.params.post_correction {
            for s in self.stab.drain() {
                self.place(s);
            }
            self.params.post_correction = false;
        }
        let path = std::mem::take(&mut self.path);
        let len = self.length;
        (self.end(doc), path, len)
    }

    /// Feed one raw pen sample. Returns the tiles that changed on screen.
    pub fn add(&mut self, doc: &mut Document, s: Sample) -> Vec<TileKey> {
        let sm = self.stab.push(s);
        self.place(sm);
        self.flush(doc)
    }

    /// Finish: post-correction, end taper, final composite, undo entry.
    pub fn end(mut self, doc: &mut Document) -> UndoEntry {
        if self.params.post_correction {
            for s in self.stab.drain() {
                self.place(s);
            }
        }
        if self.dabs.is_empty() {
            // A tap: one dab at the pen position.
            if let Some(s) = self.last {
                self.push_dab(s.x, s.y, s.pressure, 0.0);
            }
        }
        if self.params.taper_end > 0.0 && self.length > 0.0 {
            // Re-render the whole stroke with the end taper now that its length is known.
            let (len, te) = (self.length, self.params.taper_end.min(self.length * 0.5));
            for d in &mut self.dabs {
                let left = len - d.dist;
                if left < te {
                    d.radius *= (left / te).max(0.0).sqrt().max(0.05);
                }
            }
            let keys: Vec<TileKey> = self.cov.tiles.keys().copied().collect();
            for k in keys {
                self.cov.tile(k).fill(0);
                self.pending.insert(k, [0, 0, TILE as u16, TILE as u16]);
            }
            let dabs = std::mem::take(&mut self.dabs);
            for d in &dabs {
                self.stamp(d);
            }
            self.dabs = dabs;
        }
        self.flush(doc);
        if let Some(l) = doc.layer_mut(self.layer) {
            for k in self.base.keys() {
                if l.raster.tile(*k).is_some_and(|t| t.is_transparent()) {
                    l.raster.put_tile(*k, None);
                }
            }
        }
        let label = self.params.name.clone();
        UndoEntry::Tiles { label, layer: self.layer, tiles: self.base.into_iter().collect() }
    }

    fn place(&mut self, s: Sample) {
        let Some(prev) = self.last else {
            self.last = Some(s);
            self.path.push(PathPoint { sample: s, dist: 0.0 });
            self.push_dab_scaled(s.x, s.y, s.pressure, 0.0, s.scale);
            return;
        };
        let (dx, dy) = (s.x - prev.x, s.y - prev.y);
        let seg = (dx * dx + dy * dy).sqrt();
        if seg < 1e-4 {
            return;
        }
        self.dir = dy.atan2(dx);
        let dt = (s.time - prev.time) as f32;
        if dt > 1e-4 {
            self.speed = self.speed * 0.6 + (seg / dt) * 0.4;
        }
        let mut t = 0.0f32;
        loop {
            let p = prev.pressure + (s.pressure - prev.pressure) * t;
            let step = (self.params.radius_at(p) * 2.0 * self.params.spacing).max(0.35);
            let need = step - self.carry;
            if t * seg + need > seg {
                self.carry += seg - t * seg;
                break;
            }
            t += need / seg;
            self.carry = 0.0;
            let p = prev.pressure + (s.pressure - prev.pressure) * t;
            let sc = prev.scale + (s.scale - prev.scale) * t;
            let dist = self.length + t * seg;
            self.push_dab_scaled(prev.x + dx * t, prev.y + dy * t, p, dist, sc);
        }
        self.length += seg;
        self.path.push(PathPoint { sample: s, dist: self.length });
        self.last = Some(s);
    }

    fn push_dab(&mut self, x: f32, y: f32, pressure: f32, dist: f32) {
        self.push_dab_scaled(x, y, pressure, dist, 1.0);
    }

    fn push_dab_scaled(&mut self, x: f32, y: f32, pressure: f32, dist: f32, scale: f32) {
        let pr = &self.params;
        let mut radius = pr.radius_at(pressure) * scale;
        if pr.taper_start > 0.0 && dist < pr.taper_start {
            radius *= (dist / pr.taper_start).sqrt().max(0.05);
        }
        if pr.speed_size != 0.0 {
            radius *= (1.0 - pr.speed_size * (self.speed / 1500.0).min(1.0)).clamp(0.15, 2.0);
        }
        if let Some((on, off)) = pr.dash {
            let period = (on + off) * pr.size;
            if period > 0.0 && dist % period > on * pr.size {
                return;
            }
        }
        let angle = pr.angle_deg.to_radians() + if pr.follow_direction { self.dir } else { 0.0 };
        self.seq = self.seq.wrapping_add(1);
        let d = Dab { x, y, radius, alpha: pr.alpha_at(pressure), dist, angle, seed: self.seq };
        self.stamp(&d);
        self.dabs.push(d);
    }

    /// Rasterize one dab (or its scattered particles) into the coverage buffer.
    fn stamp(&mut self, d: &Dab) {
        match self.params.scatter {
            None => self.stamp_one(d.x, d.y, d.radius, d.angle, d.alpha),
            Some(sc) => {
                for k in 0..sc.count as u32 {
                    let rnd = |j: u32| hash01(d.seed, k * 8 + j);
                    let a = rnd(0) * std::f32::consts::TAU;
                    let off = rnd(1).sqrt() * sc.spread * d.radius;
                    let r = d.radius * sc.size * (1.0 - sc.size_jitter * rnd(2));
                    let ang = d.angle + sc.angle_jitter * (rnd(3) * 2.0 - 1.0) * std::f32::consts::PI;
                    let alpha = d.alpha * (1.0 - sc.opacity_jitter * rnd(4));
                    self.stamp_one(d.x + a.cos() * off, d.y + a.sin() * off, r, ang, alpha);
                }
            }
        }
    }

    fn stamp_one(&mut self, cx: f32, cy: f32, radius: f32, angle: f32, alpha: f32) {
        let plain = self.params.tip == TipShape::Round && self.params.roundness >= 0.999 && self.params.grain.is_none() && self.tone.is_none();
        if !plain {
            return self.stamp_shaped(cx, cy, radius, angle, alpha);
        }
        let d = Dab { x: cx, y: cy, radius, alpha, dist: 0.0, angle, seed: 0 };
        let r = d.radius.max(0.3);
        let hard = self.params.hardness.clamp(0.0, 1.0);
        let aa = if self.params.anti_alias { 1.0 } else { 0.0 };
        // Edge: hard brushes fade over 1px, soft brushes over (1-hardness) of the radius.
        let fade = (r * (1.0 - hard)).max(aa).max(1e-3);
        let inner = (r - fade * 0.5).max(0.0);
        let (x0, y0) = ((d.x - r - 1.0).floor() as i32, (d.y - r - 1.0).floor() as i32);
        let (x1, y1) = ((d.x + r + 1.0).ceil() as i32 + 1, (d.y + r + 1.0).ceil() as i32 + 1);
        let accumulate = self.params.coverage == Coverage::Accumulate;
        let strength = if accumulate { self.params.flow } else { 1.0 } * d.alpha;
        for k in ink_core::tile::keys_in(x0, y0, x1, y1) {
            let ox = k.0 * TILE as i32;
            let oy = k.1 * TILE as i32;
            let (lx0, ly0) = ((x0 - ox).max(0) as usize, (y0 - oy).max(0) as usize);
            let (lx1, ly1) = ((x1 - ox).min(TILE as i32) as usize, (y1 - oy).min(TILE as i32) as usize);
            let pr = self.pending.entry(k).or_insert([u16::MAX, u16::MAX, 0, 0]);
            *pr = [pr[0].min(lx0 as u16), pr[1].min(ly0 as u16), pr[2].max(lx1 as u16), pr[3].max(ly1 as u16)];
            let buf = self.cov.tile(k);
            for ly in ly0..ly1 {
                let py = (oy + ly as i32) as f32 + 0.5 - d.y;
                let row = &mut buf[ly * TILE..ly * TILE + TILE];
                for lx in lx0..lx1 {
                    let px = (ox + lx as i32) as f32 + 0.5 - d.x;
                    let dist = (px * px + py * py).sqrt();
                    let a = if aa == 0.0 && hard >= 1.0 {
                        if dist <= r { 1.0 } else { 0.0 }
                    } else {
                        let t = ((dist - inner) / fade).clamp(0.0, 1.0);
                        let s = 1.0 - t;
                        if hard >= 1.0 { s } else { s * s * (3.0 - 2.0 * s) }
                    };
                    if a <= 0.0 {
                        continue;
                    }
                    let a = (a * strength * 65535.0) as u32;
                    let c = &mut row[lx];
                    *c = if accumulate {
                        (*c as u32 + a * (65535 - *c as u32) / 65535).min(65535) as u16
                    } else {
                        (*c as u32).max(a) as u16
                    };
                }
            }
        }
    }

    /// General dab: SDF tip with rotation and roundness, paper grain, tone pattern.
    fn stamp_shaped(&mut self, cx: f32, cy: f32, radius: f32, angle: f32, alpha: f32) {
        let r = radius.max(0.3);
        let pr = &self.params;
        let round = pr.roundness.clamp(0.05, 1.0);
        let hard = pr.hardness.clamp(0.0, 1.0);
        let aa = if pr.anti_alias { 1.0 } else { 0.0 };
        // Fade width in pixels, measured across the thin axis so squashed tips stay crisp.
        let fade = (r * round * (1.0 - hard)).max(aa).max(1e-3);
        let (sn, cs) = angle.sin_cos();
        let accumulate = pr.coverage == Coverage::Accumulate;
        let strength = if accumulate { pr.flow } else { 1.0 } * alpha;
        let (tip, grain) = (pr.tip, pr.grain);
        let ext = r + 1.5;
        let (x0, y0) = ((cx - ext).floor() as i32, (cy - ext).floor() as i32);
        let (x1, y1) = ((cx + ext).ceil() as i32 + 1, (cy + ext).ceil() as i32 + 1);
        for k in ink_core::tile::keys_in(x0, y0, x1, y1) {
            let ox = k.0 * TILE as i32;
            let oy = k.1 * TILE as i32;
            let (lx0, ly0) = ((x0 - ox).max(0) as usize, (y0 - oy).max(0) as usize);
            let (lx1, ly1) = ((x1 - ox).min(TILE as i32) as usize, (y1 - oy).min(TILE as i32) as usize);
            let pe = self.pending.entry(k).or_insert([u16::MAX, u16::MAX, 0, 0]);
            *pe = [pe[0].min(lx0 as u16), pe[1].min(ly0 as u16), pe[2].max(lx1 as u16), pe[3].max(ly1 as u16)];
            let tone = self.tone.as_ref();
            let buf = self.cov.tiles.entry(k).or_insert_with(|| vec![0u16; TILE_PX].into_boxed_slice());
            for ly in ly0..ly1 {
                let gy = (oy + ly as i32) as f32 + 0.5;
                let py = gy - cy;
                for lx in lx0..lx1 {
                    let gx = (ox + lx as i32) as f32 + 0.5;
                    let px = gx - cx;
                    // Into tip space: undo rotation, stretch the thin axis.
                    let qx = (px * cs + py * sn) / r;
                    let qy = (-px * sn + py * cs) / (r * round);
                    let dist = sdf(tip, qx, qy) * r * round;
                    let t = ((dist + fade * 0.5) / fade).clamp(0.0, 1.0);
                    if t >= 1.0 {
                        continue;
                    }
                    let s = 1.0 - t;
                    let mut a = if hard >= 1.0 { s } else { s * s * (3.0 - 2.0 * s) };
                    if let Some(g) = grain {
                        a *= g.factor(gx, gy);
                    }
                    if let Some(tn) = tone {
                        a *= tn.coverage(gx as i32, gy as i32, false);
                    }
                    if a <= 0.0 {
                        continue;
                    }
                    let a = (a * strength * 65535.0) as u32;
                    let c = &mut buf[ly * TILE + lx];
                    *c = if accumulate {
                        (*c as u32 + a * (65535 - *c as u32) / 65535).min(65535) as u16
                    } else {
                        (*c as u32).max(a) as u16
                    };
                }
            }
        }
    }

    /// Recomposite pending tiles from (base, coverage) into the layer.
    fn flush(&mut self, doc: &mut Document) -> Vec<TileKey> {
        let jobs: Vec<(TileKey, [u16; 4])> = self.pending.drain().collect();
        let keys: Vec<TileKey> = jobs.iter().map(|(k, _)| *k).collect();
        let Some(layer) = doc.layer_mut(self.layer) else { return Vec::new() };
        if layer.props.locked {
            return Vec::new();
        }
        let color = self.color;
        let erase = self.params.paint == Paint::Erase;
        let lock_alpha = self.lock_alpha;
        for &(k, [x0, y0, x1, y1]) in &jobs {
            let base = self.base.entry(k).or_insert_with(|| layer.raster.tile(k).cloned()).clone();
            if base.is_none() && (erase || lock_alpha) {
                continue; // nothing to erase / nothing to paint onto
            }
            let cov = self.cov.tiles.get(&k).expect("pending tile has coverage");
            // Write in place: the first write un-shares the tile from `base` (one copy per tile
            // per stroke); after that only the changed rectangle is recomputed.
            let out = layer.raster.tile_mut(k);
            for y in y0 as usize..y1 as usize {
                for i in y * TILE + x0 as usize..y * TILE + x1 as usize {
                let c = cov[i];
                if c == 0 {
                    continue;
                }
                let k8 = ((c as u32 * 255 + 32767) / 65535) as u8;
                let b = base.as_ref().map_or(Rgba::TRANSPARENT, |t| t.px[i]);
                out.px[i] = if erase {
                    b.scale(255 - k8)
                } else if lock_alpha {
                    // Recolor visible pixels, keep their alpha.
                    let src = color.scale(b.a());
                    let mixed = src.scale(k8);
                    Rgba([0, 1, 2, 3].map(|j| if j == 3 { b.0[3] } else { mixed.0[j].saturating_add(ink_core::color::mul8(b.0[j], 255 - k8)) }))
                } else {
                    color.scale(k8).over(b)
                };
                }
            }
        }
        doc.mark_dirty(keys.iter().copied());
        keys
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ink_core::Layer;

    fn doc() -> (Document, LayerId) {
        let mut d = Document::new(512, 512, 600);
        let id = d.push_layer(Layer::new("ink"));
        (d, id)
    }

    fn line(d: &mut Document, id: LayerId, p: BrushParams, from: (f32, f32), to: (f32, f32), pressure: f32) -> UndoEntry {
        let mut s = Stroke::begin(d, id, p, Rgba::BLACK);
        for i in 0..=50 {
            let t = i as f32 / 50.0;
            s.add(d, Sample::new(from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t, pressure));
        }
        s.end(d)
    }

    #[test]
    fn overlapping_dabs_do_not_exceed_opacity() {
        let (mut d, id) = doc();
        let mut p = BrushParams::g_pen(20.0);
        p.opacity = 0.5;
        p.taper_start = 0.0;
        p.taper_end = 0.0;
        p.size_curve = None;
        line(&mut d, id, p, (50.0, 100.0), (450.0, 100.0), 1.0);
        let a = d.layer(id).unwrap().raster.get(250, 100).a();
        assert!((126..=129).contains(&a), "alpha {a}");
    }

    #[test]
    fn pressure_changes_width_and_taper_thins_ends() {
        let (mut d, id) = doc();
        let mut p = BrushParams::g_pen(16.0);
        p.stabilization = 0;
        line(&mut d, id, p, (40.0, 200.0), (470.0, 200.0), 1.0);
        let r = &d.layer(id).unwrap().raster;
        let width = |x: i32| (180..220).filter(|&y| r.get(x, y).a() > 127).count();
        assert!(width(255) >= 14, "middle {}", width(255));
        assert!(width(45) < width(255), "start taper {} vs {}", width(45), width(255));
        assert!(width(466) < width(255), "end taper {} vs {}", width(466), width(255));

        let (mut d2, id2) = doc();
        let mut light = BrushParams::g_pen(16.0);
        light.taper_start = 0.0;
        light.taper_end = 0.0;
        line(&mut d2, id2, light, (40.0, 200.0), (470.0, 200.0), 0.2);
        let r2 = &d2.layer(id2).unwrap().raster;
        let w2 = (180..220).filter(|&y| r2.get(255, y).a() > 127).count();
        assert!(w2 < 8, "light pressure width {w2}");
    }

    #[test]
    fn undo_restores_pre_stroke_tiles() {
        let (mut d, id) = doc();
        let mut h = ink_core::History::default();
        let e = line(&mut d, id, BrushParams::g_pen(10.0), (10.0, 10.0), (400.0, 300.0), 1.0);
        h.push(e);
        assert!(d.layer(id).unwrap().raster.tile_count() > 0);
        h.undo(&mut d);
        assert_eq!(d.layer(id).unwrap().raster.tile_count(), 0);
    }

    #[test]
    fn eraser_removes_ink() {
        let (mut d, id) = doc();
        let mut p = BrushParams::g_pen(12.0);
        p.taper_start = 0.0;
        p.taper_end = 0.0;
        line(&mut d, id, p, (20.0, 50.0), (300.0, 50.0), 1.0);
        assert!(d.layer(id).unwrap().raster.get(150, 50).a() == 255);
        line(&mut d, id, BrushParams::eraser(40.0), (150.0, 0.0), (150.0, 120.0), 1.0);
        assert_eq!(d.layer(id).unwrap().raster.get(150, 50).a(), 0);
    }

    #[test]
    fn stabilizer_smooths_jitter() {
        let mut st = Stabilizer::new(30);
        let mut max_dev = 0.0f32;
        for i in 0..200 {
            let jitter = if i % 2 == 0 { 3.0 } else { -3.0 };
            let o = st.push(Sample::new(i as f32, 100.0 + jitter, 1.0));
            if i > 20 {
                max_dev = max_dev.max((o.y - 100.0).abs());
            }
        }
        assert!(max_dev < 0.6, "deviation {max_dev}");
    }
}
