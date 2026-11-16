use crate::geom::{Affine, Pt, closest_on_segment, segment_intersection};
use ink_core::{Document, Layer, Raster, Rgba};
use ink_raster::{BrushParams, PathPoint, Sample, Stroke};

/// An editable vector stroke: the smoothed path with the width baked into each point.
#[derive(Clone, Debug)]
pub struct VStroke {
    pub id: u64,
    pub points: Vec<Sample>,
    /// Brush used to render; tapers and stabilization are already baked into `points`.
    pub brush: BrushParams,
    pub color: Rgba,
}

impl VStroke {
    /// Build from a finished raster-engine path: bakes start/end taper into per-point scale so
    /// that cutting the stroke later never re-tapers the cut ends.
    pub fn from_path(
        id: u64,
        path: &[PathPoint],
        length: f32,
        brush: &BrushParams,
        color: Rgba,
    ) -> Self {
        let (ts, te) = (brush.taper_start, brush.taper_end.min(length * 0.5));
        let points = path
            .iter()
            .map(|p| {
                let mut s = p.sample;
                let mut k = 1.0f32;
                if ts > 0.0 && p.dist < ts {
                    k *= (p.dist / ts).sqrt().max(0.05);
                }
                let left = length - p.dist;
                if te > 0.0 && left < te {
                    k *= (left / te).max(0.0).sqrt().max(0.05);
                }
                s.scale *= k;
                s
            })
            .collect();
        let mut b = brush.clone();
        b.taper_start = 0.0;
        b.taper_end = 0.0;
        b.stabilization = 0;
        b.post_correction = false;
        Self {
            id,
            points,
            brush: b,
            color,
        }
    }

