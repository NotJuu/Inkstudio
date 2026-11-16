//! Ruler tool: perspective (1/2/3 vanishing points), symmetry (mirror, kaleidoscope,
//! rotational), parallel, focus and concentric rulers. One ruler per page; brush strokes snap to
//! it (perspective, parallel, …) or are copied by it (symmetry).

use super::*;
use ink_vector::{Ruler, RulerSnap};
use std::f32::consts::{FRAC_PI_2, PI, TAU};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RulerKind {
    OnePoint,
    TwoPoint,
    ThreePoint,
    Mirror,
    Kaleidoscope,
    Rotational,
    Parallel,
    Focus,
    Concentric,
}

impl RulerKind {
    pub const ALL: [RulerKind; 9] = [
        RulerKind::OnePoint,
        RulerKind::TwoPoint,
        RulerKind::ThreePoint,
        RulerKind::Mirror,
        RulerKind::Kaleidoscope,
        RulerKind::Rotational,
        RulerKind::Parallel,
        RulerKind::Focus,
        RulerKind::Concentric,
    ];

    pub fn name(self) -> &'static str {
        match self {
            RulerKind::OnePoint => "1-point perspective",
            RulerKind::TwoPoint => "2-point perspective",
            RulerKind::ThreePoint => "3-point perspective",
            RulerKind::Mirror => "Mirror symmetry",
            RulerKind::Kaleidoscope => "Kaleidoscope",
            RulerKind::Rotational => "Rotational symmetry",
            RulerKind::Parallel => "Parallel lines",
            RulerKind::Focus => "Focus lines",
            RulerKind::Concentric => "Concentric circles",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            RulerKind::OnePoint | RulerKind::TwoPoint | RulerKind::ThreePoint => icons::PERSPECTIVE,
            RulerKind::Mirror => icons::FLIP_HORIZONTAL,
            RulerKind::Kaleidoscope => icons::FLOWER,
            RulerKind::Rotational => icons::ARROWS_CLOCKWISE,
            RulerKind::Parallel => icons::LIST,
            RulerKind::Focus => icons::SUN,
            RulerKind::Concentric => icons::TARGET,
        }
    }

    /// A ruler of this kind laid out for a w × h page.
    pub fn make(self, w: f32, h: f32) -> Ruler {
        let c = Pt::new(w / 2.0, h / 2.0);
        let horizon = h * 0.42;
        match self {
            RulerKind::OnePoint => Ruler::Perspective {
                vps: vec![Pt::new(w / 2.0, horizon)],
            },
            RulerKind::TwoPoint => Ruler::Perspective {
                vps: vec![Pt::new(-w * 0.35, horizon), Pt::new(w * 1.35, horizon)],
            },
            RulerKind::ThreePoint => Ruler::Perspective {
                vps: vec![
                    Pt::new(-w * 0.35, horizon),
                    Pt::new(w * 1.35, horizon),
                    Pt::new(w / 2.0, h * 2.2),
                ],
            },
            RulerKind::Mirror => Ruler::Symmetry {
                center: c,
                angle: FRAC_PI_2,
                lines: 1,
                mirror: true,
            },
            RulerKind::Kaleidoscope => Ruler::Symmetry {
                center: c,
                angle: FRAC_PI_2,
                lines: 6,
                mirror: true,
            },
            RulerKind::Rotational => Ruler::Symmetry {
                center: c,
                angle: 0.0,
                lines: 6,
                mirror: false,
            },
            RulerKind::Parallel => Ruler::Parallel { angle: 0.0 },
            RulerKind::Focus => Ruler::Focus { center: c },
            RulerKind::Concentric => Ruler::Concentric { center: c },
        }
    }

    pub fn of(r: &Ruler) -> Option<RulerKind> {
        Some(match r {
            Ruler::Perspective { vps } => match vps.len() {
                1 => RulerKind::OnePoint,
                2 => RulerKind::TwoPoint,
                _ => RulerKind::ThreePoint,
            },
            Ruler::Symmetry {
                mirror: true,
                lines: 1,
                ..
            } => RulerKind::Mirror,
            Ruler::Symmetry { mirror: true, .. } => RulerKind::Kaleidoscope,
            Ruler::Symmetry { .. } => RulerKind::Rotational,
            Ruler::Parallel { .. } => RulerKind::Parallel,
            Ruler::Focus { .. } => RulerKind::Focus,
            Ruler::Concentric { .. } => RulerKind::Concentric,
            Ruler::Straight { .. } => return None,
        })
    }
}

