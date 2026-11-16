//! Static preview of the main workspace (S03) built only from ink-ui primitives.
//! It is a design reference, not the app shell: no document, no real canvas renderer.
//! Rendered by `examples/design_gallery.rs` into `replica/design/workspace-*.png`.

use crate::icons;
use crate::theme::{icon_font, palette, text, tokens};
use crate::widgets::*;
use egui::{Align2, Color32, CornerRadius, Frame, Margin, Pos2, Rect, RichText, Stroke, Ui, pos2, vec2};

pub struct WorkspaceState {
    pub tool: usize,
    pub sub_tool: usize,
    pub size: f32,
    pub opacity: f32,
    pub stabilization: f32,
    pub size_pressure: bool,
    pub opacity_pressure: bool,
    pub color: Hsv,
    pub color_tab: usize,
    pub layer_opacity: f32,
    pub blend: usize,
    pub doc_tab: usize,
}

impl Default for WorkspaceState {
    fn default() -> Self {
        Self {
            tool: 1,
            sub_tool: 0,
            size: 0.8,
            opacity: 100.0,
            stabilization: 12.0,
            size_pressure: true,
            opacity_pressure: false,
            color: Hsv { h: 0.62, s: 0.35, v: 0.12 },
            color_tab: 0,
            layer_opacity: 100.0,
            blend: 0,
            doc_tab: 0,
        }
    }
}

const TOOLS: [(&str, &str); 14] = [
    (icons::MAGNIFYING_GLASS, "Zoom"),
    (icons::PEN_NIB, "Pen"),
    (icons::PENCIL_SIMPLE, "Pencil"),
    (icons::PAINT_BRUSH, "Brush"),
    (icons::SPRAY_BOTTLE, "Airbrush"),
    (icons::ERASER, "Eraser"),
    (icons::PAINT_BUCKET, "Fill"),
    (icons::GRADIENT, "Gradient"),
    (icons::SELECTION, "Selection"),
    (icons::ARROWS_OUT_CARDINAL, "Move / transform"),
    (icons::SQUARES_FOUR, "Frame border"),
    (icons::TEXT_T, "Text"),
    (icons::CHAT_CIRCLE, "Balloon"),
    (icons::RULER, "Ruler"),
];

fn separator_v(ui: &mut Ui) {
    let p = palette(ui);
    let (r, _) = ui.allocate_exact_size(vec2(9.0, 20.0), egui::Sense::hover());
    ui.painter().line_segment([pos2(r.center().x, r.top()), pos2(r.center().x, r.bottom())], Stroke::new(1.0, p.border_strong));
}

