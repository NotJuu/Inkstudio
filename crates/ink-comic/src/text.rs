//! Lettering: horizontal and vertical (tategaki) text layout and glyph rasterization.
//!
//! Vertical rules implemented (the ones manga lettering needs):
//! * columns run top to bottom and are placed right to left;
//! * long vowel marks, dashes, tildes, ellipses and brackets are rotated 90°;
//! * 。、 sit in the upper-right of their cell; small kana are nudged up and right;
//! * Latin letters and digits stay upright, one per cell.
//!
//! Fonts come from a fallback chain: each character uses the first font that has it.

use ab_glyph::{Font, FontVec, PxScale, ScaleFont};
use ink_core::{Raster, Rgba};
use ink_vector::Pt;
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TextDir {
    Horizontal,
    #[default]
    Vertical,
}

/// Font fallback chain.
pub struct Fonts {
    faces: Vec<FontVec>,
    pub names: Vec<String>,
}

impl std::fmt::Debug for Fonts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Fonts").field("names", &self.names).finish()
    }
}

/// Font files to try, in order: Latin lettering fonts first, then Japanese-capable fonts.
const CANDIDATES: &[&str] = &[
    // Bundled with Inkstudio (open fonts).
    "assets/fonts/Inter-Regular.otf",
    "assets/fonts/NotoSansJP-Regular.otf",
    // Linux
    "/usr/share/fonts/opentype/inter/Inter-Medium.otf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/opentype/noto/NotoSansCJK-Medium.ttc",
    "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/truetype/wqy/wqy-zenhei.ttc",
    "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
    // Windows
    "C:\\Windows\\Fonts\\arial.ttf",
    "C:\\Windows\\Fonts\\YuGothM.ttc",
    "C:\\Windows\\Fonts\\meiryo.ttc",
    "C:\\Windows\\Fonts\\msgothic.ttc",
    // macOS
    "/System/Library/Fonts/Helvetica.ttc",
    "/System/Library/Fonts/ヒラギノ角ゴシック W5.ttc",
    "/System/Library/Fonts/Hiragino Sans GB.ttc",
    "/Library/Fonts/Arial Unicode.ttf",
];

impl Fonts {
    /// Load every available candidate (at most one Latin + all CJK-capable fonts found).
    pub fn system() -> Self {
        let mut faces = Vec::new();
        let mut names = Vec::new();
        for path in CANDIDATES {
            let p = Path::new(path);
            if let Ok(bytes) = std::fs::read(p) {
                if let Ok(f) = FontVec::try_from_vec_and_index(bytes, 0) {
                    faces.push(f);
                    names.push(p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
                }
            }
        }
        Self { faces, names }
    }

    pub fn from_bytes(fonts: Vec<Vec<u8>>) -> Self {
        let faces: Vec<FontVec> = fonts.into_iter().filter_map(|b| FontVec::try_from_vec_and_index(b, 0).ok()).collect();
        let names = (0..faces.len()).map(|i| format!("font {i}")).collect();
        Self { faces, names }
    }

    pub fn is_empty(&self) -> bool {
        self.faces.is_empty()
    }

    /// True if some font has a glyph for `c`.
    pub fn covers(&self, c: char) -> bool {
        self.face_for(c).is_some()
    }

    fn face_for(&self, c: char) -> Option<&FontVec> {
        self.faces.iter().find(|f| f.glyph_id(c).0 != 0)
    }
}

/// One placed glyph: `cell` is the box it is centered in (origin top-left, in px).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Placed {
    ch: char,
    cell_x: f32,
    cell_y: f32,
    cell: f32,
    rotate: bool,
    /// Offset inside the cell, as a fraction of the cell (0.5, 0.5 = centered).
    anchor: (f32, f32),
    /// Horizontal text uses the font's own advance and baseline instead of cells.
    baseline: Option<(f32, f32)>,
}

/// Laid-out text, relative to its own top-left (0, 0).
#[derive(Clone, Debug, Default)]
pub struct TextLayout {
    glyphs: Vec<Placed>,
    pub width: f32,
    pub height: f32,
    size: f32,
}

fn rotated_in_vertical(c: char) -> bool {
    matches!(
        c,
        'ー' | '－' | '-' | '—' | '―' | '–' | '~' | '〜' | '～' | '…' | '‥' | '(' | ')' | '（' | '）' | '「' | '」' | '『' | '』' | '[' | ']' | '【' | '】' | '〈' | '〉' | '《' | '》' | '<' | '>' | '='
    )
}

fn is_small_kana(c: char) -> bool {
    "ぁぃぅぇぉっゃゅょゎァィゥェォッャュョヮヵヶ".contains(c)
}

fn is_comma_period(c: char) -> bool {
    matches!(c, '。' | '、' | '．' | '，')
}

