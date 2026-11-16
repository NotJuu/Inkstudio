//! Bucket fill for flatting line art.
//!
//! * **Refer to other layers**: the region is found on the reference layers (or all layers),
//!   while the color goes onto the target layer under the line art.
//! * **Close gap**: the line mask is dilated by `gap` px before the flood fill, so small breaks
//!   in the line art do not leak; the region is then grown back by `gap` px into free pixels.
//! * **Expand** (color margin): grows the result under the lines so no white halo remains.
//!
//! Every pass is O(pixels): separable distance dilation and a scanline flood fill, with rayon
//! over rows. Masks are one byte per pixel.

use ink_core::tile::{TILE, TileKey};
use ink_core::{Document, LayerId, Rgba, UndoEntry};
use rayon::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refer {
    /// Only the target layer.
    Target,
    /// Layers marked as reference layers.
    References,
    /// Every visible layer.
    AllLayers,
}

#[derive(Clone, Copy, Debug)]
pub struct FillOptions {
    pub refer: Refer,
    /// Max straight-RGBA channel difference to the seed color, 0..=255.
    pub tolerance: u8,
    /// Close gaps up to this size (px). 0 = off.
    pub gap: u32,
    /// Grow the filled area by this many px (under the line art).
    pub expand: u32,
}

impl Default for FillOptions {
    fn default() -> Self {
        Self {
            refer: Refer::References,
            tolerance: 24,
            gap: 0,
            expand: 1,
        }
    }
}

/// Layers the fill looks at.
fn reference_layers(doc: &Document, target: LayerId, refer: Refer) -> Vec<&ink_core::Layer> {
    doc.layers()
        .iter()
        .filter(|(id, l)| match refer {
            Refer::Target => *id == target,
            Refer::References => l.props.visible && (l.props.reference || *id == target),
            Refer::AllLayers => l.props.visible,
        })
        .map(|(_, l)| l)
        .collect()
}

#[inline]
fn reference_px(
    layers: &[&ink_core::Layer],
    tiles: &[Option<&Arc<ink_core::Tile>>],
    i: usize,
) -> Rgba {
    let mut c = Rgba::TRANSPARENT;
    for (l, t) in layers.iter().zip(tiles) {
        if let Some(t) = t {
            let s = t.px[i];
            if s.a() != 0 {
                c = s.scale(l.props.opacity).over(c);
            }
        }
    }
    c
}

/// Barrier mask (1 = differs from the seed color), built tile by tile in parallel straight from
/// the layer tiles: no full-page color image is ever allocated. Empty areas cost one compare.
fn barrier_mask(doc: &Document, layers: &[&ink_core::Layer], seed: Rgba, tol: u8) -> Vec<u8> {
    let (w, h) = (doc.width as usize, doc.height as usize);
    let tw = w.div_ceil(TILE);
    let empty_blocked = (Rgba::TRANSPARENT.distance(seed) > tol) as u8;
    let mut mask = vec![0u8; w * h];
    mask.par_chunks_mut(w * TILE)
        .enumerate()
        .for_each(|(ty, band)| {
            let rows = band.len() / w;
            for tx in 0..tw {
                let key = (tx as i32, ty as i32);
                let tiles: Vec<Option<&Arc<ink_core::Tile>>> =
                    layers.iter().map(|l| l.raster.tile(key)).collect();
                let x0 = tx * TILE;
                let x1 = (x0 + TILE).min(w);
                if tiles.iter().all(Option::is_none) {
                    for y in 0..rows {
                        band[y * w + x0..y * w + x1].fill(empty_blocked);
                    }
                    continue;
                }
                for y in 0..rows {
                    for x in x0..x1 {
                        let c = reference_px(layers, &tiles, y * TILE + (x - x0));
                        band[y * w + x] = (c.distance(seed) > tol) as u8;
                    }
                }
            }
        });
    mask
}

