//! Brush tip shapes, paper grain and scatter.
//!
//! Tips are signed distance functions in *unit space* (radius 1), so every shape gets exact
//! anti-aliasing and soft edges from the same code, at any size, angle and roundness.

/// Shape of one dab.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TipShape {
    Round,
    Square,
    /// `points`-pointed star; `inner` = inner radius as a fraction of the outer one.
    Star {
        points: u8,
        inner: f32,
    },
    Heart,
    /// Rounded petals.
    Flower {
        petals: u8,
    },
    /// Circle outline; `thickness` as a fraction of the radius.
    Ring {
        thickness: f32,
    },
    /// `lines` parallel lines inside the circle; `width` = line width as a fraction of the pitch.
    Hatch {
        lines: u8,
        width: f32,
    },
}

/// Paper texture: value noise in canvas space that eats into the coverage (pencil, charcoal).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Grain {
    /// Size of a grain cell in pixels.
    pub scale: f32,
    /// 0 = no texture, 1 = texture fully controls coverage.
    pub strength: f32,
    /// > 1 makes the texture harsher (more speckle).
    pub contrast: f32,
}

/// Many small dabs around each dab position (sparkles, flowers, spray, leaves).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scatter {
    pub count: u8,
    /// Spread radius as a multiple of the brush radius.
    pub spread: f32,
    /// Particle size as a fraction of the brush radius.
    pub size: f32,
    /// 0..=1 random shrink.
    pub size_jitter: f32,
    /// 0..=1 random rotation (1 = full turn).
    pub angle_jitter: f32,
    /// 0..=1 random fade.
    pub opacity_jitter: f32,
}

/// Signed distance in unit space: negative inside, 0 on the edge (radius 1), positive outside.
#[inline]
pub fn sdf(shape: TipShape, x: f32, y: f32) -> f32 {
    let len = (x * x + y * y).sqrt();
    match shape {
        TipShape::Round => len - 1.0,
        TipShape::Square => x.abs().max(y.abs()) - 1.0,
        TipShape::Star { points, inner } => {
            let n = points.max(3) as f32;
            let sector = std::f32::consts::TAU / n;
            let theta = y.atan2(x) + std::f32::consts::FRAC_PI_2; // first point straight up
            let phi = theta - (theta / sector).round() * sector;
            let (px, py) = (len * phi.cos(), len * phi.sin().abs());
            let (ax, ay) = (1.0, 0.0);
            let half = sector / 2.0;
            let (bx, by) = (inner * half.cos(), inner * half.sin());
            let (ex, ey) = (bx - ax, by - ay);
            let t = (((px - ax) * ex + (py - ay) * ey) / (ex * ex + ey * ey)).clamp(0.0, 1.0);
            let (dx, dy) = (px - ax - ex * t, py - ay - ey * t);
            let d = (dx * dx + dy * dy).sqrt();
            let inside = ex * (py - ay) - ey * (px - ax) > 0.0;
            if inside { -d } else { d }
        }
        TipShape::Heart => {
            // Inigo Quilez's heart, tip pointing down on the canvas (y grows downward).
            let k = 0.62;
            let hx = (x * k).abs();
            let hy = -y * k + 0.48;
            let d = if hy + hx > 1.0 {
                let (ux, uy) = (hx - 0.25, hy - 0.75);
                (ux * ux + uy * uy).sqrt() - std::f32::consts::SQRT_2 / 4.0
            } else {
                let (ax, ay) = (hx, hy - 1.0);
                let m = 0.5 * (hx + hy).max(0.0);
                let (bx, by) = (hx - m, hy - m);
                (ax * ax + ay * ay).min(bx * bx + by * by).sqrt() * (hx - hy).signum()
            };
            d / k
        }
        TipShape::Flower { petals } => {
            // One rounded lobe per petal: r(θ) peaks once per petal, plus a solid center.
            let theta = y.atan2(x) + std::f32::consts::FRAC_PI_2;
            let lobe = (0.5 + 0.5 * (petals.max(3) as f32 * theta).cos()).powf(0.7);
            let r = 0.42 + 0.58 * lobe;
            ((len - r) * 0.7).min(len - 0.38)
        }
        TipShape::Ring { thickness } => {
            let t = thickness.clamp(0.05, 1.0);
            (len - (1.0 - t / 2.0)).abs() - t / 2.0
        }
        TipShape::Hatch { lines, width } => {
            let n = lines.max(1) as f32;
            let pitch = 2.0 / n;
            let f = ((y + 1.0) / pitch).fract();
            let d_line = (f - 0.5).abs() * pitch - width.clamp(0.05, 1.0) * pitch * 0.5;
            d_line.max(len - 1.0)
        }
    }
}

