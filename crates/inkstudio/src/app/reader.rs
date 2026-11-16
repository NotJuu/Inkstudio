//! Reader: read the book like a printed manga. Right binding reads right to left: the cover
//! stands alone, then two-page spreads with the earlier page on the right.

use super::*;
use crate::book::Binding;
use ink_core::Document;

pub struct Reader {
    /// Index into `spreads()`.
    pub spread: usize,
    pub double: bool,
    tex: HashMap<usize, egui::TextureHandle>,
    generation: u64,
}

/// Pages grouped into what you see at once, in reading order. With `double`, the cover (page 0)
/// is alone, then pairs; each group lists pages in reading order.
pub fn spreads(n: usize, double: bool) -> Vec<Vec<usize>> {
    if n == 0 {
        return Vec::new();
    }
    if !double {
        return (0..n).map(|i| vec![i]).collect();
    }
    let mut out = vec![vec![0]];
    let mut i = 1;
    while i < n {
        if i + 1 < n {
            out.push(vec![i, i + 1]);
        } else {
            out.push(vec![i]);
        }
        i += 2;
    }
    out
}

/// Render a page for reading (no draft layers), at `width` pixels. Tiles are composited one
/// row at a time in parallel, so memory stays small even for 600 dpi pages.
fn render_page(doc: &Document, width: usize) -> egui::ColorImage {
    use rayon::prelude::*;
    let k = width as f32 / doc.width as f32;
    let (tw, th) = (
        width.max(1),
        ((doc.height as f32 * k).round() as usize).max(1),
    );
    let step = 1.0 / k;
    let paper = if doc.paper.a() == 0 {
        Rgba::WHITE
    } else {
        doc.paper
    };
    let mut px = vec![Color32::WHITE; tw * th];
    let (ntx, _) = doc.tiles_wh();
    let tile = ink_core::tile::TILE as f32;
    let mut y = 0;
    while y < th {
        let ty = (((y as f32 + 0.5) * step) / tile) as i32;
        let tiles: Vec<ink_core::Tile> = (0..ntx as i32)
            .into_par_iter()
            .map(|tx| doc.composite_tile((tx, ty), false))
            .collect();
        while y < th && ((((y as f32 + 0.5) * step) / tile) as i32) == ty {
            let cy = (((y as f32 + 0.5) * step) as i32).min(doc.height as i32 - 1);
            for x in 0..tw {
                let cx = (((x as f32 + 0.5) * step) as i32).min(doc.width as i32 - 1);
                let (key, lx, ly) = ink_core::tile::split(cx, cy);
                let c = tiles[key.0 as usize].get(lx, ly).over(paper).to_straight();
                px[y * tw + x] = Color32::from_rgb(c[0], c[1], c[2]);
            }
            y += 1;
        }
    }
    egui::ColorImage {
        size: [tw, th],
        source_size: vec2(tw as f32, th as f32),
        pixels: px,
    }
}

impl InkApp {
    pub fn open_reader(&mut self) {
        let Some(b) = &self.book else { return };
        let double = b.pages.len() > 2 && b.settings.kind == SheetKind::Book;
        let groups = spreads(b.pages.len(), double);
        let spread = groups
            .iter()
            .position(|g| g.contains(&b.current))
            .unwrap_or(0);
        self.reader = Some(Reader {
            spread,
            double,
            tex: HashMap::new(),
            generation: 0,
        });
    }