/// Square (Chebyshev) dilation by `r` (capped at 254 px): a 1-D distance pass along rows
/// (parallel), then along columns as row-major sweeps so memory is read sequentially.
pub fn dilate(mask: &mut [u8], w: usize, h: usize, r: u32) {
    if r == 0 {
        return;
    }
    let r = r.min(254) as u8;
    mask.par_chunks_mut(w).for_each(|row| {
        let mut d = 255u8;
        let mut fwd = vec![255u8; row.len()];
        for (i, v) in row.iter().enumerate() {
            d = if *v != 0 { 0 } else { d.saturating_add(1) };
            fwd[i] = d;
        }
        d = 255;
        for i in (0..row.len()).rev() {
            d = if row[i] != 0 { 0 } else { d.saturating_add(1) };
            row[i] = (d.min(fwd[i]) <= r) as u8;
        }
    });
    // Columns: a sliding window count over 2r+1 rows. Each band of rows starts its own window,
    // so bands run in parallel, and the inner loop is a plain vectorizable add over a row.
    let src = mask.to_vec();
    let r = r as usize;
    let band_rows = 64;
    mask.par_chunks_mut(w * band_rows)
        .enumerate()
        .for_each(|(b, out)| {
            let y0 = b * band_rows;
            let rows = out.len() / w;
            let mut count = vec![0u16; w];
            let add = |count: &mut [u16], y: usize, sign: bool| {
                let row = &src[y * w..(y + 1) * w];
                for (c, v) in count.iter_mut().zip(row) {
                    if sign {
                        *c += *v as u16
                    } else {
                        *c -= *v as u16
                    }
                }
            };
            for y in y0.saturating_sub(r)..(y0 + r + 1).min(h) {
                add(&mut count, y, true);
            }
            for ry in 0..rows {
                let y = y0 + ry;
                for (o, c) in out[ry * w..(ry + 1) * w].iter_mut().zip(&count) {
                    *o = (*c > 0) as u8;
                }
                // Slide: drop row y - r, add row y + r + 1.
                if y >= r {
                    add(&mut count, y - r, false);
                }
                if y + r + 1 < h {
                    add(&mut count, y + r + 1, true);
                }
            }
        });
}

/// 4-connected scanline flood fill over pixels where `blocked` is 0. Returns the region mask.
pub fn flood(blocked: &[u8], w: usize, h: usize, sx: usize, sy: usize) -> Vec<u8> {
    let mut out = vec![0u8; w * h];
    if blocked[sy * w + sx] != 0 {
        return out;
    }
    let free = |i: usize, out: &[u8]| blocked[i] == 0 && out[i] == 0;
    let mut stack = vec![(sx, sy)];
    while let Some((x, y)) = stack.pop() {
        if !free(y * w + x, &out) {
            continue;
        }
        let mut l = x;
        while l > 0 && free(y * w + l - 1, &out) {
            l -= 1;
        }
        let mut r = x;
        while r + 1 < w && free(y * w + r + 1, &out) {
            r += 1;
        }
        out[y * w + l..=y * w + r].fill(1);
        for ny in [y.wrapping_sub(1), y + 1] {
            if ny >= h {
                continue;
            }
            let mut i = l;
            while i <= r {
                if free(ny * w + i, &out) {
                    stack.push((i, ny));
                    while i <= r && blocked[ny * w + i] == 0 {
                        i += 1;
                    }
                }
                i += 1;
            }
        }
    }
    out
}

