//! Drive the real Inkstudio UI headless with synthetic pen, mouse and keyboard events,
//! then save screenshots. Verifies input → tools → engine → canvas end to end.
//!
//!   cargo run --release -p inkstudio --example drive -- <out-dir>

use egui::{Event, Key, Modifiers, PointerButton, Pos2, RawInput, TouchDeviceId, TouchId, TouchPhase, vec2};
use ink_comic::BalloonStyle;
use ink_ui::headless::Renderer;
use inkstudio::app::Tool;
use inkstudio::book::{SheetKind, SheetSettings};
use inkstudio::InkApp;
use std::path::{Path, PathBuf};

struct Harness {
    ctx: egui::Context,
    app: InkApp,
    r: Renderer,
    t: f64,
    size: egui::Vec2,
    last: Vec<egui::epaint::ClippedPrimitive>,
}

impl Harness {
    fn new() -> Self {
        let ctx = egui::Context::default();
        let app = InkApp::new(&ctx);
        Self { ctx, app, r: Renderer::default(), t: 0.0, size: vec2(1600.0, 1000.0), last: Vec::new() }
    }

    fn frame(&mut self, events: Vec<Event>) {
        self.t += 1.0 / 60.0;
        let raw = RawInput {
            screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, self.size)),
            time: Some(self.t),
            focused: true,
            events,
            ..Default::default()
        };
        self.ctx.set_pixels_per_point(1.0);
        let app = &mut self.app;
        let out = self.ctx.run_ui(raw, |ui| app.ui(ui));
        self.r.update_textures(&out.textures_delta);
        self.last = self.ctx.tessellate(out.shapes, out.pixels_per_point);
    }

    fn idle(&mut self, n: usize) {
        for _ in 0..n {
            self.frame(Vec::new());
        }
    }

    fn screen(&self, x: f32, y: f32) -> Pos2 {
        self.app.view.to_screen(x, y)
    }

    /// A pen stroke (with pressure) along canvas points, several samples per frame.
    fn pen(&mut self, pts: &[(f32, f32)], pressure: impl Fn(f32) -> f32) {
        let n = pts.len().max(2) - 1;
        let touch = |phase, p: Pos2, f: f32| Event::Touch { device_id: TouchDeviceId(7), id: TouchId(1), phase, pos: p, force: Some(f) };
        let mut evs = vec![touch(TouchPhase::Start, self.screen(pts[0].0, pts[0].1), pressure(0.0))];
        for (i, (x, y)) in pts.iter().enumerate().skip(1) {
            evs.push(touch(TouchPhase::Move, self.screen(*x, *y), pressure(i as f32 / n as f32)));
            if evs.len() >= 6 {
                self.frame(std::mem::take(&mut evs));
            }
        }
        let last = pts[pts.len() - 1];
        evs.push(touch(TouchPhase::End, self.screen(last.0, last.1), 0.0));
        self.frame(evs);
    }

    fn click(&mut self, p: Pos2) {
        self.frame(vec![Event::PointerMoved(p), Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE }]);
        self.frame(vec![Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE }]);
    }

    fn drag(&mut self, a: Pos2, b: Pos2) {
        self.frame(vec![Event::PointerMoved(a), Event::PointerButton { pos: a, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE }]);
        for i in 1..=8 {
            self.frame(vec![Event::PointerMoved(a.lerp(b, i as f32 / 8.0))]);
        }
        self.frame(vec![Event::PointerButton { pos: b, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE }]);
    }

    fn key(&mut self, key: Key) {
        let k = |pressed| Event::Key { key, physical_key: None, pressed, repeat: false, modifiers: Modifiers::NONE };
        self.frame(vec![k(true), k(false)]);
    }

    fn type_text(&mut self, s: &str) {
        for (i, line) in s.split('\n').enumerate() {
            if i > 0 {
                self.key(Key::Enter);
            }
            self.frame(vec![Event::Text(line.into())]);
        }
    }

    fn snap(&mut self, path: &Path) {
        let img = self.r.render(&self.last, self.size.x as usize, self.size.y as usize, 1.0, egui::Color32::from_gray(30));
        let f = std::fs::File::create(path).unwrap();
        let mut e = png::Encoder::new(std::io::BufWriter::new(f), img.width as u32, img.height as u32);
        e.set_color(png::ColorType::Rgba);
        e.set_depth(png::BitDepth::Eight);
        e.write_header().unwrap().write_image_data(&img.rgba).unwrap();
        println!("wrote {}", path.display());
    }
}

fn circle(cx: f32, cy: f32, r: f32, a0: f32, a1: f32, n: usize) -> Vec<(f32, f32)> {
    (0..=n).map(|i| {
        let a = a0 + (a1 - a0) * i as f32 / n as f32;
        (cx + r * a.cos(), cy + r * a.sin())
    }).collect()
}