/// Stable hash → 0..1.
#[inline]
pub fn hash01(a: u32, b: u32) -> f32 {
    let mut h = a.wrapping_mul(0x9E37_79B1) ^ b.wrapping_mul(0x85EB_CA77).rotate_left(13);
    h ^= h >> 16;
    h = h.wrapping_mul(0x7FEB_352D);
    h ^= h >> 15;
    h = h.wrapping_mul(0x846C_A68B);
    h ^= h >> 16;
    (h >> 8) as f32 / (1u32 << 24) as f32
}

#[inline]
fn value_noise(x: f32, y: f32) -> f32 {
    let (xi, yi) = (x.floor(), y.floor());
    let (fx, fy) = (x - xi, y - yi);
    let (sx, sy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let (ix, iy) = (xi as i32 as u32, yi as i32 as u32);
    let h = |dx: u32, dy: u32| hash01(ix.wrapping_add(dx), iy.wrapping_add(dy));
    let top = h(0, 0) + (h(1, 0) - h(0, 0)) * sx;
    let bot = h(0, 1) + (h(1, 1) - h(0, 1)) * sx;
    top + (bot - top) * sy
}

impl Grain {
    /// Coverage multiplier at canvas pixel (x, y). Fixed to the canvas, like real paper.
    #[inline]
    pub fn factor(&self, x: f32, y: f32) -> f32 {
        let s = self.scale.max(0.5);
        let n = 0.65 * value_noise(x / s, y / s)
            + 0.35 * value_noise(x / (s * 0.43) + 17.0, y / (s * 0.43) + 5.0);
        let n = ((n - 0.5) * self.contrast + 0.5).clamp(0.0, 1.0);
        1.0 - self.strength.clamp(0.0, 1.0) * (1.0 - n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shapes_are_inside_at_center_and_outside_far_away() {
        let shapes = [
            TipShape::Round,
            TipShape::Square,
            TipShape::Star {
                points: 5,
                inner: 0.45,
            },
            TipShape::Heart,
            TipShape::Flower { petals: 5 },
            TipShape::Hatch {
                lines: 3,
                width: 0.5,
            },
        ];
        for s in shapes {
            assert!(sdf(s, 0.0, 0.05) < 0.0, "{s:?} center");
            assert!(sdf(s, 2.0, 2.0) > 0.0, "{s:?} far");
        }
        let ring = TipShape::Ring { thickness: 0.2 };
        assert!(sdf(ring, 0.0, 0.0) > 0.0, "ring is hollow");
        assert!(sdf(ring, 0.9, 0.0) < 0.0, "ring edge is solid");
    }

    #[test]
    fn star_points_and_valleys() {
        let s = TipShape::Star {
            points: 5,
            inner: 0.4,
        };
        assert!(sdf(s, 0.0, -0.95) < 0.0, "top point is inside");
        // Halfway between two points at 0.8 radius is outside (a valley).
        let a = -std::f32::consts::FRAC_PI_2 + std::f32::consts::TAU / 10.0;
        assert!(sdf(s, 0.8 * a.cos(), 0.8 * a.sin()) > 0.0);
    }

    #[test]
    fn grain_is_deterministic_and_bounded() {
        let g = Grain {
            scale: 3.0,
            strength: 0.8,
            contrast: 2.0,
        };
        for i in 0..1000 {
            let (x, y) = (i as f32 * 1.7, i as f32 * 0.3);
            let f = g.factor(x, y);
            assert!((0.2 - 1e-6..=1.0).contains(&f));
            assert_eq!(f, g.factor(x, y));
        }
    }
}
