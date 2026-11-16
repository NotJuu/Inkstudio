//! CPU renderer for egui output: turns tessellated meshes into an RGBA8 image.
//!
//! Used for design-system screenshots and golden-image UI tests in CI, where no GPU exists.
//! The real app renders with wgpu; this only needs to be faithful enough to review layouts.

use egui::epaint::{ClippedPrimitive, ImageDelta, Primitive, TextureId};
use egui::{Color32, Context, ImageData, RawInput, TexturesDelta};
use std::collections::HashMap;

pub struct Image {
    pub width: usize,
    pub height: usize,
    /// Premultiplied-alpha RGBA, row-major.
    pub rgba: Vec<u8>,
}

#[derive(Default)]
pub struct Renderer {
    textures: HashMap<TextureId, (usize, usize, Vec<Color32>)>,
}

impl Renderer {
    pub fn update_textures(&mut self, delta: &TexturesDelta) {
        for (id, deltas) in &delta.set {
            for d in deltas {
                self.set(*id, d);
            }
        }
        for id in &delta.free {
            self.textures.remove(id);
        }
    }

    fn set(&mut self, id: TextureId, d: &ImageDelta) {
        let (w, h, px): (usize, usize, Vec<Color32>) = match &d.image {
            ImageData::Color(img) => (img.width(), img.height(), img.pixels.clone()),
        };
        match d.pos {
            None => {
                self.textures.insert(id, (w, h, px));
            }
            Some([x0, y0]) => {
                let tex = self
                    .textures
                    .get_mut(&id)
                    .expect("partial update of unknown texture");
                for y in 0..h {
                    for x in 0..w {
                        tex.2[(y0 + y) * tex.0 + x0 + x] = px[y * w + x];
                    }
                }
            }
        }
    }

    fn sample(&self, id: TextureId, u: f32, v: f32) -> [f32; 4] {
        let Some((w, h, px)) = self.textures.get(&id) else {
            return [1.0; 4];
        };
        // Bilinear, clamp to edge.
        let fx = (u * *w as f32 - 0.5).clamp(0.0, *w as f32 - 1.0);
        let fy = (v * *h as f32 - 0.5).clamp(0.0, *h as f32 - 1.0);
        let (x0, y0) = (fx.floor() as usize, fy.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
        let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
        let get = |x: usize, y: usize| {
            let c = px[y * w + x];
            [c.r() as f32, c.g() as f32, c.b() as f32, c.a() as f32]
        };
        let (a, b, c, d) = (get(x0, y0), get(x1, y0), get(x0, y1), get(x1, y1));
        let mut out = [0.0; 4];
        for i in 0..4 {
            let top = a[i] + (b[i] - a[i]) * tx;
            let bot = c[i] + (d[i] - c[i]) * tx;
            out[i] = (top + (bot - top) * ty) / 255.0;
        }
        out
    }

    /// Rasterize primitives (in points) at `ppp` pixels per point onto a cleared canvas.
    pub fn render(
        &self,
        prims: &[ClippedPrimitive],
        width: usize,
        height: usize,
        ppp: f32,
        clear: Color32,
    ) -> Image {
        let mut buf: Vec<[f32; 4]> = vec![
            [
                clear.r() as f32 / 255.0,
                clear.g() as f32 / 255.0,
                clear.b() as f32 / 255.0,
                clear.a() as f32 / 255.0
            ];
            width * height
        ];
        for cp in prims {
            let Primitive::Mesh(mesh) = &cp.primitive else {
                continue;
            };
            let clip = cp.clip_rect;
            let cx0 = ((clip.min.x * ppp).floor().max(0.0)) as i32;
            let cy0 = ((clip.min.y * ppp).floor().max(0.0)) as i32;
            let cx1 = ((clip.max.x * ppp).ceil() as i32).min(width as i32);
            let cy1 = ((clip.max.y * ppp).ceil() as i32).min(height as i32);
            for tri in mesh.indices.chunks_exact(3) {
                let v = [
                    &mesh.vertices[tri[0] as usize],
                    &mesh.vertices[tri[1] as usize],
                    &mesh.vertices[tri[2] as usize],
                ];
                let pts = v.map(|v| (v.pos.x * ppp, v.pos.y * ppp));
                let area = (pts[1].0 - pts[0].0) * (pts[2].1 - pts[0].1)
                    - (pts[2].0 - pts[0].0) * (pts[1].1 - pts[0].1);
                if area.abs() < 1e-6 {
                    continue;
                }
                let minx =
                    (pts.iter().map(|p| p.0).fold(f32::MAX, f32::min).floor() as i32).max(cx0);
                let maxx =
                    (pts.iter().map(|p| p.0).fold(f32::MIN, f32::max).ceil() as i32).min(cx1);
                let miny =
                    (pts.iter().map(|p| p.1).fold(f32::MAX, f32::min).floor() as i32).max(cy0);
                let maxy =
                    (pts.iter().map(|p| p.1).fold(f32::MIN, f32::max).ceil() as i32).min(cy1);
                let cols = v.map(|v| {
                    let c = v.color;
                    [
                        c.r() as f32 / 255.0,
                        c.g() as f32 / 255.0,
                        c.b() as f32 / 255.0,
                        c.a() as f32 / 255.0,
                    ]
                });
                for y in miny..maxy {
                    for x in minx..maxx {
                        let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                        let w0 = ((pts[1].0 - px) * (pts[2].1 - py)
                            - (pts[2].0 - px) * (pts[1].1 - py))
                            / area;
                        let w1 = ((pts[2].0 - px) * (pts[0].1 - py)
                            - (pts[0].0 - px) * (pts[2].1 - py))
                            / area;
                        let w2 = 1.0 - w0 - w1;
                        if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                            continue;
                        }
                        let u = w0 * v[0].uv.x + w1 * v[1].uv.x + w2 * v[2].uv.x;
                        let vv = w0 * v[0].uv.y + w1 * v[1].uv.y + w2 * v[2].uv.y;
                        let t = self.sample(mesh.texture_id, u, vv);
                        let mut src = [0.0f32; 4];
                        for i in 0..4 {
                            src[i] = (w0 * cols[0][i] + w1 * cols[1][i] + w2 * cols[2][i]) * t[i];
                        }
                        // Premultiplied "over", in gamma space like egui's own renderers.
                        let dst = &mut buf[y as usize * width + x as usize];
                        for i in 0..4 {
                            dst[i] = src[i] + dst[i] * (1.0 - src[3]);
                        }
                    }
                }
            }
        }
        let rgba = buf
            .iter()
            .flat_map(|c| c.map(|v| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8))
            .collect();
        Image {
            width,
            height,
            rgba,
        }
    }
}

/// Run `ui` for a few frames (so layout and focus settle) and render the last one.
pub fn screenshot(
    ctx: &Context,
    size: egui::Vec2,
    ppp: f32,
    clear: Color32,
    mut ui: impl FnMut(&mut egui::Ui),
) -> Image {
    let mut r = Renderer::default();
    let mut last = None;
    for _ in 0..4 {
        let input = RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            focused: true,
            ..Default::default()
        };
        ctx.set_pixels_per_point(ppp);
        let out = ctx.run_ui(input, |u| ui(u));
        r.update_textures(&out.textures_delta);
        last = Some(ctx.tessellate(out.shapes, out.pixels_per_point));
    }
    let (w, h) = (
        (size.x * ppp).round() as usize,
        (size.y * ppp).round() as usize,
    );
    r.render(&last.unwrap(), w, h, ppp, clear)
}