fn line(a: (f32, f32), b: (f32, f32), n: usize) -> Vec<(f32, f32)> {
    (0..=n).map(|i| {
        let t = i as f32 / n as f32;
        (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
    }).collect()
}

fn main() {
    let out = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "replica/app".into()));
    std::fs::create_dir_all(&out).unwrap();
    let swell = |t: f32| 0.25 + 0.75 * (t * std::f32::consts::PI).sin();

    // 1. Start screen: the New sheet dialog.
    let mut h = Harness::new();
    h.idle(3);
    h.snap(&out.join("1-new-sheet.png"));

    // 2. A manga page: ink with pen pressure, panels, balloons with typed text.
    let s = SheetSettings { name: "chapter-01".into(), dpi: 350, ..Default::default() };
    h.app.open(s);
    h.idle(12);
    let (w, hgt) = (h.app.book.as_ref().unwrap().page().doc.width as f32, h.app.book.as_ref().unwrap().page().doc.height as f32);
    let (sx, sy) = (w / 2600.0, hgt / 3600.0); // author in a 2600×3600 space
    let m = |x: f32, y: f32| (x * sx, y * sy);
    h.app.select_brush("g_pen");
    // Panel 1: a face.
    let face: Vec<_> = circle(900.0, 640.0, 260.0, 0.3, 2.9, 80).into_iter().map(|(x, y)| m(x, y)).collect();
    h.pen(&face, swell);
    for k in 0..7 {
        let x = 700.0 + k as f32 * 60.0;
        let hair: Vec<_> = line((x - 40.0, 380.0), (x + 40.0, 640.0 - (k % 3) as f32 * 40.0), 30).into_iter().map(|(a, b)| m(a, b)).collect();
        h.pen(&hair, swell);
    }
    for ex in [810.0, 990.0] {
        let eye: Vec<_> = circle(ex, 650.0, 38.0, 0.0, std::f32::consts::TAU, 30).into_iter().map(|(x, y)| m(x, y)).collect();
        h.pen(&eye, |_| 0.8);
    }
    // Speed lines in panel 1 with the effect pen.
    h.app.select_brush("speed_line_pen");
    for k in 0..14 {
        let y = 300.0 + k as f32 * 55.0;
        let l: Vec<_> = line((2350.0, y), (1500.0 + (k % 4) as f32 * 60.0, y), 40).into_iter().map(|(a, b)| m(a, b)).collect();
        h.pen(&l, |t| 1.0 - t * 0.6);
    }
    // Tone pen in panel 3.
    h.app.select_brush("tone_pen_dots");
    for k in 0..5 {
        let y = 1650.0 + k as f32 * 70.0;
        let l: Vec<_> = line((300.0, y), (1100.0, y), 30).into_iter().map(|(a, b)| m(a, b)).collect();
        h.pen(&l, |_| 1.0);
    }
    // Balloons: pick styles, place, type.
    h.app.tool = Tool::Balloon;
    h.idle(2);
    let place = |h: &mut Harness, style: BalloonStyle, x: f32, y: f32, text: &str| {
        h.app.balloon_style = style;
        h.idle(1);
        let p = h.screen(x * sx, y * sy);
        h.click(p);
        h.idle(2);
        // Select all placeholder text and replace it.
        h.frame(vec![Event::Key { key: Key::A, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::COMMAND }]);
        h.type_text(text);
        h.idle(2);
    };
    place(&mut h, BalloonStyle::Oval, 1550.0, 520.0, "ちょっと\n待って！");
    place(&mut h, BalloonStyle::Shout, 2050.0, 1700.0, "うるさい！！");
    place(&mut h, BalloonStyle::Thought, 700.0, 2850.0, "ラーメン\n食べたい…");
    // Drag the last balloon's tail.
    let tip = { let b = h.app.book.as_ref().unwrap(); b.page().balloons.last().unwrap().tail_tip };
    let to = h.screen(tip.x + 200.0 * sx, tip.y + 120.0 * sy);
    let from = h.screen(tip.x, tip.y);
    h.drag(from, to);
    h.idle(20);
    h.snap(&out.join("2-manga-page-balloon-tool.png"));
    h.app.tool = Tool::Brush;
    h.idle(4);
    h.snap(&out.join("3-manga-page-brushes.png"));
    // Export check.
    h.app.export_to = Some(out.clone());
    h.app.export_pages(false, 0.0);
    assert!(h.app.last_export.as_ref().is_some_and(|p| p.exists()), "page PNG exported");

    // 3. A multi-page book with the page strip.
    let mut h = Harness::new();
    h.idle(2);
    let s = SheetSettings { name: "volume-1".into(), kind: SheetKind::Book, pages: 6, dpi: 150, ..Default::default() };
    h.app.open(s);
    h.idle(10);
    h.app.select_brush("brush_pen");
    let page_w = h.app.book.as_ref().unwrap().page().doc.width as f32;
    for k in 0..5 {
        let y = 300.0 + k as f32 * 60.0;
        let l = line((page_w * 0.2, y), (page_w * 0.8, y + 40.0), 30);
        h.pen(&l, swell);
    }
    h.idle(30);
    h.snap(&out.join("4-book-pages.png"));

    // 4. Animation: two drawings, onion skin, timeline.
    let mut h = Harness::new();
    h.idle(2);
    let s = SheetSettings { name: "bounce".into(), kind: SheetKind::Animation, trim_mm: (1280.0 * 25.4 / 96.0, 720.0 * 25.4 / 96.0), dpi: 96, fps: 12.0, frames: 12, template: None, ..Default::default() };
    h.app.open(s);
    h.idle(6);
    h.app.select_brush("g_pen");
    let ball = |cx: f32, cy: f32| circle(cx, cy, 70.0, 0.0, std::f32::consts::TAU * 1.02, 60);
    h.pen(&ball(300.0, 200.0), |_| 0.9);
    for (f, (x, y)) in [(3usize, (520.0, 420.0)), (6, (740.0, 560.0))] {
        {
            let b = h.app.book.as_mut().unwrap();
            let page = b.page_mut();
            let tl = page.timeline.as_mut().unwrap();
            tl.new_cel(&mut page.doc, 0, f);
            page.set_frame(f);
        }
        h.idle(2);
        h.pen(&ball(x, y), |_| 0.9);
    }
    h.idle(8);
    h.snap(&out.join("5-animation-onion-skin.png"));
    h.app.export_to = Some(out.clone());
    h.app.export_gif(0.0);
    assert!(h.app.last_export.as_ref().is_some_and(|p| p.extension().is_some_and(|e| e == "gif") && p.exists()), "GIF exported");
    println!("all scenarios passed");
}