impl TextLayout {
    /// Lay out `text` at `size` px. Lines are separated by '\n'.
    pub fn new(fonts: &Fonts, text: &str, dir: TextDir, size: f32) -> Self {
        let size = size.max(1.0);
        let mut glyphs = Vec::new();
        let lines: Vec<&str> = text.split('\n').collect();
        match dir {
            TextDir::Vertical => {
                let pitch = size * 1.45;
                let ncols = lines.len().max(1);
                let tallest = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0) as f32 * size;
                let width = (ncols as f32 - 1.0) * pitch + size;
                for (col, line) in lines.iter().enumerate() {
                    // First line is the rightmost column.
                    let x = width - size - col as f32 * pitch;
                    for (row, ch) in line.chars().enumerate() {
                        let anchor = if is_comma_period(ch) {
                            (0.78, 0.28)
                        } else if is_small_kana(ch) {
                            (0.58, 0.42)
                        } else {
                            (0.5, 0.5)
                        };
                        glyphs.push(Placed { ch, cell_x: x, cell_y: row as f32 * size, cell: size, rotate: rotated_in_vertical(ch), anchor, baseline: None });
                    }
                }
                Self { glyphs, width, height: tallest, size }
            }
            TextDir::Horizontal => {
                let line_h = size * 1.25;
                let mut widths = Vec::new();
                let mut rows: Vec<Vec<(char, f32)>> = Vec::new();
                for line in &lines {
                    let mut x = 0.0;
                    let mut row = Vec::new();
                    for ch in line.chars() {
                        let adv = fonts.face_for(ch).map_or(size * 0.5, |f| f.as_scaled(PxScale::from(size)).h_advance(f.glyph_id(ch)));
                        row.push((ch, x));
                        x += adv;
                    }
                    widths.push(x);
                    rows.push(row);
                }
                let width = widths.iter().cloned().fold(0.0, f32::max);
                for (i, row) in rows.into_iter().enumerate() {
                    let off = (width - widths[i]) / 2.0; // centered, as in balloons
                    for (ch, x) in row {
                        glyphs.push(Placed {
                            ch,
                            cell_x: 0.0,
                            cell_y: 0.0,
                            cell: size,
                            rotate: false,
                            anchor: (0.5, 0.5),
                            baseline: Some((off + x, i as f32 * line_h + size * 0.92)),
                        });
                    }
                }
                Self { glyphs, width, height: lines.len() as f32 * line_h, size }
            }
        }
    }

    /// Draw with the layout's top-left at `origin`.
    pub fn render(&self, fonts: &Fonts, r: &mut Raster, origin: Pt, color: Rgba) {
        for g in &self.glyphs {
            let Some(face) = fonts.face_for(g.ch) else { continue };
            let scale = PxScale::from(self.size);
            let gid = face.glyph_id(g.ch);
            let glyph = gid.with_scale_and_position(scale, ab_glyph::point(0.0, 0.0));
            let Some(outline) = face.outline_glyph(glyph) else { continue };
            let b = outline.px_bounds();
            let (gw, gh) = (b.width().ceil() as usize + 1, b.height().ceil() as usize + 1);
            let mut bmp = vec![0f32; gw * gh];
            outline.draw(|x, y, c| {
                if let Some(v) = bmp.get_mut(y as usize * gw + x as usize) {
                    *v = c;
                }
            });
            // Destination: rotation swaps the bitmap's axes (90° clockwise).
            let (dw, dh) = if g.rotate { (gh, gw) } else { (gw, gh) };
            let (dx, dy) = match g.baseline {
                Some((bx, by)) => (origin.x + bx + b.min.x, origin.y + by + b.min.y),
                None => (
                    origin.x + g.cell_x + g.cell * g.anchor.0 - dw as f32 / 2.0,
                    origin.y + g.cell_y + g.cell * g.anchor.1 - dh as f32 / 2.0,
                ),
            };
            let (dx, dy) = (dx.round() as i32, dy.round() as i32);
            for y in 0..dh {
                for x in 0..dw {
                    let c = if g.rotate { bmp[(gh - 1 - x) * gw + y] } else { bmp[y * gw + x] };
                    if c <= 0.0 {
                        continue;
                    }
                    let (px, py) = (dx + x as i32, dy + y as i32);
                    let a = (c.min(1.0) * 255.0 + 0.5) as u8;
                    let d = r.get(px, py);
                    r.set(px, py, color.scale(a).over(d));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fonts() -> Option<Fonts> {
        let f = Fonts::system();
        (!f.is_empty() && f.covers('あ')).then_some(f)
    }

    #[test]
    fn vertical_columns_go_right_to_left_and_down() {
        let Some(f) = fonts() else { return eprintln!("no CJK font; skipped") };
        let l = TextLayout::new(&f, "あい\nう", TextDir::Vertical, 40.0);
        assert_eq!(l.glyphs.len(), 3);
        let a = l.glyphs[0];
        let i = l.glyphs[1];
        let u = l.glyphs[2];
        assert!(i.cell_y > a.cell_y && (i.cell_x - a.cell_x).abs() < 1e-3, "second char below the first");
        assert!(u.cell_x < a.cell_x, "second line left of the first");
        assert!((l.width - (1.45 * 40.0 + 40.0)).abs() < 1e-3);
    }

    #[test]
    fn long_vowel_is_rotated_and_period_sits_top_right() {
        let Some(f) = fonts() else { return };
        let l = TextLayout::new(&f, "ラーメン。", TextDir::Vertical, 40.0);
        assert!(l.glyphs[1].rotate);
        assert!(!l.glyphs[0].rotate);
        assert!(l.glyphs[4].anchor.0 > 0.7 && l.glyphs[4].anchor.1 < 0.35);
    }

    #[test]
    fn renders_ink_inside_the_layout_box() {
        let Some(f) = fonts() else { return };
        for dir in [TextDir::Vertical, TextDir::Horizontal] {
            let l = TextLayout::new(&f, "マンガ!\nHello", dir, 48.0);
            let mut r = Raster::new();
            l.render(&f, &mut r, Pt::new(100.0, 100.0), Rgba::BLACK);
            let px = r.read_rect(0, 0, 600, 600);
            let mut n = 0;
            for (i, c) in px.iter().enumerate() {
                if c.a() > 0 {
                    n += 1;
                    let (x, y) = ((i % 600) as f32, (i / 600) as f32);
                    assert!(x >= 95.0 && y >= 90.0 && x <= 105.0 + l.width && y <= 110.0 + l.height, "{dir:?} ink at {x},{y} outside {}x{}", l.width, l.height);
                }
            }
            assert!(n > 500, "{dir:?} drew {n} px");
        }
    }
}
