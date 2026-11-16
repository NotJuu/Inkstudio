//! Coloring tools: eyedropper (and Alt-pick), lasso fill, gradient, "fill all closed areas"
//! (flats), one-click shading layer, and the Color palette (wheel, sliders + hex, sets, history).

use super::*;
use ink_core::tile::{TILE, TileKey};
use ink_core::{Document, Tile};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GradientMode {
    MainToSub,
    MainToTransparent,
}

/// Distinct, mid-saturated flat colors (golden-ratio hue steps) for auto flats.
pub fn flat_color(i: usize) -> Rgba {
    let h = (0.13 + i as f32 * 0.618_034).fract();
    let s = 0.35 + 0.15 * ((i / 7) % 2) as f32;
    let v = 0.82 + 0.1 * ((i / 3) % 2) as f32;
    let c = Hsv { h, s, v }.to_color32();
    Rgba::from_straight(c.r(), c.g(), c.b(), 255)
}

/// Gradient color at parameter t (0..1) in straight space.
fn lerp_color(a: [u8; 4], b: [u8; 4], t: f32) -> Rgba {
    let l = |x: u8, y: u8| {
        (x as f32 + (y as f32 - x as f32) * t)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    Rgba::from_straight(l(a[0], b[0]), l(a[1], b[1]), l(a[2], b[2]), l(a[3], b[3]))
}

/// Paint a linear gradient from `a` to `b` over the whole layer. Returns the undo entry.
pub fn gradient(
    doc: &mut Document,
    target: LayerId,
    a: Pt,
    b: Pt,
    from: [u8; 4],
    to: [u8; 4],
) -> Option<UndoEntry> {
    use rayon::prelude::*;
    let (tw, th) = doc.tiles_wh();
    let (w, h) = (doc.width as usize, doc.height as usize);
    let layer = doc.layer_mut(target)?;
    if layer.props.locked {
        return None;
    }
    let lock_alpha = layer.props.lock_alpha;
    let d = b.sub(a);
    let len2 = d.dot(d).max(1e-3);
    let keys: Vec<TileKey> = (0..th as i32)
        .flat_map(|y| (0..tw as i32).map(move |x| (x, y)))
        .filter(|k| !lock_alpha || layer.raster.tile(*k).is_some())
        .collect();
    let old: Vec<(TileKey, Option<Arc<Tile>>)> = keys
        .iter()
        .map(|k| (*k, layer.raster.tile(*k).cloned()))
        .collect();
    let new: Vec<(TileKey, Tile)> = old
        .par_iter()
        .map(|(k, t)| {
            let mut tile = t.as_deref().cloned().unwrap_or_else(Tile::empty);
            for ly in 0..TILE {
                let y = k.1 as usize * TILE + ly;
                if y >= h {
                    break;
                }
                for lx in 0..TILE {
                    let x = k.0 as usize * TILE + lx;
                    if x >= w {
                        break;
                    }
                    let p = Pt::new(x as f32 + 0.5, y as f32 + 0.5).sub(a);
                    let t = (p.dot(d) / len2).clamp(0.0, 1.0);
                    let c = lerp_color(from, to, t);
                    let i = ly * TILE + lx;
                    let base = tile.px[i];
                    tile.px[i] = if lock_alpha {
                        c.scale(base.a())
                    } else {
                        c.over(base)
                    };
                }
            }
            (*k, tile)
        })
        .collect();
    for (k, t) in new {
        layer.raster.put_tile(k, Some(Arc::new(t)));
    }
    doc.mark_dirty(keys);
    Some(UndoEntry::Tiles {
        label: "Gradient".into(),
        layer: target,
        tiles: old,
    })
}

/// Fill a polygon on a layer with undo.
pub fn fill_lasso(
    doc: &mut Document,
    target: LayerId,
    poly: &[Pt],
    color: Rgba,
) -> Option<UndoEntry> {
    if poly.len() < 3 {
        return None;
    }
    let (w, h) = (doc.width as f32, doc.height as f32);
    let layer = doc.layer_mut(target)?;
    if layer.props.locked {
        return None;
    }
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for p in poly {
        x0 = x0.min(p.x);
        y0 = y0.min(p.y);
        x1 = x1.max(p.x);
        y1 = y1.max(p.y);
    }
    let (x0, y0, x1, y1) = (x0.max(0.0), y0.max(0.0), x1.min(w - 1.0), y1.min(h - 1.0));
    if x1 < x0 || y1 < y0 {
        return None;
    }
    let keys: Vec<TileKey> =
        ink_core::tile::keys_in(x0 as i32 - 2, y0 as i32 - 2, x1 as i32 + 2, y1 as i32 + 2)
            .filter(|k| k.0 >= 0 && k.1 >= 0)
            .collect();
    let old: Vec<(TileKey, Option<Arc<Tile>>)> = keys
        .iter()
        .map(|k| (*k, layer.raster.tile(*k).cloned()))
        .collect();
    let clipped: Vec<Pt> = poly
        .iter()
        .map(|p| Pt::new(p.x.clamp(0.0, w), p.y.clamp(0.0, h)))
        .collect();
    ink_comic::paint::fill_polygon(&mut layer.raster, &clipped, color);
    doc.mark_dirty(keys);
    Some(UndoEntry::Tiles {
        label: "Lasso fill".into(),
        layer: target,
        tiles: old,
    })
}

/// Color under a canvas point (composited, as seen). Transparent → paper.
pub fn pick(doc: &Document, pt: Pt) -> Option<[u8; 3]> {
    if pt.x < 0.0 || pt.y < 0.0 || pt.x >= doc.width as f32 || pt.y >= doc.height as f32 {
        return None;
    }
    let (k, lx, ly) = ink_core::tile::split(pt.x as i32, pt.y as i32);
    let t = doc.composite_tile(k, false);
    let c = t.get(lx, ly);
    let s = c
        .over(if doc.paper.a() == 0 {
            Rgba::WHITE
        } else {
            doc.paper
        })
        .to_straight();
    Some([s[0], s[1], s[2]])
}

fn rgb_of(c: Color32) -> [u8; 3] {
    [c.r(), c.g(), c.b()]
}

impl InkApp {
    /// Remember a color in the history (newest first, 16 entries).
    pub(super) fn remember_color(&mut self) {
        let c = rgb_of(self.color.to_color32());
        let h = &mut self.settings.color_history;
        if h.first() == Some(&c) {
            return;
        }
        h.retain(|x| *x != c);
        h.insert(0, c);
        h.truncate(16);
        self.settings_dirty = true;
    }

    pub(super) fn eyedropper_event(&mut self, ev: PenEvent) {
        let s = match ev {
            PenEvent::Down(s) | PenEvent::Move(s) => s,
            PenEvent::Up(_) => {
                self.remember_color();
                return;
            }
        };
        let c = self.view.to_canvas(s.pos);
        let Some(b) = &self.book else { return };
        if let Some(rgb) = pick(&b.page().doc, Pt::new(c.0, c.1)) {
            self.color = hsv_from(Color32::from_rgb(rgb[0], rgb[1], rgb[2]));
        }
    }

    pub(super) fn lasso_event(&mut self, ev: PenEvent, now: f64) {
        let (s, phase) = match ev {
            PenEvent::Down(s) => (s, 0),
            PenEvent::Move(s) => (s, 1),
            PenEvent::Up(s) => (s, 2),
        };
        let c = self.view.to_canvas(s.pos);
        let pt = Pt::new(c.0, c.1);
        match phase {
            0 => self.lasso = vec![pt],
            1 => {
                if self
                    .lasso
                    .last()
                    .is_some_and(|l| l.sub(pt).len() * self.view.zoom > 2.0)
                {
                    self.lasso.push(pt);
                }
            }
            _ => {
                let poly = std::mem::take(&mut self.lasso);
                if poly.len() < 3 {
                    return;
                }
                let Some(target) = self.target_layer(now) else {
                    return;
                };
                let color = to_rgba(self.color.to_color32());
                let page = self.book.as_mut().unwrap().page_mut();
                if let Some(e) = fill_lasso(&mut page.doc, target, &poly, color) {
                    page.push_doc(e);
                }
                self.remember_color();
            }
        }
    }

    pub(super) fn gradient_event(&mut self, ev: PenEvent, now: f64) {
        let (s, phase) = match ev {
            PenEvent::Down(s) => (s, 0),
            PenEvent::Move(s) => (s, 1),
            PenEvent::Up(s) => (s, 2),
        };
        let c = self.view.to_canvas(s.pos);
        let pt = Pt::new(c.0, c.1);
        match phase {
            0 => self.gradient = Some((pt, pt)),
            1 => {
                if let Some(g) = &mut self.gradient {
                    g.1 = pt;
                }
            }
            _ => {
                let Some((a, _)) = self.gradient.take() else {
                    return;
                };
                let b = pt;
                if a.sub(b).len() * self.view.zoom < 4.0 {
                    return;
                }
                let Some(target) = self.target_layer(now) else {
                    return;
                };
                let m = self.color.to_color32();
                let from = [m.r(), m.g(), m.b(), 255];
                let to = match self.gradient_mode {
                    GradientMode::MainToSub => self.sub_color.to_straight(),
                    GradientMode::MainToTransparent => [m.r(), m.g(), m.b(), 0],
                };
                let page = self.book.as_mut().unwrap().page_mut();
                if let Some(e) = gradient(&mut page.doc, target, a, b, from, to) {
                    page.push_doc(e);
                }
                self.remember_color();
            }
        }
    }

    /// Fill every closed area of the line art with its own flat color on a new "Flats" layer
    /// under the ink.
    pub fn fill_all_areas(&mut self, now: f64) {
        let Some(b) = self.book.as_mut() else { return };
        let page = b.page_mut();
        let snap = page.doc.snapshot_layers();
        let below = page.doc.index_of(page.ink_layer).unwrap_or(0);
        let id = page.doc.add_layer(Layer::new(tr("Flats")), below);
        page.push_doc(UndoEntry::Layers {
            label: "New layer".into(),
            layers: snap,
        });
        let mut o = self.fill;
        if o.refer == Refer::Target {
            o.refer = Refer::References;
        }
        let min_area = ((b.settings.dpi as f32 / 25.4).powi(2) * 2.0) as usize; // ≈ 2 mm²
        let page = b.page_mut();
        match ink_raster::fill::fill_all_regions(&mut page.doc, id, &o, min_area, flat_color) {
            Some((e, n)) => {
                page.push_doc(e);
                self.active_layer = Some(id);
                self.toast(
                    ToastKind::Success,
                    trf(
                        "{n} areas filled on the Flats layer",
                        &[("n", n.to_string())],
                    ),
                    now,
                );
            }
            None => {
                self.toast(
                    ToastKind::Info,
                    tr("No closed areas found. Draw line art on the Ink layer first."),
                    now,
                );
                self.undo();
            }
        }
    }

    /// New layer clipped to the selected one, ready for shadows or highlights.
    pub fn add_clipped_layer(&mut self, name: &str, blend: BlendMode) {
        let Some(b) = self.book.as_mut() else { return };
        let page = b.page_mut();
        let base = self
            .active_layer
            .filter(|id| page.doc.layer(*id).is_some() && !page.is_object_layer(*id))
            .unwrap_or(page.ink_layer);
        let at = page.doc.index_of(base).map_or(0, |i| i + 1);
        let snap = page.doc.snapshot_layers();
        let mut l = Layer::new(tr(name));
        l.props.blend = blend;
        l.props.clip_below = true;
        let id = page.doc.add_layer(l, at);
        page.push_doc(UndoEntry::Layers {
            label: "New layer".into(),
            layers: snap,
        });
        self.active_layer = Some(id);
    }

    /// Color palette: wheel / sliders / sets, plus history.
    pub(super) fn color_pane(&mut self, ui: &mut Ui) {
        let p = palette(ui);
        egui::ScrollArea::vertical()
            .id_salt("color")
            .show(ui, |ui| {
                egui::Frame::new()
                    .inner_margin(egui::Margin::symmetric(8, 6))
                    .show(ui, |ui| {
                        tab_strip(
                            ui,
                            &[tr("Wheel"), tr("Sliders"), tr("Set")],
                            &mut self.color_tab,
                        );
                        ui.add_space(6.0);
                        match self.color_tab {
                            0 => {
                                ui.horizontal(|ui| {
                                    let size = (ui.available_width() - 70.0).clamp(100.0, 220.0);
                                    if color_wheel(ui, &mut self.color, size).drag_stopped() {
                                        self.remember_color();
                                    }
                                    ui.add_space(4.0);
                                    let s = self.sub_color.to_straight();
                                    let (_, _, swap) = color_pair(
                                        ui,
                                        self.color.to_color32(),
                                        Color32::from_rgb(s[0], s[1], s[2]),
                                    );
                                    if swap {
                                        self.swap_colors();
                                    }
                                });
                            }
                            1 => self.color_sliders(ui),
                            _ => self.color_sets(ui),
                        }
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new(tr("Recent colors"))
                                .text_style(text::caption())
                                .color(p.text_muted),
                        );
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing = vec2(3.0, 3.0);
                            if self.settings.color_history.is_empty() {
                                ui.label(
                                    RichText::new(tr("Colors you paint with appear here."))
                                        .text_style(text::caption())
                                        .color(p.text_muted),
                                );
                            }
                            for c in self.settings.color_history.clone() {
                                let c = Color32::from_rgb(c[0], c[1], c[2]);
                                if swatch(ui, c, 18.0, self.color.to_color32() == c).clicked() {
                                    self.color = hsv_from(c);
                                }
                            }
                        });
                    });
            });
    }

    fn color_sliders(&mut self, ui: &mut Ui) {
        let p = palette(ui);
        ui.spacing_mut().slider_width = (ui.available_width() - 120.0).max(60.0);
        let mut h = self.color.h * 360.0;
        let mut s = self.color.s * 100.0;
        let mut v = self.color.v * 100.0;
        let mut changed = false;
        ui.label(
            RichText::new("HSV")
                .text_style(text::caption())
                .color(p.text_muted),
        );
        changed |= ui
            .add(
                egui::Slider::new(&mut h, 0.0..=360.0)
                    .text("H")
                    .suffix("°")
                    .max_decimals(0),
            )
            .changed();
        changed |= ui
            .add(
                egui::Slider::new(&mut s, 0.0..=100.0)
                    .text("S")
                    .suffix(" %")
                    .max_decimals(0),
            )
            .changed();
        changed |= ui
            .add(
                egui::Slider::new(&mut v, 0.0..=100.0)
                    .text("V")
                    .suffix(" %")
                    .max_decimals(0),
            )
            .changed();
        if changed {
            self.color = Hsv {
                h: (h / 360.0).clamp(0.0, 1.0),
                s: s / 100.0,
                v: v / 100.0,
            };
        }
        let c = self.color.to_color32();
        let mut rgb = [c.r() as f32, c.g() as f32, c.b() as f32];
        let mut rgb_changed = false;
        ui.label(
            RichText::new("RGB")
                .text_style(text::caption())
                .color(p.text_muted),
        );
        for (i, n) in ["R", "G", "B"].iter().enumerate() {
            rgb_changed |= ui
                .add(
                    egui::Slider::new(&mut rgb[i], 0.0..=255.0)
                        .text(*n)
                        .max_decimals(0),
                )
                .changed();
        }
        if rgb_changed {
            self.color = hsv_from(Color32::from_rgb(rgb[0] as u8, rgb[1] as u8, rgb[2] as u8));
        }
        ui.horizontal(|ui| {
            ui.label(tr("Hex"));
            let c = self.color.to_color32();
            let mut hex = format!("#{:02X}{:02X}{:02X}", c.r(), c.g(), c.b());
            let r = ui.add(
                egui::TextEdit::singleline(&mut hex)
                    .desired_width(80.0)
                    .font(egui::TextStyle::Monospace),
            );
            if r.changed() {
                if let Some(c) = parse_hex(&hex) {
                    self.color = hsv_from(c);
                }
            }
            if r.lost_focus() {
                self.remember_color();
            }
            let (sw, _) = ui.allocate_exact_size(vec2(32.0, 18.0), egui::Sense::hover());
            ui.painter().rect_filled(sw, 2, c);
        });
    }

    fn color_sets(&mut self, ui: &mut Ui) {
        let p = palette(ui);
        if self.settings.color_sets.is_empty() {
            self.settings.color_sets = crate::settings::default_color_sets();
        }
        self.color_set = self.color_set.min(self.settings.color_sets.len() - 1);
        ui.horizontal(|ui| {
            let name = self.settings.color_sets[self.color_set].0.clone();
            egui::ComboBox::from_id_salt("color-set")
                .width(140.0)
                .selected_text(tr(&name))
                .show_ui(ui, |ui| {
                    for (i, (n, _)) in self.settings.color_sets.iter().enumerate() {
                        ui.selectable_value(&mut self.color_set, i, tr(n));
                    }
                });
            if ui
                .add(IconButton::new(icons::PLUS, tr("New color set")).size(24.0))
                .clicked()
            {
                let n = self.settings.color_sets.len() + 1;
                self.settings
                    .color_sets
                    .push((format!("{} {n}", tr("My colors")), Vec::new()));
                self.color_set = self.settings.color_sets.len() - 1;
                self.settings_dirty = true;
            }
            if ui
                .add_enabled(
                    self.settings.color_sets.len() > 1,
                    IconButton::new(icons::TRASH, tr("Delete color set")).size(24.0),
                )
                .clicked()
            {
                self.settings.color_sets.remove(self.color_set);
                self.color_set = 0;
                self.settings_dirty = true;
            }
        });
        let cur = self.color.to_color32();
        let mut remove = None;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(3.0, 3.0);
            for (i, c) in self.settings.color_sets[self.color_set]
                .1
                .clone()
                .into_iter()
                .enumerate()
            {
                let c = Color32::from_rgb(c[0], c[1], c[2]);
                let r = swatch(ui, c, 22.0, cur == c)
                    .on_hover_text(tr("Click to use · right-click to remove"));
                if r.clicked() {
                    self.color = hsv_from(c);
                    self.remember_color();
                }
                if r.secondary_clicked() {
                    remove = Some(i);
                }
            }
            if ui
                .add(IconButton::new(icons::PLUS, tr("Add the main color to this set")).size(22.0))
                .clicked()
            {
                self.settings.color_sets[self.color_set].1.push(rgb_of(cur));
                self.settings_dirty = true;
            }
        });
        if let Some(i) = remove {
            self.settings.color_sets[self.color_set].1.remove(i);
            self.settings_dirty = true;
        }
        ui.label(
            RichText::new(tr("Sets are saved with your settings."))
                .text_style(text::caption())
                .color(p.text_muted),
        );
    }

    /// Coloring helpers shown under the Fill / Lasso / Gradient tools.
    pub(super) fn coloring_actions(&mut self, ui: &mut Ui) {
        let p = palette(ui);
        let now = ui.input(|i| i.time);
        ui.add_space(6.0);
        ui.label(
            RichText::new(tr("Quick coloring"))
                .text_style(text::caption())
                .color(p.text_muted),
        );
        let w = ui.available_width().min(260.0);
        if ui.add(Button::new(tr("Fill closed areas (flats)")).icon(icons::MAGIC_WAND).min_width(w)).on_hover_text(tr("Every area enclosed by line art gets its own flat color on a new layer under the ink.")).clicked() {
            self.fill_all_areas(now);
        }
        if ui
            .add(
                Button::new(tr("Shadow layer"))
                    .icon(icons::STACK_PLUS)
                    .min_width(w),
            )
            .on_hover_text(tr(
                "New layer clipped to the selected one, blend mode Multiply",
            ))
            .clicked()
        {
            self.add_clipped_layer("Shadow", BlendMode::Multiply);
        }
        if ui
            .add(Button::new(tr("Light layer")).icon(icons::SUN).min_width(w))
            .on_hover_text(tr(
                "New layer clipped to the selected one, blend mode Screen",
            ))
            .clicked()
        {
            self.add_clipped_layer("Light", BlendMode::Screen);
        }
    }
}

