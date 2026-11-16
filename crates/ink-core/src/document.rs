use crate::tile::{Tile, TileKey, TILE, TILE_PX};
use crate::{Layer, Rgba};
use rayon::prelude::*;
use std::collections::HashSet;
use std::sync::Arc;

/// Stable layer identity (survives reordering; used by undo and by the file format).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LayerId(pub u64);

/// One page: a canvas size and a bottom-to-top layer stack.
pub struct Document {
    pub width: u32,
    pub height: u32,
    pub dpi: u32,
    pub paper: Rgba,
    layers: Vec<(LayerId, Layer)>,
    next_id: u64,
    /// Tiles whose composite must be recomputed. `None` = everything.
    dirty: Option<HashSet<TileKey>>,
}

impl Document {
    pub fn new(width: u32, height: u32, dpi: u32) -> Self {
        Self { width, height, dpi, paper: Rgba::WHITE, layers: Vec::new(), next_id: 1, dirty: None }
    }

    // ---- layers ---------------------------------------------------------------------------

    pub fn add_layer(&mut self, layer: Layer, index: usize) -> LayerId {
        let id = LayerId(self.next_id);
        self.next_id += 1;
        let index = index.min(self.layers.len());
        self.layers.insert(index, (id, layer));
        self.dirty = None;
        id
    }

    pub fn push_layer(&mut self, layer: Layer) -> LayerId {
        let n = self.layers.len();
        self.add_layer(layer, n)
    }

    pub fn remove_layer(&mut self, id: LayerId) -> Option<Layer> {
        let i = self.index_of(id)?;
        self.dirty = None;
        Some(self.layers.remove(i).1)
    }

    pub fn move_layer(&mut self, id: LayerId, to: usize) {
        if let Some(i) = self.index_of(id) {
            let l = self.layers.remove(i);
            self.layers.insert(to.min(self.layers.len()), l);
            self.dirty = None;
        }
    }

    pub fn layers(&self) -> &[(LayerId, Layer)] {
        &self.layers
    }

    pub fn index_of(&self, id: LayerId) -> Option<usize> {
        self.layers.iter().position(|(i, _)| *i == id)
    }

    pub fn layer(&self, id: LayerId) -> Option<&Layer> {
        self.layers.iter().find(|(i, _)| *i == id).map(|(_, l)| l)
    }

    /// Mutable layer access. Callers changing pixels must report the tiles with
    /// [`Document::mark_dirty`]; property changes should call [`Document::mark_dirty_layer`].
    pub fn layer_mut(&mut self, id: LayerId) -> Option<&mut Layer> {
        self.layers.iter_mut().find(|(i, _)| *i == id).map(|(_, l)| l)
    }

    /// Snapshot of the whole stack (cheap: tiles are shared). Used for structural undo.
    pub fn snapshot_layers(&self) -> Vec<(LayerId, Layer)> {
        self.layers.clone()
    }

    pub(crate) fn replace_layers(&mut self, layers: Vec<(LayerId, Layer)>) -> Vec<(LayerId, Layer)> {
        self.dirty = None;
        std::mem::replace(&mut self.layers, layers)
    }

    // ---- dirty tracking -------------------------------------------------------------------

    pub fn mark_dirty(&mut self, keys: impl IntoIterator<Item = TileKey>) {
        if let Some(d) = &mut self.dirty {
            d.extend(keys);
        }
    }

    pub fn mark_dirty_layer(&mut self, id: LayerId) {
        let keys: Vec<TileKey> = self.layer(id).map(|l| l.raster.keys().copied().collect()).unwrap_or_default();
        self.mark_dirty(keys);
    }

    pub fn mark_all_dirty(&mut self) {
        self.dirty = None;
    }

    /// Take the set of tiles to recomposite (all canvas tiles if everything is dirty).
    pub fn take_dirty(&mut self) -> Vec<TileKey> {
        let (tw, th) = self.tiles_wh();
        match self.dirty.replace(HashSet::new()) {
            None => (0..th as i32).flat_map(move |y| (0..tw as i32).map(move |x| (x, y))).collect(),
            Some(set) => set.into_iter().filter(|(x, y)| *x >= 0 && *y >= 0 && (*x as u32) < tw && (*y as u32) < th).collect(),
        }
    }

    /// Canvas size in tiles.
    pub fn tiles_wh(&self) -> (u32, u32) {
        (self.width.div_ceil(TILE as u32), self.height.div_ceil(TILE as u32))
    }

    // ---- compositing ----------------------------------------------------------------------