pub fn show(ui: &mut Ui, s: &mut WorkspaceState) {
    let p = palette(ui);

    // ---- Menu bar ------------------------------------------------------------------------
    egui::Panel::top("menu")
        .exact_size(tokens::layout::MENU_BAR_H)
        .frame(Frame::new().fill(p.bg).inner_margin(Margin::symmetric(4, 0)))
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                for m in ["File", "Edit", "Page", "Layer", "Select", "View", "Filter", "Animation", "Window", "Help"] {
                    ui.add(Button::new(m).variant(ButtonVariant::Ghost).size(ButtonSize::Sm));
                }
            });
        });

    // ---- Tool options / command bar --------------------------------------------------------
    egui::Panel::top("command")
        .exact_size(tokens::layout::COMMAND_BAR_H + 2.0)
        .frame(Frame::new().fill(p.surface).inner_margin(Margin::symmetric(6, 0)))
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                for (i, l) in [(icons::FILE_PLUS, "New"), (icons::FOLDER_OPEN, "Open"), (icons::FLOPPY_DISK, "Save")] {
                    ui.add(IconButton::new(i, l).size(28.0));
                }
                separator_v(ui);
                for (i, l) in [(icons::ARROW_COUNTER_CLOCKWISE, "Undo"), (icons::ARROW_CLOCKWISE, "Redo")] {
                    ui.add(IconButton::new(i, l).size(28.0));
                }
                separator_v(ui);
                ui.add(IconButton::new(icons::SELECTION_SLASH, "Deselect").size(28.0));
                ui.add(IconButton::new(icons::FLIP_HORIZONTAL, "Flip canvas").size(28.0));
                ui.add(IconButton::new(icons::MAGNET, "Snap to ruler").selected(true).size(28.0));
                separator_v(ui);
                ui.label(RichText::new(icons::PEN_NIB).font(icon_font(tokens::icon::MD)).color(p.text_muted));
                ui.label(RichText::new("G-pen").text_style(text::body_strong()));
                ui.add_space(8.0);
                ui.scope(|ui| {
                    ui.spacing_mut().slider_width = 90.0;
                    slider_field(ui, "Size", &mut s.size, 0.1..=50.0, " mm", Some(&mut s.size_pressure));
                    ui.add_space(6.0);
                    slider_field(ui, "Opacity", &mut s.opacity, 0.0..=100.0, " %", None);
                    ui.add_space(6.0);
                    slider_field(ui, "Stabilization", &mut s.stabilization, 0.0..=100.0, "", None);
                });
            });
        });

    // ---- Status bar ------------------------------------------------------------------------
    egui::Panel::bottom("status")
        .exact_size(tokens::layout::STATUS_BAR_H)
        .frame(Frame::new().fill(p.bg).inner_margin(Margin::symmetric(10, 0)))
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                let st = |t: &str| RichText::new(t).text_style(text::caption()).color(p.text_muted);
                ui.label(st("Zoom 25.3%"));
                ui.add_space(12.0);
                ui.label(st("Rotate 0°"));
                ui.add_space(12.0);
                ui.label(st("4299 × 6071 px · 600 dpi · Monochrome"));
                ui.add_space(12.0);
                ui.label(st("Page 7 / 24 · Right binding"));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(st("Pressure 0.62 · Tilt 34°"));
                    ui.add_space(12.0);
                    ui.label(st("Autosaved 2 min ago"));
                });
            });
        });

    // ---- Tool column -----------------------------------------------------------------------
    egui::Panel::left("tools")
        .exact_size(tokens::layout::TOOL_COLUMN_W + 4.0)
        .resizable(false)
        .frame(Frame::new().fill(p.surface).inner_margin(Margin::symmetric(4, 6)))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            for (i, (icon, name)) in TOOLS.iter().enumerate() {
                if i == 8 || i == 10 {
                    ui.add_space(4.0);
                    let (r, _) = ui.allocate_exact_size(vec2(32.0, 1.0), egui::Sense::hover());
                    ui.painter().rect_filled(r, 0, p.border_strong);
                    ui.add_space(4.0);
                }
                if ui.add(IconButton::new(icon, *name).selected(s.tool == i)).clicked() {
                    s.tool = i;
                }
            }
            ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                ui.add_space(4.0);
                let (r, _) = ui.allocate_exact_size(vec2(32.0, 32.0), egui::Sense::hover());
                // Mini main/sub color chips at the bottom of the toolbox.
                let sub = Rect::from_min_size(r.min + vec2(12.0, 12.0), vec2(20.0, 20.0));
                let main = Rect::from_min_size(r.min, vec2(20.0, 20.0));
                ui.painter().rect(sub, CornerRadius::same(1), Color32::WHITE, Stroke::new(1.0, p.border_strong), egui::StrokeKind::Inside);
                ui.painter().rect(main, CornerRadius::same(1), s.color.to_color32(), Stroke::new(1.0, p.border_strong), egui::StrokeKind::Inside);
            });
        });

    // ---- Left dock: sub tool + tool property ----------------------------------------------
    egui::Panel::left("left-dock")
        .exact_size(232.0)
        .resizable(false)
        .frame(Frame::new().fill(p.surface))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            palette_header(ui, "Sub tool · Pen");
            Frame::new().inner_margin(Margin::symmetric(4, 4)).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 1.0;
                for (i, (icon, name)) in [
                    (icons::PEN_NIB, "G-pen"),
                    (icons::PEN_NIB, "Mapping pen"),
                    (icons::PEN_NIB, "Round pen"),
                    (icons::PEN, "Marker"),
                    (icons::PEN, "Textured ink"),
                    (icons::PEN_NIB_STRAIGHT, "Rough inker"),
                ]
                .iter()
                .enumerate()
                {
                    if list_row(ui, Some(icon), name, s.sub_tool == i).clicked() {
                        s.sub_tool = i;
                    }
                }
            });
            palette_header(ui, "Tool property · G-pen");
            Frame::new().inner_margin(Margin::symmetric(8, 6)).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                ui.spacing_mut().slider_width = 70.0;
                slider_field(ui, "Size", &mut s.size, 0.1..=50.0, " mm", Some(&mut s.size_pressure));
                slider_field(ui, "Opacity", &mut s.opacity, 0.0..=100.0, " %", Some(&mut s.opacity_pressure));
                slider_field(ui, "Stabilize", &mut s.stabilization, 0.0..=100.0, "", None);
                let mut taper = 30.0;
                slider_field(ui, "Taper", &mut taper, 0.0..=100.0, " %", None);
                ui.add_space(4.0);
                let mut aa = true;
                ui.checkbox(&mut aa, "Anti-aliasing");
                let mut vec_snap = true;
                ui.checkbox(&mut vec_snap, "Snap to vector");
                ui.add_space(6.0);
                // Pressure curve preview.
                let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 70.0), egui::Sense::hover());
                ui.painter().rect(r, CornerRadius::same(2), p.surface_sunken, Stroke::new(1.0, p.border), egui::StrokeKind::Inside);
                let pts: Vec<Pos2> = (0..=24)
                    .map(|i| {
                        let t = i as f32 / 24.0;
                        let y = t.powf(1.6);
                        pos2(r.left() + 6.0 + t * (r.width() - 12.0), r.bottom() - 6.0 - y * (r.height() - 12.0))
                    })
                    .collect();
                ui.painter().add(egui::Shape::line(pts, Stroke::new(1.5, p.accent_text)));
                ui.painter().text(r.left_top() + vec2(6.0, 4.0), Align2::LEFT_TOP, "Pressure → size", text::caption().resolve(ui.style()), p.text_muted);
            });
        });

    // ---- Right dock: color, navigator, layers -----------------------------------------------
    egui::Panel::right("right-dock")
        .exact_size(292.0)
        .resizable(false)
        .frame(Frame::new().fill(p.surface))
        .show(ui, |ui| {
            ui.set_max_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 0.0;
            palette_header(ui, "Color");
            Frame::new().inner_margin(Margin::symmetric(8, 4)).show(ui, |ui| {
                tab_strip(ui, &["Wheel", "Sliders", "Set", "History"], &mut s.color_tab);
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    color_wheel(ui, &mut s.color, 160.0);
                    ui.add_space(4.0);
                    ui.vertical(|ui| {
                        color_pair(ui, s.color.to_color32(), Color32::WHITE);
                    });
                });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 3.0;
                    let cols = [
                        Color32::BLACK, Color32::from_gray(64), Color32::from_gray(128), Color32::from_gray(192),
                        Color32::WHITE, Color32::from_rgb(214, 70, 60), Color32::from_rgb(232, 170, 60),
                        Color32::from_rgb(70, 150, 90), Color32::from_rgb(60, 110, 200), Color32::TRANSPARENT,
                    ];
                    for (i, c) in cols.into_iter().enumerate() {
                        swatch(ui, c, 18.0, i == 0);
                    }
                });
                ui.add_space(4.0);
            });
            palette_header(ui, "Navigator");
            Frame::new().inner_margin(Margin::symmetric(8, 6)).show(ui, |ui| {
                let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 92.0), egui::Sense::hover());
                ui.painter().rect_filled(r, 0, p.canvas_surround);
                let page = Rect::from_center_size(r.center(), vec2(58.0, 82.0));
                ui.painter().rect_filled(page, 0, Color32::WHITE);
                draw_page_art(ui, page, 0.12);
                let view = Rect::from_min_size(page.min + vec2(-4.0, 14.0), vec2(48.0, 40.0));
                ui.painter().rect_stroke(view, 0, Stroke::new(1.5, p.overlay_trim), egui::StrokeKind::Inside);
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().slider_width = 140.0;
                    let mut zoom = 25.3f32;
                    ui.add(egui::Slider::new(&mut zoom, 1.0..=400.0).logarithmic(true).suffix(" %").max_decimals(1));
                });
            });
            palette_header(ui, "Layer");
            Frame::new().inner_margin(Margin::symmetric(6, 4)).show(ui, |ui| {
                ui.horizontal(|ui| {
                    let modes = ["Normal", "Multiply", "Screen", "Overlay"];
                    egui::ComboBox::from_id_salt("ws-blend").width(110.0).selected_text(modes[s.blend]).show_ui(ui, |ui| {
                        for (i, m) in modes.iter().enumerate() {
                            ui.selectable_value(&mut s.blend, i, *m);
                        }
                    });
                    ui.add(egui::DragValue::new(&mut s.layer_opacity).range(0.0..=100.0).suffix(" %"));
                });
                ui.add_space(3.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 1.0;
                    for (i, l) in [
                        (icons::FILE_PLUS, "New raster layer"),
                        (icons::BEZIER_CURVE, "New vector layer"),
                        (icons::FOLDER_SIMPLE_PLUS, "New folder"),
                        (icons::INTERSECT_SQUARE, "Clip to layer below"),
                        (icons::FLAG, "Reference layer"),
                        (icons::MASK_HAPPY, "Add layer mask"),
                        (icons::TRASH, "Delete layer"),
                    ] {
                        ui.add(IconButton::new(i, l).selected(l == "Clip to layer below" && false).size(24.0));
                    }
                });
            });
            let layers: [(&str, LayerKind, u8, bool, bool, bool); 9] = [
                ("Balloons & text", LayerKind::Balloon, 0, true, false, false),
                ("Panels", LayerKind::Frame, 0, true, false, false),
                ("Panel 3 · Ink", LayerKind::Vector, 1, true, false, true),
                ("Tone 60 lpi 20%", LayerKind::Tone, 1, true, false, false),
                ("Flats", LayerKind::Raster, 1, true, false, false),
                ("Speed lines", LayerKind::Vector, 1, true, false, false),
                ("Sketch", LayerKind::Raster, 0, false, true, false),
                ("Rough layout", LayerKind::Raster, 0, false, true, false),
                ("Paper", LayerKind::Paper, 0, true, true, false),
            ];
            for (name, kind, depth, visible, locked, selected) in layers {
                let mut st = LayerRowState {
                    name: name.into(),
                    kind,
                    depth,
                    visible,
                    locked,
                    clipped: name.starts_with("Tone"),
                    has_mask: kind == LayerKind::Frame,
                    reference: name == "Panel 3 · Ink",
                    draft: name == "Sketch" || name == "Rough layout",
                    selected,
                    opacity: if name == "Sketch" { 90 } else { 255 },
                    blend: if name.starts_with("Tone") { "Multiply" } else { "Normal" },
                    thumb: None,
                };
                if kind == LayerKind::Paper {
                    st.reference = false;
                }
                layer_row(ui, &st);
            }
        });

    // ---- Center: document tabs + canvas ---------------------------------------------------
    egui::CentralPanel::default().frame(Frame::new().fill(p.canvas_surround)).show(ui, |ui| {
        let full = ui.max_rect();
        // Document tabs.
        let tabs_rect = Rect::from_min_size(full.min, vec2(full.width(), 28.0));
        ui.painter().rect_filled(tabs_rect, 0, p.bg);
        let mut x = tabs_rect.left();
        for (i, name) in ["chapter-03.inks · p.7", "chapter-03.inks · p.8", "cover-sketch.inks"].iter().enumerate() {
            let w = 190.0;
            let r = Rect::from_min_size(pos2(x, tabs_rect.top()), vec2(w, 28.0));
            let on = s.doc_tab == i;
            ui.painter().rect_filled(r, 0, if on { p.canvas_surround } else { p.bg });
            if on {
                ui.painter().line_segment([r.left_top(), r.right_top()], Stroke::new(2.0, p.accent_indicator));
            }
            ui.painter().line_segment([r.right_top(), r.right_bottom()], Stroke::new(1.0, p.border));
            ui.painter().text(pos2(r.left() + 12.0, r.center().y), Align2::LEFT_CENTER, *name, text::body().resolve(ui.style()), if on { p.text } else { p.text_muted });
            ui.painter().text(pos2(r.right() - 14.0, r.center().y), Align2::CENTER_CENTER, icons::X, icon_font(12.0), p.text_muted);
            if i == 0 {
                ui.painter().circle_filled(pos2(r.right() - 30.0, r.center().y), 3.0, p.text_muted); // unsaved dot
            }
            x += w;
        }

        // Canvas: B5 page, 3 mm bleed, inner border, panels.
        let area = Rect::from_min_max(pos2(full.left(), tabs_rect.bottom()), full.max).shrink(28.0);
        let (cw, ch) = (182.0 + 6.0 + 20.0, 257.0 + 6.0 + 20.0); // trim + bleed + margin, mm
        let scale = (area.height() / ch).min(area.width() / cw);
        let canvas = Rect::from_center_size(area.center(), vec2(cw * scale, ch * scale));
        // Drop shadow.
        ui.painter().rect_filled(canvas.translate(vec2(3.0, 4.0)), 0, Color32::from_black_alpha(70));
        ui.painter().rect_filled(canvas, 0, Color32::WHITE);
        let bleed = canvas.shrink(10.0 * scale);
        let trim = bleed.shrink(3.0 * scale);
        let safe = Rect::from_min_max(trim.min + vec2(15.0, 17.0) * scale, trim.max - vec2(15.0, 17.0) * scale);
        draw_page_art(ui, safe, scale);
        let dash = |a: Pos2, b: Pos2, c: Color32| {
            ui.painter().add(egui::Shape::dashed_line(&[a, b], Stroke::new(1.0, c), 6.0, 4.0));
        };
        for r in [(bleed, p.overlay_trim), (trim, p.overlay_trim), (safe, p.overlay_safe)] {
            let (q, c) = r;
            dash(q.left_top(), q.right_top(), c);
            dash(q.right_top(), q.right_bottom(), c);
            dash(q.right_bottom(), q.left_bottom(), c);
            dash(q.left_bottom(), q.left_top(), c);
        }
        // Crop marks.
        for (cx, cy) in [(trim.left(), trim.top()), (trim.right(), trim.top()), (trim.left(), trim.bottom()), (trim.right(), trim.bottom())] {
            let sx = if cx < canvas.center().x { -1.0 } else { 1.0 };
            let sy = if cy < canvas.center().y { -1.0 } else { 1.0 };
            ui.painter().line_segment([pos2(cx + sx * 4.0, cy), pos2(cx + sx * 9.0 * scale, cy)], Stroke::new(1.0, Color32::BLACK));
            ui.painter().line_segment([pos2(cx, cy + sy * 4.0), pos2(cx, cy + sy * 9.0 * scale)], Stroke::new(1.0, Color32::BLACK));
        }
        // Brush cursor over panel 3.
        let cur = pos2(safe.left() + safe.width() * 0.32, safe.top() + safe.height() * 0.66);
        ui.painter().circle_stroke(cur, 6.0, Stroke::new(3.0, Color32::from_white_alpha(160)));
        ui.painter().circle_stroke(cur, 6.0, Stroke::new(1.0, Color32::BLACK));
    });
}

