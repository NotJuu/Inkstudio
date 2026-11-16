//! Raster helpers for comic objects.

use ink_core::{Document, Layer, Raster, Rgba};
use ink_raster::{BrushParams, Sample, Stroke};
use ink_vector::Pt;

/// Fill a polygon (non-zero winding) with anti-aliasing: 4 sub-scanlines per pixel row and
/// exact fractional coverage at span ends.
pub fn fill_polygon(r: &mut Raster, poly: &[Pt], color: Rgba) {
    if poly.len() < 3 {
        return;
    }
    let (mut y0, mut y1) = (f32::MAX, f32::MIN);
    let (mut x0, mut x1) = (f32::MAX, f32::MIN);
    for p in poly {
        y0 = y0.min(p.y);
        y1 = y1.max(p.y);
        x0 = x0.min(p.x);
        x1 = x1.max(p.x);
    }
    let (py0, py1) = (y0.floor() as i32, y1.ceil() as i32);
    let (px0, px1) = (x0.floor() as i32 - 1, x1.ceil() as i32 + 1);
    let w = (px1 - px0 + 1).max(1) as usize;
    let mut cov = vec![0f32; w];
    const SUB: usize = 4;
    for py in py0..py1 {
        cov.iter_mut().for_each(|c| *c = 0.0);
        for s in 0..SUB {
            let sy = py as f32 + (s as f32 + 0.5) / SUB as f32;
            // Crossings with winding direction.
            let mut xs: Vec<(f32, i32)> = Vec::new();
            for i in 0..poly.len() {
                let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
                if (a.y <= sy) != (b.y <= sy) {
                    let t = (sy - a.y) / (b.y - a.y);
                    xs.push((a.x + (b.x - a.x) * t, if b.y > a.y { 1 } else { -1 }));
                }
            }
            xs.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut wind = 0;
            for k in 0..xs.len() {
                let before = wind;
                wind += xs[k].1;
                if before == 0 && wind != 0 {
                    // span starts at xs[k]; find where it ends
                    let start = xs[k].0;
                    let mut wnd = wind;
                    let mut end = start;
                    for x in xs.iter().skip(k + 1) {
                        wnd += x.1;
                        if wnd == 0 {
                            end = x.0;
                            break;
                        }
                    }
                    add_span(&mut cov, px0, start, end, 1.0 / SUB as f32);
                }
            }
        }
        for (i, c) in cov.iter().enumerate() {
            if *c > 0.002 {
                let x = px0 + i as i32;
                let a = (c.min(1.0) * 255.0 + 0.5) as u8;
                let dst = r.get(x, py);
                r.set(x, py, color.scale(a).over(dst));
            }
        }
    }
}

fn add_span(cov: &mut [f32], px0: i32, a: f32, b: f32, weight: f32) {
    if b <= a {
        return;
    }
    let (ia, ib) = (a.floor() as i32, b.floor() as i32);
    let idx = |x: i32| (x - px0) as usize;
    if ia == ib {
        if let Some(c) = cov.get_mut(idx(ia)) {
            *c += (b - a) * weight;
        }
        return;
    }
    if let Some(c) = cov.get_mut(idx(ia)) {
        *c += (ia as f32 + 1.0 - a) * weight;
    }
    for x in ia + 1..ib {
        if let Some(c) = cov.get_mut(idx(x)) {
            *c += weight;
        }
    }
    if let Some(c) = cov.get_mut(idx(ib)) {
        *c += (b - ib as f32) * weight;
    }
}

/// Pen used for balloon and panel outlines: uniform, crisp.
pub fn outline_pen(width: f32) -> BrushParams {
    BrushParams {
        name: "Outline".into(),
        min_size: 1.0,
        size_curve: None,
        taper_start: 0.0,
        taper_end: 0.0,
        stabilization: 0,
        post_correction: false,
        spacing: 0.05,
        ..BrushParams::g_pen(width)
    }
}

/// Stroke a polyline (closed if `closed`) into the raster with the brush engine.
pub fn stroke_polyline(r: &mut Raster, pts: &[Pt], closed: bool, pen: &BrushParams, color: Rgba, canvas: (u32, u32)) {
    if pts.len() < 2 {
        return;
    }
    let mut doc = Document::new(canvas.0, canvas.1, 600);
    let id = doc.push_layer(Layer { props: ink_core::LayerProps::named("tmp"), raster: std::mem::take(r) });
    let mut s = Stroke::begin(&doc, id, pen.clone(), color);
    for p in pts {
        s.add(&mut doc, Sample::new(p.x, p.y, 1.0));
    }
    if closed {
        s.add(&mut doc, Sample::new(pts[0].x, pts[0].y, 1.0));
        if pts.len() > 1 {
            s.add(&mut doc, Sample::new(pts[1].x, pts[1].y, 1.0));
        }
    }
    let _ = s.end(&mut doc);
    *r = doc.remove_layer(id).map(|l| l.raster).unwrap_or_default();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fills_square_with_exact_coverage() {
        let mut r = Raster::new();
        let sq = [Pt::new(10.0, 10.0), Pt::new(20.5, 10.0), Pt::new(20.5, 20.0), Pt::new(10.0, 20.0)];
        fill_polygon(&mut r, &sq, Rgba::BLACK);
        assert_eq!(r.get(15, 15).a(), 255);
        assert_eq!(r.get(9, 15).a(), 0);
        let half = r.get(20, 15).a();
        assert!((120..=135).contains(&half), "half pixel coverage {half}");
    }
}