fn parse_hex(s: &str) -> Option<Color32> {
    let s = s.trim().trim_start_matches('#');
    let v = |i: usize| u8::from_str_radix(s.get(i..i + 2)?, 16).ok();
    match s.len() {
        6 => Some(Color32::from_rgb(v(0)?, v(2)?, v(4)?)),
        3 => {
            let d = |i: usize| {
                u8::from_str_radix(s.get(i..i + 1)?, 16)
                    .ok()
                    .map(|x| x * 17)
            };
            Some(Color32::from_rgb(d(0)?, d(1)?, d(2)?))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_and_flats() {
        assert_eq!(parse_hex("#FF8000"), Some(Color32::from_rgb(255, 128, 0)));
        assert_eq!(parse_hex("0f0"), Some(Color32::from_rgb(0, 255, 0)));
        assert_eq!(parse_hex("zz"), None);
        let a: Vec<Rgba> = (0..12).map(flat_color).collect();
        for i in 0..a.len() {
            for j in i + 1..a.len() {
                assert_ne!(a[i], a[j]);
            }
        }
    }

    #[test]
    fn gradient_and_lasso_are_undoable() {
        let mut doc = Document::new(600, 300, 72);
        let id = doc.push_layer(Layer::new("L"));
        let e = gradient(
            &mut doc,
            id,
            Pt::new(0.0, 0.0),
            Pt::new(600.0, 0.0),
            [0, 0, 0, 255],
            [255, 255, 255, 255],
        )
        .unwrap();
        let l = doc.layer(id).unwrap();
        assert!(l.raster.get(5, 10).to_straight()[0] < 10);
        assert!(l.raster.get(595, 10).to_straight()[0] > 245);
        let mid = l.raster.get(300, 10).to_straight()[0];
        assert!((120..136).contains(&mid), "{mid}");
        let UndoEntry::Tiles { tiles, .. } = &e else {
            panic!()
        };
        assert!(
            tiles.iter().all(|(_, t)| t.is_none()),
            "layer was empty before"
        );

        let mut doc = Document::new(400, 400, 72);
        let id = doc.push_layer(Layer::new("L"));
        let tri = [
            Pt::new(10.0, 10.0),
            Pt::new(300.0, 10.0),
            Pt::new(10.0, 300.0),
        ];
        fill_lasso(&mut doc, id, &tri, Rgba::BLACK).unwrap();
        let l = doc.layer(id).unwrap();
        assert_eq!(l.raster.get(50, 50), Rgba::BLACK);
        assert_eq!(l.raster.get(290, 290).a(), 0);
        assert_eq!(pick(&doc, Pt::new(50.0, 50.0)), Some([0, 0, 0]));
        assert_eq!(
            pick(&doc, Pt::new(390.0, 390.0)).map(|c| c[0] > 200),
            Some(true)
        );
    }
}
