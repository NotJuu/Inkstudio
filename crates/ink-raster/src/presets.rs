//! The Inkstudio brush catalog: manga first, but covering sketching, painting and effects.
//!
//! Sizes are in pixels for a 600 dpi page (10 px ≈ 0.42 mm). Every preset is an ordinary
//! `BrushParams`, so users can duplicate and tweak any of them (sub tools, S04/S05).
//! All presets are original Inkstudio settings.

use crate::brush::{BrushParams, Coverage};
use crate::curve::Curve;
use crate::tip::{Grain, Scatter, TipShape};
use crate::tone::{DotShape, ToneSettings};
use ink_core::Rgba;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Category {
    Ink,
    Pencil,
    Paint,
    Airbrush,
    Eraser,
    Effect,
    Decoration,
    Tone,
}

impl Category {
    pub const ALL: [Category; 8] = [
        Category::Ink,
        Category::Pencil,
        Category::Paint,
        Category::Airbrush,
        Category::Eraser,
        Category::Effect,
        Category::Decoration,
        Category::Tone,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Category::Ink => "Ink pens",
            Category::Pencil => "Pencils",
            Category::Paint => "Paint brushes",
            Category::Airbrush => "Airbrush & spray",
            Category::Eraser => "Erasers",
            Category::Effect => "Manga effects",
            Category::Decoration => "Decoration",
            Category::Tone => "Tone pens",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Preset {
    /// Stable identifier (files and shortcuts refer to it).
    pub id: &'static str,
    pub category: Category,
    /// What it is for, shown as the sub tool tooltip.
    pub use_for: &'static str,
    pub params: BrushParams,
}

/// Plain round pen without taper: the base most presets start from.
fn base(name: &str, size: f32) -> BrushParams {
    BrushParams { name: name.into(), taper_start: 0.0, taper_end: 0.0, ..BrushParams::g_pen(size) }
}

fn grain(scale: f32, strength: f32, contrast: f32) -> Option<Grain> {
    Some(Grain { scale, strength, contrast })
}

fn scatter(count: u8, spread: f32, size: f32, size_jitter: f32, angle_jitter: f32, opacity_jitter: f32) -> Option<Scatter> {
    Some(Scatter { count, spread, size, size_jitter, angle_jitter, opacity_jitter })
}

fn tone(lpi: f32, density: f32, angle_deg: f32, shape: DotShape) -> Option<ToneSettings> {
    Some(ToneSettings { lpi, density, angle_deg, shape, color: Rgba::BLACK })
}

fn p(id: &'static str, category: Category, use_for: &'static str, params: BrushParams) -> Preset {
    Preset { id, category, use_for, params }
}

/// Every built-in brush, grouped by category in display order.
pub fn catalog() -> Vec<Preset> {
    use Category::*;
    vec![
        // ---- Ink pens ---------------------------------------------------------------------------
        p("g_pen", Ink, "Main inking: strong thick–thin with pressure", BrushParams::g_pen(10.0)),
        p("mapping_pen", Ink, "Fine detail, faces, hatching", BrushParams::mapping_pen(5.0)),
        p("maru_pen", Ink, "Even fine lines for backgrounds and eyes", BrushParams {
            name: "Round pen".into(),
            min_size: 0.35,
            size_curve: Some(Curve::linear()),
            taper_start: 8.0,
            taper_end: 12.0,
            ..BrushParams::g_pen(4.0)
        }),
        p("kabura_pen", Ink, "Between G-pen and round pen: crisp, moderate swell", BrushParams {
            name: "Turnip pen".into(),
            min_size: 0.25,
            size_curve: Some(Curve::linear()),
            ..BrushParams::g_pen(7.0)
        }),
        p("brush_pen", Ink, "Expressive outlines: thins when you draw fast", BrushParams {
            name: "Brush pen".into(),
            min_size: 0.03,
            size_curve: Some(Curve::gamma(1.6)),
            speed_size: 0.45,
            taper_start: 60.0,
            taper_end: 120.0,
            roundness: 0.85,
            angle_deg: 30.0,
            follow_direction: true,
            stabilization: 8,
            ..BrushParams::g_pen(24.0)
        }),
        p("sumi_brush", Ink, "Ink brush with dry edges for bold strokes and effects", BrushParams {
            name: "Sumi brush".into(),
            min_size: 0.05,
            size_curve: Some(Curve::gamma(1.4)),
            speed_size: 0.6,
            taper_start: 80.0,
            taper_end: 200.0,
            grain: grain(2.5, 0.45, 2.2),
            stabilization: 6,
            ..BrushParams::g_pen(40.0)
        }),
        p("calligraphy", Ink, "Flat nib at a fixed angle: lettering, sound effects", BrushParams {
            name: "Calligraphy nib".into(),
            min_size: 0.6,
            roundness: 0.22,
            angle_deg: 40.0,
            taper_start: 0.0,
            taper_end: 0.0,
            ..BrushParams::g_pen(22.0)
        }),
        p("fineliner", Ink, "Uniform technical line: panel borders, mechanical parts", BrushParams {
            min_size: 1.0,
            size_curve: None,
            stabilization: 6,
            ..base("Fineliner", 5.0)
        }),
        p("marker", Ink, "Chisel marker for flat, bold shapes", BrushParams {
            min_size: 1.0,
            size_curve: None,
            hardness: 0.85,
            roundness: 0.55,
            angle_deg: 20.0,
            ..base("Marker", 22.0)
        }),
        p("dry_ink", Ink, "G-pen with a rough, dry-ink edge", BrushParams {
            name: "Dry ink pen".into(),
            grain: grain(2.0, 0.5, 2.0),
            ..BrushParams::g_pen(12.0)
        }),
        p("speed_line_pen", Ink, "Long, very smooth lines that fade out: speed and focus lines", BrushParams {
            name: "Effect-line pen".into(),
            min_size: 0.05,
            taper_start: 10.0,
            taper_end: 400.0,
            stabilization: 30,
            ..BrushParams::g_pen(8.0)
        }),
        // ---- Pencils ----------------------------------------------------------------------------
        p("pencil_hb", Pencil, "Everyday sketching with paper texture", BrushParams {
            grain: grain(2.0, 0.55, 1.8),
            ..BrushParams { name: "HB pencil".into(), ..BrushParams::pencil(6.0) }
        }),
        p("pencil_2b", Pencil, "Soft, dark sketch lines; shade with low pressure", BrushParams {
            name: "Soft pencil 2B".into(),
            hardness: 0.6,
            min_size: 0.5,
            grain: grain(2.4, 0.65, 2.0),
            ..BrushParams::pencil(10.0)
        }),
        p("mech_pencil", Pencil, "Thin, even construction lines", BrushParams {
            name: "Mechanical pencil".into(),
            min_size: 0.85,
            hardness: 0.9,
            grain: grain(1.6, 0.3, 1.5),
            ..BrushParams::pencil(4.0)
        }),
        p("rough_sketch", Pencil, "Light, loose layout sketch (use with a blue color)", BrushParams {
            name: "Layout pencil".into(),
            opacity: 0.55,
            grain: grain(2.0, 0.4, 1.6),
            stabilization: 0,
            ..BrushParams::pencil(8.0)
        }),
        p("charcoal", Pencil, "Charcoal: grainy, angled, for value studies", BrushParams {
            name: "Charcoal".into(),
            hardness: 0.5,
            roundness: 0.6,
            follow_direction: true,
            angle_deg: 90.0,
            grain: grain(4.0, 0.8, 2.5),
            min_size: 0.6,
            ..BrushParams::pencil(30.0)
        }),
        p("colored_pencil", Pencil, "Colored pencil with visible paper tooth", BrushParams {
            name: "Colored pencil".into(),
            grain: grain(2.2, 0.55, 2.2),
            ..BrushParams::pencil(10.0)
        }),
        p("crayon", Pencil, "Waxy crayon: very rough edges", BrushParams {
            name: "Crayon".into(),
            hardness: 0.7,
            grain: grain(3.0, 0.85, 3.0),
            min_size: 0.7,
            ..BrushParams::pencil(18.0)
        }),
        // ---- Paint brushes ----------------------------------------------------------------------
        p("round_brush", Paint, "General painting: size and opacity follow pressure", BrushParams {
            name: "Round brush".into(),
            min_size: 0.3,
            hardness: 0.8,
            opacity_curve: Some(Curve::gamma(0.8)),
            spacing: 0.1,
            ..BrushParams::g_pen(30.0)
        }),
        p("flat_brush", Paint, "Flat brush: broad side strokes, crisp edges", BrushParams {
            name: "Flat brush".into(),
            min_size: 0.6,
            roundness: 0.3,
            angle_deg: 90.0,
            follow_direction: true,
            hardness: 0.9,
            taper_start: 0.0,
            taper_end: 0.0,
            ..BrushParams::g_pen(40.0)
        }),
        p("dry_brush", Paint, "Bristle marks along the stroke: fur, wood, rough fills", BrushParams {
            tip: TipShape::Hatch { lines: 6, width: 0.45 },
            follow_direction: true,
            grain: grain(1.5, 0.35, 1.8),
            min_size: 0.7,
            spacing: 0.05,
            ..base("Dry brush", 40.0)
        }),
        p("watercolor", Paint, "Transparent wash that builds up", BrushParams {
            name: "Watercolor wash".into(),
            coverage: Coverage::Accumulate,
            flow: 0.12,
            hardness: 0.25,
            min_size: 0.6,
            opacity: 0.6,
            grain: grain(6.0, 0.25, 1.5),
            spacing: 0.08,
            taper_start: 0.0,
            taper_end: 0.0,
            stabilization: 0,
            ..BrushParams::g_pen(70.0)
        }),
        p("gouache", Paint, "Opaque, slightly flat paint", BrushParams {
            min_size: 0.5,
            roundness: 0.7,
            follow_direction: true,
            angle_deg: 90.0,
            hardness: 0.92,
            ..base("Gouache", 34.0)
        }),
        p("flatting", Paint, "Hard, uniform brush for flats and touch-ups", BrushParams {
            min_size: 1.0,
            size_curve: None,
            stabilization: 0,
            ..base("Flatting brush", 40.0)
        }),
        // ---- Airbrush & spray --------------------------------------------------------------------
        p("airbrush_soft", Airbrush, "Soft gradients and glows", BrushParams { name: "Soft airbrush".into(), ..BrushParams::airbrush(160.0) }),
        p("airbrush_hard", Airbrush, "Airbrush with a firmer edge", BrushParams {
            name: "Hard airbrush".into(),
            hardness: 0.6,
            flow: 0.15,
            ..BrushParams::airbrush(80.0)
        }),
        p("spray", Airbrush, "Spray-can speckle", BrushParams {
            name: "Spray".into(),
            scatter: scatter(14, 1.0, 0.05, 0.5, 0.0, 0.5),
            spacing: 0.25,
            coverage: Coverage::Max,
            ..BrushParams::airbrush(70.0)
        }),
        p("grit", Airbrush, "Sand, dust and gritty texture", BrushParams {
            name: "Grit".into(),
            scatter: scatter(22, 1.0, 0.025, 0.6, 0.0, 0.3),
            spacing: 0.35,
            coverage: Coverage::Max,
            flow: 1.0,
            hardness: 1.0,
            ..BrushParams::airbrush(90.0)
        }),
        // ---- Erasers ------------------------------------------------------------------------------
        p("eraser_hard", Eraser, "Clean, hard-edged erasing", BrushParams { name: "Hard eraser".into(), ..BrushParams::eraser(30.0) }),
        p("eraser_soft", Eraser, "Soft edge for lightening", BrushParams {
            name: "Soft eraser".into(),
            hardness: 0.2,
            ..BrushParams::eraser(60.0)
        }),
        p("kneaded_eraser", Eraser, "Gently lifts pencil sketches", BrushParams {
            name: "Kneaded eraser".into(),
            coverage: Coverage::Accumulate,
            flow: 0.18,
            hardness: 0.3,
            ..BrushParams::eraser(50.0)
        }),
        p("rough_eraser", Eraser, "Erases with a textured, broken edge", BrushParams {
            name: "Rough eraser".into(),
            grain: grain(2.5, 0.75, 2.2),
            ..BrushParams::eraser(40.0)
        }),
        p("tone_scratch", Eraser, "Scratch lines into tone or black: highlights, shine", BrushParams {
            name: "Tone scratch".into(),
            pattern: tone(40.0, 0.5, 45.0, DotShape::Line),
            ..BrushParams::eraser(60.0)
        }),
        p("white_scratch", Eraser, "Thin tapered scratch lines (like a knife on screentone)", BrushParams {
            name: "Scratch knife".into(),
            paint: crate::brush::Paint::Erase,
            ..BrushParams::mapping_pen(4.0)
        }),
        // ---- Manga effects ------------------------------------------------------------------------
        p("speed_lines", Effect, "Bundle of parallel streaks in one stroke", BrushParams {
            tip: TipShape::Hatch { lines: 7, width: 0.22 },
            follow_direction: true,
            min_size: 1.0,
            size_curve: None,
            taper_start: 120.0,
            taper_end: 300.0,
            stabilization: 25,
            spacing: 0.03,
            ..base("Speed-line brush", 70.0)
        }),
        p("hair", Effect, "Hair strands: several tapered lines at once", BrushParams {
            tip: TipShape::Hatch { lines: 4, width: 0.3 },
            follow_direction: true,
            min_size: 0.3,
            taper_start: 20.0,
            taper_end: 150.0,
            spacing: 0.04,
            ..BrushParams { name: "Hair brush".into(), ..BrushParams::g_pen(20.0) }
        }),
        p("dashed", Effect, "Dashed line: stitches, motion paths, cut lines", BrushParams {
            min_size: 1.0,
            size_curve: None,
            dash: Some((2.5, 1.5)),
            ..base("Dashed line", 5.0)
        }),
        p("stitch", Effect, "Sewing stitches on clothes", BrushParams {
            min_size: 1.0,
            size_curve: None,
            roundness: 0.35,
            follow_direction: true,
            dash: Some((1.6, 1.4)),
            ..base("Stitch line", 8.0)
        }),
        p("rain", Effect, "Slanted rain streaks", BrushParams {
            roundness: 0.06,
            angle_deg: 75.0,
            scatter: scatter(4, 3.0, 0.7, 0.6, 0.0, 0.4),
            spacing: 0.8,
            min_size: 1.0,
            size_curve: None,
            ..base("Rain", 40.0)
        }),
        p("snow", Effect, "Snow, dust motes, bokeh dots (use white or a light color)", BrushParams {
            scatter: scatter(3, 3.0, 0.14, 0.7, 0.0, 0.4),
            spacing: 1.2,
            min_size: 1.0,
            size_curve: None,
            hardness: 0.7,
            ..base("Snow / bokeh", 50.0)
        }),
        p("smoke", Effect, "Clouds, smoke, steam", BrushParams {
            scatter: scatter(5, 1.2, 0.6, 0.5, 0.0, 0.5),
            coverage: Coverage::Accumulate,
            flow: 0.12,
            hardness: 0.0,
            spacing: 0.3,
            min_size: 1.0,
            size_curve: None,
            ..base("Cloud / smoke", 80.0)
        }),
        p("bubbles", Effect, "Bubbles and water drops", BrushParams {
            tip: TipShape::Ring { thickness: 0.18 },
            scatter: scatter(2, 2.0, 0.45, 0.7, 0.0, 0.0),
            spacing: 1.4,
            min_size: 1.0,
            size_curve: None,
            ..base("Bubbles", 50.0)
        }),
        p("grass", Effect, "Grass tufts and fur clumps", BrushParams {
            roundness: 0.1,
            angle_deg: 90.0,
            scatter: scatter(6, 1.5, 1.0, 0.6, 0.06, 0.0),
            spacing: 0.5,
            min_size: 1.0,
            size_curve: None,
            ..base("Grass", 36.0)
        }),
        p("leaves", Effect, "Foliage: scattered leaf shapes", BrushParams {
            roundness: 0.45,
            scatter: scatter(4, 2.0, 0.35, 0.4, 1.0, 0.2),
            spacing: 0.6,
            min_size: 1.0,
            size_curve: None,
            ..base("Leaves", 60.0)
        }),
        // ---- Decoration ---------------------------------------------------------------------------
        p("sparkle", Decoration, "Four-point sparkles for shine and magic", BrushParams {
            tip: TipShape::Star { points: 4, inner: 0.16 },
            scatter: scatter(2, 2.5, 0.55, 0.8, 0.08, 0.0),
            spacing: 1.2,
            min_size: 1.0,
            size_curve: None,
            ..base("Sparkles", 50.0)
        }),
        p("stars", Decoration, "Scattered five-point stars", BrushParams {
            tip: TipShape::Star { points: 5, inner: 0.45 },
            scatter: scatter(1, 2.0, 0.5, 0.6, 0.3, 0.0),
            spacing: 1.4,
            min_size: 1.0,
            size_curve: None,
            ..base("Stars", 50.0)
        }),
        p("hearts", Decoration, "Hearts for shoujo backgrounds and reactions", BrushParams {
            tip: TipShape::Heart,
            scatter: scatter(1, 2.0, 0.5, 0.6, 0.15, 0.0),
            spacing: 1.4,
            min_size: 1.0,
            size_curve: None,
            ..base("Hearts", 50.0)
        }),
        p("flowers", Decoration, "Flower background for romance scenes", BrushParams {
            tip: TipShape::Flower { petals: 5 },
            scatter: scatter(2, 2.2, 0.5, 0.6, 1.0, 0.0),
            spacing: 1.3,
            min_size: 1.0,
            size_curve: None,
            ..base("Flowers", 60.0)
        }),
        p("stipple", Decoration, "Stippled dots for shading by hand", BrushParams {
            scatter: scatter(6, 2.0, 0.12, 0.5, 0.0, 0.0),
            spacing: 0.8,
            min_size: 1.0,
            size_curve: None,
            ..base("Stipple", 40.0)
        }),
        // ---- Tone pens ----------------------------------------------------------------------------
        p("tone_pen_dots", Tone, "Paint screentone directly: 60 lpi, 20%", BrushParams {
            min_size: 1.0,
            size_curve: None,
            pattern: tone(60.0, 0.2, 45.0, DotShape::Circle),
            ..base("Tone pen 60 lpi 20%", 60.0)
        }),
        p("tone_pen_dark", Tone, "Darker tone for shadows: 60 lpi, 40%", BrushParams {
            min_size: 1.0,
            size_curve: None,
            pattern: tone(60.0, 0.4, 45.0, DotShape::Circle),
            ..base("Tone pen 60 lpi 40%", 60.0)
        }),
        p("tone_pen_lines", Tone, "Line tone: hatching texture, motion", BrushParams {
            min_size: 1.0,
            size_curve: None,
            pattern: tone(50.0, 0.3, 45.0, DotShape::Line),
            ..base("Line tone pen", 60.0)
        }),
        p("tone_pen_cross", Tone, "Cross-hatch tone for dark shading", BrushParams {
            min_size: 1.0,
            size_curve: None,
            pattern: tone(30.0, 0.25, 45.0, DotShape::Cross),
            ..base("Cross-hatch tone pen", 60.0)
        }),
        p("tone_pen_sand", Tone, "Sand/noise tone: ground, stone, grit", BrushParams {
            min_size: 1.0,
            size_curve: None,
            pattern: tone(80.0, 0.15, 0.0, DotShape::Noise),
            ..base("Sand tone pen", 60.0)
        }),
    ]
}

/// Look up a built-in preset by id.
pub fn find(id: &str) -> Option<Preset> {
    catalog().into_iter().find(|p| p.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brush::{Sample, Stroke};
    use ink_core::{Document, Layer};
    use std::collections::HashSet;

    fn draw(params: &BrushParams, doc: &mut Document, layer: ink_core::LayerId) {
        let mut s = Stroke::begin(doc, layer, params.clone(), Rgba::BLACK);
        for i in 0..=120 {
            let t = i as f32 / 120.0;
            let x = 60.0 + t * 700.0;
            let y = 200.0 + (t * 6.0).sin() * 60.0;
            let mut smp = Sample::new(x, y, 0.3 + 0.7 * (t * std::f32::consts::PI).sin());
            smp.time = t as f64 * 0.6;
            s.add(doc, smp);
        }
        let _ = s.end(doc);
    }

    #[test]
    fn catalog_is_large_and_ids_are_unique() {
        let c = catalog();
        assert!(c.len() >= 45, "{} presets", c.len());
        let ids: HashSet<&str> = c.iter().map(|p| p.id).collect();
        assert_eq!(ids.len(), c.len(), "duplicate ids");
        for cat in Category::ALL {
            assert!(c.iter().any(|p| p.category == cat), "{cat:?} is empty");
        }
    }

    #[test]
    fn every_painting_preset_draws_and_stays_near_the_path() {
        for preset in catalog().into_iter().filter(|p| p.category != Category::Eraser) {
            let mut d = Document::new(1024, 512, 600);
            let id = d.push_layer(Layer::new("l"));
            draw(&preset.params, &mut d, id);
            let r = &d.layer(id).unwrap().raster;
            let px = r.read_rect(0, 0, 1024, 512);
            let inked = px.iter().filter(|c| c.a() > 0).count();
            assert!(inked > 200, "{} drew only {inked} px", preset.id);
            // Nothing far from the path (scatter spreads at most a few radii).
            let reach = preset.params.size * 2.0 + 80.0;
            for (i, c) in px.iter().enumerate() {
                if c.a() > 0 {
                    let (x, y) = ((i % 1024) as f32, (i / 1024) as f32);
                    assert!(x > 60.0 - reach && x < 760.0 + reach && (y - 200.0).abs() < 60.0 + reach, "{} painted stray pixel at {x},{y}", preset.id);
                }
            }
        }
    }

    #[test]
    fn rendering_is_deterministic() {
        for id in ["sparkle", "spray", "pencil_2b", "brush_pen", "tone_pen_dots"] {
            let preset = find(id).unwrap();
            let render = || {
                let mut d = Document::new(1024, 512, 600);
                let l = d.push_layer(Layer::new("l"));
                draw(&preset.params, &mut d, l);
                d.layer(l).unwrap().raster.read_rect(0, 0, 1024, 512)
            };
            assert!(render() == render(), "{id} is not deterministic");
        }
    }

    #[test]
    fn erasers_remove_ink() {
        for preset in catalog().into_iter().filter(|p| p.category == Category::Eraser) {
            let mut d = Document::new(1024, 512, 600);
            let id = d.push_layer(Layer::new("l"));
            for y in 0..512 {
                for x in 0..1024 {
                    d.layer_mut(id).unwrap().raster.set(x, y, Rgba::BLACK);
                }
            }
            draw(&preset.params, &mut d, id);
            let px = d.layer(id).unwrap().raster.read_rect(0, 0, 1024, 512);
            let lightened = px.iter().filter(|c| c.a() < 255).count();
            assert!(lightened > 200, "{} erased only {lightened} px", preset.id);
        }
    }

    #[test]
    fn tone_pen_paints_only_dots() {
        let preset = find("tone_pen_dots").unwrap();
        let mut d = Document::new(1024, 512, 600);
        let id = d.push_layer(Layer::new("l"));
        let mut s = Stroke::begin(&d, id, preset.params.clone(), Rgba::BLACK);
        for i in 0..=100 {
            s.add(&mut d, Sample::new(100.0 + i as f32 * 8.0, 250.0, 1.0));
        }
        let _ = s.end(&mut d);
        // Sample well inside the 60 px wide stroke.
        let px = d.layer(id).unwrap().raster.read_rect(300, 235, 200, 30);
        let inked = px.iter().filter(|c| c.a() > 127).count() as f32 / px.len() as f32;
        assert!((0.12..0.3).contains(&inked), "tone pen coverage {inked}");
    }

    #[test]
    fn brush_pen_thins_when_fast() {
        let preset = find("brush_pen").unwrap();
        let width_at_speed = |seconds: f64| {
            let mut d = Document::new(1024, 512, 600);
            let id = d.push_layer(Layer::new("l"));
            let mut s = Stroke::begin(&d, id, preset.params.clone(), Rgba::BLACK);
            for i in 0..=100 {
                let t = i as f32 / 100.0;
                let mut smp = Sample::new(100.0 + t * 800.0, 250.0, 1.0);
                smp.time = t as f64 * seconds;
                s.add(&mut d, smp);
            }
            let _ = s.end(&mut d);
            let r = &d.layer(id).unwrap().raster;
            (200..300).filter(|&y| r.get(500, y).a() > 127).count()
        };
        let slow = width_at_speed(4.0);
        let fast = width_at_speed(0.2);
        assert!(fast < slow, "fast {fast} vs slow {slow}");
    }
}
