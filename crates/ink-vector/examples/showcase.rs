//! Showcase + performance check on a print-size manga page.
//!
//!   cargo run --release -p ink-vector --example showcase -- [out.png]
//!
//! Builds a B4 page at 600 dpi (6071 × 8598 px) and uses every engine feature:
//! panel borders, G-pen strokes with pressure and taper, perspective and focus-line rulers,
//! a vector layer cleaned with "erase up to intersection", gap-closing fill on a flats layer
//! (referring to the line art), and a 60 lpi screentone clipped to the flats.
//! Prints timings for each step, then writes a downscaled PNG preview.

use ink_core::{blend::BlendMode, Document, History, Layer, Rgba};
use ink_raster::fill::{fill_region, FillOptions, Refer};
use ink_raster::tone::{apply_tone, DotShape, ToneSettings};
use ink_raster::{BrushParams, Sample, Stroke};
use ink_vector::{EraseMode, Pt, Ruler, RulerSnap, VStroke, VectorLayer};
use std::time::Instant;

fn timed<T>(label: &str, f: impl FnOnce() -> T) -> T {
    let t = Instant::now();
    let r = f();
    println!("{label:<52} {:>8.1} ms", t.elapsed().as_secs_f64() * 1000.0);
    r
}

/// Pressure that rises and falls along the stroke, like a real pen.
fn pen_pressure(t: f32) -> f32 {
    (0.35 + 0.65 * (std::f32::consts::PI * t).sin()).clamp(0.0, 1.0)
}

fn draw(doc: &mut Document, layer: ink_core::LayerId, h: &mut History, brush: &BrushParams, pts: &[Pt], ruler: Option<&Ruler>) -> usize {
    let mut st = Stroke::begin(doc, layer, brush.clone(), Rgba::BLACK);
    let mut snap = RulerSnap::new();
    let n = pts.len().max(2) - 1;
    for (i, p) in pts.iter().enumerate() {
        let q = match ruler {
            Some(r) => snap.snap(r, *p),
            None => *p,
        };
        st.add(doc, Sample::new(q.x, q.y, pen_pressure(i as f32 / n as f32)));
    }
    h.push(st.end(doc));
    pts.len()
}

/// A wobbly hand-drawn path from a to b with `n` samples.
fn hand(a: Pt, b: Pt, n: usize, wobble: f32, seed: u32) -> Vec<Pt> {
    let dir = b.sub(a);
    let nrm = Pt::new(-dir.y, dir.x).norm();
    (0..=n)
        .map(|i| {
            let t = i as f32 / n as f32;
            let w = ((t * 23.0 + seed as f32).sin() * 0.6 + (t * 57.0 + seed as f32 * 1.7).sin() * 0.4) * wobble;
            a.lerp(b, t).add(nrm.mul(w))
        })
        .collect()
}

/// Keep the longest run of points inside the rectangle (what a panel folder's mask does).
fn clip(pts: Vec<Pt>, lo: Pt, hi: Pt) -> Vec<Pt> {
    let inside = |p: &Pt| p.x > lo.x && p.y > lo.y && p.x < hi.x && p.y < hi.y;
    let mut best: Vec<Pt> = Vec::new();
    let mut cur: Vec<Pt> = Vec::new();
    for p in pts {
        if inside(&p) {
            cur.push(p);
        } else if !cur.is_empty() {
            if cur.len() > best.len() {
                best = std::mem::take(&mut cur);
            }
            cur.clear();
        }
    }
    if cur.len() > best.len() {
        best = cur;
    }
    best
}

