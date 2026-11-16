//! Rulers snap freehand input while you draw (S15).
//!
//! The pen still draws freehand (pressure, speed); only the position is constrained.
//! Line-type rulers decide a direction once the pen has moved a few pixels, then lock it for
//! the rest of the stroke — that is what makes perspective inking fast.

use crate::geom::{Affine, Pt, project_on_line};

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Ruler {
    /// Snap onto the infinite line through a and b.
    Straight { a: Pt, b: Pt },
    /// 1, 2 or 3 vanishing points. With 1 or 2 points, horizontal / vertical guide directions
    /// are offered too (as in one- and two-point perspective).
    Perspective { vps: Vec<Pt> },
    /// Every stroke runs at `angle` (radians): hatching, speed lines.
    Parallel { angle: f32 },
    /// Every stroke runs toward/away from `center`: focus / impact lines.
    Focus { center: Pt },
    /// Every stroke follows a circle around `center` (radius set by where the stroke starts).
    Concentric { center: Pt },
    /// `lines` symmetry axes through `center` starting at `angle`; `mirror` = line symmetry,
    /// otherwise rotational symmetry.
    Symmetry {
        center: Pt,
        angle: f32,
        lines: u32,
        mirror: bool,
    },
}

/// Per-stroke state: the direction a line ruler locked onto.
#[derive(Clone, Debug, Default)]
pub struct RulerSnap {
    start: Option<Pt>,
    dir: Option<Pt>,
}

/// Movement (px) before a line ruler picks its direction.
const DECIDE_PX: f32 = 8.0;

impl RulerSnap {
    pub fn new() -> Self {
        Self::default()
    }

    /// Constrain a pen position. Call with every sample of the stroke, in order.
    pub fn snap(&mut self, ruler: &Ruler, p: Pt) -> Pt {
        let start = *self.start.get_or_insert(p);
        match ruler {
            Ruler::Straight { a, b } => project_on_line(p, *a, b.sub(*a)),
            Ruler::Concentric { center } => {
                let r = start.sub(*center).len();
                let d = p.sub(*center);
                if d.len() < 1e-3 {
                    start
                } else {
                    center.add(d.norm().mul(r))
                }
            }
            Ruler::Parallel { angle } => {
                project_on_line(p, start, Pt::new(angle.cos(), angle.sin()))
            }
            Ruler::Focus { center } => {
                let dir = start.sub(*center);
                if dir.len() < 1e-3 {
                    p
                } else {
                    project_on_line(p, start, dir)
                }
            }
            Ruler::Perspective { vps } => {
                if self.dir.is_none() {
                    let moved = p.sub(start);
                    if moved.len() < DECIDE_PX {
                        return start;
                    }
                    let m = moved.norm();
                    let mut candidates: Vec<Pt> =
                        vps.iter().map(|vp| vp.sub(start).norm()).collect();
                    if vps.len() <= 2 {
                        candidates.push(Pt::new(0.0, 1.0)); // verticals
                    }
                    if vps.len() == 1 {
                        candidates.push(Pt::new(1.0, 0.0)); // horizontals
                    }
                    self.dir = candidates
                        .into_iter()
                        .max_by(|a, b| a.dot(m).abs().total_cmp(&b.dot(m).abs()));
                }
                project_on_line(p, start, self.dir.unwrap_or(Pt::new(1.0, 0.0)))
            }
            Ruler::Symmetry { .. } => p, // symmetry copies strokes instead of constraining them
        }
    }
}

impl Ruler {
    /// Extra copies to draw for symmetry rulers (the original stroke is not included).
    pub fn symmetry_transforms(&self) -> Vec<Affine> {
        let Ruler::Symmetry {
            center,
            angle,
            lines,
            mirror,
        } = self
        else {
            return Vec::new();
        };
        let n = (*lines).max(1);
        let mut out = Vec::new();
        if *mirror {
            // n mirror axes → 2n copies (n rotations × {identity, reflection}) minus the original.
            for k in 0..n {
                let rot = std::f32::consts::TAU * k as f32 / n as f32;
                if k > 0 {
                    out.push(Affine::rotate_about(*center, rot));
                }
                out.push(Affine::mirror_about(*center, angle + rot / 2.0));
            }
        } else {
            for k in 1..n {
                out.push(Affine::rotate_about(
                    *center,
                    std::f32::consts::TAU * k as f32 / n as f32,
                ));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Pt, b: Pt) -> bool {
        a.sub(b).len() < 1e-3
    }

    #[test]
    fn perspective_locks_to_nearest_vanishing_point() {
        let r = Ruler::Perspective {
            vps: vec![Pt::new(-1000.0, 0.0), Pt::new(1000.0, 0.0)],
        };
        let mut s = RulerSnap::new();
        s.snap(&r, Pt::new(0.0, 100.0));
        // Moving mostly right → toward the right vanishing point; later wobble stays on that line.
        s.snap(&r, Pt::new(10.0, 99.0));
        let q = s.snap(&r, Pt::new(200.0, 120.0));
        // Line from (0,100) to (1000,0): y = 100 - 0.1x.
        assert!((q.y - (100.0 - 0.1 * q.x)).abs() < 0.01, "{q:?}");
        // Vertical candidate in 2-point perspective.
        let mut v = RulerSnap::new();
        v.snap(&r, Pt::new(50.0, 50.0));
        let q = v.snap(&r, Pt::new(51.0, 80.0));
        assert!((q.x - 50.0).abs() < 1e-3, "{q:?}");
    }

    #[test]
    fn focus_and_parallel_and_concentric() {
        let f = Ruler::Focus {
            center: Pt::new(0.0, 0.0),
        };
        let mut s = RulerSnap::new();
        s.snap(&f, Pt::new(100.0, 100.0));
        let q = s.snap(&f, Pt::new(150.0, 160.0));
        assert!(
            (q.x - q.y).abs() < 1e-3,
            "stays on the ray through the center: {q:?}"
        );

        let p = Ruler::Parallel { angle: 0.0 };
        let mut s = RulerSnap::new();
        s.snap(&p, Pt::new(10.0, 20.0));
        assert!(close(s.snap(&p, Pt::new(80.0, 35.0)), Pt::new(80.0, 20.0)));

        let c = Ruler::Concentric {
            center: Pt::new(0.0, 0.0),
        };
        let mut s = RulerSnap::new();
        s.snap(&c, Pt::new(50.0, 0.0));
        let q = s.snap(&c, Pt::new(0.0, 80.0));
        assert!(close(q, Pt::new(0.0, 50.0)), "{q:?}");
    }

    #[test]
    fn symmetry_copies() {
        let c = Pt::new(100.0, 100.0);
        let mirror = Ruler::Symmetry {
            center: c,
            angle: std::f32::consts::FRAC_PI_2,
            lines: 1,
            mirror: true,
        };
        let t = mirror.symmetry_transforms();
        assert_eq!(t.len(), 1);
        assert!(
            close(t[0].apply(Pt::new(90.0, 40.0)), Pt::new(110.0, 40.0)),
            "vertical mirror axis"
        );

        let rot = Ruler::Symmetry {
            center: c,
            angle: 0.0,
            lines: 4,
            mirror: false,
        };
        let t = rot.symmetry_transforms();
        assert_eq!(t.len(), 3);
        assert!(close(
            t[0].apply(Pt::new(150.0, 100.0)),
            Pt::new(100.0, 150.0)
        ));

        let kaleido = Ruler::Symmetry {
            center: c,
            angle: 0.0,
            lines: 6,
            mirror: true,
        };
        assert_eq!(kaleido.symmetry_transforms().len(), 11);
    }
}
