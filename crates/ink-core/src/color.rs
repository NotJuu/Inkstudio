/// Premultiplied RGBA, 8 bits per channel (sRGB-encoded, blended in sRGB like most paint apps).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[repr(C)]
pub struct Rgba(pub [u8; 4]);

impl Rgba {
    pub const TRANSPARENT: Rgba = Rgba([0, 0, 0, 0]);
    pub const BLACK: Rgba = Rgba([0, 0, 0, 255]);
    pub const WHITE: Rgba = Rgba([255, 255, 255, 255]);

    /// From straight (non-premultiplied) components.
    pub fn from_straight(r: u8, g: u8, b: u8, a: u8) -> Self {
        let pm = |c: u8| mul8(c, a);
        Rgba([pm(r), pm(g), pm(b), a])
    }

    pub fn a(self) -> u8 {
        self.0[3]
    }

    /// Straight components (for display, color picking and export).
    pub fn to_straight(self) -> [u8; 4] {
        let a = self.0[3];
        if a == 0 {
            return [0, 0, 0, 0];
        }
        let un = |c: u8| ((c as u32 * 255 + a as u32 / 2) / a as u32).min(255) as u8;
        [un(self.0[0]), un(self.0[1]), un(self.0[2]), a]
    }

    /// Scale all channels (premultiplied) by `k` in 0..=255.
    #[inline]
    pub fn scale(self, k: u8) -> Self {
        Rgba(self.0.map(|c| mul8(c, k)))
    }

    /// Source-over for premultiplied colors.
    #[inline]
    pub fn over(self, dst: Rgba) -> Rgba {
        let inv = 255 - self.0[3];
        Rgba([
            self.0[0].saturating_add(mul8(dst.0[0], inv)),
            self.0[1].saturating_add(mul8(dst.0[1], inv)),
            self.0[2].saturating_add(mul8(dst.0[2], inv)),
            self.0[3].saturating_add(mul8(dst.0[3], inv)),
        ])
    }

    /// Euclidean-ish distance in straight RGBA, 0..=255, used by fill / auto select tolerance.
    pub fn distance(self, other: Rgba) -> u8 {
        let a = self.to_straight();
        let b = other.to_straight();
        let mut m = 0u8;
        for i in 0..4 {
            m = m.max(a[i].abs_diff(b[i]));
        }
        m
    }
}

/// Exact 8-bit multiply with rounding: round(a*b/255).
#[inline(always)]
pub fn mul8(a: u8, b: u8) -> u8 {
    let t = a as u32 * b as u32 + 128;
    ((t + (t >> 8)) >> 8) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mul8_is_exact() {
        for a in 0..=255u32 {
            for b in 0..=255u32 {
                let want = ((a * b) as f32 / 255.0).round() as u8;
                assert_eq!(mul8(a as u8, b as u8), want, "{a}*{b}");
            }
        }
    }

    #[test]
    fn premultiply_roundtrip() {
        let c = Rgba::from_straight(200, 100, 50, 128);
        let s = c.to_straight();
        for (x, y) in s.iter().zip([200u8, 100, 50, 128]) {
            assert!(x.abs_diff(y) <= 1);
        }
    }
}