/// Distance (canvas px) of the angle handle from the ruler's center.
fn arm(w: f32, h: f32) -> f32 {
    w.min(h) * 0.22
}

/// Draggable handles of a ruler, in canvas coordinates.
pub fn handles(r: &Ruler, w: f32, h: f32) -> Vec<Pt> {
    let a = arm(w, h);
    match r {
        Ruler::Perspective { vps } => vps.clone(),
        Ruler::Symmetry { center, angle, .. } => vec![
            *center,
            center.add(Pt::new(angle.cos(), angle.sin()).mul(a)),
        ],
        Ruler::Parallel { angle } => {
            let c = Pt::new(w / 2.0, h / 2.0);
            vec![c.add(Pt::new(angle.cos(), angle.sin()).mul(a))]
        }
        Ruler::Focus { center } | Ruler::Concentric { center } => vec![*center],
        Ruler::Straight { a, b } => vec![*a, *b],
    }
}

/// Move handle `i` to `p`.
pub fn set_handle(r: &mut Ruler, i: usize, p: Pt, w: f32, h: f32) {
    match r {
        Ruler::Perspective { vps } => {
            if let Some(v) = vps.get_mut(i) {
                *v = p;
            }
            // Keep the horizon level in 2- and 3-point perspective: both horizon points share y.
            if vps.len() >= 2 && i < 2 {
                let y = p.y;
                vps[1 - i].y = y;
            }
        }
        Ruler::Symmetry { center, angle, .. } => {
            if i == 0 {
                *center = p;
            } else {
                let d = p.sub(*center);
                *angle = snap_angle(d.y.atan2(d.x));
            }
        }
        Ruler::Parallel { angle } => {
            let d = p.sub(Pt::new(w / 2.0, h / 2.0));
            *angle = snap_angle(d.y.atan2(d.x));
        }
        Ruler::Focus { center } | Ruler::Concentric { center } => *center = p,
        Ruler::Straight { a, b } => {
            if i == 0 {
                *a = p
            } else {
                *b = p
            }
        }
    }
}

/// Snap angles to 15° steps when within 2°.
fn snap_angle(a: f32) -> f32 {
    let step = 15f32.to_radians();
    let s = (a / step).round() * step;
    if (a - s).abs() < 2f32.to_radians() {
        s
    } else {
        a
    }
}

