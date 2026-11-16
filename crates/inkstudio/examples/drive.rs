//! Drive the real Inkstudio UI headless with synthetic pen, mouse and keyboard events,
//! then save screenshots. Verifies input → tools → engine → canvas end to end.
//!
//!   cargo run --release -p inkstudio --example drive -- <out-dir>

use egui::{Event, Key, Modifiers, PointerButton, Pos2, RawInput, TouchDeviceId, TouchId, TouchPhase, vec2};
use ink_comic::BalloonStyle;
use ink_ui::headless::Renderer;
use ink_ui::i18n::Lang;
use inkstudio::app::{Pane, Tool};
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
    fn new(config: &Path) -> Self {
        let ctx = egui::Context::default();
        let app = InkApp::with_config(&ctx, config.to_path_buf());
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

    fn key_mod(&mut self, key: Key, modifiers: Modifiers) {
        let k = |pressed| Event::Key { key, physical_key: None, pressed, repeat: false, modifiers };
        self.frame(vec![k(true)]);
        self.frame(vec![k(false)]);
    }

    fn key(&mut self, key: Key) {
        self.key_mod(key, Modifiers::NONE);
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
    (0..=n)
        .map(|i| {
            let a = a0 + (a1 - a0) * i as f32 / n as f32;
            (cx + r * a.cos(), cy + r * a.sin())
        })
        .collect()
}

fn line(a: (f32, f32), b: (f32, f32), n: usize) -> Vec<(f32, f32)> {
    (0..=n)
        .map(|i| {
            let t = i as f32 / n as f32;
            (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
        })
        .collect()
}

/// Draw a page with ink, speed lines, tone and three balloons (Japanese typed via keys).
fn manga_page(h: &mut Harness) {
    let swell = |t: f32| 0.25 + 0.75 * (t * std::f32::consts::PI).sin();
    let (w, hgt) = { let d = &h.app.book.as_ref().unwrap().page().doc; (d.width as f32, d.height as f32) };
    let (sx, sy) = (w / 2600.0, hgt / 3600.0);
    let m = |x: f32, y: f32| (x * sx, y * sy);
    h.app.select_brush("g_pen");
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
    h.app.select_brush("speed_line_pen");
    for k in 0..14 {
        let y = 300.0 + k as f32 * 55.0;
        let l: Vec<_> = line((2350.0, y), (1500.0 + (k % 4) as f32 * 60.0, y), 40).into_iter().map(|(a, b)| m(a, b)).collect();
        h.pen(&l, |t| 1.0 - t * 0.6);
    }
    h.app.select_brush("tone_pen_dots");
    for k in 0..5 {
        let y = 1650.0 + k as f32 * 70.0;
        let l: Vec<_> = line((300.0, y), (1100.0, y), 30).into_iter().map(|(a, b)| m(a, b)).collect();
        h.pen(&l, |_| 1.0);
    }
    h.app.tool = Tool::Balloon;
    h.idle(2);
    let place = |h: &mut Harness, style: BalloonStyle, x: f32, y: f32, text: &str| {
        h.app.balloon_style = style;
        h.idle(1);
        let p = h.screen(x * sx, y * sy);
        h.click(p);
        h.idle(2);
        h.frame(vec![Event::Key { key: Key::A, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::COMMAND }]);
        h.type_text(text);
        h.idle(2);
    };
    place(h, BalloonStyle::Oval, 1550.0, 520.0, "ちょっと\n待って！");
    place(h, BalloonStyle::Shout, 2050.0, 1700.0, "うるさい！！");
    place(h, BalloonStyle::Thought, 700.0, 2850.0, "ラーメン\n食べたい…");
    let tip = h.app.book.as_ref().unwrap().page().balloons.last().unwrap().tail_tip;
    let (from, to) = (h.screen(tip.x, tip.y), h.screen(tip.x + 200.0 * sx, tip.y + 120.0 * sy));
    h.drag(from, to);
    h.idle(20);
}

fn main() {
    let out = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "replica/app".into()));
    std::fs::create_dir_all(&out).unwrap();
    let work = std::env::temp_dir().join(format!("inkstudio-drive-{}", std::process::id()));
    let config = work.join("config");
    let docs = work.join("docs");
    std::fs::create_dir_all(&docs).unwrap();

    // 1. Start: New sheet dialog (English).
    let mut h = Harness::new(&config);
    h.app.set_lang(Lang::En);
    h.idle(15);
    h.snap(&out.join("1-new-sheet.png"));

    // 2. Manga page with docked palettes, drawn with pen events; balloons typed via keys.
    h.app.open(SheetSettings { name: "chapter-01".into(), dpi: 350, ..Default::default() });
    h.idle(12);
    manga_page(&mut h);
    h.snap(&out.join("2-manga-page-balloon-tool.png"));
    assert!(h.app.is_dirty(), "edits mark the book unsaved");

    // 3. Save with Ctrl+S → file dialog → type a name → Enter.
    h.app.settings.last_dir = Some(docs.clone());
    h.key_mod(Key::S, Modifiers::COMMAND);
    h.idle(2);
    h.frame(vec![Event::Key { key: Key::A, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::COMMAND }]);
    h.type_text("chapter-01");
    h.key(Key::Enter);
    h.idle(3);
    let saved = docs.join("chapter-01.inks");
    assert!(saved.exists(), "saved through the dialog");
    assert!(!h.app.is_dirty(), "clean after save");
    let balloons = h.app.book.as_ref().unwrap().page().balloons.len();

    // 4. German UI, rotated + mirrored view, keep drawing on the rotated canvas.
    h.app.set_lang(Lang::De);
    h.app.tool = Tool::Brush;
    h.idle(2);
    h.key(Key::Num6); // +15°
    h.key(Key::Num6); // +30°
    // Rotate tool: drag around the view center for a few more degrees.
    h.app.tool = Tool::Rotate;
    let c = h.app.canvas_rect.center();
    h.drag(c + vec2(200.0, 0.0), c + vec2(196.0, 40.0));
    h.app.select_brush("brush_pen");
    let (w, hgt) = { let d = &h.app.book.as_ref().unwrap().page().doc; (d.width as f32, d.height as f32) };
    let l = line((w * 0.15, hgt * 0.72), (w * 0.45, hgt * 0.80), 40);
    h.pen(&l, |t| 0.3 + 0.7 * (t * std::f32::consts::PI).sin());
    // Arrow keys pan, Ctrl+wheel style zoom via '+'.
    h.key(Key::ArrowLeft);
    h.key(Key::Plus);
    h.idle(10);
    assert!(h.app.view.angle.to_degrees() > 30.0, "rotated: {}", h.app.view.angle.to_degrees());
    h.snap(&out.join("3-german-rotated-canvas.png"));
    h.key(Key::M); // mirror
    h.idle(6);
    assert!(h.app.view.flip);
    h.snap(&out.join("4-mirrored-canvas.png"));

    // 5. Floating palette: undock the navigator into its own window.
    if let Some(path) = h.app.dock.find_tab(&Pane::Navigator) {
        h.app.dock.remove_tab(path);
    }
    h.app.dock.add_window(vec![Pane::Navigator]);
    h.key(Key::Num5); // reset rotation
    h.key(Key::M); // unmirror
    h.key_mod(Key::Num0, Modifiers::COMMAND); // fit
    h.idle(12);
    h.snap(&out.join("5-floating-navigator.png"));

    // 6. Unsaved changes prompt on Ctrl+N.
    assert!(h.app.is_dirty(), "the rotated-view stroke is unsaved");
    h.key_mod(Key::N, Modifiers::COMMAND);
    h.idle(15); // let the dialog finish fading in
    h.snap(&out.join("6-unsaved-prompt.png"));

    // 7. Open the saved file in a fresh app through Ctrl+O and the dialog.
    let mut h2 = Harness::new(&config);
    h2.idle(2);
    // Close the start dialog by opening the file via the menu shortcut.
    h2.app.settings.last_dir = Some(docs.clone());
    h2.app.open(SheetSettings { dpi: 72, trim_mm: (50.0, 50.0), ..Default::default() }); // something to replace
    h2.idle(2);
    h2.key_mod(Key::O, Modifiers::COMMAND);
    h2.idle(15);
    h2.snap(&out.join("7-open-dialog.png"));
    h2.frame(vec![Event::Key { key: Key::A, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::COMMAND }]);
    h2.type_text("chapter-01.inks");
    h2.key(Key::Enter);
    h2.idle(20);
    let b = h2.app.book.as_ref().expect("opened");
    assert_eq!(b.page().balloons.len(), balloons, "balloons survive save/open");
    assert_eq!(b.path.as_deref(), Some(saved.as_path()));
    assert!(h2.app.settings.recent.first().is_some_and(|r| r == &saved), "recent files updated");
    h2.snap(&out.join("8-reopened.png"));

    // Settings persisted: language and dock layout.
    let s = inkstudio::settings::Settings::load_from(&config);
    assert_eq!(s.lang.as_deref(), Some("de"));

    // 8. Animation (German): two drawings and onion skin.
    let mut h = Harness::new(&config);
    h.idle(2);
    h.app.open(SheetSettings { name: "bounce".into(), kind: SheetKind::Animation, trim_mm: (1280.0 * 25.4 / 96.0, 720.0 * 25.4 / 96.0), dpi: 96, fps: 12.0, frames: 12, template: None, ..Default::default() });
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
    // Show the timeline tab in the bottom dock.
    if let Some(path) = h.app.dock.find_tab(&Pane::Timeline) {
        let _ = h.app.dock.set_active_tab(path);
    }
    h.idle(8);
    h.snap(&out.join("9-animation-onion-skin.png"));
    h.app.export_to = Some(out.clone());
    h.app.export_gif(0.0);
    assert!(h.app.last_export.as_ref().is_some_and(|p| p.extension().is_some_and(|e| e == "gif") && p.exists()), "GIF exported");
    assert_eq!(h.app.settings.active_layout, "animation", "animation sheets switch to the animation layout");

    // 9. Layouts: lettering with the balloon tool, canvas focus, and a user layout.
    h2.app.switch_layout("lettering");
    h2.app.tool = Tool::Balloon;
    h2.app.selected_balloon = h2.app.book.as_ref().unwrap().page().balloons.first().map(|b| b.id);
    h2.idle(10);
    h2.snap(&out.join("10-layout-lettering.png"));
    h2.app.switch_layout("focus");
    h2.app.tool = Tool::Brush;
    h2.idle(10);
    h2.snap(&out.join("11-layout-focus.png"));
    h2.app.switch_layout("manga");
    if let Some(path) = h2.app.dock.find_tab(&Pane::Navigator) {
        h2.app.dock.remove_tab(path);
    }
    h2.app.layout_name = Some(String::new());
    h2.idle(2);
    h2.type_text("Mein Inking");
    h2.idle(14);
    h2.snap(&out.join("12-save-layout.png"));
    h2.key(Key::Enter);
    h2.idle(2);
    assert_eq!(h2.app.settings.active_layout, "custom:Mein Inking");
    assert!(h2.app.settings.custom_layouts.iter().any(|n| n == "Mein Inking"));

    // 10. Autosave, "crash", recovery.
    let mut hc = Harness::new(&config);
    hc.idle(2);
    hc.app.open(SheetSettings { name: "crash-test".into(), dpi: 150, ..Default::default() });
    hc.idle(4);
    hc.app.settings.autosave_secs = 1;
    hc.app.select_brush("g_pen");
    let (w, hgt) = { let d = &hc.app.book.as_ref().unwrap().page().doc; (d.width as f32, d.height as f32) };
    hc.pen(&line((w * 0.2, hgt * 0.3), (w * 0.8, hgt * 0.35), 40), |t| 0.3 + 0.7 * (t * std::f32::consts::PI).sin());
    hc.idle(80); // > 1 s of UI time
    hc.app.finish_autosave(hc.t);
    let rec = inkstudio::recovery::list(&config);
    assert_eq!(rec.len(), 1, "one recovery copy written");
    assert!(hc.app.is_dirty(), "a recovery copy doesn't mark the project saved");
    let ink_before = { let p = hc.app.book.as_ref().unwrap().page(); p.doc.layer(p.ink_layer).unwrap().raster.tile_count() };
    drop(hc); // the app "crashes" without saving
    let mut hr = Harness::new(&config);
    hr.idle(15);
    hr.snap(&out.join("13-recovery-dialog.png"));
    hr.app.recover(0, hr.t);
    hr.idle(4);
    let b = hr.app.book.as_ref().expect("recovered");
    assert_eq!(b.settings.name, "crash-test");
    assert!(b.is_dirty(), "recovered work is unsaved until the user saves it");
    let ink_after = { let p = b.page(); p.doc.layer(p.ink_layer).unwrap().raster.tile_count() };
    assert_eq!(ink_before, ink_after, "the stroke survived the crash");
    // Saving removes the recovery copy.
    hr.app.save_to(&docs.join("crash-test.inks"), hr.t);
    assert!(inkstudio::recovery::list(&config).is_empty());
    let _ = std::fs::remove_dir_all(&work);
    println!("all scenarios passed");
}
