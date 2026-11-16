use crate::blend::BlendMode;
use crate::tile::{split, Tile, TileKey, TILE};
use crate::Rgba;
use std::collections::HashMap;
use std::sync::Arc;

/// Sparse raster: only non-empty tiles are stored. Tiles are shared (`Arc`) and copied on write.
#[derive(Clone, Default)]
pub struct Raster {
    tiles: HashMap<TileKey, Arc<Tile>>,
}

impl Raster {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn tile(&self, key: TileKey) -> Option<&Arc<Tile>> {
        self.tiles.get(&key)
    }

    /// Mutable access, creating an empty tile if needed and un-sharing it if it is shared.
    pub fn tile_mut(&mut self, key: TileKey) -> &mut Tile {
        Arc::make_mut(self.tiles.entry(key).or_insert_with(|| Arc::new(Tile::empty())))
    }

    /// Replace (or remove, with `None`) a tile. Used by undo and by stroke compositing.
    pub fn put_tile(&mut self, key: TileKey, tile: Option<Arc<Tile>>) {
        match tile {
            Some(t) => {
                self.tiles.insert(key, t);
            }
            None => {
                self.tiles.remove(&key);
            }
        }
    }

    pub fn keys(&self) -> impl Iterator<Item = &TileKey> {
        self.tiles.keys()
    }

    pub fn tile_count(&self) -> usize {
        self.tiles.len()
    }

    pub fn get(&self, x: i32, y: i32) -> Rgba {
        let (k, tx, ty) = split(x, y);
        self.tiles.get(&k).map_or(Rgba::TRANSPARENT, |t| t.get(tx, ty))
    }

    pub fn set(&mut self, x: i32, y: i32, c: Rgba) {
        let (k, tx, ty) = split(x, y);
        if c.a() == 0 && !self.tiles.contains_key(&k) {
            return;
        }
        self.tile_mut(k).set(tx, ty, c);
    }

    /// Drop tiles that became fully transparent (after erasing), to keep files and memory small.
    pub fn prune(&mut self) {
        self.tiles.retain(|_, t| !t.is_transparent());
    }

    /// Read a rectangle as straight-alpha RGBA8 (for export / thumbnails).
    pub fn read_rect(&self, x0: i32, y0: i32, w: usize, h: usize) -> Vec<Rgba> {
        let mut out = vec![Rgba::TRANSPARENT; w * h];
        for (ky, row) in out.chunks_mut(w).enumerate() {
            let y = y0 + ky as i32;
            let mut x = x0;
            while x < x0 + w as i32 {
                let (k, tx, ty) = split(x, y);
                let run = (TILE - tx).min((x0 + w as i32 - x) as usize);
                if let Some(t) = self.tiles.get(&k) {
                    let src = &t.px[ty * TILE + tx..ty * TILE + tx + run];
                    row[(x - x0) as usize..(x - x0) as usize + run].copy_from_slice(src);
                }
                x += run as i32;
            }
        }
        out
    }
}

/// User-visible layer properties (layer palette, S07/S08).
#[derive(Clone, Debug)]
pub struct LayerProps {
    pub name: String,
    pub visible: bool,
    pub opacity: u8,
    pub blend: BlendMode,
    pub clip_below: bool,
    pub reference: bool,
    pub draft: bool,
    pub lock_alpha: bool,
    pub locked: bool,
}

impl LayerProps {
    pub fn named(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            visible: true,
            opacity: 255,
            blend: BlendMode::Normal,
            clip_below: false,
            reference: false,
            draft: false,
            lock_alpha: false,
            locked: false,
        }
    }
}

#[derive(Clone)]
pub struct Layer {
    pub props: LayerProps,
    pub raster: Raster,
}

impl Layer {
    pub fn new(name: impl Into<String>) -> Self {
        Self { props: LayerProps::named(name), raster: Raster::new() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_on_write_shares_until_written() {
        let mut a = Raster::new();
        a.set(10, 10, Rgba::BLACK);
        let b = a.clone();
        assert!(Arc::ptr_eq(a.tile((0, 0)).unwrap(), b.tile((0, 0)).unwrap()));
        a.set(11, 10, Rgba::BLACK);
        assert!(!Arc::ptr_eq(a.tile((0, 0)).unwrap(), b.tile((0, 0)).unwrap()));
        assert_eq!(b.get(11, 10), Rgba::TRANSPARENT);
        assert_eq!(a.get(11, 10), Rgba::BLACK);
    }

    #[test]
    fn negative_coordinates_and_read_rect() {
        let mut r = Raster::new();
        r.set(-1, -1, Rgba::WHITE);
        r.set(300, 2, Rgba::BLACK);
        assert_eq!(r.get(-1, -1), Rgba::WHITE);
        let px = r.read_rect(-1, -1, 302, 4);
        assert_eq!(px[0], Rgba::WHITE);
        assert_eq!(px[3 * 302 + 301], Rgba::BLACK);
    }
}