    /// Composite the visible stack for one tile, on paper. Handles opacity, blend modes and
    /// clipping groups (a clipped layer is masked by the alpha of the nearest non-clipped layer below).
    pub fn composite_tile(&self, key: TileKey, include_draft: bool) -> Tile {
        let mut out = Tile::filled(self.paper);
        // Alpha of the current clipping base, needed to mask clipped layers above it.
        let mut base_alpha: Option<Arc<Tile>> = None;
        let mut base_visible = false;
        for (_, l) in &self.layers {
            let p = &l.props;
            if !p.clip_below {
                base_alpha = l.raster.tile(key).cloned();
                base_visible = p.visible && (include_draft || !p.draft);
            }
            if !p.visible || (!include_draft && p.draft) || (p.clip_below && !base_visible) {
                continue;
            }
            let Some(src) = l.raster.tile(key) else { continue };
            let clip_mask = if p.clip_below { Some(base_alpha.as_deref()) } else { None };
            if let Some(None) = clip_mask {
                continue; // clipping base is empty here: nothing shows
            }
            for i in 0..TILE_PX {
                let mut s = src.px[i];
                if s.0[3] == 0 {
                    continue;
                }
                let mut k = p.opacity;
                if let Some(Some(m)) = clip_mask {
                    k = crate::color::mul8(k, m.px[i].0[3]);
                }
                if k != 255 {
                    s = s.scale(k);
                }
                out.px[i] = p.blend.composite(s, out.px[i]);
            }
        }
        out
    }

    /// Composite many tiles in parallel.
    pub fn composite_tiles(&self, keys: &[TileKey], include_draft: bool) -> Vec<(TileKey, Tile)> {
        keys.par_iter().map(|&k| (k, self.composite_tile(k, include_draft))).collect()
    }

    /// Flatten the whole page into straight-alpha RGBA8 rows (export). Draft layers excluded.
    /// Bands of tile rows are composited and converted in parallel, straight into the output.
    pub fn flatten_rgba8(&self) -> Vec<u8> {
        let (w, h) = (self.width as usize, self.height as usize);
        let tw = w.div_ceil(TILE);
        let mut out = vec![0u8; w * h * 4];
        out.par_chunks_mut(w * TILE * 4).enumerate().for_each(|(ty, band)| {
            let rows = band.len() / (w * 4);
            for tx in 0..tw {
                let t = self.composite_tile((tx as i32, ty as i32), false);
                let x0 = tx * TILE;
                let cols = TILE.min(w - x0);
                for row in 0..rows {
                    let dst = &mut band[(row * w + x0) * 4..(row * w + x0 + cols) * 4];
                    for (col, px) in dst.chunks_exact_mut(4).enumerate() {
                        let c = t.px[row * TILE + col];
                        // Paper is opaque, so the composite is opaque: no un-premultiply needed.
                        let c = if c.0[3] == 255 { c.0 } else { c.to_straight() };
                        px.copy_from_slice(&c);
                    }
                }
            }
        });
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blend::BlendMode;
    use crate::undo::{History, UndoEntry};

    #[test]
    fn clipping_masks_to_layer_below() {
        let mut d = Document::new(256, 256, 72);
        let mut base = Layer::new("base");
        base.raster.set(5, 5, Rgba::from_straight(255, 0, 0, 255));
        d.push_layer(base);
        let mut clip = Layer::new("tone");
        clip.props.clip_below = true;
        clip.raster.set(5, 5, Rgba::BLACK);
        clip.raster.set(6, 5, Rgba::BLACK); // outside the base: must not show
        d.push_layer(clip);
        let t = d.composite_tile((0, 0), true);
        assert_eq!(t.get(5, 5), Rgba::BLACK);
        assert_eq!(t.get(6, 5), Rgba::WHITE);
    }

    #[test]
    fn multiply_layer_over_paper() {
        let mut d = Document::new(256, 256, 72);
        let mut l = Layer::new("m");
        l.props.blend = BlendMode::Multiply;
        l.raster.set(0, 0, Rgba::from_straight(128, 128, 128, 255));
        d.push_layer(l);
        let c = d.composite_tile((0, 0), true).get(0, 0).to_straight();
        assert!(c[0].abs_diff(128) <= 1);
    }

    #[test]
    fn tile_undo_redo() {
        let mut d = Document::new(512, 512, 72);
        let id = d.push_layer(Layer::new("ink"));
        let mut h = History::default();
        let before = d.layer(id).unwrap().raster.tile((0, 0)).cloned();
        d.layer_mut(id).unwrap().raster.set(1, 1, Rgba::BLACK);
        h.push(UndoEntry::Tiles { label: "stroke".into(), layer: id, tiles: vec![((0, 0), before)] });
        assert_eq!(d.layer(id).unwrap().raster.get(1, 1), Rgba::BLACK);
        h.undo(&mut d);
        assert_eq!(d.layer(id).unwrap().raster.get(1, 1), Rgba::TRANSPARENT);
        h.redo(&mut d);
        assert_eq!(d.layer(id).unwrap().raster.get(1, 1), Rgba::BLACK);
    }

    #[test]
    fn layer_undo_restores_stack() {
        let mut d = Document::new(64, 64, 72);
        d.push_layer(Layer::new("a"));
        let mut h = History::default();
        let snap = d.snapshot_layers();
        d.push_layer(Layer::new("b"));
        h.push(UndoEntry::Layers { label: "add".into(), layers: snap });
        assert_eq!(d.layers().len(), 2);
        h.undo(&mut d);
        assert_eq!(d.layers().len(), 1);
        h.redo(&mut d);
        assert_eq!(d.layers().len(), 2);
    }
}
