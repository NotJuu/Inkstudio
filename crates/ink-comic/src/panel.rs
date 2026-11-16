//! Panel frames (Koma): rectangles or convex polygons with a border, divided with gutters.

use crate::paint::{outline_pen, stroke_polyline};
use ink_core::{Raster, Rgba};
use ink_vector::Pt;

#[derive(Clone, Debug, PartialEq)]
pub struct Panel {
    pub id: u64,
    /// Convex polygon, clockwise or counter-clockwise.
    pub poly: Vec<Pt>,
}

impl Panel {
    pub fn contains(&self, p: Pt) -> bool {
        // Winding-free point-in-polygon (ray casting).
        let n = self.poly.len();
        let mut inside = false;
        for i in 0..n {
            let (a, b) = (self.poly[i], self.poly[(i + 1) % n]);
            if (a.y > p.y) != (b.y > p.y) && p.x < a.x + (b.x - a.x) * (p.y - a.y) / (b.y - a.y) {
                inside = !inside;
            }
        }
        inside
    }

    pub fn bounds(&self) -> (Pt, Pt) {
        let mut lo = Pt::new(f32::MAX, f32::MAX);
        let mut hi = Pt::new(f32::MIN, f32::MIN);
        for p in &self.poly {
            lo = Pt::new(lo.x.min(p.x), lo.y.min(p.y));
            hi = Pt::new(hi.x.max(p.x), hi.y.max(p.y));
        }
        (lo, hi)
    }
}

/// Layout presets for a new page's panels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelTemplate {
    Single,
    TwoRows,
    ThreeRows,
    /// Four stacked panels (4-koma strip).
    FourKoma,
    Grid2x2,
    Grid2x3,
    /// Wide top panel, two side by side, wide bottom.
    Manga5,
}

impl PanelTemplate {
    pub const ALL: [PanelTemplate; 7] = [
        PanelTemplate::Single,
        PanelTemplate::TwoRows,
        PanelTemplate::ThreeRows,
        PanelTemplate::FourKoma,
        PanelTemplate::Grid2x2,
        PanelTemplate::Grid2x3,
        PanelTemplate::Manga5,
    ];

    pub fn name(self) -> &'static str {
        match self {
            PanelTemplate::Single => "1 panel",
            PanelTemplate::TwoRows => "2 rows",
            PanelTemplate::ThreeRows => "3 rows",
            PanelTemplate::FourKoma => "4-koma",
            PanelTemplate::Grid2x2 => "2 × 2",
            PanelTemplate::Grid2x3 => "2 × 3",
            PanelTemplate::Manga5 => "5-panel manga",
        }
    }
}

#[derive(Clone, Debug)]
pub struct PanelSet {
    pub panels: Vec<Panel>,
    /// Border width in pixels.
    pub border: f32,
    /// Gutter between panels in pixels (horizontal splits, vertical splits).
    pub gutter: (f32, f32),
    pub color: Rgba,
    next_id: u64,
}

impl Default for PanelSet {
    fn default() -> Self {
        Self { panels: Vec::new(), border: 12.0, gutter: (100.0, 50.0), color: Rgba::BLACK, next_id: 0 }
    }
}

/// Keep the part of a convex polygon where `side(p) >= 0`.
fn clip_half(poly: &[Pt], side: impl Fn(Pt) -> f32) -> Vec<Pt> {
    let n = poly.len();
    let mut out = Vec::new();
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        let (sa, sb) = (side(a), side(b));
        if sa >= 0.0 {
            out.push(a);
        }
        if (sa >= 0.0) != (sb >= 0.0) {
            let t = sa / (sa - sb);
            out.push(a.lerp(b, t));
        }
    }
    out
}

impl PanelSet {
    /// Gutter defaults scaled to the page DPI (≈ 4 mm / 2 mm, border ≈ 0.5 mm).
    pub fn for_dpi(dpi: u32) -> Self {
        let mm = dpi as f32 / 25.4;
        Self { border: (0.5 * mm).max(1.0), gutter: (4.0 * mm, 2.0 * mm), ..Self::default() }
    }

    fn id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    pub fn add_rect(&mut self, a: Pt, b: Pt) -> u64 {
        let (lo, hi) = (Pt::new(a.x.min(b.x), a.y.min(b.y)), Pt::new(a.x.max(b.x), a.y.max(b.y)));
        let id = self.id();
        self.panels.push(Panel { id, poly: vec![lo, Pt::new(hi.x, lo.y), hi, Pt::new(lo.x, hi.y)] });
        id
    }

    pub fn hit(&self, p: Pt) -> Option<usize> {
        self.panels.iter().rposition(|pn| pn.contains(p))
    }

    /// Split panel `idx` along the line through a and b, leaving a gutter between the halves.
    /// Mostly-horizontal cuts use the horizontal gutter, mostly-vertical cuts the vertical one.
    pub fn divide(&mut self, idx: usize, a: Pt, b: Pt) -> bool {
        let Some(panel) = self.panels.get(idx).cloned() else { return false };
        let d = b.sub(a);
        let len = d.len();
        if len < 1.0 {
            return false;
        }
        let horizontal = d.x.abs() >= d.y.abs();
        let g = if horizontal { self.gutter.0 } else { self.gutter.1 } / 2.0;
        let side = |p: Pt| d.cross(p.sub(a)) / len;
        let left = clip_half(&panel.poly, |p| side(p) - g);
        let right = clip_half(&panel.poly, |p| -side(p) - g);
        if left.len() < 3 || right.len() < 3 {
            return false;
        }
        let (id1, id2) = (self.id(), self.id());
        self.panels.splice(idx..=idx, [Panel { id: id1, poly: left }, Panel { id: id2, poly: right }]);
        true
    }