/// Grow `region` by up to `r` px into pixels where `blocked` is 0 (breadth-first, 4-connected).
/// Only boundary pixels seed the search, so the cost follows the region's outline, not its area.
fn grow_into(region: &mut [u8], blocked: &[u8], w: usize, h: usize, r: u32) {
    let open = |j: usize, region: &[u8]| region[j] == 0 && blocked[j] == 0;
    let mut frontier: Vec<usize> = region
        .par_chunks(w)
        .enumerate()
        .flat_map_iter(|(y, row)| {
            let reg = &*region;
            row.iter().enumerate().filter_map(move |(x, v)| {
                if *v == 0 {
                    return None;
                }
                let i = y * w + x;
                let edge = (x > 0 && open(i - 1, reg))
                    || (x + 1 < w && open(i + 1, reg))
                    || (y > 0 && open(i - w, reg))
                    || (y + 1 < h && open(i + w, reg));
                edge.then_some(i)
            })
        })
        .collect();
    for _ in 0..r {
        let mut next = Vec::new();
        for &i in &frontier {
            let (x, y) = (i % w, i / w);
            for (ok, j) in [
                (x > 0, i.wrapping_sub(1)),
                (x + 1 < w, i + 1),
                (y > 0, i.wrapping_sub(w)),
                (y + 1 < h, i + w),
            ] {
                if ok && open(j, region) {
                    region[j] = 1;
                    next.push(j);
                }
            }
        }
        if next.is_empty() {
            break;
        }
        frontier = next;
    }
}

/// Compute the fill region for a click at (x, y).
pub fn fill_region(
    doc: &Document,
    target: LayerId,
    x: i32,
    y: i32,
    o: &FillOptions,
) -> Option<(usize, usize, Vec<u8>)> {
    let (w, h) = (doc.width as usize, doc.height as usize);
    if x < 0 || y < 0 || x as usize >= w || y as usize >= h {
        return None;
    }
    let layers = reference_layers(doc, target, o.refer);
    let (k, lx, ly) = ink_core::tile::split(x, y);
    let seed_tiles: Vec<Option<&Arc<ink_core::Tile>>> =
        layers.iter().map(|l| l.raster.tile(k)).collect();
    let seed = reference_px(&layers, &seed_tiles, ly * TILE + lx);
    let barrier = barrier_mask(doc, &layers, seed, o.tolerance);
    let (sx, sy) = (x as usize, y as usize);
    let mut region = if o.gap > 0 {
        let mut closed = barrier.clone();
        dilate(&mut closed, w, h, o.gap);
        if closed[sy * w + sx] == 0 {
            let mut reg = flood(&closed, w, h, sx, sy);
            drop(closed);
            grow_into(&mut reg, &barrier, w, h, o.gap + 1);
            reg
        } else if barrier[sy * w + sx] == 0 {
            // Clicked inside an area narrower than the gap size: fill it without gap closing.
            flood(&barrier, w, h, sx, sy)
        } else {
            return None;
        }
    } else if barrier[sy * w + sx] == 0 {
        flood(&barrier, w, h, sx, sy)
    } else {
        return None;
    };
    if o.expand > 0 {
        dilate(&mut region, w, h, o.expand);
    }
    Some((w, h, region))
}