    /// Full-window reader on top of everything.
    pub(super) fn reader_ui(&mut self, ctx: &egui::Context) {
        let Some(b) = &self.book else {
            self.reader = None;
            return;
        };
        let rtl = b.settings.binding == Binding::Right;
        let r = self.reader.as_mut().unwrap();
        let groups = spreads(b.pages.len(), r.double);
        r.spread = r.spread.min(groups.len().saturating_sub(1));
        let screen = ctx.content_rect();
        let mut close = false;
        let mut delta: i32 = 0;
        let mut toggle_double = false;
        // ---- keys: in right-to-left books the left arrow moves forward -------------------------
        ctx.input(|i| {
            if i.key_pressed(Key::Escape) || i.key_pressed(Key::F5) {
                close = true;
            }
            let (fwd, back) = if rtl {
                (Key::ArrowLeft, Key::ArrowRight)
            } else {
                (Key::ArrowRight, Key::ArrowLeft)
            };
            if i.key_pressed(fwd) || i.key_pressed(Key::Space) || i.key_pressed(Key::PageDown) {
                delta += 1;
            }
            if i.key_pressed(back) || i.key_pressed(Key::PageUp) {
                delta -= 1;
            }
            if i.key_pressed(Key::Home) {
                delta = -10_000;
            }
            if i.key_pressed(Key::End) {
                delta = 10_000;
            }
            if i.key_pressed(Key::D) {
                toggle_double = true;
            }
            let s = i.smooth_scroll_delta.y;
            if s < -20.0 {
                delta += 1;
            } else if s > 20.0 {
                delta -= 1;
            }
        });
        let bar_h = 40.0;
        let area = Rect::from_min_max(screen.min + vec2(0.0, bar_h), screen.max - vec2(0.0, 28.0));
        let group = groups.get(r.spread).cloned().unwrap_or_default();
        // Page size on screen: fit the (single or double) spread into the area.
        let (pw, ph) = b
            .pages
            .first()
            .map_or((1.0, 1.4), |pg| (pg.doc.width as f32, pg.doc.height as f32));
        let cols = if r.double { 2.0 } else { 1.0 };
        let scale = ((area.width() - 40.0) / (pw * cols)).min((area.height() - 20.0) / ph);
        let size = vec2(pw * scale, ph * scale);
        // ---- textures: one new page per frame (current spread first, then neighbours) ---------
        let want: Vec<usize> = group
            .iter()
            .copied()
            .chain(groups.get(r.spread + 1).into_iter().flatten().copied())
            .chain(
                r.spread
                    .checked_sub(1)
                    .and_then(|s| groups.get(s))
                    .into_iter()
                    .flatten()
                    .copied(),
            )
            .collect();
        let px_w = ((size.x * ctx.pixels_per_point()).round() as usize).clamp(64, 2400);
        if let Some(i) = want.iter().copied().find(|i| {
            r.tex
                .get(i)
                .is_none_or(|t| (t.size()[0] as i32 - px_w as i32).abs() > 8)
        }) {
            let img = render_page(&b.pages[i].doc, px_w);
            r.generation += 1;
            r.tex.insert(
                i,
                ctx.load_texture(
                    format!("reader{i}-{}", r.generation),
                    img,
                    egui::TextureOptions::LINEAR,
                ),
            );
            ctx.request_repaint();
        }
        egui::Area::new(egui::Id::new("reader"))
            .order(egui::Order::Foreground)
            .fixed_pos(screen.min)
            .show(ctx, |ui| {
                let (rect, resp) = ui.allocate_exact_size(screen.size(), egui::Sense::click());
                let painter = ui.painter();
                painter.rect_filled(rect, 0, Color32::from_gray(18));
                // ---- top bar ------------------------------------------------------------------------
                let title = if b.info.title.trim().is_empty() {
                    b.settings.name.clone()
                } else {
                    b.info.title.clone()
                };
                painter.text(
                    pos2(rect.left() + 16.0, rect.top() + bar_h / 2.0),
                    Align2::LEFT_CENTER,
                    title,
                    egui::FontId::proportional(16.0),
                    Color32::from_gray(230),
                );
                let labels: Vec<String> = group.iter().map(|i| b.page_label(*i)).collect();
                let pos = format!(
                    "{}  ·  {} / {}",
                    labels.join(" – "),
                    r.spread + 1,
                    groups.len()
                );
                painter.text(
                    pos2(rect.center().x, rect.top() + bar_h / 2.0),
                    Align2::CENTER_CENTER,
                    pos,
                    egui::FontId::proportional(14.0),
                    Color32::from_gray(200),
                );
                let hint = if rtl {
                    tr("Right to left · ← next page · → previous · D single/double · Esc close")
                } else {
                    tr("Left to right · → next page · ← previous · D single/double · Esc close")
                };
                painter.text(
                    pos2(rect.center().x, rect.bottom() - 14.0),
                    Align2::CENTER_CENTER,
                    hint,
                    egui::FontId::proportional(12.0),
                    Color32::from_gray(140),
                );
                // ---- pages -----------------------------------------------------------------------------
                // Lay out reading order; in RTL the first page read sits on the right.
                let n = group.len() as f32;
                let total = vec2(size.x * n, size.y);
                let origin = pos2(
                    area.center().x - total.x / 2.0,
                    area.center().y - total.y / 2.0,
                );
                for (k, i) in group.iter().enumerate() {
                    let slot = if rtl { group.len() - 1 - k } else { k };
                    let mut pr =
                        Rect::from_min_size(origin + vec2(slot as f32 * size.x, 0.0), size);
                    // A lone cover sits on the side it will be on in print.
                    if r.double && group.len() == 1 {
                        let shift = if (*i == 0) == rtl {
                            size.x / 2.0
                        } else {
                            -size.x / 2.0
                        };
                        pr = pr.translate(vec2(shift, 0.0));
                    }
                    painter.rect_filled(
                        pr.translate(vec2(4.0, 6.0)),
                        0,
                        Color32::from_black_alpha(120),
                    );
                    match r.tex.get(i) {
                        Some(t) => {
                            painter.image(
                                t.id(),
                                pr,
                                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                                Color32::WHITE,
                            );
                        }
                        None => {
                            painter.rect_filled(pr, 0, Color32::from_gray(235));
                            painter.text(
                                pr.center(),
                                Align2::CENTER_CENTER,
                                tr("Loading…"),
                                egui::FontId::proportional(14.0),
                                Color32::from_gray(120),
                            );
                        }
                    }
                }
                if r.double && group.len() == 2 {
                    let x = origin.x + size.x;
                    painter.line_segment(
                        [pos2(x, origin.y), pos2(x, origin.y + size.y)],
                        Stroke::new(1.0, Color32::from_black_alpha(60)),
                    );
                }
                // ---- buttons -----------------------------------------------------------------------------
                let bar =
                    Rect::from_min_size(pos2(rect.right() - 300.0, rect.top()), vec2(300.0, bar_h));
                ui.scope_builder(
                    egui::UiBuilder::new()
                        .max_rect(bar.shrink2(vec2(8.0, 6.0)))
                        .layout(egui::Layout::right_to_left(egui::Align::Center)),
                    |ui| {
                        if ui
                            .add(IconButton::new(icons::X, tr("Close reader (Esc)")).size(28.0))
                            .clicked()
                        {
                            close = true;
                        }
                        let (icon, tip) = if r.double {
                            (icons::SQUARE, tr("Single pages (D)"))
                        } else {
                            (icons::BOOK_OPEN, tr("Two-page spreads (D)"))
                        };
                        if ui.add(IconButton::new(icon, tip).size(28.0)).clicked() {
                            toggle_double = true;
                        }
                        let (next_icon, prev_icon) = if rtl {
                            (icons::CARET_LEFT, icons::CARET_RIGHT)
                        } else {
                            (icons::CARET_RIGHT, icons::CARET_LEFT)
                        };
                        // Order on screen follows reading direction.
                        if rtl {
                            if ui
                                .add(IconButton::new(prev_icon, tr("Previous page")).size(28.0))
                                .clicked()
                            {
                                delta -= 1;
                            }
                            if ui
                                .add(IconButton::new(next_icon, tr("Next page")).size(28.0))
                                .clicked()
                            {
                                delta += 1;
                            }
                        } else {
                            if ui
                                .add(IconButton::new(next_icon, tr("Next page")).size(28.0))
                                .clicked()
                            {
                                delta += 1;
                            }
                            if ui
                                .add(IconButton::new(prev_icon, tr("Previous page")).size(28.0))
                                .clicked()
                            {
                                delta -= 1;
                            }
                        }
                    },
                );
                // Clicking the page side you read toward turns the page.
                if resp.clicked() {
                    if let Some(pos) = resp.interact_pointer_pos() {
                        if pos.y > rect.top() + bar_h {
                            let left = pos.x < rect.center().x;
                            delta += if left == rtl { 1 } else { -1 };
                        }
                    }
                }
            });
        let r = self.reader.as_mut().unwrap();
        if toggle_double {
            let first = group.first().copied().unwrap_or(0);
            r.double = !r.double;
            r.spread = spreads(b.pages.len(), r.double)
                .iter()
                .position(|g| g.contains(&first))
                .unwrap_or(0);
        } else if delta != 0 {
            r.spread = (r.spread as i64 + delta as i64)
                .clamp(0, groups.len().saturating_sub(1) as i64) as usize;
        }
        if close {
            // Return to the page that was being read.
            let at = spreads(b.pages.len(), r.double)
                .get(r.spread)
                .and_then(|g| g.first().copied());
            self.reader = None;
            if let (Some(at), Some(b)) = (at, &mut self.book) {
                if at != b.current {
                    b.current = at;
                    self.active_layer = Some(b.page().ink_layer);
                    self.after_page_change();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spreads_put_the_cover_alone_then_pairs() {
        assert_eq!(spreads(1, true), vec![vec![0]]);
        assert_eq!(spreads(5, true), vec![vec![0], vec![1, 2], vec![3, 4]]);
        assert_eq!(spreads(4, true), vec![vec![0], vec![1, 2], vec![3]]);
        assert_eq!(spreads(3, false).len(), 3);
    }

    #[test]
    fn reader_render_is_page_shaped() {
        let mut doc = Document::new(1000, 1400, 72);
        doc.paper = Rgba::WHITE;
        let id = doc.push_layer(Layer::new("ink"));
        doc.layer_mut(id).unwrap().raster.set(500, 700, Rgba::BLACK);
        let img = render_page(&doc, 200);
        assert_eq!(img.size, [200, 280]);
        assert_eq!(img.pixels[0], Color32::WHITE);
    }
}