fn main() {
    let out = std::env::args().nth(1).unwrap_or_else(|| "replica/showcase.png".into());
    let (w, h) = (6071u32, 8598u32); // B4 at 600 dpi
    println!("Page {w} x {h} px ({:.1} MP) at 600 dpi, {} threads\n", (w * h) as f64 / 1e6, rayon::current_num_threads());
    let mut doc = Document::new(w, h, 600);
    let mut hist = History::default();

    let mut flats_l = Layer::new("Flats");
    flats_l.props.blend = BlendMode::Normal;
    let flats = doc.push_layer(flats_l);
    let mut tone_l = Layer::new("Tone");
    tone_l.props.clip_below = true;
    let tone = doc.push_layer(tone_l);
    let mut ink_l = Layer::new("Ink");
    ink_l.props.reference = true;
    let ink = doc.push_layer(ink_l);
    let mut frames_l = Layer::new("Panels");
    frames_l.props.reference = true;
    let frames = doc.push_layer(frames_l);

    // ---- Panel borders (straight ruler, mapping pen) ----------------------------------------
    let m = 420.0;
    let (l, r, t, b) = (m, w as f32 - m, m, h as f32 - m);
    let y1 = t + (b - t) * 0.36;
    let y2 = t + (b - t) * 0.40;
    let xm = l + (r - l) * 0.55;
    let mut border = BrushParams::mapping_pen(14.0);
    border.taper_start = 0.0;
    border.taper_end = 0.0;
    border.min_size = 1.0;
    let rects = [
        (Pt::new(l, t), Pt::new(r, y1)),
        (Pt::new(l, y2), Pt::new(xm - 50.0, b)),
        (Pt::new(xm + 50.0, y2), Pt::new(r, b)),
    ];
    let samples: usize = timed("Panel borders (12 straight-ruler strokes)", || {
        let mut n = 0;
        for (a, c) in rects {
            let corners = [a, Pt::new(c.x, a.y), c, Pt::new(a.x, c.y), a];
            for k in 0..4 {
                let ruler = Ruler::Straight { a: corners[k], b: corners[k + 1] };
                n += draw(&mut doc, frames, &mut hist, &border, &hand(corners[k], corners[k + 1], 400, 6.0, k as u32), Some(&ruler));
            }
        }
        n
    });
    let _ = samples;

    // ---- Panel 1: focus lines toward an impact point ----------------------------------------
    let focus = Pt::new(l + (r - l) * 0.68, t + (y1 - t) * 0.45);
    let mut speed = BrushParams::g_pen(10.0);
    speed.taper_start = 30.0;
    speed.taper_end = 400.0;
    let n_focus = 140;
    let t0 = Instant::now();
    let mut focus_samples = 0;
    for i in 0..n_focus {
        let a = i as f32 / n_focus as f32 * std::f32::consts::TAU + (i as f32 * 1.3).sin() * 0.02;
        let dir = Pt::new(a.cos(), a.sin());
        let start = focus.add(dir.mul(3600.0));
        let end = focus.add(dir.mul(700.0 + (i as f32 * 7.7).sin().abs() * 600.0));
        let ruler = Ruler::Focus { center: focus };
        let path = clip(hand(start, end, 240, 10.0, i), Pt::new(l + 20.0, t + 20.0), Pt::new(r - 20.0, y1 - 20.0));
        if path.len() > 2 {
            focus_samples += draw(&mut doc, ink, &mut hist, &speed, &path, Some(&ruler));
        }
    }
    let el = t0.elapsed().as_secs_f64() * 1000.0;
    println!("{:<52} {:>8.1} ms  ({:.3} ms per pen sample)", format!("Focus lines: {n_focus} G-pen strokes, {focus_samples} samples"), el, el / focus_samples as f64);

    // ---- Panel 2: two-point perspective street ---------------------------------------------
    let p2 = (Pt::new(l, y2), Pt::new(xm - 50.0, b));
    let horizon = p2.0.y + (p2.1.y - p2.0.y) * 0.35;
    let vps = vec![Pt::new(p2.0.x - 2500.0, horizon), Pt::new(p2.1.x + 2500.0, horizon)];
    let persp = Ruler::Perspective { vps: vps.clone() };
    let mut pen = BrushParams::g_pen(9.0);
    pen.taper_start = 40.0;
    pen.taper_end = 60.0;
    let (plo, phi) = (Pt::new(p2.0.x + 20.0, p2.0.y + 20.0), Pt::new(p2.1.x - 20.0, p2.1.y - 20.0));
    timed("Perspective: building + street, snapped to 2 vanishing points", || {
        // Building corner and its edges toward both vanishing points.
        let corner_x = p2.0.x + (p2.1.x - p2.0.x) * 0.45;
        let (top, bottom) = (horizon - 1500.0, horizon + 1700.0);
        for (k, y) in [top, top + 500.0, top + 1000.0, horizon + 400.0, horizon + 900.0, bottom].into_iter().enumerate() {
            for (j, vp) in vps.iter().enumerate() {
                let a = Pt::new(corner_x, y);
                let b = a.lerp(*vp, 0.9);
                let path = clip(hand(a, b, 200, 10.0, (k * 2 + j) as u32), plo, phi);
                if path.len() > 2 {
                    draw(&mut doc, ink, &mut hist, &pen, &path, Some(&persp));
                }
            }
        }
        // Verticals: the corner and window columns on both faces.
        for i in 0..9 {
            let f = i as f32 / 8.0;
            let x = corner_x + (f - 0.5) * (p2.1.x - p2.0.x) * 0.9;
            let a = Pt::new(x, top - 400.0 + (f - 0.5).abs() * 900.0);
            let b = Pt::new(x + 3.0, bottom - (f - 0.5).abs() * 900.0);
            let path = clip(hand(a, b, 200, 10.0, 90 + i), plo, phi);
            if path.len() > 2 {
                draw(&mut doc, ink, &mut hist, &pen, &path, Some(&persp));
            }
        }
    });

    // ---- Panel 3: a vector cross-hatch, cleaned with "erase up to intersection" -------------
    let p3 = (Pt::new(xm + 50.0, y2), Pt::new(r, b));
    let mut vec_layer = VectorLayer::new();
    let mut vpen = BrushParams::g_pen(8.0);
    vpen.stabilization = 20;
    timed("Vector: 24 strokes recorded with stabilization", || {
        for i in 0..12 {
            for (k, (a, c)) in [
                (Pt::new(p3.0.x + 80.0, p3.0.y + 300.0 + i as f32 * 260.0), Pt::new(p3.1.x - 80.0, p3.0.y + 900.0 + i as f32 * 260.0)),
                (Pt::new(p3.0.x + 200.0 + i as f32 * 180.0, p3.0.y + 150.0), Pt::new(p3.0.x + 100.0 + i as f32 * 180.0, p3.1.y - 400.0)),
            ]
            .into_iter()
            .enumerate()
            {
                let mut scratch = Document::new(w, h, 600);
                let sid = scratch.push_layer(Layer::new("rec"));
                let mut st = Stroke::begin(&scratch, sid, vpen.clone(), Rgba::BLACK);
                for (j, p) in hand(a, c, 200, 14.0, (i * 2 + k) as u32).iter().enumerate() {
                    st.add(&mut scratch, Sample::new(p.x, p.y, pen_pressure(j as f32 / 200.0)));
                }
                let (_, path, len) = st.end_with_path(&mut scratch);
                vec_layer.push(VStroke::from_path(0, &path, len, &vpen, Rgba::BLACK));
            }
        }
    });
    let erased = timed("Vector eraser: 30 × erase up to intersection", || {
        let mut n = 0;
        for i in 0..30 {
            let p = Pt::new(p3.0.x + 400.0 + (i % 6) as f32 * 300.0, p3.0.y + 900.0 + (i / 6) as f32 * 700.0);
            n += vec_layer.erase(p, 25.0, EraseMode::UpToIntersection) as usize;
        }
        n
    });
    println!("  {} erase hits, {} strokes remain", erased, vec_layer.strokes.len());
    let vraster = timed("Vector layer re-render", || vec_layer.render(w, h, 600));
    let vid = doc.push_layer(Layer { props: ink_core::LayerProps::named("Vector ink"), raster: vraster });
    doc.layer_mut(vid).unwrap().props.reference = true;

    // ---- Flats: gap-closing fill referring to line art ---------------------------------------
    let opts = FillOptions { refer: Refer::References, tolerance: 40, gap: 6, expand: 2 };
    let (fw, fh, region) = timed("Fill region (refer to line art, close gap 6 px)", || fill_region(&doc, flats, (l + 300.0) as i32, (y2 + 300.0) as i32, &opts).unwrap());
    let flat_color = Rgba::from_straight(235, 235, 235, 255);
    let e = timed("Fill apply to flats layer", || ink_raster::fill::apply_region(&mut doc, flats, fw, fh, &region, flat_color, "Fill").unwrap());
    hist.push(e);

    // ---- 60 lpi 25% tone, clipped to the flats ------------------------------------------------
    let ts = ToneSettings { lpi: 60.0, density: 0.25, angle_deg: 45.0, shape: DotShape::Circle, color: Rgba::BLACK };
    let e = timed("Screentone 60 lpi 25% over the filled area (1-bit)", || apply_tone(&mut doc, tone, &region, ts, true).unwrap());
    hist.push(e);

    // ---- Composite, undo, export ---------------------------------------------------------------
    doc.mark_all_dirty();
    let keys = doc.take_dirty();
    timed(&format!("Full-page composite ({} tiles, parallel)", keys.len()), || doc.composite_tiles(&keys, true));
    let one_tile = vec![(5, 10)];
    let t1 = Instant::now();
    for _ in 0..100 {
        std::hint::black_box(doc.composite_tiles(&one_tile, true));
    }
    println!("{:<52} {:>8.3} ms", "Recomposite 1 dirty tile (what a pen sample costs)", t1.elapsed().as_secs_f64() * 1000.0 / 100.0);
    timed("Undo (tone) + redo", || {
        hist.undo(&mut doc);
        hist.redo(&mut doc);
    });
    let rgba = timed("Flatten whole page to RGBA8 for export", || doc.flatten_rgba8());
    println!("\nUndo history: {} steps", hist.len());

    // Downscale 4× (box filter) for a viewable preview.
    let s = 4usize;
    let (pw, ph) = (w as usize / s, h as usize / s);
    let mut prev = vec![0u8; pw * ph * 3];
    for y in 0..ph {
        for x in 0..pw {
            let mut acc = [0u32; 3];
            for dy in 0..s {
                for dx in 0..s {
                    let i = ((y * s + dy) * w as usize + x * s + dx) * 4;
                    for c in 0..3 {
                        acc[c] += rgba[i + c] as u32;
                    }
                }
            }
            for c in 0..3 {
                prev[(y * pw + x) * 3 + c] = (acc[c] / (s * s) as u32) as u8;
            }
        }
    }
    let f = std::fs::File::create(&out).expect("create png");
    let mut enc = png::Encoder::new(std::io::BufWriter::new(f), pw as u32, ph as u32);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(&prev).unwrap();
    println!("wrote {out} ({pw} x {ph}, 1/4 scale)");

    // Full-resolution crop of the tone + ink so the dots are visible.
    let (cx, cy, cw, ch) = ((l + 150.0) as usize, (y2 + 150.0) as usize, 900usize, 600usize);
    let mut crop = vec![0u8; cw * ch * 3];
    for y in 0..ch {
        for x in 0..cw {
            let i = ((cy + y) * w as usize + cx + x) * 4;
            crop[(y * cw + x) * 3..(y * cw + x) * 3 + 3].copy_from_slice(&rgba[i..i + 3]);
        }
    }
    let crop_path = out.replace(".png", "-detail.png");
    let f = std::fs::File::create(&crop_path).expect("create png");
    let mut enc = png::Encoder::new(std::io::BufWriter::new(f), cw as u32, ch as u32);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(&crop).unwrap();
    println!("wrote {crop_path} (100% crop)");
}