    fn pts(&self) -> impl Iterator<Item = Pt> + '_ {
        self.points.iter().map(|s| Pt::new(s.x, s.y))
    }

    /// Cumulative arc length at each point.
    fn cumulative(&self) -> Vec<f32> {
        let mut acc = 0.0;
        let mut out = Vec::with_capacity(self.points.len());
        let mut prev: Option<Pt> = None;
        for p in self.pts() {
            if let Some(q) = prev {
                acc += p.sub(q).len();
            }
            out.push(acc);
            prev = Some(p);
        }
        out
    }

    /// Bounding box grown by the brush radius.
    pub fn bounds(&self) -> (Pt, Pt) {
        let r = self.brush.size * 0.5 + 2.0;
        let mut lo = Pt::new(f32::MAX, f32::MAX);
        let mut hi = Pt::new(f32::MIN, f32::MIN);
        for p in self.pts() {
            lo = Pt::new(lo.x.min(p.x - r), lo.y.min(p.y - r));
            hi = Pt::new(hi.x.max(p.x + r), hi.y.max(p.y + r));
        }
        (lo, hi)
    }

    /// Sub-stroke between arc lengths [l0, l1], interpolating the cut points.
    fn slice(&self, l0: f32, l1: f32, new_id: u64) -> Option<VStroke> {
        if l1 - l0 < 0.5 || self.points.len() < 2 {
            return None;
        }
        let cum = self.cumulative();
        let at = |l: f32| -> Sample {
            let i = cum.partition_point(|&c| c < l).clamp(1, cum.len() - 1);
            let (a, b) = (self.points[i - 1], self.points[i]);
            let seg = cum[i] - cum[i - 1];
            let t = if seg > 0.0 {
                ((l - cum[i - 1]) / seg).clamp(0.0, 1.0)
            } else {
                0.0
            };
            Sample {
                x: a.x + (b.x - a.x) * t,
                y: a.y + (b.y - a.y) * t,
                pressure: a.pressure + (b.pressure - a.pressure) * t,
                time: a.time,
                scale: a.scale + (b.scale - a.scale) * t,
            }
        };
        let mut pts = vec![at(l0)];
        for (i, s) in self.points.iter().enumerate() {
            if cum[i] > l0 && cum[i] < l1 {
                pts.push(*s);
            }
        }
        pts.push(at(l1));
        Some(VStroke {
            id: new_id,
            points: pts,
            brush: self.brush.clone(),
            color: self.color,
        })
    }

    /// Arc length of the closest point to `p` if within `radius`.
    fn touch(&self, p: Pt, radius: f32) -> Option<f32> {
        let cum = self.cumulative();
        let pts: Vec<Pt> = self.pts().collect();
        let reach = radius + self.brush.size * 0.5;
        let mut best: Option<(f32, f32)> = None;
        if pts.len() == 1 {
            let d = pts[0].sub(p).len();
            return (d <= reach).then_some(0.0);
        }
        for i in 1..pts.len() {
            let (q, t) = closest_on_segment(p, pts[i - 1], pts[i]);
            let d = q.sub(p).len();
            if d <= reach && best.is_none_or(|b| d < b.0) {
                best = Some((d, cum[i - 1] + (cum[i] - cum[i - 1]) * t));
            }
        }
        best.map(|b| b.1)
    }

    /// Arc lengths where this stroke crosses `other`.
    fn crossings(&self, other: &VStroke) -> Vec<f32> {
        let (alo, ahi) = self.bounds();
        let (blo, bhi) = other.bounds();
        if ahi.x < blo.x || bhi.x < alo.x || ahi.y < blo.y || bhi.y < alo.y {
            return Vec::new();
        }
        let cum = self.cumulative();
        let a: Vec<Pt> = self.pts().collect();
        let b: Vec<Pt> = other.pts().collect();
        let mut out = Vec::new();
        for i in 1..a.len() {
            for j in 1..b.len() {
                if let Some((t, _)) = segment_intersection(a[i - 1], a[i], b[j - 1], b[j]) {
                    out.push(cum[i - 1] + (cum[i] - cum[i - 1]) * t);
                }
            }
        }
        out
    }

    pub fn transformed(&self, m: &Affine, new_id: u64) -> VStroke {
        let mut s = self.clone();
        s.id = new_id;
        for p in &mut s.points {
            let q = m.apply(Pt::new(p.x, p.y));
            p.x = q.x;
            p.y = q.y;
        }
        s
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EraseMode {
    /// Remove only the part under the eraser.
    Touched,
    /// Remove the touched stretch up to the nearest crossing with another line on each side.
    UpToIntersection,
    /// Remove every touched line entirely.
    WholeLine,
}

/// Strokes of one vector layer.
#[derive(Clone, Debug, Default)]
pub struct VectorLayer {
    pub strokes: Vec<VStroke>,
    next_id: u64,
}

impl VectorLayer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn next_id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    pub fn push(&mut self, mut s: VStroke) -> u64 {
        s.id = self.next_id();
        let id = s.id;
        self.strokes.push(s);
        id
    }

    /// Erase with a round eraser at `p`. Returns true if anything changed.
    pub fn erase(&mut self, p: Pt, radius: f32, mode: EraseMode) -> bool {
        let mut changed = false;
        let mut out: Vec<VStroke> = Vec::with_capacity(self.strokes.len());
        let all = self.strokes.clone();
        for s in std::mem::take(&mut self.strokes) {
            let Some(at) = s.touch(p, radius) else {
                out.push(s);
                continue;
            };
            changed = true;
            let total = *s.cumulative().last().unwrap_or(&0.0);
            let (cut0, cut1) = match mode {
                EraseMode::WholeLine => continue,
                EraseMode::Touched => {
                    // Remove the stretch whose centerline is within the eraser radius.
                    let pts: Vec<Pt> = s.pts().collect();
                    let cum = s.cumulative();
                    let inside = |l: f32| {
                        let i = cum
                            .partition_point(|&c| c < l)
                            .clamp(1, cum.len().max(2) - 1);
                        if pts.len() < 2 {
                            return true;
                        }
                        let seg = cum[i] - cum[i - 1];
                        let t = if seg > 0.0 {
                            ((l - cum[i - 1]) / seg).clamp(0.0, 1.0)
                        } else {
                            0.0
                        };
                        pts[i - 1].lerp(pts[i], t).sub(p).len() <= radius + s.brush.size * 0.5
                    };
                    let step = 0.5;
                    let mut a = at;
                    while a > 0.0 && inside(a - step) {
                        a -= step;
                    }
                    let mut b = at;
                    while b < total && inside(b + step) {
                        b += step;
                    }
                    (a.max(0.0), b.min(total))
                }
                EraseMode::UpToIntersection => {
                    let mut xs: Vec<f32> = all
                        .iter()
                        .filter(|o| o.id != s.id)
                        .flat_map(|o| s.crossings(o))
                        .collect();
                    xs.sort_by(|a, b| a.total_cmp(b));
                    let a = xs.iter().rev().find(|&&x| x < at).copied().unwrap_or(0.0);
                    let b = xs.iter().find(|&&x| x > at).copied().unwrap_or(total);
                    (a, b)
                }
            };
            if let Some(left) = s.slice(0.0, cut0, s.id) {
                out.push(left);
            }
            let rid = self.next_id();
            if let Some(right) = s.slice(cut1, total, rid) {
                out.push(right);
            }
        }
        self.strokes = out;
        changed
    }

    /// Line width correction: scale every stroke's width (e.g. 0.8 = thinner).
    pub fn scale_width(&mut self, k: f32) {
        for s in &mut self.strokes {
            for p in &mut s.points {
                p.scale *= k;
            }
        }
    }

    /// Render all strokes into a fresh raster with the shared brush engine.
    pub fn render(&self, width: u32, height: u32, dpi: u32) -> Raster {
        let mut doc = Document::new(width, height, dpi);
        let id = doc.push_layer(Layer::new("vector"));
        for s in &self.strokes {
            let mut st = Stroke::begin(&doc, id, s.brush.clone(), s.color);
            for p in &s.points {
                st.add(&mut doc, *p);
            }
            let _ = st.end(&mut doc);
        }
        doc.remove_layer(id).map(|l| l.raster).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn straight(layer: &mut VectorLayer, a: Pt, b: Pt) -> u64 {
        let n = 60;
        let points: Vec<Sample> = (0..=n)
            .map(|i| {
                let p = a.lerp(b, i as f32 / n as f32);
                Sample::new(p.x, p.y, 1.0)
            })
            .collect();
        let mut brush = BrushParams::g_pen(4.0);
        brush.taper_start = 0.0;
        brush.taper_end = 0.0;
        brush.stabilization = 0;
        layer.push(VStroke {
            id: 0,
            points,
            brush,
            color: Rgba::BLACK,
        })
    }

    fn total(s: &VStroke) -> f32 {
        *s.cumulative().last().unwrap()
    }

    #[test]
    fn erase_up_to_intersection_trims_between_crossings() {
        // A horizontal line crossed by two verticals at x=100 and x=200; erase at x=150.
        let mut l = VectorLayer::new();
        straight(&mut l, Pt::new(0.0, 100.0), Pt::new(300.0, 100.0));
        straight(&mut l, Pt::new(100.0, 0.0), Pt::new(100.0, 200.0));
        straight(&mut l, Pt::new(200.0, 0.0), Pt::new(200.0, 200.0));
        assert!(l.erase(Pt::new(150.0, 100.0), 3.0, EraseMode::UpToIntersection));
        assert_eq!(
            l.strokes.len(),
            4,
            "horizontal split into two pieces + two verticals"
        );
        let horiz: Vec<&VStroke> = l
            .strokes
            .iter()
            .filter(|s| {
                (s.points[0].y - 100.0).abs() < 0.1
                    && (s.points.last().unwrap().y - 100.0).abs() < 0.1
            })
            .collect();
        assert_eq!(horiz.len(), 2);
        let mut ends: Vec<(f32, f32)> = horiz
            .iter()
            .map(|s| (s.points[0].x, s.points.last().unwrap().x))
            .collect();
        ends.sort_by(|a, b| a.0.total_cmp(&b.0));
        assert!(
            (ends[0].1 - 100.0).abs() < 0.5,
            "left piece ends at the crossing: {:?}",
            ends
        );
        assert!(
            (ends[1].0 - 200.0).abs() < 0.5,
            "right piece starts at the crossing: {:?}",
            ends
        );
    }

    #[test]
    fn erase_up_to_intersection_without_crossing_removes_whole_line() {
        let mut l = VectorLayer::new();
        straight(&mut l, Pt::new(0.0, 0.0), Pt::new(100.0, 0.0));
        l.erase(Pt::new(50.0, 0.0), 2.0, EraseMode::UpToIntersection);
        assert!(l.strokes.is_empty());
    }

    #[test]
    fn erase_touched_cuts_a_hole() {
        let mut l = VectorLayer::new();
        straight(&mut l, Pt::new(0.0, 0.0), Pt::new(200.0, 0.0));
        l.erase(Pt::new(100.0, 0.0), 10.0, EraseMode::Touched);
        assert_eq!(l.strokes.len(), 2);
        let removed = 200.0 - l.strokes.iter().map(total).sum::<f32>();
        assert!((15.0..=30.0).contains(&removed), "removed {removed}px");
    }

    #[test]
    fn whole_line_and_untouched() {
        let mut l = VectorLayer::new();
        straight(&mut l, Pt::new(0.0, 0.0), Pt::new(200.0, 0.0));
        straight(&mut l, Pt::new(0.0, 50.0), Pt::new(200.0, 50.0));
        l.erase(Pt::new(10.0, 0.0), 2.0, EraseMode::WholeLine);
        assert_eq!(l.strokes.len(), 1);
        assert!((l.strokes[0].points[0].y - 50.0).abs() < 0.1);
    }

    #[test]
    fn render_matches_brush_and_width_correction_thins() {
        let mut l = VectorLayer::new();
        straight(&mut l, Pt::new(20.0, 100.0), Pt::new(230.0, 100.0));
        let w = |l: &VectorLayer| {
            let r = l.render(256, 256, 600);
            (80..120).filter(|&y| r.get(128, y).a() > 127).count()
        };
        let before = w(&l);
        l.scale_width(0.5);
        let after = w(&l);
        assert!(before >= 3 && after < before, "{before} -> {after}");
    }
}