/// Fill every closed area of the line art with its own color (flatting in one click).
/// Areas touching the canvas edge (the background) and specks smaller than `min_area` px are
/// skipped. Colors come from `palette(i)`. Returns the undo entry and the number of areas filled.
pub fn fill_all_regions(
    doc: &mut Document,
    target: LayerId,
    o: &FillOptions,
    min_area: usize,
    palette: impl Fn(usize) -> Rgba,
) -> Option<(UndoEntry, usize)> {
    let (w, h) = (doc.width as usize, doc.height as usize);
    let layers = reference_layers(doc, target, o.refer);
    // Barrier = anything not (nearly) transparent on the reference layers.
    let barrier_orig = barrier_mask(doc, &layers, Rgba::TRANSPARENT, o.tolerance);
    let mut closed = barrier_orig.clone();
    dilate(&mut closed, w, h, o.gap);
    // Connected components of the free pixels (scanline flood, labels written in place).
    let mut label = vec![0u32; w * h];
    let mut sizes: Vec<usize> = vec![0];
    let mut touches_edge: Vec<bool> = vec![false];
    let mut stack = Vec::new();
    for start in 0..w * h {
        if closed[start] != 0 || label[start] != 0 {
            continue;
        }
        let id = sizes.len() as u32;
        sizes.push(0);
        touches_edge.push(false);
        stack.push(start);
        while let Some(i) = stack.pop() {
            if label[i] != 0 || closed[i] != 0 {
                continue;
            }
            let (x, y) = (i % w, i / w);
            let mut l = x;
            while l > 0 && closed[y * w + l - 1] == 0 && label[y * w + l - 1] == 0 {
                l -= 1;
            }
            let mut r = x;
            while r + 1 < w && closed[y * w + r + 1] == 0 && label[y * w + r + 1] == 0 {
                r += 1;
            }
            for xx in l..=r {
                label[y * w + xx] = id;
            }
            sizes[id as usize] += r - l + 1;
            if l == 0 || r == w - 1 || y == 0 || y == h - 1 {
                touches_edge[id as usize] = true;
            }
            for ny in [y.wrapping_sub(1), y + 1] {
                if ny >= h {
                    continue;
                }
                let mut xx = l;
                while xx <= r {
                    let j = ny * w + xx;
                    if closed[j] == 0 && label[j] == 0 {
                        stack.push(j);
                        while xx <= r && closed[ny * w + xx] == 0 {
                            xx += 1;
                        }
                    }
                    xx += 1;
                }
            }
        }
    }
    drop(closed);
    // Grow every region back by the gap (and the expand margin) into unlabeled free pixels,
    // so colors reach the line art and slip under it.
    let grow = o.gap + 1 + o.expand;
    for _ in 0..grow {
        let prev = label.clone();
        label.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
            for x in 0..w {
                if row[x] != 0 {
                    continue;
                }
                let i = y * w + x;
                let n = [
                    if x > 0 { prev[i - 1] } else { 0 },
                    if x + 1 < w { prev[i + 1] } else { 0 },
                    if y > 0 { prev[i - w] } else { 0 },
                    if y + 1 < h { prev[i + w] } else { 0 },
                ];
                if let Some(&v) = n.iter().find(|v| **v != 0) {
                    row[x] = v;
                }
            }
        });
    }
    let keep: Vec<bool> = (0..sizes.len())
        .map(|i| i != 0 && !touches_edge[i] && sizes[i] >= min_area)
        .collect();
    let mut colors = vec![Rgba::TRANSPARENT; sizes.len()];
    let mut n = 0;
    for (i, k) in keep.iter().enumerate() {
        if *k {
            colors[i] = palette(n);
            n += 1;
        }
    }
    if n == 0 {
        return None;
    }
    // Paint all regions at once: one region mask per color would be wasteful, so write directly.
    let layer = doc.layer_mut(target)?;
    if layer.props.locked {
        return None;
    }
    let mut base: HashMap<TileKey, Option<Arc<ink_core::Tile>>> = HashMap::new();
    for y in 0..h {
        for x in 0..w {
            let l = label[y * w + x] as usize;
            if l == 0 || !keep[l] {
                continue;
            }
            let (k, lx, ly) = ink_core::tile::split(x as i32, y as i32);
            base.entry(k)
                .or_insert_with(|| layer.raster.tile(k).cloned());
            let t = layer.raster.tile_mut(k);
            let b = t.get(lx, ly);
            t.set(lx, ly, colors[l].over(b));
        }
    }
    let keys: Vec<TileKey> = base.keys().copied().collect();
    doc.mark_dirty(keys);
    Some((
        UndoEntry::Tiles {
            label: "Fill all areas".into(),
            layer: target,
            tiles: base.into_iter().collect(),
        },
        n,
    ))
}