    /// Replace all panels with a template inside the safe area (lo..hi).
    pub fn apply_template(&mut self, t: PanelTemplate, lo: Pt, hi: Pt) {
        self.panels.clear();
        let id = self.add_rect(lo, hi);
        let (w, h) = (hi.x - lo.x, hi.y - lo.y);
        let hsplit = |s: &mut Self, frac: f32| {
            let y = lo.y + h * frac;
            let i = s.hit(Pt::new(lo.x + w * 0.5, y - s.gutter.0)).or_else(|| s.hit(Pt::new(lo.x + 1.0, y - s.gutter.0)));
            if let Some(i) = i {
                s.divide(i, Pt::new(lo.x - 10.0, y), Pt::new(hi.x + 10.0, y));
            }
        };
        let vsplit_row = |s: &mut Self, y_mid: f32, frac: f32| {
            let x = lo.x + w * frac;
            if let Some(i) = s.hit(Pt::new(x - s.gutter.1, y_mid)) {
                s.divide(i, Pt::new(x, lo.y - 10.0), Pt::new(x, hi.y + 10.0));
            }
        };
        let _ = id;
        match t {
            PanelTemplate::Single => {}
            PanelTemplate::TwoRows => hsplit(self, 0.5),
            PanelTemplate::ThreeRows => {
                hsplit(self, 1.0 / 3.0);
                hsplit(self, 2.0 / 3.0);
            }
            PanelTemplate::FourKoma => {
                for f in [0.25, 0.5, 0.75] {
                    hsplit(self, f);
                }
            }
            PanelTemplate::Grid2x2 => {
                hsplit(self, 0.5);
                vsplit_row(self, lo.y + h * 0.25, 0.5);
                vsplit_row(self, lo.y + h * 0.75, 0.5);
            }
            PanelTemplate::Grid2x3 => {
                hsplit(self, 1.0 / 3.0);
                hsplit(self, 2.0 / 3.0);
                for r in [1.0 / 6.0, 0.5, 5.0 / 6.0] {
                    vsplit_row(self, lo.y + h * r, 0.5);
                }
            }
            PanelTemplate::Manga5 => {
                hsplit(self, 0.3);
                hsplit(self, 0.68);
                vsplit_row(self, lo.y + h * 0.49, 0.55);
                vsplit_row(self, lo.y + h * 0.84, 0.4);
            }
        }
    }

    /// Render all borders into a fresh raster.
    pub fn render(&self, canvas: (u32, u32)) -> Raster {
        let mut r = Raster::new();
        let pen = outline_pen(self.border);
        for p in &self.panels {
            stroke_polyline(&mut r, &p.poly, true, &pen, self.color, canvas);
        }
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn divide_leaves_gutter() {
        let mut s = PanelSet { gutter: (40.0, 20.0), ..Default::default() };
        s.add_rect(Pt::new(0.0, 0.0), Pt::new(400.0, 600.0));
        assert!(s.divide(0, Pt::new(-10.0, 300.0), Pt::new(410.0, 300.0)));
        assert_eq!(s.panels.len(), 2);
        let (a, b) = (s.panels[0].bounds(), s.panels[1].bounds());
        let gap = if a.0.y < b.0.y { b.0.y - a.1.y } else { a.0.y - b.1.y };
        assert!((gap - 40.0).abs() < 0.01, "gap {gap}");
        // Diagonal cut also works.
        let top = s.hit(Pt::new(200.0, 140.0)).unwrap();
        assert!(s.divide(top, Pt::new(0.0, 0.0), Pt::new(400.0, 280.0)));
        assert_eq!(s.panels.len(), 3);
    }

    #[test]
    fn templates_produce_expected_counts() {
        for (t, n) in [
            (PanelTemplate::Single, 1),
            (PanelTemplate::TwoRows, 2),
            (PanelTemplate::ThreeRows, 3),
            (PanelTemplate::FourKoma, 4),
            (PanelTemplate::Grid2x2, 4),
            (PanelTemplate::Grid2x3, 6),
            (PanelTemplate::Manga5, 5),
        ] {
            let mut s = PanelSet::for_dpi(600);
            s.apply_template(t, Pt::new(500.0, 500.0), Pt::new(4500.0, 7000.0));
            assert_eq!(s.panels.len(), n, "{t:?}");
        }
    }

    #[test]
    fn hit_and_render() {
        let mut s = PanelSet { border: 6.0, ..Default::default() };
        s.add_rect(Pt::new(10.0, 10.0), Pt::new(200.0, 100.0));
        assert_eq!(s.hit(Pt::new(50.0, 50.0)), Some(0));
        assert_eq!(s.hit(Pt::new(250.0, 50.0)), None);
        let r = s.render((256, 256));
        assert_eq!(r.get(100, 10).a(), 255);
        assert_eq!(r.get(100, 50).a(), 0);
    }
}
