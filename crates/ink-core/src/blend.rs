//! Layer blend modes on premultiplied 8-bit colors (separable modes, W3C compositing formulas).

use crate::color::{mul8, Rgba};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum BlendMode {
    #[default]
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    Add,
    Subtract,
    Difference,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
}

impl BlendMode {
    pub const ALL: [BlendMode; 13] = [
        BlendMode::Normal,
        BlendMode::Multiply,
        BlendMode::Screen,
        BlendMode::Overlay,
        BlendMode::Darken,
        BlendMode::Lighten,
        BlendMode::Add,
        BlendMode::Subtract,
        BlendMode::Difference,
        BlendMode::ColorDodge,
        BlendMode::ColorBurn,
        BlendMode::HardLight,
        BlendMode::SoftLight,
    ];

    pub fn name(self) -> &'static str {
        match self {
            BlendMode::Normal => "Normal",
            BlendMode::Multiply => "Multiply",
            BlendMode::Screen => "Screen",
            BlendMode::Overlay => "Overlay",
            BlendMode::Darken => "Darken",
            BlendMode::Lighten => "Lighten",
            BlendMode::Add => "Add",
            BlendMode::Subtract => "Subtract",
            BlendMode::Difference => "Difference",
            BlendMode::ColorDodge => "Color dodge",
            BlendMode::ColorBurn => "Color burn",
            BlendMode::HardLight => "Hard light",
            BlendMode::SoftLight => "Soft light",
        }
    }

    /// The blend function B(cb, cs) on straight 0..1 values.
    #[inline]
    fn f(self, cb: f32, cs: f32) -> f32 {
        match self {
            BlendMode::Normal => cs,
            BlendMode::Multiply => cb * cs,
            BlendMode::Screen => cb + cs - cb * cs,
            BlendMode::Overlay => BlendMode::HardLight.f(cs, cb),
            BlendMode::Darken => cb.min(cs),
            BlendMode::Lighten => cb.max(cs),
            BlendMode::Add => (cb + cs).min(1.0),
            BlendMode::Subtract => (cb - cs).max(0.0),
            BlendMode::Difference => (cb - cs).abs(),
            BlendMode::ColorDodge => {
                if cb == 0.0 {
                    0.0
                } else if cs >= 1.0 {
                    1.0
                } else {
                    (cb / (1.0 - cs)).min(1.0)
                }
            }
            BlendMode::ColorBurn => {
                if cb >= 1.0 {
                    1.0
                } else if cs <= 0.0 {
                    0.0
                } else {
                    1.0 - ((1.0 - cb) / cs).min(1.0)
                }
            }
            BlendMode::HardLight => {
                if cs <= 0.5 {
                    cb * 2.0 * cs
                } else {
                    let s = 2.0 * cs - 1.0;
                    cb + s - cb * s
                }
            }
            BlendMode::SoftLight => {
                if cs <= 0.5 {
                    cb - (1.0 - 2.0 * cs) * cb * (1.0 - cb)
                } else {
                    let d = if cb <= 0.25 { ((16.0 * cb - 12.0) * cb + 4.0) * cb } else { cb.sqrt() };
                    cb + (2.0 * cs - 1.0) * (d - cb)
                }
            }
        }
    }

    /// Composite `src` (already scaled by layer opacity) onto `dst`. Both premultiplied.
    #[inline]
    pub fn composite(self, src: Rgba, dst: Rgba) -> Rgba {
        let sa = src.0[3];
        if sa == 0 {
            return dst;
        }
        if self == BlendMode::Normal || dst.0[3] == 0 {
            return src.over(dst);
        }
        // General formula: co = cs*(1-ab) + cb*(1-as) + as*ab*B(cb, cs)   (premultiplied cs, cb)
        let s = src.to_straight();
        let d = dst.to_straight();
        let (asf, abf) = (sa as f32 / 255.0, dst.0[3] as f32 / 255.0);
        let mut out = [0u8; 4];
        for i in 0..3 {
            let cs = s[i] as f32 / 255.0;
            let cb = d[i] as f32 / 255.0;
            let b = self.f(cb, cs);
            let co = cs * asf * (1.0 - abf) + cb * abf * (1.0 - asf) + asf * abf * b;
            out[i] = (co * 255.0 + 0.5).clamp(0.0, 255.0) as u8;
        }
        out[3] = sa.saturating_add(mul8(dst.0[3], 255 - sa));
        Rgba(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multiply_white_is_identity_and_black_is_black() {
        let red = Rgba::from_straight(200, 30, 30, 255);
        assert_eq!(BlendMode::Multiply.composite(Rgba::WHITE, red), red);
        assert_eq!(BlendMode::Multiply.composite(Rgba::BLACK, red), Rgba::BLACK);
    }

    #[test]
    fn screen_black_is_identity() {
        let c = Rgba::from_straight(10, 120, 240, 255);
        assert_eq!(BlendMode::Screen.composite(Rgba::BLACK, c), c);
    }

    #[test]
    fn normal_matches_over() {
        let s = Rgba::from_straight(255, 0, 0, 128);
        let d = Rgba::from_straight(0, 0, 255, 255);
        assert_eq!(BlendMode::Normal.composite(s, d), s.over(d));
    }
}