/// Paint `region` onto `target` with `color` (Normal). Returns the undo entry.
pub fn apply_region(
    doc: &mut Document,
    target: LayerId,
    w: usize,
    h: usize,
    region: &[u8],
    color: Rgba,
    label: &str,
) -> Option<UndoEntry> {
    let layer = doc.layer_mut(target)?;
    if layer.props.locked {
        return None;
    }
    let lock_alpha = layer.props.lock_alpha;
    let mut base: HashMap<TileKey, Option<Arc<ink_core::Tile>>> = HashMap::new();
    let (tw, th) = (w.div_ceil(TILE), h.div_ceil(TILE));
    for ty in 0..th {
        for tx in 0..tw {
            let any = (ty * TILE..((ty + 1) * TILE).min(h)).any(|y| {
                region[y * w + tx * TILE..y * w + ((tx + 1) * TILE).min(w)]
                    .iter()
                    .any(|v| *v != 0)
            });
            if !any {
                continue;
            }
            let k = (tx as i32, ty as i32);
            if lock_alpha && layer.raster.tile(k).is_none() {
                continue;
            }
            base.insert(k, layer.raster.tile(k).cloned());
            let tile = layer.raster.tile_mut(k);
            for ly in 0..TILE {
                let y = ty * TILE + ly;
                if y >= h {
                    break;
                }
                for lx in 0..TILE {
                    let x = tx * TILE + lx;
                    if x >= w || region[y * w + x] == 0 {
                        continue;
                    }
                    let b = tile.px[ly * TILE + lx];
                    tile.px[ly * TILE + lx] = if lock_alpha {
                        color.scale(b.a())
                    } else {
                        color.over(b)
                    };
                }
            }
        }
    }
    let keys: Vec<TileKey> = base.keys().copied().collect();
    doc.mark_dirty(keys);
    Some(UndoEntry::Tiles {
        label: label.into(),
        layer: target,
        tiles: base.into_iter().collect(),
    })
}

/// Region + paint in one call.
pub fn fill(
    doc: &mut Document,
    target: LayerId,
    x: i32,
    y: i32,
    color: Rgba,
    o: &FillOptions,
) -> Option<UndoEntry> {
    let (w, h, region) = fill_region(doc, target, x, y, o)?;
    apply_region(doc, target, w, h, &region, color, "Fill")
}

#[cfg(test)]
mod tests {
    use super::*;
    use ink_core::Layer;

    /// A square outline 2px thick with a `gap`-px opening on the right side.
    fn lineart(gap: i32) -> (Document, LayerId, LayerId) {
        let mut d = Document::new(300, 300, 600);
        let mut lines = Layer::new("lines");
        lines.props.reference = true;
        for i in 50..=250 {
            for t in 0..2 {
                lines.raster.set(i, 50 + t, Rgba::BLACK);
                lines.raster.set(i, 249 + t, Rgba::BLACK);
                lines.raster.set(50 + t, i, Rgba::BLACK);
                if !(150..150 + gap).contains(&i) {
                    lines.raster.set(249 + t, i, Rgba::BLACK);
                }
            }
        }
        let flats = d.push_layer(Layer::new("flats"));
        let lines = d.push_layer(lines);
        (d, flats, lines)
    }

    #[test]
    fn fills_inside_closed_shape_on_another_layer() {
        let (mut d, flats, lines) = lineart(0);
        let red = Rgba::from_straight(220, 40, 40, 255);
        fill(
            &mut d,
            flats,
            150,
            150,
            red,
            &FillOptions {
                expand: 0,
                ..Default::default()
            },
        )
        .unwrap();
        let f = &d.layer(flats).unwrap().raster;
        assert_eq!(f.get(150, 150), red);
        assert_eq!(f.get(10, 10), Rgba::TRANSPARENT);
        assert_eq!(
            d.layer(lines).unwrap().raster.get(150, 150),
            Rgba::TRANSPARENT
        );
    }

