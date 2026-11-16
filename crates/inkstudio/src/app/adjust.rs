//! Layer settings beyond the palette row: rename, duplicate, merge down, and color
//! adjustments (brightness/contrast, hue/saturation/lightness, invert, threshold, posterize)
//! with live preview and one undo step.

use super::*;
use ink_core::Tile;
use ink_core::tile::TileKey;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Adjustments {
    /// −100..100
    pub brightness: f32,
    /// −100..100
    pub contrast: f32,
    /// −180..180 degrees
    pub hue: f32,
    /// −100..100
    pub saturation: f32,
    /// −100..100
    pub lightness: f32,
    pub invert: bool,
    /// Turn into pure black/white at this luminance (manga 1-bit).
    pub threshold: Option<u8>,
    /// Number of tone levels per channel (2..=16).
    pub posterize: Option<u8>,
}

impl Default for Adjustments {
    fn default() -> Self {
        Self {
            brightness: 0.0,
            contrast: 0.0,
            hue: 0.0,
            saturation: 0.0,
            lightness: 0.0,
            invert: false,
            threshold: None,
            posterize: None,
        }
    }
}

fn rgb_to_hsl(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let (mx, mn) = (r.max(g).max(b), r.min(g).min(b));
    let l = (mx + mn) / 2.0;
    if mx - mn < 1e-6 {
        return (0.0, 0.0, l);
    }
    let d = mx - mn;
    let s = if l > 0.5 {
        d / (2.0 - mx - mn)
    } else {
        d / (mx + mn)
    };
    let h = if mx == r {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if mx == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    (h / 6.0, s, l)
}

fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (f32, f32, f32) {
    if s <= 0.0 {
        return (l, l, l);
    }
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    let f = |mut t: f32| {
        t = t.rem_euclid(1.0);
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };
    (f(h + 1.0 / 3.0), f(h), f(h - 1.0 / 3.0))
}

impl Adjustments {
    pub fn is_identity(&self) -> bool {
        *self == Self::default()
    }

    /// Adjust one premultiplied pixel.
    pub fn apply(&self, c: Rgba) -> Rgba {
        let a = c.a();
        if a == 0 {
            return c;
        }
        let s = c.to_straight();
        let (mut r, mut g, mut b) = (
            s[0] as f32 / 255.0,
            s[1] as f32 / 255.0,
            s[2] as f32 / 255.0,
        );
        if self.brightness != 0.0 || self.contrast != 0.0 {
            let k = if self.contrast >= 0.0 {
                1.0 / (1.0 - self.contrast / 100.0 * 0.99)
            } else {
                1.0 + self.contrast / 100.0
            };
            let off = self.brightness / 100.0;
            let f = |v: f32| (v - 0.5) * k + 0.5 + off;
            (r, g, b) = (f(r), f(g), f(b));
        }
        if self.hue != 0.0 || self.saturation != 0.0 || self.lightness != 0.0 {
            let (h, sat, l) = rgb_to_hsl(r.clamp(0.0, 1.0), g.clamp(0.0, 1.0), b.clamp(0.0, 1.0));
            let h = h + self.hue / 360.0;
            let sat = (sat * (1.0 + self.saturation / 100.0)).clamp(0.0, 1.0);
            let l = if self.lightness >= 0.0 {
                l + (1.0 - l) * self.lightness / 100.0
            } else {
                l * (1.0 + self.lightness / 100.0)
            };
            (r, g, b) = hsl_to_rgb(h, sat, l.clamp(0.0, 1.0));
        }
        if self.invert {
            (r, g, b) = (1.0 - r, 1.0 - g, 1.0 - b);
        }
        if let Some(n) = self.posterize {
            let n = n.clamp(2, 16) as f32 - 1.0;
            let f = |v: f32| (v.clamp(0.0, 1.0) * n).round() / n;
            (r, g, b) = (f(r), f(g), f(b));
        }
        if let Some(t) = self.threshold {
            let lum = 0.2126 * r + 0.7152 * g + 0.0722 * b;
            let v = if lum * 255.0 >= t as f32 { 1.0 } else { 0.0 };
            (r, g, b) = (v, v, v);
        }
        let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        Rgba::from_straight(q(r), q(g), q(b), a)
    }
}

/// The open "Adjust colors" dialog: original tiles kept for preview, cancel and undo.
pub struct AdjustDialog {
    layer: LayerId,
    page: usize,
    orig: Vec<(TileKey, Arc<Tile>)>,
    pub a: Adjustments,
    shown: Adjustments,
}

/// Apply `a` to the original tiles (in parallel) and put the results into the layer.
fn preview(doc: &mut Document, layer: LayerId, orig: &[(TileKey, Arc<Tile>)], a: &Adjustments) {
    use rayon::prelude::*;
    let tiles: Vec<(TileKey, Arc<Tile>)> = orig
        .par_iter()
        .map(|(k, t)| {
            if a.is_identity() {
                return (*k, t.clone());
            }
            let mut n = (**t).clone();
            for px in n.px.iter_mut() {
                *px = a.apply(*px);
            }
            (*k, Arc::new(n))
        })
        .collect();
    if let Some(l) = doc.layer_mut(layer) {
        let keys: Vec<TileKey> = tiles.iter().map(|(k, _)| *k).collect();
        for (k, t) in tiles {
            l.raster.put_tile(k, Some(t));
        }
        doc.mark_dirty(keys);
    }
}

use ink_core::Document;

impl InkApp {
    pub fn open_adjust(&mut self, now: f64) {
        self.commit_transform();
        let Some(target) = self.target_layer(now) else {
            return;
        };
        let b = self.book.as_mut().unwrap();
        let page = b.current;
        let l = b.page().doc.layer(target).unwrap();
        let orig: Vec<(TileKey, Arc<Tile>)> = l
            .raster
            .keys()
            .filter_map(|k| l.raster.tile(*k).map(|t| (*k, t.clone())))
            .collect();
        if orig.is_empty() {
            self.toast(
                ToastKind::Info,
                tr("This layer is empty: nothing to adjust."),
                now,
            );
            return;
        }
        self.adjust = Some(AdjustDialog {
            layer: target,
            page,
            orig,
            a: Adjustments::default(),
            shown: Adjustments::default(),
        });
    }

    /// Finish the adjust dialog: keep (one undo step) or restore.
    pub fn close_adjust(&mut self, keep: bool) {
        let Some(d) = self.adjust.take() else { return };
        let Some(b) = self.book.as_mut() else { return };
        let page = &mut b.pages[d.page];
        if keep && !d.a.is_identity() {
            if d.shown != d.a {
                preview(&mut page.doc, d.layer, &d.orig, &d.a);
            }
            page.push_doc(UndoEntry::Tiles {
                label: "Adjust colors".into(),
                layer: d.layer,
                tiles: d.orig.into_iter().map(|(k, t)| (k, Some(t))).collect(),
            });
        } else {
            preview(&mut page.doc, d.layer, &d.orig, &Adjustments::default());
        }
    }

    pub(super) fn adjust_dialog(&mut self, ctx: &egui::Context) {
        let p = theme::palette_ctx(ctx);
        let Some(d) = &mut self.adjust else { return };
        let name = self
            .book
            .as_ref()
            .and_then(|b| b.pages[d.page].doc.layer(d.layer))
            .map(|l| tr(&l.props.name).to_string())
            .unwrap_or_default();
        let mut done = None;
        egui::Modal::new(egui::Id::new("adjust"))
            .frame(modal_frame(p))
            .show(ctx, |ui| {
                ui.set_width(420.0);
                ui.label(RichText::new(tr("Adjust colors")).text_style(text::title()));
                ui.label(
                    RichText::new(trf(
                        "Layer: {name} · changes show live on the canvas",
                        &[("name", name)],
                    ))
                    .color(p.text_muted),
                );
                ui.add_space(tokens::space::S4);
                let a = &mut d.a;
                ui.spacing_mut().slider_width = 220.0;
                ui.label(
                    RichText::new(tr("Brightness & contrast")).text_style(text::body_strong()),
                );
                ui.add(
                    egui::Slider::new(&mut a.brightness, -100.0..=100.0)
                        .text(tr("Brightness"))
                        .max_decimals(0),
                );
                ui.add(
                    egui::Slider::new(&mut a.contrast, -100.0..=100.0)
                        .text(tr("Contrast"))
                        .max_decimals(0),
                );
                ui.add_space(6.0);
                ui.label(
                    RichText::new(tr("Hue, saturation, lightness")).text_style(text::body_strong()),
                );
                ui.add(
                    egui::Slider::new(&mut a.hue, -180.0..=180.0)
                        .text(tr("Hue"))
                        .suffix("°")
                        .max_decimals(0),
                );
                ui.add(
                    egui::Slider::new(&mut a.saturation, -100.0..=100.0)
                        .text(tr("Saturation"))
                        .max_decimals(0),
                );
                ui.add(
                    egui::Slider::new(&mut a.lightness, -100.0..=100.0)
                        .text(tr("Lightness"))
                        .max_decimals(0),
                );
                ui.add_space(6.0);
                ui.label(RichText::new(tr("Effects")).text_style(text::body_strong()));
                ui.checkbox(&mut a.invert, tr("Invert"));
                ui.horizontal(|ui| {
                    let mut on = a.threshold.is_some();
                    if ui
                        .checkbox(&mut on, tr("Threshold (black & white)"))
                        .changed()
                    {
                        a.threshold = on.then_some(128);
                    }
                    if let Some(t) = &mut a.threshold {
                        ui.add(egui::Slider::new(t, 1..=254));
                    }
                });
                ui.horizontal(|ui| {
                    let mut on = a.posterize.is_some();
                    if ui.checkbox(&mut on, tr("Posterize")).changed() {
                        a.posterize = on.then_some(4);
                    }
                    if let Some(n) = &mut a.posterize {
                        ui.add(egui::Slider::new(n, 2..=16).text(tr("levels")));
                    }
                });
                ui.add_space(tokens::space::S5);
                ui.horizontal(|ui| {
                    if ui
                        .add(
                            Button::new(tr("OK"))
                                .variant(ButtonVariant::Primary)
                                .min_width(80.0),
                        )
                        .clicked()
                    {
                        done = Some(true);
                    }
                    if ui.add(Button::new(tr("Cancel")).min_width(80.0)).clicked()
                        || ui.input(|i| i.key_pressed(Key::Escape))
                    {
                        done = Some(false);
                    }
                    if ui.add(Button::new(tr("Reset")).min_width(80.0)).clicked() {
                        *a = Adjustments::default();
                    }
                });
            });
        // Live preview: recompute only when something changed.
        if let (Some(d), Some(b)) = (&mut self.adjust, &mut self.book) {
            if d.a != d.shown {
                preview(&mut b.pages[d.page].doc, d.layer, &d.orig, &d.a);
                d.shown = d.a;
            }
        }
        if let Some(keep) = done {
            self.close_adjust(keep);
        }
    }

    /// "Layer" menu.
    pub(super) fn layer_menu(&mut self, ui: &mut Ui, now: f64) {
        let has = self.book.is_some();
        let sel = self.book.as_ref().and_then(|b| {
            self.active_layer
                .filter(|id| b.page().doc.layer(*id).is_some() && !b.page().is_object_layer(*id))
        });
        if ui
            .add_enabled(has, egui::Button::new(tr("New layer")))
            .clicked()
        {
            self.new_layer();
            ui.close();
        }
        if ui
            .add_enabled(
                sel.is_some(),
                egui::Button::new(format!("{}  (Ctrl+J)", tr("Duplicate layer"))),
            )
            .clicked()
        {
            self.duplicate_layer();
            ui.close();
        }
        if ui
            .add_enabled(sel.is_some(), egui::Button::new(tr("Merge down")))
            .clicked()
        {
            self.merge_down(now);
            ui.close();
        }
        if ui
            .add_enabled(sel.is_some(), egui::Button::new(tr("Rename layer…")))
            .clicked()
        {
            if let (Some(id), Some(b)) = (sel, &self.book) {
                self.rename_layer = Some((id, b.page().doc.layer(id).unwrap().props.name.clone()));
            }
            ui.close();
        }
        ui.separator();
        if ui
            .add_enabled(
                sel.is_some(),
                egui::Button::new(format!("{}  (Ctrl+U)", tr("Adjust colors…"))),
            )
            .clicked()
        {
            self.open_adjust(now);
            ui.close();
        }
        if ui
            .add_enabled(
                sel.is_some(),
                egui::Button::new(format!("{}  (Ctrl+T)", tr("Transform"))),
            )
            .clicked()
        {
            self.begin_transform(now);
            ui.close();
        }
        if ui
            .add_enabled(
                has,
                egui::Button::new(format!("{}  (Ctrl+Shift+I)", tr("Import image…"))),
            )
            .clicked()
        {
            self.import_image_dialog();
            ui.close();
        }
    }

    pub fn new_layer(&mut self) {
        let Some(b) = self.book.as_mut() else { return };
        let page = b.page_mut();
        let snap = page.doc.snapshot_layers();
        let at = self
            .active_layer
            .and_then(|a| page.doc.index_of(a))
            .map_or(page.doc.layers().len(), |i| i + 1);
        let n = page.doc.layers().len() + 1;
        let id = page
            .doc
            .add_layer(Layer::new(trf("Layer {n}", &[("n", n.to_string())])), at);
        page.push_doc(UndoEntry::Layers {
            label: "New layer".into(),
            layers: snap,
        });
        self.active_layer = Some(id);
    }

    /// Copy the selected layer (pixels and settings) right above it.
    pub fn duplicate_layer(&mut self) {
        let Some(b) = self.book.as_mut() else { return };
        let page = b.page_mut();
        let Some(id) = self
            .active_layer
            .filter(|id| page.doc.layer(*id).is_some() && !page.is_object_layer(*id))
        else {
            return;
        };
        let snap = page.doc.snapshot_layers();
        let mut l = page.doc.layer(id).unwrap().clone();
        l.props.name = format!("{} {}", tr(&l.props.name), tr("copy"));
        let at = page.doc.index_of(id).map_or(0, |i| i + 1);
        let new = page.doc.add_layer(l, at);
        page.doc.mark_all_dirty();
        page.push_doc(UndoEntry::Layers {
            label: "Duplicate layer".into(),
            layers: snap,
        });
        self.active_layer = Some(new);
    }

    /// Paint the selected layer onto the one below (with its blend mode and opacity) and remove it.
    pub fn merge_down(&mut self, now: f64) {
        let Some(b) = self.book.as_mut() else { return };
        let page = b.page_mut();
        let Some(id) = self.active_layer.filter(|id| page.doc.layer(*id).is_some()) else {
            return;
        };
        let i = page.doc.index_of(id).unwrap_or(0);
        if i == 0 {
            return;
        }
        let below = page.doc.layers()[i - 1].0;
        if page.is_object_layer(id) || page.is_object_layer(below) {
            self.toast(
                ToastKind::Warning,
                tr("Panel and balloon layers can't be merged."),
                now,
            );
            return;
        }
        let snap = page.doc.snapshot_layers();
        let upper = page.doc.layer(id).unwrap().clone();
        let lower = page.doc.layer_mut(below).unwrap();
        for k in upper.raster.keys() {
            let src = upper.raster.tile(*k).unwrap();
            let dst = lower.raster.tile_mut(*k);
            for (s, d) in src.px.iter().zip(dst.px.iter_mut()) {
                if s.a() == 0 {
                    continue;
                }
                let s = if upper.props.opacity == 255 {
                    *s
                } else {
                    s.scale(upper.props.opacity)
                };
                *d = upper.props.blend.composite(s, *d);
            }
        }
        page.doc.remove_layer(id);
        page.doc.mark_all_dirty();
        page.push_doc(UndoEntry::Layers {
            label: "Merge down".into(),
            layers: snap,
        });
        self.active_layer = Some(below);
    }

    pub(super) fn rename_layer_dialog(&mut self, ctx: &egui::Context) {
        let p = theme::palette_ctx(ctx);
        let Some((id, name)) = &mut self.rename_layer else {
            return;
        };
        let id = *id;
        let mut done = None;
        egui::Modal::new(egui::Id::new("rename-layer"))
            .frame(modal_frame(p))
            .show(ctx, |ui| {
                ui.set_width(340.0);
                ui.label(RichText::new(tr("Rename layer")).text_style(text::title()));
                ui.add_space(8.0);
                let r = ui.add(egui::TextEdit::singleline(name).desired_width(f32::INFINITY));
                r.request_focus();
                if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    done = Some(true);
                }
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui
                        .add(
                            Button::new(tr("OK"))
                                .variant(ButtonVariant::Primary)
                                .min_width(80.0),
                        )
                        .clicked()
                    {
                        done = Some(true);
                    }
                    if ui.add(Button::new(tr("Cancel")).min_width(80.0)).clicked()
                        || ui.input(|i| i.key_pressed(Key::Escape))
                    {
                        done = Some(false);
                    }
                });
            });
        if let Some(ok) = done {
            let (_, name) = self.rename_layer.take().unwrap();
            if ok && !name.trim().is_empty() {
                if let Some(b) = &mut self.book {
                    let page = b.page_mut();
                    let snap = page.doc.snapshot_layers();
                    if let Some(l) = page.doc.layer_mut(id) {
                        l.props.name = name.trim().to_string();
                        page.push_doc(UndoEntry::Layers {
                            label: "Rename layer".into(),
                            layers: snap,
                        });
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adjustments() {
        let grey = Rgba::from_straight(100, 100, 100, 255);
        assert_eq!(Adjustments::default().apply(grey), grey);
        let inv = Adjustments {
            invert: true,
            ..Default::default()
        }
        .apply(grey)
        .to_straight();
        assert_eq!(inv[0], 155);
        let bright = Adjustments {
            brightness: 50.0,
            ..Default::default()
        }
        .apply(grey)
        .to_straight();
        assert!(bright[0] > 200);
        let th = Adjustments {
            threshold: Some(128),
            ..Default::default()
        };
        assert_eq!(th.apply(grey).to_straight()[..3], [0, 0, 0]);
        assert_eq!(
            th.apply(Rgba::from_straight(200, 200, 200, 255))
                .to_straight()[..3],
            [255, 255, 255]
        );
        // Hue 120° turns red into green; alpha is kept.
        let g = Adjustments {
            hue: 120.0,
            ..Default::default()
        }
        .apply(Rgba::from_straight(255, 0, 0, 128))
        .to_straight();
        assert!(g[1] > 200 && g[0] < 30 && g[3] == 128, "{g:?}");
        let gray = Adjustments {
            saturation: -100.0,
            ..Default::default()
        }
        .apply(Rgba::from_straight(255, 0, 0, 255))
        .to_straight();
        assert_eq!(gray[0], gray[1]);
        let post = Adjustments {
            posterize: Some(2),
            ..Default::default()
        }
        .apply(Rgba::from_straight(90, 170, 30, 255))
        .to_straight();
        assert_eq!(post[..3], [0, 255, 0]);
    }
}