/// Paint guide lines and handles of `r` over the page.
pub fn paint(
    painter: &egui::Painter,
    v: &View,
    r: &Ruler,
    w: f32,
    h: f32,
    active: bool,
    hovered: Option<usize>,
) {
    let alpha = if active { 170 } else { 90 };
    let guide = Color32::from_rgba_unmultiplied(70, 150, 255, alpha);
    let strong = Color32::from_rgba_unmultiplied(255, 120, 60, alpha.max(160));
    let far = (w + h) * 4.0;
    let line = |a: Pt, b: Pt, c: Color32, width: f32| {
        painter.line_segment(
            [v.to_screen(a.x, a.y), v.to_screen(b.x, b.y)],
            Stroke::new(width, c),
        );
    };
    let center = Pt::new(w / 2.0, h / 2.0);
    match r {
        Ruler::Perspective { vps } => {
            if vps.len() >= 2 {
                line(vps[0], vps[1], strong, 1.5);
                let d = vps[1].sub(vps[0]).norm();
                line(vps[0].sub(d.mul(far)), vps[1].add(d.mul(far)), strong, 1.5);
            } else if let Some(vp) = vps.first() {
                line(Pt::new(-far, vp.y), Pt::new(far, vp.y), strong, 1.5);
            }
            for vp in vps {
                // Fan of lines through the page from each vanishing point.
                let to_c = center.sub(*vp);
                let base = to_c.y.atan2(to_c.x);
                let dist = to_c.len().max(1.0);
                let spread = ((w.max(h) / dist).atan() * 1.2).min(PI);
                let n = if active { 18 } else { 10 };
                for k in 0..=n {
                    let a = if vps.len() == 1 {
                        TAU * k as f32 / n as f32
                    } else {
                        base - spread + 2.0 * spread * k as f32 / n as f32
                    };
                    line(*vp, vp.add(Pt::new(a.cos(), a.sin()).mul(far)), guide, 1.0);
                }
            }
        }
        Ruler::Symmetry {
            center: c,
            angle,
            lines,
            mirror,
        } => {
            let n = (*lines).max(1);
            for k in 0..n {
                let a = if *mirror {
                    angle + PI * k as f32 / n as f32
                } else {
                    angle + TAU * k as f32 / n as f32
                };
                let d = Pt::new(a.cos(), a.sin());
                let from = if *mirror { c.sub(d.mul(far)) } else { *c };
                line(
                    from,
                    c.add(d.mul(far)),
                    if k == 0 { strong } else { guide },
                    if k == 0 { 2.0 } else { 1.2 },
                );
            }
        }
        Ruler::Parallel { angle } => {
            let d = Pt::new(angle.cos(), angle.sin());
            let nrm = Pt::new(-d.y, d.x);
            let step = w.max(h) / 14.0;
            for k in -14..=14 {
                let o = center.add(nrm.mul(step * k as f32));
                line(o.sub(d.mul(far)), o.add(d.mul(far)), guide, 1.0);
            }
        }
        Ruler::Focus { center: c } => {
            for k in 0..36 {
                let a = TAU * k as f32 / 36.0;
                line(*c, c.add(Pt::new(a.cos(), a.sin()).mul(far)), guide, 1.0);
            }
        }
        Ruler::Concentric { center: c } => {
            let s = v.to_screen(c.x, c.y);
            let step = w.max(h) / 12.0 * v.zoom;
            for k in 1..=14 {
                painter.circle_stroke(s, step * k as f32, Stroke::new(1.0, guide));
            }
        }
        Ruler::Straight { a, b } => line(*a, *b, strong, 2.0),
    }
    if active {
        for (i, hp) in handles(r, w, h).iter().enumerate() {
            let s = v.to_screen(hp.x, hp.y);
            let big = hovered == Some(i);
            painter.circle(
                s,
                if big { 9.0 } else { 7.0 },
                Color32::from_rgb(255, 255, 255),
                Stroke::new(2.0, strong),
            );
            painter.circle_filled(s, 2.5, strong);
        }
    }
}

impl InkApp {
    /// Snap a brush sample to the page's ruler (non-symmetry rulers).
    pub(super) fn snap_sample(&mut self, smp: &mut Sample) {
        let (Some(snap), Some(b)) = (&mut self.ruler_snap, &self.book) else {
            return;
        };
        let Some(r) = &b.page().ruler else { return };
        let q = snap.snap(r, Pt::new(smp.x, smp.y));
        smp.x = q.x;
        smp.y = q.y;
    }

