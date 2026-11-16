use crate::Rgba;

/// Tile edge in pixels.
pub const TILE: usize = 256;
pub const TILE_PX: usize = TILE * TILE;

/// Tile coordinates (tile column, tile row). Can be negative: layers may extend past the canvas.
pub type TileKey = (i32, i32);

/// One 256×256 block of premultiplied pixels.
#[derive(Clone)]
pub struct Tile {
    pub px: Box<[Rgba; TILE_PX]>,
}

impl Tile {
    pub fn empty() -> Self {
        // Allocate on the heap directly (256 KiB would overflow small stacks).
        let v = vec![Rgba::TRANSPARENT; TILE_PX].into_boxed_slice();
        Tile { px: v.try_into().unwrap_or_else(|_| unreachable!()) }
    }

    pub fn filled(c: Rgba) -> Self {
        let mut t = Self::empty();
        t.px.fill(c);
        t
    }

    #[inline]
    pub fn get(&self, x: usize, y: usize) -> Rgba {
        self.px[y * TILE + x]
    }

    #[inline]
    pub fn set(&mut self, x: usize, y: usize, c: Rgba) {
        self.px[y * TILE + x] = c;
    }

    pub fn is_transparent(&self) -> bool {
        self.px.iter().all(|p| p.0[3] == 0)
    }
}

/// Which tile a pixel coordinate falls in, and the offset inside it.
#[inline]
pub fn split(x: i32, y: i32) -> (TileKey, usize, usize) {
    let t = TILE as i32;
    ((x.div_euclid(t), y.div_euclid(t)), x.rem_euclid(t) as usize, y.rem_euclid(t) as usize)
}

/// All tile keys overlapping the pixel rectangle [x0, x1) × [y0, y1).
pub fn keys_in(x0: i32, y0: i32, x1: i32, y1: i32) -> impl Iterator<Item = TileKey> {
    let t = TILE as i32;
    let (tx0, ty0) = (x0.div_euclid(t), y0.div_euclid(t));
    let (tx1, ty1) = ((x1 - 1).div_euclid(t), (y1 - 1).div_euclid(t));
    (ty0..=ty1).flat_map(move |ty| (tx0..=tx1).map(move |tx| (tx, ty)))
}
