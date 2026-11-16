//! Drive the real Inkstudio UI headless with synthetic pen, mouse and keyboard events,
//! then save screenshots. Verifies input → tools → engine → canvas end to end.
//!
//!   cargo run --release -p inkstudio --example drive -- <out-dir>

use egui::{
    Event, Key, Modifiers, PointerButton, Pos2, RawInput, TouchDeviceId, TouchId, TouchPhase, vec2,
};
use ink_comic::BalloonStyle;
use ink_ui::headless::Renderer;
use ink_ui::i18n::Lang;
use inkstudio::InkApp;
use inkstudio::app::RulerKind;
use inkstudio::app::{Pane, Tool};
use inkstudio::book::{PageRole, SheetKind, SheetSettings};
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
        Self {
            ctx,
            app,
            r: Renderer::default(),
            t: 0.0,
            size: vec2(1600.0, 1000.0),
            last: Vec::new(),
        }
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
        let touch = |phase, p: Pos2, f: f32| Event::Touch {
            device_id: TouchDeviceId(7),
            id: TouchId(1),
            phase,
            pos: p,
            force: Some(f),
        };
        let mut evs = vec![touch(
            TouchPhase::Start,
            self.screen(pts[0].0, pts[0].1),
            pressure(0.0),
        )];
        for (i, (x, y)) in pts.iter().enumerate().skip(1) {
            evs.push(touch(
                TouchPhase::Move,
                self.screen(*x, *y),
                pressure(i as f32 / n as f32),
            ));
            if evs.len() >= 6 {
                self.frame(std::mem::take(&mut evs));
            }
        }
        let last = pts[pts.len() - 1];
        evs.push(touch(TouchPhase::End, self.screen(last.0, last.1), 0.0));
        self.frame(evs);
    }

    fn click(&mut self, p: Pos2) {
        self.frame(vec![
            Event::PointerMoved(p),
            Event::PointerButton {
                pos: p,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
        ]);
        self.frame(vec![Event::PointerButton {
            pos: p,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        }]);
    }

    fn drag(&mut self, a: Pos2, b: Pos2) {
        self.frame(vec![
            Event::PointerMoved(a),
            Event::PointerButton {
                pos: a,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
        ]);
        for i in 1..=8 {
            self.frame(vec![Event::PointerMoved(a.lerp(b, i as f32 / 8.0))]);
        }
        self.frame(vec![Event::PointerButton {
            pos: b,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        }]);
    }

    fn key_mod(&mut self, key: Key, modifiers: Modifiers) {
        let k = |pressed| Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers,
        };
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
        let img = self.r.render(
            &self.last,
            self.size.x as usize,
            self.size.y as usize,
            1.0,
            egui::Color32::from_gray(30),
        );
        let f = std::fs::File::create(path).unwrap();
        let mut e = png::Encoder::new(
            std::io::BufWriter::new(f),
            img.width as u32,
            img.height as u32,
        );
        e.set_color(png::ColorType::Rgba);
        e.set_depth(png::BitDepth::Eight);
        e.write_header()
            .unwrap()
            .write_image_data(&img.rgba)
            .unwrap();
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
    let (w, hgt) = {
        let d = &h.app.book.as_ref().unwrap().page().doc;
        (d.width as f32, d.height as f32)
    };
    let (sx, sy) = (w / 2600.0, hgt / 3600.0);
    let m = |x: f32, y: f32| (x * sx, y * sy);
    h.app.select_brush("g_pen");
    let face: Vec<_> = circle(900.0, 640.0, 260.0, 0.3, 2.9, 80)
        .into_iter()
        .map(|(x, y)| m(x, y))
        .collect();
    h.pen(&face, swell);
    for k in 0..7 {
        let x = 700.0 + k as f32 * 60.0;
        let hair: Vec<_> = line(
            (x - 40.0, 380.0),
            (x + 40.0, 640.0 - (k % 3) as f32 * 40.0),
            30,
        )
        .into_iter()
        .map(|(a, b)| m(a, b))
        .collect();
        h.pen(&hair, swell);
    }
    for ex in [810.0, 990.0] {
        let eye: Vec<_> = circle(ex, 650.0, 38.0, 0.0, std::f32::consts::TAU, 30)
            .into_iter()
            .map(|(x, y)| m(x, y))
            .collect();
        h.pen(&eye, |_| 0.8);
    }
    h.app.select_brush("speed_line_pen");
    for k in 0..14 {
        let y = 300.0 + k as f32 * 55.0;
        let l: Vec<_> = line((2350.0, y), (1500.0 + (k % 4) as f32 * 60.0, y), 40)
            .into_iter()
            .map(|(a, b)| m(a, b))
            .collect();
        h.pen(&l, |t| 1.0 - t * 0.6);
    }
    h.app.select_brush("tone_pen_dots");
    for k in 0..5 {
        let y = 1650.0 + k as f32 * 70.0;
        let l: Vec<_> = line((300.0, y), (1100.0, y), 30)
            .into_iter()
            .map(|(a, b)| m(a, b))
            .collect();
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
        h.frame(vec![Event::Key {
            key: Key::A,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::COMMAND,
        }]);
        h.type_text(text);
        h.idle(2);
    };
    place(h, BalloonStyle::Oval, 1550.0, 520.0, "ちょっと\n待って！");
    place(h, BalloonStyle::Shout, 2050.0, 1700.0, "うるさい！！");
    place(
        h,
        BalloonStyle::Thought,
        700.0,
        2850.0,
        "ラーメン\n食べたい…",
    );
    let tip = h
        .app
        .book
        .as_ref()
        .unwrap()
        .page()
        .balloons
        .last()
        .unwrap()
        .tail_tip;
    let (from, to) = (
        h.screen(tip.x, tip.y),
        h.screen(tip.x + 200.0 * sx, tip.y + 120.0 * sy),
    );
    h.drag(from, to);
    h.idle(20);
}

fn main() {
    let out = PathBuf::from(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| "replica/app".into()),
    );
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
    h.app.open(SheetSettings {
        name: "chapter-01".into(),
        dpi: 350,
        ..Default::default()
    });
    h.idle(12);
    manga_page(&mut h);
    h.snap(&out.join("2-manga-page-balloon-tool.png"));
    assert!(h.app.is_dirty(), "edits mark the book unsaved");

    // 3. Save with Ctrl+S → file dialog → type a name → Enter.
    h.app.settings.last_dir = Some(docs.clone());
    h.key_mod(Key::S, Modifiers::COMMAND);
    h.idle(2);
    h.frame(vec![Event::Key {
        key: Key::A,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::COMMAND,
    }]);
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
    let (w, hgt) = {
        let d = &h.app.book.as_ref().unwrap().page().doc;
        (d.width as f32, d.height as f32)
    };
    let l = line((w * 0.15, hgt * 0.72), (w * 0.45, hgt * 0.80), 40);
    h.pen(&l, |t| 0.3 + 0.7 * (t * std::f32::consts::PI).sin());
    // Arrow keys pan, Ctrl+wheel style zoom via '+'.
    h.key(Key::ArrowLeft);
    h.key(Key::Plus);
    h.idle(10);
    assert!(
        h.app.view.angle.to_degrees() > 30.0,
        "rotated: {}",
        h.app.view.angle.to_degrees()
    );
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
    h2.app.open(SheetSettings {
        dpi: 72,
        trim_mm: (50.0, 50.0),
        ..Default::default()
    }); // something to replace
    h2.idle(2);
    h2.key_mod(Key::O, Modifiers::COMMAND);
    h2.idle(15);
    h2.snap(&out.join("7-open-dialog.png"));
    h2.frame(vec![Event::Key {
        key: Key::A,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::COMMAND,
    }]);
    h2.type_text("chapter-01.inks");
    h2.key(Key::Enter);
    h2.idle(20);
    let b = h2.app.book.as_ref().expect("opened");
    assert_eq!(
        b.page().balloons.len(),
        balloons,
        "balloons survive save/open"
    );
    assert_eq!(b.path.as_deref(), Some(saved.as_path()));
    assert!(
        h2.app.settings.recent.first().is_some_and(|r| r == &saved),
        "recent files updated"
    );
    h2.snap(&out.join("8-reopened.png"));

    // Settings persisted: language and dock layout.
    let s = inkstudio::settings::Settings::load_from(&config);
    assert_eq!(s.lang.as_deref(), Some("de"));

    // 8. Animation (German): two drawings and onion skin.
    let mut h = Harness::new(&config);
    h.idle(2);
    h.app.open(SheetSettings {
        name: "bounce".into(),
        kind: SheetKind::Animation,
        trim_mm: (1280.0 * 25.4 / 96.0, 720.0 * 25.4 / 96.0),
        dpi: 96,
        fps: 12.0,
        frames: 12,
        template: None,
        ..Default::default()
    });
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
    assert!(
        h.app
            .last_export
            .as_ref()
            .is_some_and(|p| p.extension().is_some_and(|e| e == "gif") && p.exists()),
        "GIF exported"
    );
    assert_eq!(
        h.app.settings.active_layout, "animation",
        "animation sheets switch to the animation layout"
    );

    // 9. Layouts: lettering with the balloon tool, canvas focus, and a user layout.
    h2.app.switch_layout("lettering");
    h2.app.tool = Tool::Balloon;
    h2.app.selected_balloon = h2
        .app
        .book
        .as_ref()
        .unwrap()
        .page()
        .balloons
        .first()
        .map(|b| b.id);
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
    assert!(
        h2.app
            .settings
            .custom_layouts
            .iter()
            .any(|n| n == "Mein Inking")
    );

    // 10. Autosave, "crash", recovery.
    let mut hc = Harness::new(&config);
    hc.idle(2);
    hc.app.open(SheetSettings {
        name: "crash-test".into(),
        dpi: 150,
        ..Default::default()
    });
    hc.idle(4);
    hc.app.settings.autosave_secs = 1;
    hc.app.select_brush("g_pen");
    let (w, hgt) = {
        let d = &hc.app.book.as_ref().unwrap().page().doc;
        (d.width as f32, d.height as f32)
    };
    hc.pen(
        &line((w * 0.2, hgt * 0.3), (w * 0.8, hgt * 0.35), 40),
        |t| 0.3 + 0.7 * (t * std::f32::consts::PI).sin(),
    );
    hc.idle(80); // > 1 s of UI time
    hc.app.finish_autosave(hc.t);
    let rec = inkstudio::recovery::list(&config);
    assert_eq!(rec.len(), 1, "one recovery copy written");
    assert!(
        hc.app.is_dirty(),
        "a recovery copy doesn't mark the project saved"
    );
    let ink_before = {
        let p = hc.app.book.as_ref().unwrap().page();
        p.doc.layer(p.ink_layer).unwrap().raster.tile_count()
    };
    drop(hc); // the app "crashes" without saving
    let mut hr = Harness::new(&config);
    hr.idle(15);
    hr.snap(&out.join("13-recovery-dialog.png"));
    hr.app.recover(0, hr.t);
    hr.idle(4);
    let b = hr.app.book.as_ref().expect("recovered");
    assert_eq!(b.settings.name, "crash-test");
    assert!(
        b.is_dirty(),
        "recovered work is unsaved until the user saves it"
    );
    let ink_after = {
        let p = b.page();
        p.doc.layer(p.ink_layer).unwrap().raster.tile_count()
    };
    assert_eq!(ink_before, ink_after, "the stroke survived the crash");
    // Saving removes the recovery copy.
    hr.app.save_to(&docs.join("crash-test.inks"), hr.t);
    assert!(inkstudio::recovery::list(&config).is_empty());

    // 11. Perspective and symmetry rulers.
    let mut hp = Harness::new(&config);
    hp.idle(2);
    hp.app.open(SheetSettings {
        name: "street".into(),
        kind: SheetKind::Illustration,
        dpi: 100,
        template: None,
        ..Default::default()
    });
    hp.idle(6);
    hp.app.select_brush("g_pen");
    let (w, hgt) = {
        let d = &hp.app.book.as_ref().unwrap().page().doc;
        (d.width as f32, d.height as f32)
    };
    {
        let page = hp.app.book.as_mut().unwrap().page_mut();
        page.ruler = Some(RulerKind::TwoPoint.make(w, hgt));
        page.ruler_snap = true;
    }
    // Wobbly strokes snap onto lines toward the vanishing points; verticals stay vertical.
    for (a, b) in [
        ((w * 0.3, hgt * 0.6), (w * 0.05, hgt * 0.5)),
        ((w * 0.3, hgt * 0.6), (w * 0.6, hgt * 0.52)),
        ((w * 0.3, hgt * 0.6), (w * 0.31, hgt * 0.85)),
        ((w * 0.6, hgt * 0.52), (w * 0.61, hgt * 0.75)),
    ] {
        let pts: Vec<(f32, f32)> = line(a, b, 30)
            .into_iter()
            .enumerate()
            .map(|(i, (x, y))| {
                (
                    x + (i as f32 * 1.3).sin() * 3.0,
                    y + (i as f32 * 0.7).cos() * 3.0,
                )
            })
            .collect();
        hp.pen(&pts, |_| 0.8);
    }
    {
        let p = hp.app.book.as_ref().unwrap().page();
        let ink = &p.doc.layer(p.ink_layer).unwrap().raster;
        // The vertical stroke landed exactly on x = 0.3 w (snapped), not on the wobble.
        let x = (w * 0.3) as i32;
        let y = (hgt * 0.75) as i32;
        assert!(ink.get(x, y).a() > 0, "vertical snapped to the start x");
    }
    hp.app.tool = Tool::Ruler;
    hp.idle(10);
    hp.snap(&out.join("14-perspective-ruler.png"));
    // Kaleidoscope: one stroke, six mirrored pairs.
    {
        let page = hp.app.book.as_mut().unwrap().page_mut();
        page.ruler = Some(RulerKind::Kaleidoscope.make(w, hgt));
    }
    hp.app.tool = Tool::Brush;
    let c = (w * 0.5, hgt * 0.5);
    hp.pen(&circle(c.0 + w * 0.12, c.1, w * 0.06, 0.0, 3.5, 30), |t| {
        0.4 + 0.6 * t
    });
    hp.pen(
        &line(
            (c.0 + w * 0.02, c.1 - w * 0.03),
            (c.0 + w * 0.28, c.1 - w * 0.09),
            20,
        ),
        |t| 1.0 - t * 0.8,
    );
    {
        let p = hp.app.book.as_ref().unwrap().page();
        let ink = &p.doc.layer(p.ink_layer).unwrap().raster;
        // A point drawn right of center appears mirrored left of center too.
        let probe = ((c.0 - w * 0.12 - w * 0.06) as i32, c.1 as i32);
        let near = (-4..=4).any(|d| {
            ink.get(probe.0 + d, probe.1).a() > 0 || ink.get(probe.0, probe.1 + d).a() > 0
        });
        assert!(near, "kaleidoscope copy on the left");
    }
    hp.app.tool = Tool::Ruler;
    hp.idle(10);
    hp.snap(&out.join("15-symmetry-kaleidoscope.png"));

    // 12. Coloring: flats for every closed area, lasso, gradient on a clipped shadow layer, eyedropper.
    let mut hk = Harness::new(&config);
    hk.idle(2);
    hk.app.open(SheetSettings {
        name: "color".into(),
        kind: SheetKind::Illustration,
        dpi: 100,
        template: None,
        color_mode: inkstudio::book::ColorMode::Color,
        ..Default::default()
    });
    hk.idle(6);
    hk.app.select_brush("g_pen");
    let (w, hgt) = {
        let d = &hk.app.book.as_ref().unwrap().page().doc;
        (d.width as f32, d.height as f32)
    };
    hk.pen(
        &circle(
            w * 0.35,
            hgt * 0.4,
            w * 0.18,
            0.0,
            std::f32::consts::TAU * 1.03,
            80,
        ),
        |_| 0.9,
    );
    hk.pen(
        &circle(
            w * 0.65,
            hgt * 0.6,
            w * 0.15,
            0.0,
            std::f32::consts::TAU * 1.03,
            80,
        ),
        |_| 0.9,
    );
    hk.pen(
        &line((w * 0.1, hgt * 0.85), (w * 0.9, hgt * 0.85), 30),
        |_| 0.9,
    );
    hk.app.fill_all_areas(hk.t);
    hk.idle(2);
    {
        let p = hk.app.book.as_ref().unwrap().page();
        let flats = p
            .doc
            .layers()
            .iter()
            .find(|(_, l)| l.props.name == "Flats")
            .expect("Flats layer");
        assert!(
            flats
                .1
                .raster
                .get((w * 0.35) as i32, (hgt * 0.4) as i32)
                .a()
                == 255,
            "circle filled"
        );
        assert!(
            flats
                .1
                .raster
                .get((w * 0.65) as i32, (hgt * 0.6) as i32)
                .a()
                == 255,
            "second circle filled"
        );
        assert_ne!(
            flats.1.raster.get((w * 0.35) as i32, (hgt * 0.4) as i32),
            flats.1.raster.get((w * 0.65) as i32, (hgt * 0.6) as i32),
            "different flat colors"
        );
    }
    hk.app
        .add_clipped_layer("Shadow", ink_core::blend::BlendMode::Multiply);
    hk.app.color = ink_ui::widgets::Hsv {
        h: 0.7,
        s: 0.35,
        v: 0.75,
    };
    hk.app.tool = Tool::Gradient;
    hk.app.gradient_mode = inkstudio::app::GradientMode::MainToTransparent;
    hk.idle(2);
    hk.pen(
        &line((w * 0.8, hgt * 0.8), (w * 0.3, hgt * 0.3), 10),
        |_| 1.0,
    );
    hk.app.tool = Tool::LassoFill;
    hk.app.color = ink_ui::widgets::Hsv {
        h: 0.12,
        s: 0.6,
        v: 0.95,
    };
    hk.idle(2);
    hk.pen(
        &circle(
            w * 0.3,
            hgt * 0.32,
            w * 0.06,
            0.0,
            std::f32::consts::TAU,
            24,
        ),
        |_| 1.0,
    );
    hk.app.tool = Tool::Eyedropper;
    hk.idle(2);
    hk.pen(&[(w * 0.3, hgt * 0.32), (w * 0.3, hgt * 0.32)], |_| 1.0);
    let picked = hk.app.color.to_color32();
    assert!(
        picked.r() > picked.b(),
        "picked the lasso's warm color: {picked:?}"
    );
    assert!(
        !hk.app.settings.color_history.is_empty(),
        "colors were remembered"
    );
    hk.app.tool = Tool::Fill;
    hk.app.color_tab = 1;
    hk.idle(10);
    hk.snap(&out.join("16-coloring.png"));

    // 13. Concept art: 2K sheet, concept layout, value check.
    let mut hx = Harness::new(&config);
    hx.idle(2);
    let mut d = inkstudio::new_sheet::NewSheet::new(true);
    d.set_kind(SheetKind::ConceptArt);
    let mut cs = d.s.clone();
    cs.name = "keyframe".into();
    cs.dpi = 72; // keep the test fast; the pixel size is what matters here
    cs.trim_mm = (1280.0 * 25.4 / 72.0, 720.0 * 25.4 / 72.0);
    hx.app.open(cs);
    hx.idle(6);
    assert_eq!(
        hx.app.settings.active_layout, "concept",
        "concept art opens the concept layout"
    );
    let (w, hgt) = {
        let d = &hx.app.book.as_ref().unwrap().page().doc;
        (d.width as f32, d.height as f32)
    };
    hx.app.color = ink_ui::widgets::Hsv {
        h: 0.58,
        s: 0.45,
        v: 0.85,
    };
    hx.app.sub_color = ink_core::Rgba::from_straight(250, 214, 170, 255);
    hx.app.tool = Tool::Gradient;
    hx.app.gradient_mode = inkstudio::app::GradientMode::MainToSub;
    hx.idle(2);
    hx.pen(&line((w * 0.5, 0.0), (w * 0.5, hgt * 0.7), 8), |_| 1.0);
    hx.app.tool = Tool::LassoFill;
    hx.app.color = ink_ui::widgets::Hsv {
        h: 0.62,
        s: 0.3,
        v: 0.35,
    };
    hx.idle(2);
    hx.pen(
        &[
            (0.0, hgt * 0.75),
            (w * 0.2, hgt * 0.45),
            (w * 0.35, hgt * 0.6),
            (w * 0.55, hgt * 0.35),
            (w * 0.8, hgt * 0.62),
            (w, hgt * 0.5),
            (w, hgt),
            (0.0, hgt),
        ],
        |_| 1.0,
    );
    hx.app.tool = Tool::Brush;
    hx.idle(10);
    hx.snap(&out.join("17-concept-art.png"));
    hx.key_mod(Key::V, Modifiers::SHIFT);
    hx.idle(10);
    assert!(hx.app.value_check, "Shift+V toggles the value check");
    hx.snap(&out.join("18-concept-value-check.png"));

    // 14. A complete manga volume and the right-to-left reader.
    let mut hb = Harness::new(&config);
    hb.idle(2);
    hb.app.open(SheetSettings {
        name: "vol1".into(),
        kind: SheetKind::Book,
        pages: 4,
        dpi: 72,
        front_matter: true,
        ..Default::default()
    });
    hb.idle(4);
    {
        let b = hb.app.book.as_mut().unwrap();
        b.info.title = "Kaze no Michi".into();
        b.info.subtitle = "Band 1".into();
        b.info.author = "Ana Schmidt".into();
        b.info.credits = vec![
            ("Lettering".into(), "Jo".into()),
            ("Lektorat".into(), "Mika".into()),
        ];
        b.info.year = "2026".into();
        let roles: Vec<_> = b.pages.iter().map(|p| p.role).collect();
        assert_eq!(
            roles.len(),
            9,
            "4 story pages + cover, title, credits, afterword, back cover"
        );
        assert_eq!(roles[0], PageRole::Cover);
        assert_eq!(roles[8], PageRole::BackCover);
        assert_eq!(
            b.page_number(3),
            Some(1),
            "story numbering starts after the front matter"
        );
    }
    hb.app.refresh_special_pages();
    hb.app.select_brush("g_pen");
    for i in 3..7 {
        hb.app.book.as_mut().unwrap().current = i;
        hb.idle(3);
        let (w, hgt) = {
            let d = &hb.app.book.as_ref().unwrap().page().doc;
            (d.width as f32, d.height as f32)
        };
        hb.pen(
            &circle(
                w * 0.5,
                hgt * 0.4,
                w * (0.1 + 0.04 * i as f32),
                0.0,
                std::f32::consts::TAU * 1.02,
                50,
            ),
            |_| 0.9,
        );
    }
    hb.app.book.as_mut().unwrap().current = 0;
    hb.idle(30);
    hb.snap(&out.join("19-book-pages.png"));
    hb.app.open_book_info();
    hb.idle(12);
    hb.snap(&out.join("20-book-info.png"));
    hb.key(Key::Escape);
    hb.idle(2);
    assert!(hb.app.book_info.is_none());
    hb.key(Key::F5);
    hb.idle(30);
    assert!(hb.app.reader.is_some(), "F5 opens the reader");
    hb.key(Key::ArrowLeft); // right-to-left: ← is the next page
    hb.idle(30);
    assert_eq!(hb.app.reader.as_ref().unwrap().spread, 1);
    hb.snap(&out.join("21-reader-rtl-spread.png"));
    hb.key(Key::Escape);
    hb.idle(2);
    assert!(hb.app.reader.is_none());
    assert_eq!(
        hb.app.book.as_ref().unwrap().current,
        1,
        "closing the reader shows the page being read"
    );

    // 15. Animation UI: two tracks, holds, onion range, scrubbing.
    let mut ha = Harness::new(&config);
    ha.idle(2);
    ha.app.open(SheetSettings {
        name: "walk".into(),
        color_mode: inkstudio::book::ColorMode::Color,
        kind: SheetKind::Animation,
        trim_mm: (1280.0 * 25.4 / 96.0, 720.0 * 25.4 / 96.0),
        dpi: 96,
        fps: 12.0,
        frames: 24,
        template: None,
        ..Default::default()
    });
    ha.idle(4);
    ha.app.select_brush("g_pen");
    {
        let b = ha.app.book.as_mut().unwrap();
        let page = b.page_mut();
        let tl = page.timeline.as_mut().unwrap();
        tl.tracks[0].name = "Figur".into();
        let bg = tl.add_track("Hintergrund");
        tl.new_cel(&mut page.doc, bg, 0);
        tl.onion.enabled = true;
    }
    for (f, x) in [(0usize, 300.0f32), (4, 420.0), (8, 540.0), (12, 660.0)] {
        {
            let b = ha.app.book.as_mut().unwrap();
            let page = b.page_mut();
            let tl = page.timeline.as_mut().unwrap();
            if f > 0 {
                tl.new_cel(&mut page.doc, 0, f);
            }
            let cel = tl.cel_at(0, f);
            page.set_frame(f);
            ha.app.active_layer = cel;
        }
        ha.idle(2);
        ha.pen(
            &circle(x, 380.0, 60.0, 0.0, std::f32::consts::TAU * 1.02, 40),
            |_| 0.9,
        );
    }
    {
        let b = ha.app.book.as_mut().unwrap();
        let page = b.page_mut();
        let tl = page.timeline.as_mut().unwrap();
        tl.set_key(0, 16, None);
        tl.insert_frames(5, 2);
        page.set_frame(9);
    }
    if let Some(path) = ha.app.dock.find_tab(&Pane::Timeline) {
        let _ = ha.app.dock.set_active_tab(path);
    }
    ha.idle(12);
    ha.snap(&out.join("22-animation-timeline.png"));
    let _ = std::fs::remove_dir_all(&work);
    println!("all scenarios passed");
}