    /// Called at stroke start: set up snapping or symmetry copies for this stroke.
    pub(super) fn begin_ruler_stroke(&mut self, st: &mut BrushStroke, alt: bool) {
        self.ruler_snap = None;
        let Some(b) = &self.book else { return };
        let page = b.page();
        // Holding Alt draws freehand once (the ruler stays).
        let Some(r) = page.ruler.as_ref().filter(|_| page.ruler_snap && !alt) else {
            return;
        };
        let t = r.symmetry_transforms();
        if matches!(r, Ruler::Symmetry { .. }) {
            st.set_mirrors(t.into_iter().map(|a| a.0).collect());
        } else {
            self.ruler_snap = Some(RulerSnap::new());
        }
    }

    pub(super) fn ruler_event(&mut self, ev: PenEvent) {
        let Some(b) = self.book.as_mut() else { return };
        let page = b.page_mut();
        let (w, h) = page.canvas();
        let (w, h) = (w as f32, h as f32);
        let (s, phase) = match ev {
            PenEvent::Down(s) => (s, 0),
            PenEvent::Move(s) => (s, 1),
            PenEvent::Up(s) => (s, 2),
        };
        let c = self.view.to_canvas(s.pos);
        let pt = Pt::new(c.0, c.1);
        match phase {
            0 => {
                let hit = page.ruler.as_ref().and_then(|r| {
                    handles(r, w, h)
                        .iter()
                        .enumerate()
                        .map(|(i, q)| (i, (self.view.to_screen(q.x, q.y) - s.pos).length()))
                        .filter(|(_, d)| *d < 14.0)
                        .min_by(|a, b| a.1.total_cmp(&b.1))
                        .map(|(i, _)| i)
                });
                match hit {
                    Some(i) => self.ruler_drag = Some(i),
                    None if page.ruler.is_none() => {
                        // First click places a new ruler centered where the user clicked.
                        let mut r = self.ruler_new.make(w, h);
                        match &mut r {
                            Ruler::Symmetry { center, .. }
                            | Ruler::Focus { center }
                            | Ruler::Concentric { center } => *center = pt,
                            Ruler::Perspective { vps } if vps.len() == 1 => vps[0] = pt,
                            _ => {}
                        }
                        page.ruler = Some(r);
                        page.ruler_snap = true;
                        page.touch();
                    }
                    None => {}
                }
            }
            1 => {
                if let (Some(i), Some(r)) = (self.ruler_drag, page.ruler.as_mut()) {
                    set_handle(r, i, pt, w, h);
                }
            }
            _ => {
                if self.ruler_drag.take().is_some() {
                    page.touch();
                }
            }
        }
    }

    /// Ruler kinds and settings (Sub tool palette).
    pub(super) fn ruler_panel(&mut self, ui: &mut Ui) {
        let p = palette(ui);
        ui.label(
            RichText::new(tr(
                "Pick a ruler, then click the page. Drag the handles to adjust it.",
            ))
            .text_style(text::caption())
            .color(p.text_muted),
        );
        ui.add_space(4.0);
        let current = self
            .book
            .as_ref()
            .and_then(|b| b.page().ruler.as_ref())
            .and_then(RulerKind::of);
        for k in RulerKind::ALL {
            let sel = self.ruler_new == k;
            let resp = list_row(ui, Some(k.icon()), tr(k.name()), sel || current == Some(k));
            if resp.clicked() {
                self.ruler_new = k;
                if let Some(b) = &mut self.book {
                    let page = b.page_mut();
                    let (w, h) = page.canvas();
                    page.ruler = Some(k.make(w as f32, h as f32));
                    page.ruler_snap = true;
                    page.touch();
                }
            }
        }
    }

