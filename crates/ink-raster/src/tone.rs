//! Screentones (halftone) for manga printing.
//!
//! A tone is defined by lines per inch (LPI), density, angle and dot shape, and is rendered at
//! the page DPI, so the dot pitch on paper is exact. Density is *calibrated*: a threshold
//! table built from the dot shape's own value distribution makes a 20% tone cover 20% of the
//! paper for every shape. Output can be 1-bit (print) or anti-aliased (screen preview).

use ink_core::tile::{TileKey, TILE, TILE_PX};
use ink_core::{Document, LayerId, Rgba, Tile, UndoEntry};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DotShape {
    Circle,
    Square,
    Diamond,
    Line,
    Cross,
    Noise,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ToneSettings {
    pub lpi: f32,
    /// 0..=1 fraction of paper covered.
    pub density: f32,
    pub angle_deg: f32,
    pub shape: DotShape,
    /// Ink color (usually black).
    pub color: Rgba,
}

impl Default for ToneSettings {
    fn default() -> Self {
        Self { lpi: 60.0, density: 0.2, angle_deg: 45.0, shape: DotShape::Circle, color: Rgba::BLACK }
    }
}

/// Spot function on cell coordinates u, v in [-1, 1]: higher value = inked first.
#[inline]
fn spot(shape: DotShape, u: f32, v: f32, cell_x: i32, cell_y: i32, sub: (f32, f32)) -> f32 {
    match shape {
        DotShape::Circle => 1.0 - (u * u + v * v) * 0.5,
        DotShape::Square => 1.0 - u.abs().max(v.abs()),
        DotShape::Diamond => 1.0 - (u.abs() + v.abs()) * 0.5,
        DotShape::Line => 1.0 - v.abs(),
        DotShape::Cross => 1.0 - u.abs().min(v.abs()),
        DotShape::Noise => {
            // Stable white-noise threshold per sub-pixel position.
            let mut h = (cell_x as u32).wrapping_mul(0x9E37_79B1) ^ (cell_y as u32).wrapping_mul(0x85EB_CA77);
            h ^= ((sub.0 * 64.0) as u32).wrapping_mul(0xC2B2_AE3D) ^ ((sub.1 * 64.0) as u32).wrapping_mul(0x27D4_EB2F);
            h ^= h >> 15;
            h = h.wrapping_mul(0x2C1B_3C6D);
            h ^= h >> 12;
            (h & 0xFFFF) as f32 / 65535.0
        }
    }
}

/// Precomputed renderer for one tone at one DPI.
pub struct Tone {
    s: ToneSettings,
    cell_px: f32,
    cos: f32,
    sin: f32,
    /// Spot value above which a pixel is inked, giving the requested density.
    threshold: f32,
}

impl Tone {
    pub fn new(s: ToneSettings, dpi: f32) -> Self {
        let cell_px = (dpi / s.lpi.max(1.0)).max(1.0);
        let (sin, cos) = s.angle_deg.to_radians().sin_cos();
        // Calibrate: sample the spot distribution over one cell and take the (1 - density) quantile.
        let n = 128;
        let mut vals: Vec<f32> = Vec::with_capacity(n * n);
        for j in 0..n {
            for i in 0..n {
                let u = (i as f32 + 0.5) / n as f32 * 2.0 - 1.0;
                let v = (j as f32 + 0.5) / n as f32 * 2.0 - 1.0;
                vals.push(spot(s.shape, u, v, i as i32, j as i32, (u, v)));
            }
        }
        vals.sort_by(|a, b| a.total_cmp(b));
        let d = s.density.clamp(0.0, 1.0);
        let threshold = if d <= 0.0 {
            f32::INFINITY
        } else if d >= 1.0 {
            f32::NEG_INFINITY
        } else {
            vals[((1.0 - d) * (vals.len() - 1) as f32) as usize]
        };
        Self { s, cell_px, cos, sin, threshold }
    }

    /// Coverage 0..=1 at pixel center (x, y). `aa` = 4×4 supersampling for screen preview.
    #[inline]
    pub fn coverage(&self, x: i32, y: i32, aa: bool) -> f32 {
        let samples: &[(f32, f32)] = if aa {
            &[(0.125, 0.125), (0.375, 0.125), (0.625, 0.125), (0.875, 0.125), (0.125, 0.375), (0.375, 0.375), (0.625, 0.375), (0.875, 0.375),
              (0.125, 0.625), (0.375, 0.625), (0.625, 0.625), (0.875, 0.625), (0.125, 0.875), (0.375, 0.875), (0.625, 0.875), (0.875, 0.875)]
        } else {
            &[(0.5, 0.5)]
        };
        let mut hit = 0;
        for &(ox, oy) in samples {
            let (px, py) = (x as f32 + ox, y as f32 + oy);
            // Rotate into tone space and find the position inside the cell.
            let tx = (px * self.cos + py * self.sin) / self.cell_px;
            let ty = (-px * self.sin + py * self.cos) / self.cell_px;
            let (cx, cy) = (tx.floor(), ty.floor());
            let u = (tx - cx) * 2.0 - 1.0;
            let v = (ty - cy) * 2.0 - 1.0;
            let val = spot(self.s.shape, u, v, cx as i32, cy as i32, (tx - cx, ty - cy));
            if val > self.threshold {
                hit += 1;
            }
        }
        hit as f32 / samples.len() as f32
    }

    pub fn settings(&self) -> &ToneSettings {
        &self.s
    }
}

/// Fill `region` (one byte per canvas pixel, non-zero = toned) on `target` with a tone.
/// `one_bit` = print-exact pixels; otherwise anti-aliased. Rows of tiles render in parallel.
pub fn apply_tone(doc: &mut Document, target: LayerId, region: &[u8], s: ToneSettings, one_bit: bool) -> Option<UndoEntry> {
    use rayon::prelude::*;
    let (w, h) = (doc.width as usize, doc.height as usize);
    let tone = Tone::new(s, doc.dpi as f32);
    let (tw, th) = (w.div_ceil(TILE), h.div_ceil(TILE));
    let layer = doc.layer(target)?;
    let jobs: Vec<(TileKey, Option<Arc<Tile>>)> = (0..th)
        .flat_map(|ty| (0..tw).map(move |tx| (tx, ty)))
        .filter(|&(tx, ty)| (ty * TILE..((ty + 1) * TILE).min(h)).any(|y| region[y * w + tx * TILE..y * w + ((tx + 1) * TILE).min(w)].iter().any(|v| *v != 0)))
        .map(|(tx, ty)| ((tx as i32, ty as i32), layer.raster.tile((tx as i32, ty as i32)).cloned()))
        .collect();
    let rendered: Vec<(TileKey, Tile)> = jobs
        .par_iter()
        .map(|(k, base)| {
            let mut t = base.as_deref().cloned().unwrap_or_else(Tile::empty);
            for ly in 0..TILE {
                let y = k.1 as usize * TILE + ly;
                if y >= h {
                    break;
                }
                for lx in 0..TILE {
                    let x = k.0 as usize * TILE + lx;
                    if x >= w || region[y * w + x] == 0 {
                        continue;
                    }
                    let c = tone.coverage(x as i32, y as i32, !one_bit);
                    if c > 0.0 {
                        let i = ly * TILE + lx;
                        t.px[i] = s.color.scale((c * 255.0 + 0.5) as u8).over(t.px[i]);
                    }
                }
            }
            (*k, t)
        })
        .collect();
    let layer = doc.layer_mut(target)?;
    for (k, t) in rendered {
        layer.raster.put_tile(k, Some(Arc::new(t)));
    }
    let keys: Vec<TileKey> = jobs.iter().map(|(k, _)| *k).collect();
    doc.mark_dirty(keys);
    let base: HashMap<TileKey, Option<Arc<Tile>>> = jobs.into_iter().collect();
    Some(UndoEntry::Tiles { label: "Tone".into(), layer: target, tiles: base.into_iter().collect() })
}

/// Measured ink coverage of a tone over a square patch (for tests and the tone dialog preview).
pub fn measured_density(s: ToneSettings, dpi: f32, size: i32) -> f32 {
    let t = Tone::new(s, dpi);
    let mut inked = 0usize;
    for y in 0..size {
        for x in 0..size {
            inked += (t.coverage(x, y, false) > 0.5) as usize;
        }
    }
    inked as f32 / (size * size) as f32
}

const _: () = assert!(TILE_PX == TILE * TILE);

#[cfg(test)]
mod tests {
    use super::*;
    use ink_core::Layer;

    #[test]
    fn density_is_calibrated_for_every_shape() {
        for shape in [DotShape::Circle, DotShape::Square, DotShape::Diamond, DotShape::Line, DotShape::Cross, DotShape::Noise] {
            for density in [0.1, 0.2, 0.5, 0.8] {
                let s = ToneSettings { density, shape, ..Default::default() };
                let m = measured_density(s, 600.0, 400);
                assert!((m - density).abs() < 0.03, "{shape:?} {density}: measured {m}");
            }
        }
    }

    #[test]
    fn dot_pitch_matches_lpi_at_dpi() {
        // 60 lpi at 600 dpi, 0°: a dot every 10 px along x.
        let s = ToneSettings { lpi: 60.0, density: 0.3, angle_deg: 0.0, ..Default::default() };
        let t = Tone::new(s, 600.0);
        let row: Vec<bool> = (0..100).map(|x| t.coverage(x, 5, false) > 0.5).collect();
        let starts = (1..100).filter(|&x| row[x] && !row[x - 1]).count();
        assert_eq!(starts, 10);
    }

    #[test]
    fn one_bit_output_is_pure() {
        let mut d = Document::new(256, 256, 600);
        let id = d.push_layer(Layer::new("tone"));
        let region = vec![1u8; 256 * 256];
        apply_tone(&mut d, id, &region, ToneSettings::default(), true).unwrap();
        let r = &d.layer(id).unwrap().raster;
        for y in 0..256 {
            for x in 0..256 {
                let a = r.get(x, y).a();
                assert!(a == 0 || a == 255, "pixel {x},{y} alpha {a}");
            }
        }
    }
}