    #[test]
    fn gap_closing_stops_leaks() {
        let red = Rgba::from_straight(220, 40, 40, 255);
        let (mut d, flats, _) = lineart(4);
        fill(
            &mut d,
            flats,
            150,
            150,
            red,
            &FillOptions {
                gap: 0,
                expand: 0,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            d.layer(flats).unwrap().raster.get(10, 10),
            red,
            "without gap closing it leaks"
        );

        let (mut d, flats, _) = lineart(4);
        fill(
            &mut d,
            flats,
            150,
            150,
            red,
            &FillOptions {
                gap: 3,
                expand: 0,
                ..Default::default()
            },
        )
        .unwrap();
        let f = &d.layer(flats).unwrap().raster;
        assert_eq!(
            f.get(10, 10),
            Rgba::TRANSPARENT,
            "gap closing keeps it inside"
        );
        assert_eq!(f.get(150, 150), red);
        assert_eq!(f.get(53, 53), red, "region grows back into corners");
        assert_eq!(f.get(246, 200), red, "region grows back to the line");
    }

    #[test]
    fn dilate_matches_brute_force() {
        let (w, h) = (97usize, 150usize);
        let mut seed = 12345u32;
        for r in [1u32, 3, 7] {
            let mask: Vec<u8> = (0..w * h)
                .map(|_| {
                    seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                    ((seed >> 16) % 97 == 0) as u8
                })
                .collect();
            let mut fast = mask.clone();
            dilate(&mut fast, w, h, r);
            let r = r as i64;
            for y in 0..h as i64 {
                for x in 0..w as i64 {
                    let mut want = 0u8;
                    for dy in -r..=r {
                        for dx in -r..=r {
                            let (xx, yy) = (x + dx, y + dy);
                            if xx >= 0
                                && yy >= 0
                                && xx < w as i64
                                && yy < h as i64
                                && mask[yy as usize * w + xx as usize] != 0
                            {
                                want = 1;
                            }
                        }
                    }
                    assert_eq!(fast[y as usize * w + x as usize], want, "r={r} at {x},{y}");
                }
            }
        }
    }

    #[test]
    fn fill_all_regions_colors_each_closed_area() {
        // Two closed squares side by side on the line layer.
        let mut d = Document::new(300, 200, 600);
        let mut lines = Layer::new("lines");
        lines.props.reference = true;
        for (x0, x1) in [(20, 130), (160, 280)] {
            for i in x0..=x1 {
                lines.raster.set(i, 30, Rgba::BLACK);
                lines.raster.set(i, 170, Rgba::BLACK);
            }
            for j in 30..=170 {
                lines.raster.set(x0, j, Rgba::BLACK);
                lines.raster.set(x1, j, Rgba::BLACK);
            }
        }
        let flats = d.push_layer(Layer::new("flats"));
        d.push_layer(lines);
        let cols = [
            Rgba::from_straight(255, 0, 0, 255),
            Rgba::from_straight(0, 0, 255, 255),
        ];
        let (_, n) = fill_all_regions(
            &mut d,
            flats,
            &FillOptions {
                gap: 0,
                expand: 0,
                ..Default::default()
            },
            20,
            |i| cols[i % 2],
        )
        .unwrap();
        assert_eq!(n, 2, "background excluded");
        let f = &d.layer(flats).unwrap().raster;
        assert_ne!(f.get(70, 100), f.get(220, 100), "different colors");
        assert!(f.get(70, 100).a() == 255 && f.get(220, 100).a() == 255);
        assert_eq!(f.get(5, 5), Rgba::TRANSPARENT, "background untouched");
    }

    #[test]
    fn expand_goes_under_lines() {
        let red = Rgba::from_straight(220, 40, 40, 255);
        let (mut d, flats, _) = lineart(0);
        fill(
            &mut d,
            flats,
            150,
            150,
            red,
            &FillOptions {
                expand: 2,
                ..Default::default()
            },
        )
        .unwrap();
        let f = &d.layer(flats).unwrap().raster;
        assert_eq!(f.get(51, 150), red);
        assert_eq!(f.get(47, 150), Rgba::TRANSPARENT);
    }
}