    /// Details of the page's ruler (Tool property palette).
    pub(super) fn ruler_props(&mut self, ui: &mut Ui) {
        let p = palette(ui);
        let Some(b) = &mut self.book else { return };
        let page = b.page_mut();
        let mut changed = false;
        match &mut page.ruler {
            None => {
                ui.label(RichText::new(tr("This page has no ruler.")).color(p.text_muted));
            }
            Some(r) => {
                if let Some(k) = RulerKind::of(r) {
                    ui.label(RichText::new(tr(k.name())).text_style(text::body_strong()));
                }
                changed |= ui
                    .checkbox(&mut page.ruler_snap, tr("Snap strokes to the ruler"))
                    .changed();
                match r {
                    Ruler::Symmetry {
                        lines,
                        mirror,
                        angle,
                        ..
                    } => {
                        let mut n = *lines as f32;
                        if slider_field(ui, tr("Axes"), &mut n, 1.0..=16.0, "", None).changed() {
                            *lines = n.round() as u32;
                            changed = true;
                        }
                        changed |= ui
                            .checkbox(mirror, tr("Mirror (off = rotate copies)"))
                            .changed();
                        let mut deg = angle.to_degrees();
                        if slider_field(ui, tr("Angle"), &mut deg, -180.0..=180.0, "°", None)
                            .changed()
                        {
                            *angle = deg.to_radians();
                            changed = true;
                        }
                    }
                    Ruler::Parallel { angle } => {
                        let mut deg = angle.to_degrees();
                        if slider_field(ui, tr("Angle"), &mut deg, -180.0..=180.0, "°", None)
                            .changed()
                        {
                            *angle = deg.to_radians();
                            changed = true;
                        }
                    }
                    Ruler::Perspective { vps } => {
                        ui.label(
                            RichText::new(trf(
                                "{n} vanishing points · the orange line is the horizon",
                                &[("n", vps.len().to_string())],
                            ))
                            .text_style(text::caption())
                            .color(p.text_muted),
                        );
                    }
                    _ => {}
                }
                ui.add_space(4.0);
                if ui
                    .add(
                        Button::new(tr("Remove ruler"))
                            .icon(icons::TRASH)
                            .size(ButtonSize::Sm),
                    )
                    .clicked()
                {
                    page.ruler = None;
                    changed = true;
                }
            }
        }
        if changed {
            page.touch();
        }
        ui.label(
            RichText::new(tr(
                "Hold Alt while drawing to ignore the ruler for one stroke.",
            ))
            .text_style(text::caption())
            .color(p.text_muted),
        );
    }

    /// Guide overlay on the canvas.
    pub(super) fn paint_ruler(&self, painter: &egui::Painter, pointer: Option<Pos2>) {
        let Some(b) = &self.book else { return };
        let page = b.page();
        let Some(r) = &page.ruler else { return };
        let (w, h) = page.canvas();
        let (w, h) = (w as f32, h as f32);
        let active = self.tool == Tool::Ruler;
        if !active && !page.ruler_snap {
            return;
        }
        let hovered = pointer.and_then(|ptr| {
            handles(r, w, h)
                .iter()
                .position(|q| (self.view.to_screen(q.x, q.y) - ptr).length() < 14.0)
        });
        paint(
            painter,
            &self.view,
            r,
            w,
            h,
            active,
            self.ruler_drag.or(hovered),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_roundtrip_and_handles_move() {
        for k in RulerKind::ALL {
            let r = k.make(1000.0, 1400.0);
            assert_eq!(RulerKind::of(&r), Some(k));
            assert!(!handles(&r, 1000.0, 1400.0).is_empty());
        }
        let mut r = RulerKind::TwoPoint.make(1000.0, 1400.0);
        set_handle(&mut r, 0, Pt::new(-200.0, 300.0), 1000.0, 1400.0);
        let Ruler::Perspective { vps } = &r else {
            panic!()
        };
        assert_eq!(vps[1].y, 300.0, "horizon stays level");
        let mut s = RulerKind::Mirror.make(1000.0, 1400.0);
        set_handle(
            &mut s,
            1,
            Pt::new(500.0 + 100.0, 700.0 + 1.0),
            1000.0,
            1400.0,
        );
        let Ruler::Symmetry { angle, .. } = s else {
            panic!()
        };
        assert!(angle.abs() < 1e-4, "snaps to 0°: {angle}");
    }
}
