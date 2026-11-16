/// 2-D point / vector in canvas pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Pt {
    pub x: f32,
    pub y: f32,
}

impl Pt {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
    pub fn sub(self, o: Pt) -> Pt {
        Pt::new(self.x - o.x, self.y - o.y)
    }
    pub fn add(self, o: Pt) -> Pt {
        Pt::new(self.x + o.x, self.y + o.y)
    }
    pub fn mul(self, k: f32) -> Pt {
        Pt::new(self.x * k, self.y * k)
    }
    pub fn dot(self, o: Pt) -> f32 {
        self.x * o.x + self.y * o.y
    }
    pub fn cross(self, o: Pt) -> f32 {
        self.x * o.y - self.y * o.x
    }
    pub fn len(self) -> f32 {
        self.dot(self).sqrt()
    }
    pub fn norm(self) -> Pt {
        let l = self.len();
        if l > 0.0 { self.mul(1.0 / l) } else { self }
    }
    pub fn lerp(self, o: Pt, t: f32) -> Pt {
        self.add(o.sub(self).mul(t))
    }
}

/// Closest point on segment ab to p, as (point, t in 0..=1).
pub fn closest_on_segment(p: Pt, a: Pt, b: Pt) -> (Pt, f32) {
    let ab = b.sub(a);
    let l2 = ab.dot(ab);
    let t = if l2 > 0.0 { (p.sub(a).dot(ab) / l2).clamp(0.0, 1.0) } else { 0.0 };
    (a.lerp(b, t), t)
}

/// Proper intersection of segments ab and cd: returns (t on ab, u on cd).
pub fn segment_intersection(a: Pt, b: Pt, c: Pt, d: Pt) -> Option<(f32, f32)> {
    let r = b.sub(a);
    let s = d.sub(c);
    let den = r.cross(s);
    if den.abs() < 1e-9 {
        return None;
    }
    let qp = c.sub(a);
    let t = qp.cross(s) / den;
    let u = qp.cross(r) / den;
    ((0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u)).then_some((t, u))
}

/// Project p onto the infinite line through a with direction dir.
pub fn project_on_line(p: Pt, a: Pt, dir: Pt) -> Pt {
    let d = dir.norm();
    a.add(d.mul(p.sub(a).dot(d)))
}

/// 2×3 affine transform: (x, y) → (a x + b y + c, d x + e y + f).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine(pub [f32; 6]);

impl Affine {
    pub const IDENTITY: Affine = Affine([1.0, 0.0, 0.0, 0.0, 1.0, 0.0]);

    pub fn apply(&self, p: Pt) -> Pt {
        let m = self.0;
        Pt::new(m[0] * p.x + m[1] * p.y + m[2], m[3] * p.x + m[4] * p.y + m[5])
    }

    /// Rotation by `angle` around `c`.
    pub fn rotate_about(c: Pt, angle: f32) -> Affine {
        let (s, co) = angle.sin_cos();
        Affine([co, -s, c.x - co * c.x + s * c.y, s, co, c.y - s * c.x - co * c.y])
    }

    /// Mirror across the line through `c` at `angle`.
    pub fn mirror_about(c: Pt, angle: f32) -> Affine {
        let (s, co) = (2.0 * angle).sin_cos();
        // Reflection matrix [[cos2a, sin2a], [sin2a, -cos2a]] around c.
        Affine([co, s, c.x - co * c.x - s * c.y, s, -co, c.y - s * c.x + co * c.y])
    }
}
