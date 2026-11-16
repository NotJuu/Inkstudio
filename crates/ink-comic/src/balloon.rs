//! Speech balloons: shapes, tails, text, hit-testing, rendering.
//!
//! Rendering uses the classic lettering trick for clean joins: first every outline (balloon
//! and tail) is stroked at twice the line width, then every interior is filled white. The
//! fills cover the inner half of all outlines, so balloon and tail merge into one shape.

use crate::paint::{fill_polygon, outline_pen, stroke_polyline};
use crate::text::{Fonts, TextDir, TextLayout};
use ink_core::{Raster, Rgba};
use ink_vector::Pt;
use std::f32::consts::{PI, TAU};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum BalloonStyle {
    /// Classic oval speech balloon.
    Oval,
    /// Rounded rectangle (calm speech, Western comics).
    Rounded,
    /// Sharp rectangle.
    Rectangle,
    /// Thought cloud with bubble tail.
    Thought,
    /// Spiky burst: shouting, surprise.
    Shout,
    /// Dashed outline: whispering.
    Whisper,
    /// Wobbly outline: fear, weakness, cold.
    Wavy,
    /// Narration box: rectangle, no tail.
    Narration,
    /// Electronic / phone voice: jagged rectangle.
    Electronic,
    /// Text only, no balloon.
    TextOnly,
}

impl BalloonStyle {
    pub const ALL: [BalloonStyle; 10] = [
        BalloonStyle::Oval,
        BalloonStyle::Rounded,
        BalloonStyle::Rectangle,
        BalloonStyle::Thought,
        BalloonStyle::Shout,
        BalloonStyle::Whisper,
        BalloonStyle::Wavy,
        BalloonStyle::Narration,
        BalloonStyle::Electronic,
        BalloonStyle::TextOnly,
    ];