/// Placeholder manga page: four panels with original stand-in art (lines, speed lines, tone, balloon).
fn draw_page_art(ui: &Ui, safe: Rect, scale: f32) {
    let painter = ui.painter_at(safe.expand(2.0));
    let ink = Color32::from_gray(20);
    let border = Stroke::new((0.8 * scale).max(1.0), ink);
    let gutter = 4.0 * scale;
    let (l, t, r, b) = (safe.left(), safe.top(), safe.right(), safe.bottom());
    let y1 = t + safe.height() * 0.30;
    let y2 = t + safe.height() * 0.62;
    let xm = l + safe.width() * 0.56;
    let panels = [
        vec![pos2(l, t), pos2(r, t), pos2(r, y1 - gutter / 2.0), pos2(l, y1 + gutter * 1.5)],
        vec![pos2(l, y1 + gutter * 2.5), pos2(xm - gutter / 2.0, y1 + gutter * 1.2), pos2(xm - gutter / 2.0, y2 - gutter / 2.0), pos2(l, y2 - gutter / 2.0)],
        vec![pos2(xm + gutter / 2.0, y1 + gutter * 0.9), pos2(r, y1 + gutter * 0.5), pos2(r, y2 - gutter / 2.0), pos2(xm + gutter / 2.0, y2 - gutter / 2.0)],
        vec![pos2(l, y2 + gutter / 2.0), pos2(r, y2 + gutter / 2.0), pos2(r, b), pos2(l, b)],
    ];

    // Panel 1: focus lines toward a point (speed / impact).
    let focus = pos2(l + safe.width() * 0.7, t + safe.height() * 0.15);
    let p1 = Rect::from_min_max(pos2(l, t), pos2(r, y1));
    let mut seed = 7u32;
    let mut rnd = || {
        seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        (seed >> 16) as f32 / 65_536.0
    };
    for i in 0..90 {
        let a = i as f32 / 90.0 * std::f32::consts::TAU + rnd() * 0.05;
        let far = focus + vec2(a.cos(), a.sin()) * safe.width();
        let near = focus + vec2(a.cos(), a.sin()) * (safe.width() * (0.16 + rnd() * 0.12));
        let clip = ui.painter_at(p1);
        clip.line_segment([near, far], Stroke::new((0.25 + rnd() * 0.5) * scale.max(1.0), ink));
    }
    // Panel 2: tone dots + horizon and a simple figure silhouette (original stand-in).
    let p2 = Rect::from_min_max(panels[1][0], panels[1][2]);
    let tone = ui.painter_at(p2);
    let step = (2.2 * scale).max(3.0);
    let mut y = p2.top();
    while y < p2.bottom() {
        let mut x = p2.left();
        while x < p2.right() {
            tone.circle_filled(pos2(x, y), step * 0.22, Color32::from_gray(120));
            x += step;
        }
        y += step;
    }
    tone.rect_filled(Rect::from_min_max(pos2(p2.left(), p2.top() + p2.height() * 0.62), p2.max), 0, Color32::WHITE);
    tone.line_segment([pos2(p2.left(), p2.top() + p2.height() * 0.62), pos2(p2.right(), p2.top() + p2.height() * 0.62)], Stroke::new(scale.max(1.0) * 0.6, ink));
    let head = pos2(p2.left() + p2.width() * 0.4, p2.top() + p2.height() * 0.4);
    tone.circle_filled(head, p2.height() * 0.11, Color32::WHITE);
    tone.circle_stroke(head, p2.height() * 0.11, Stroke::new(scale.max(1.0) * 0.6, ink));
    tone.line_segment([head + vec2(0.0, p2.height() * 0.11), head + vec2(0.0, p2.height() * 0.45)], Stroke::new(scale.max(1.0) * 0.7, ink));
    // Panel 3: perspective lines + a balloon with text.
    let p3 = Rect::from_min_max(panels[2][0], panels[2][2]);
    let pp = ui.painter_at(p3);
    let vp = pos2(p3.center().x, p3.top() + p3.height() * 0.35);
    for i in 0..9 {
        let x = p3.left() + i as f32 / 8.0 * p3.width();
        pp.line_segment([vp, pos2(x, p3.bottom())], Stroke::new(0.5 * scale.max(1.0), Color32::from_gray(110)));
    }
    let balloon = Rect::from_center_size(p3.center() + vec2(0.0, -p3.height() * 0.12), vec2(p3.width() * 0.62, p3.height() * 0.36));
    pp.add(egui::Shape::ellipse_filled(balloon.center(), balloon.size() / 2.0, Color32::WHITE));
    pp.add(egui::Shape::ellipse_stroke(balloon.center(), balloon.size() / 2.0, Stroke::new(scale.max(1.0) * 0.7, ink)));
    if scale > 1.0 {
        pp.text(balloon.center(), Align2::CENTER_CENTER, "Wait—\nnot yet!", egui::FontId::proportional(4.2 * scale), ink);
    }
    // Panel 4: horizontal speed lines.
    let p4 = Rect::from_min_max(panels[3][0], panels[3][2]);
    let sp = ui.painter_at(p4);
    for i in 0..40 {
        let yy = p4.top() + (i as f32 + 0.5) / 40.0 * p4.height();
        let x0 = p4.left() + rnd() * p4.width() * 0.35;
        sp.line_segment([pos2(x0, yy), pos2(p4.right(), yy)], Stroke::new((0.3 + rnd() * 0.4) * scale.max(1.0), ink));
    }
    // Panel borders on top.
    for poly in panels {
        painter.add(egui::Shape::closed_line(poly, border));
    }
}