    pub fn name(self) -> &'static str {
        match self {
            BalloonStyle::Oval => "Speech",
            BalloonStyle::Rounded => "Rounded",
            BalloonStyle::Rectangle => "Box",
            BalloonStyle::Thought => "Thought",
            BalloonStyle::Shout => "Shout",
            BalloonStyle::Whisper => "Whisper",
            BalloonStyle::Wavy => "Wavy",
            BalloonStyle::Narration => "Narration",
            BalloonStyle::Electronic => "Electronic",
            BalloonStyle::TextOnly => "Text only",
        }
    }

    pub fn default_tail(self) -> TailStyle {
        match self {
            BalloonStyle::Thought => TailStyle::Bubbles,
            BalloonStyle::Narration | BalloonStyle::TextOnly => TailStyle::None,
            _ => TailStyle::Pointed,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TailStyle {
    None,
    Pointed,
    Curved,
    Bubbles,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Balloon {
    pub id: u64,
    pub style: BalloonStyle,
    pub center: Pt,
    /// Half width / half height of the body.
    pub half: Pt,
    pub tail: TailStyle,
    /// Tail tip in canvas pixels.
    pub tail_tip: Pt,
    pub text: String,
    pub dir: TextDir,
    /// Font size in pixels.
    pub font_px: f32,
    /// Outline width in pixels.
    pub line: f32,
    pub ink: Rgba,
    pub paper: Rgba,
    /// Resize the body to the text whenever the text changes.
    pub auto_fit: bool,
}

/// What a pointer is over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BalloonHit {
    Body,
    Tail,
    /// Corner resize handle 0..4 (top-left, top-right, bottom-right, bottom-left).
    Corner(u8),
}

impl Balloon {
    /// A balloon at `at` sized for a page of `dpi`, with sensible lettering defaults.
    pub fn new(id: u64, style: BalloonStyle, at: Pt, dpi: u32, text: &str, dir: TextDir) -> Self {
        let mm = dpi as f32 / 25.4;
        Self {
            id,
            style,
            center: at,
            half: Pt::new(18.0 * mm, 14.0 * mm),
            tail: style.default_tail(),
            tail_tip: Pt::new(at.x - 14.0 * mm, at.y + 26.0 * mm),
            text: text.into(),
            dir,
            font_px: 3.6 * mm, // ≈ 10 pt lettering
            line: (0.35 * mm).max(1.0),
            ink: Rgba::BLACK,
            paper: Rgba::WHITE,
            auto_fit: true,
        }
    }

    pub fn layout(&self, fonts: &Fonts) -> TextLayout {
        TextLayout::new(fonts, &self.text, self.dir, self.font_px)
    }

    /// Size the body around the text with padding that suits the shape.
    pub fn fit_to_text(&mut self, fonts: &Fonts) {
        let l = self.layout(fonts);
        let (w, h) = (l.width.max(self.font_px), l.height.max(self.font_px));
        let k = match self.style {
            // An ellipse needs √2 × the text box to contain it.
            BalloonStyle::Oval
            | BalloonStyle::Thought
            | BalloonStyle::Wavy
            | BalloonStyle::Whisper => 0.75,
            BalloonStyle::Shout => 0.95,
            _ => 0.5,
        };
        let pad = self.font_px * 0.9;
        self.half = Pt::new(w * k + pad, h * k + pad);
    }

    /// Body outline polygon.
    pub fn outline(&self) -> Vec<Pt> {
        let (c, rx, ry) = (self.center, self.half.x, self.half.y);
        let ellipse = |n: usize, f: &dyn Fn(f32) -> f32| -> Vec<Pt> {
            (0..n)
                .map(|i| {
                    let t = i as f32 / n as f32 * TAU;
                    let k = f(t);
                    Pt::new(c.x + rx * k * t.cos(), c.y + ry * k * t.sin())
                })
                .collect()
        };
        let rect = |r: f32, steps: usize| -> Vec<Pt> {
            let r = r.min(rx).min(ry);
            let mut v = Vec::new();
            let corners = [
                (c.x + rx - r, c.y - ry + r, -PI / 2.0),
                (c.x + rx - r, c.y + ry - r, 0.0),
                (c.x - rx + r, c.y + ry - r, PI / 2.0),
                (c.x - rx + r, c.y - ry + r, PI),
            ];
            for (cx, cy, a0) in corners {
                for s in 0..=steps {
                    let a = a0 + s as f32 / steps.max(1) as f32 * PI / 2.0;
                    v.push(Pt::new(cx + r * a.cos(), cy + r * a.sin()));
                }
            }
            v
        };
        let seed = self.id as u32;
        match self.style {
            BalloonStyle::Oval | BalloonStyle::Whisper | BalloonStyle::TextOnly => {
                ellipse(160, &|_| 1.0)
            }
            BalloonStyle::Wavy => ellipse(320, &|t| 1.0 + 0.035 * (t * 26.0).sin()),
            BalloonStyle::Thought => {
                let bumps = 11.0;
                ellipse(330, &|t| {
                    0.9 + 0.14 * ((t * bumps / 2.0).sin().abs()).powf(0.6)
                })
            }
            BalloonStyle::Shout => {
                let spikes = 18;
                (0..spikes * 2)
                    .map(|i| {
                        let t = i as f32 / (spikes * 2) as f32 * TAU;
                        let jitter = crate_hash(seed, i as u32);
                        let k = if i % 2 == 0 {
                            1.22 + 0.18 * jitter
                        } else {
                            0.93
                        };
                        Pt::new(c.x + rx * k * t.cos(), c.y + ry * k * t.sin())
                    })
                    .collect()
            }
            BalloonStyle::Rounded => rect(rx.min(ry) * 0.45, 10),
            BalloonStyle::Rectangle | BalloonStyle::Narration => rect(0.0, 0),
            BalloonStyle::Electronic => {
                // Rectangle edges broken into small zigzags.
                let base = rect(0.0, 0);
                let mut v = Vec::new();
                for i in 0..4 {
                    let (a, b) = (base[i], base[(i + 1) % 4]);
                    let n = ((b.sub(a).len() / (self.font_px * 0.6)).ceil() as usize).max(2);
                    let normal = Pt::new(-(b.y - a.y), b.x - a.x).norm();
                    for k in 0..n {
                        let p = a.lerp(b, k as f32 / n as f32);
                        let off = if k % 2 == 0 {
                            0.0
                        } else {
                            -self.font_px * 0.25
                        };
                        v.push(p.add(normal.mul(off)));
                    }
                }
                v
            }
        }
    }

    /// Point on the body edge (well inside the outline) facing angle `t`, used for tail bases.
    fn inner_edge(&self, t: f32) -> Pt {
        Pt::new(
            self.center.x + self.half.x * 0.8 * t.cos(),
            self.center.y + self.half.y * 0.8 * t.sin(),
        )
    }

    fn tail_angle(&self) -> f32 {
        let d = self.tail_tip.sub(self.center);
        (d.y / self.half.y.max(1.0)).atan2(d.x / self.half.x.max(1.0))
    }

    /// Tail polygon (pointed / curved).
    pub fn tail_polygon(&self) -> Option<Vec<Pt>> {
        if !matches!(self.tail, TailStyle::Pointed | TailStyle::Curved) {
            return None;
        }
        let t = self.tail_angle();
        let spread = 0.22;
        let (a, b) = (self.inner_edge(t - spread), self.inner_edge(t + spread));
        let tip = self.tail_tip;
        if self.tail == TailStyle::Pointed {
            return Some(vec![a, tip, b]);
        }
        // Curved: both sides bend the same way (quadratic curves).
        let mid = a.lerp(b, 0.5).lerp(tip, 0.5);
        let bend = Pt::new(-(tip.y - mid.y), tip.x - mid.x)
            .norm()
            .mul(tip.sub(mid).len() * 0.35);
        let ctrl = mid.add(bend);
        let quad = |p0: Pt, p1: Pt, p2: Pt, n: usize| -> Vec<Pt> {
            (0..=n)
                .map(|i| {
                    let s = i as f32 / n as f32;
                    p0.mul((1.0 - s) * (1.0 - s))
                        .add(p1.mul(2.0 * s * (1.0 - s)))
                        .add(p2.mul(s * s))
                })
                .collect()
        };
        let mut v = quad(a, ctrl.lerp(a, 0.2), tip, 16);
        let mut back = quad(tip, ctrl.lerp(b, 0.2), b, 16);
        back.remove(0);
        v.extend(back);
        Some(v)
    }

    /// Thought-bubble circles from the body toward the tip: (center, radius).
    pub fn tail_bubbles(&self) -> Vec<(Pt, f32)> {
        if self.tail != TailStyle::Bubbles {
            return Vec::new();
        }
        let start = self.inner_edge(self.tail_angle()).lerp(self.center, -0.35);
        let r0 = self.half.x.min(self.half.y) * 0.16;
        (0..3)
            .map(|i| {
                let s = (i as f32 + 1.0) / 3.0;
                (start.lerp(self.tail_tip, s), r0 * (1.0 - 0.28 * i as f32))
            })
            .collect()
    }

    fn handles(&self) -> [Pt; 4] {
        let (c, h) = (self.center, self.half);
        [
            Pt::new(c.x - h.x, c.y - h.y),
            Pt::new(c.x + h.x, c.y - h.y),
            Pt::new(c.x + h.x, c.y + h.y),
            Pt::new(c.x - h.x, c.y + h.y),
        ]
    }

    /// Hit-test in canvas pixels; `slop` = handle grab radius.
    pub fn hit(&self, p: Pt, slop: f32) -> Option<BalloonHit> {
        if self.tail != TailStyle::None && p.sub(self.tail_tip).len() <= slop {
            return Some(BalloonHit::Tail);
        }
        for (i, h) in self.handles().iter().enumerate() {
            if p.sub(*h).len() <= slop {
                return Some(BalloonHit::Corner(i as u8));
            }
        }
        let d = p.sub(self.center);
        let (nx, ny) = (d.x / self.half.x.max(1.0), d.y / self.half.y.max(1.0));
        let inside = match self.style {
            BalloonStyle::Rectangle
            | BalloonStyle::Narration
            | BalloonStyle::Rounded
            | BalloonStyle::Electronic
            | BalloonStyle::TextOnly => nx.abs() <= 1.0 && ny.abs() <= 1.0,
            _ => nx * nx + ny * ny <= 1.15,
        };
        inside.then_some(BalloonHit::Body)
    }

    /// Drag a corner to `p`: the opposite corner stays fixed.
    pub fn drag_corner(&mut self, corner: u8, p: Pt) {
        let opp = self.handles()[((corner + 2) % 4) as usize];
        let min = self.font_px * 1.2;
        let half = Pt::new(
            ((p.x - opp.x).abs() / 2.0).max(min),
            ((p.y - opp.y).abs() / 2.0).max(min),
        );
        self.center = Pt::new((p.x + opp.x) / 2.0, (p.y + opp.y) / 2.0);
        self.half = half;
        self.auto_fit = false;
    }

    /// Move body and tail together.
    pub fn translate(&mut self, d: Pt) {
        self.center = self.center.add(d);
        self.tail_tip = self.tail_tip.add(d);
    }

    /// Bounding box including tail and outline (for redraw regions).
    pub fn bounds(&self) -> (Pt, Pt) {
        let m = self.line * 2.0 + 4.0;
        let mut lo = Pt::new(
            self.center.x - self.half.x * 1.45,
            self.center.y - self.half.y * 1.45,
        );
        let mut hi = Pt::new(
            self.center.x + self.half.x * 1.45,
            self.center.y + self.half.y * 1.45,
        );
        if self.tail != TailStyle::None {
            lo = Pt::new(lo.x.min(self.tail_tip.x), lo.y.min(self.tail_tip.y));
            hi = Pt::new(hi.x.max(self.tail_tip.x), hi.y.max(self.tail_tip.y));
        }
        (Pt::new(lo.x - m, lo.y - m), Pt::new(hi.x + m, hi.y + m))
    }

    /// Render body, tail and text into `r`.
    pub fn render(&self, fonts: &Fonts, r: &mut Raster, canvas: (u32, u32)) {
        if self.style != BalloonStyle::TextOnly {
            let body = self.outline();
            let tail = self.tail_polygon();
            let mut pen = outline_pen(self.line * 2.0);
            if self.style == BalloonStyle::Whisper {
                pen.dash = Some((1.2, 1.0));
            }
            // 1. Outlines, doubled.
            stroke_polyline(r, &body, true, &pen, self.ink, canvas);
            if let Some(t) = &tail {
                stroke_polyline(r, t, true, &outline_pen(self.line * 2.0), self.ink, canvas);
            }
            // 2. Interiors cover the inner half of every outline → one merged shape.
            fill_polygon(r, &body, self.paper);
            if let Some(t) = &tail {
                fill_polygon(r, t, self.paper);
            }
            // 3. Thought bubbles are separate small balloons.
            for (c, rad) in self.tail_bubbles() {
                let circle: Vec<Pt> = (0..48)
                    .map(|i| {
                        let a = i as f32 / 48.0 * TAU;
                        Pt::new(c.x + rad * a.cos(), c.y + rad * 0.8 * a.sin())
                    })
                    .collect();
                stroke_polyline(
                    r,
                    &circle,
                    true,
                    &outline_pen(self.line * 2.0),
                    self.ink,
                    canvas,
                );
                fill_polygon(r, &circle, self.paper);
            }
        }
        let l = self.layout(fonts);
        let origin = Pt::new(
            self.center.x - l.width / 2.0,
            self.center.y - l.height / 2.0,
        );
        l.render(fonts, r, origin, self.ink);
    }
}

fn crate_hash(a: u32, b: u32) -> f32 {
    ink_raster::tip::hash01(a, b)
}

/// Render many balloons into a fresh raster, in order (later ones on top).
pub fn render_all(balloons: &[Balloon], fonts: &Fonts, canvas: (u32, u32)) -> Raster {
    let mut r = Raster::new();
    for b in balloons {
        b.render(fonts, &mut r, canvas);
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fonts() -> Fonts {
        Fonts::system()
    }

    #[test]
    fn every_style_renders_a_closed_white_body_with_black_outline() {
        let f = fonts();
        for (i, style) in BalloonStyle::ALL.into_iter().enumerate() {
            let mut b = Balloon::new(
                i as u64 + 1,
                style,
                Pt::new(600.0, 500.0),
                300,
                "",
                TextDir::Vertical,
            );
            b.half = Pt::new(220.0, 160.0);
            let mut r = Raster::new();
            b.render(&f, &mut r, (1200, 1000));
            if style == BalloonStyle::TextOnly {
                assert_eq!(r.get(600, 500).a(), 0);
                continue;
            }
            assert_eq!(r.get(600, 500), Rgba::WHITE, "{style:?} center is white");
            // Walk right from the center: must hit black ink before leaving the bounds.
            let hit =
                (600..1000).any(|x| r.get(x, 500).to_straight()[0] < 60 && r.get(x, 500).a() > 200);
            assert!(hit, "{style:?} has an outline on the right");
            assert_eq!(r.get(1150, 950).a(), 0, "{style:?} nothing far away");
        }
    }

    #[test]
    fn tail_merges_with_body() {
        let f = fonts();
        let mut b = Balloon::new(
            1,
            BalloonStyle::Oval,
            Pt::new(500.0, 400.0),
            300,
            "",
            TextDir::Vertical,
        );
        b.half = Pt::new(200.0, 150.0);
        b.tail_tip = Pt::new(500.0, 800.0);
        let mut r = Raster::new();
        b.render(&f, &mut r, (1000, 1000));
        // Where the tail meets the body (just inside the body's bottom edge) there is no
        // outline across the tail: it's white.
        assert_eq!(r.get(500, 548), Rgba::WHITE, "join is open");
        assert_eq!(
            r.get(500, 650).to_straight()[0],
            255,
            "tail interior is white"
        );
    }

    #[test]
    fn hit_testing_and_dragging() {
        let mut b = Balloon::new(
            1,
            BalloonStyle::Oval,
            Pt::new(500.0, 400.0),
            300,
            "テスト",
            TextDir::Vertical,
        );
        b.half = Pt::new(200.0, 150.0);
        assert_eq!(b.hit(Pt::new(500.0, 400.0), 10.0), Some(BalloonHit::Body));
        assert_eq!(b.hit(b.tail_tip, 10.0), Some(BalloonHit::Tail));
        assert_eq!(
            b.hit(Pt::new(300.0, 250.0), 10.0),
            Some(BalloonHit::Corner(0))
        );
        assert_eq!(b.hit(Pt::new(900.0, 900.0), 10.0), None);
        let tip = b.tail_tip;
        b.translate(Pt::new(10.0, 20.0));
        assert_eq!(b.tail_tip, Pt::new(tip.x + 10.0, tip.y + 20.0));
        b.drag_corner(2, Pt::new(800.0, 700.0));
        assert!((b.center.x - 555.0).abs() < 1e-3 && (b.half.x - 245.0).abs() < 1e-3);
    }

    #[test]
    fn fit_grows_with_text() {
        let f = fonts();
        if f.is_empty() {
            return;
        }
        let mut b = Balloon::new(
            1,
            BalloonStyle::Oval,
            Pt::new(500.0, 400.0),
            300,
            "あ",
            TextDir::Vertical,
        );
        b.fit_to_text(&f);
        let small = b.half;
        b.text = "ありがとう\nございます".into();
        b.fit_to_text(&f);
        assert!(b.half.x > small.x && b.half.y > small.y);
    }
}
