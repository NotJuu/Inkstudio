//! Frame-by-frame animation.
//!
//! A `Track` is an animation folder: its cels are ordinary layers of the page's `Document`.
//! Exposure keys say which cel shows from a frame on (holds are implicit until the next key;
//! a `None` key is a blank). Applying a frame just toggles cel visibility, so drawing,
//! undo, fill and every brush work on cels unchanged. Onion skin tints neighbouring cels.

use ink_core::tile::{TileKey, TILE_PX};
use ink_core::{Document, Layer, LayerId, Rgba, Tile};
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub struct Track {
    pub name: String,
    pub cels: Vec<LayerId>,
    /// frame → cel index (None = blank from this frame on).
    pub keys: BTreeMap<usize, Option<usize>>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OnionSkin {
    pub enabled: bool,
    pub before: usize,
    pub after: usize,
    pub before_color: Rgba,
    pub after_color: Rgba,
    /// Opacity of the nearest onion frame (farther ones fade).
    pub opacity: f32,
}

impl Default for OnionSkin {
    fn default() -> Self {
        Self {
            enabled: true,
            before: 2,
            after: 1,
            before_color: Rgba::from_straight(220, 60, 60, 255),
            after_color: Rgba::from_straight(40, 150, 80, 255),
            opacity: 0.35,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Timeline {
    pub fps: f32,
    pub frames: usize,
    pub tracks: Vec<Track>,
    pub onion: OnionSkin,
    pub looping: bool,
}

impl Timeline {
    pub fn new(fps: f32, frames: usize) -> Self {
        Self { fps: fps.clamp(1.0, 120.0), frames: frames.max(1), tracks: Vec::new(), onion: OnionSkin::default(), looping: true }
    }

    pub fn add_track(&mut self, name: impl Into<String>) -> usize {
        self.tracks.push(Track { name: name.into(), cels: Vec::new(), keys: BTreeMap::new() });
        self.tracks.len() - 1
    }

    /// Cel index shown on `frame` (the last key at or before it).
    pub fn cel_index(&self, track: usize, frame: usize) -> Option<usize> {
        self.tracks.get(track)?.keys.range(..=frame).next_back().and_then(|(_, v)| *v)
    }

    pub fn cel_at(&self, track: usize, frame: usize) -> Option<LayerId> {
        let t = self.tracks.get(track)?;
        self.cel_index(track, frame).and_then(|i| t.cels.get(i).copied())
    }

    /// True if `frame` starts a new drawing (a key) on `track`.
    pub fn is_key(&self, track: usize, frame: usize) -> bool {
        self.tracks.get(track).is_some_and(|t| t.keys.contains_key(&frame))
    }

    /// Expose `cel` (or blank) from `frame` on.
    pub fn set_key(&mut self, track: usize, frame: usize, cel: Option<usize>) {
        if let Some(t) = self.tracks.get_mut(track) {
            t.keys.insert(frame, cel);
        }
    }

    pub fn clear_key(&mut self, track: usize, frame: usize) {
        if let Some(t) = self.tracks.get_mut(track) {
            t.keys.remove(&frame);
        }
    }

    /// Create a new cel layer in `doc` and expose it from `frame`. Returns its layer id.
    pub fn new_cel(&mut self, doc: &mut Document, track: usize, frame: usize) -> Option<LayerId> {
        let t = self.tracks.get_mut(track)?;
        let n = t.cels.len() + 1;
        // Insert right above the track's last cel so the stack stays grouped.
        let at = t.cels.iter().filter_map(|c| doc.index_of(*c)).max().map_or(doc.layers().len(), |i| i + 1);
        let id = doc.add_layer(Layer::new(format!("{} {}", t.name, n)), at);
        t.cels.push(id);
        t.keys.insert(frame, Some(n - 1));
        Some(id)
    }

    /// The cel to draw on at `frame`; creates one if the frame is blank.
    pub fn drawing_cel(&mut self, doc: &mut Document, track: usize, frame: usize) -> Option<LayerId> {
        match self.cel_at(track, frame) {
            Some(id) => Some(id),
            None => self.new_cel(doc, track, frame),
        }
    }

    /// Show only the cels exposed on `frame`.
    pub fn apply(&self, doc: &mut Document, frame: usize) {
        for (ti, t) in self.tracks.iter().enumerate() {
            let shown = self.cel_at(ti, frame);
            for &c in &t.cels {
                if let Some(l) = doc.layer_mut(c) {
                    let v = Some(c) == shown;
                    if l.props.visible != v {
                        l.props.visible = v;
                    }
                }
            }
        }
        doc.mark_all_dirty();
    }

    /// Onion layers for `frame`: (layer, tint, alpha 0..1). Distinct drawings only.
    pub fn onion_layers(&self, frame: usize) -> Vec<(LayerId, Rgba, f32)> {
        if !self.onion.enabled {
            return Vec::new();
        }
        let mut out = Vec::new();
        for (ti, _) in self.tracks.iter().enumerate() {
            let cur = self.cel_at(ti, frame);
            let mut collect = |dir: i64, count: usize, color: Rgba| {
                let mut last = cur;
                let mut found = 0;
                let mut f = frame as i64;
                while found < count {
                    f += dir;
                    if f < 0 || f >= self.frames as i64 {
                        break;
                    }
                    let c = self.cel_at(ti, f as usize);
                    if c.is_some() && c != last {
                        found += 1;
                        out.push((c.unwrap(), color, self.onion.opacity / found as f32));
                        last = c;
                    }
                }
            };
            collect(-1, self.onion.before, self.onion.before_color);
            collect(1, self.onion.after, self.onion.after_color);
        }
        out
    }

    pub fn frame_duration(&self) -> f32 {
        1.0 / self.fps
    }

    /// Next frame for playback.
    pub fn next_frame(&self, frame: usize) -> usize {
        if frame + 1 >= self.frames {
            if self.looping { 0 } else { frame }
        } else {
            frame + 1
        }
    }
}

/// Composite a display tile with onion-skin cels tinted on top.
pub fn composite_with_onion(doc: &Document, key: TileKey, onion: &[(LayerId, Rgba, f32)]) -> Tile {
    let mut t = doc.composite_tile(key, true);
    for &(id, tint, alpha) in onion {
        let Some(src) = doc.layer(id).and_then(|l| l.raster.tile(key)) else { continue };
        let k = (alpha.clamp(0.0, 1.0) * 255.0) as u8;
        for i in 0..TILE_PX {
            let a = src.px[i].a();
            if a == 0 {
                continue;
            }
            let c = tint.scale(ink_core::color::mul8(a, k));
            t.px[i] = c.over(t.px[i]);
        }
    }
    t
}

/// Render every frame (straight RGBA8, downscaled by `scale_div`) and encode an animated GIF.
pub fn export_gif(doc: &mut Document, tl: &Timeline, scale_div: u32, out: impl std::io::Write) -> Result<(), String> {
    let s = scale_div.max(1) as usize;
    let (w, h) = (doc.width as usize, doc.height as usize);
    let (gw, gh) = (w / s, h / s);
    if gw == 0 || gh == 0 || gw > u16::MAX as usize || gh > u16::MAX as usize {
        return Err("image too large or too small for GIF".into());
    }
    let mut enc = gif::Encoder::new(out, gw as u16, gh as u16, &[]).map_err(|e| e.to_string())?;
    if tl.looping {
        enc.set_repeat(gif::Repeat::Infinite).map_err(|e| e.to_string())?;
    }
    let delay = (100.0 / tl.fps).round().max(1.0) as u16;
    for f in 0..tl.frames {
        tl.apply(doc, f);
        let full = doc.flatten_rgba8();
        let mut small = vec![0u8; gw * gh * 4];
        for y in 0..gh {
            for x in 0..gw {
                let mut acc = [0u32; 4];
                for dy in 0..s {
                    for dx in 0..s {
                        let i = ((y * s + dy) * w + x * s + dx) * 4;
                        for c in 0..4 {
                            acc[c] += full[i + c] as u32;
                        }
                    }
                }
                for c in 0..4 {
                    small[(y * gw + x) * 4 + c] = (acc[c] / (s * s) as u32) as u8;
                }
            }
        }
        let mut frame = gif::Frame::from_rgba_speed(gw as u16, gh as u16, &mut small, 10);
        frame.delay = delay;
        enc.write_frame(&frame).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (Document, Timeline) {
        let mut tl = Timeline::new(12.0, 12);
        tl.add_track("Character");
        (Document::new(256, 256, 72), tl)
    }

    #[test]
    fn holds_keys_and_blanks() {
        let (mut d, mut tl) = setup();
        let a = tl.new_cel(&mut d, 0, 0).unwrap();
        let b = tl.new_cel(&mut d, 0, 4).unwrap();
        tl.set_key(0, 8, None);
        assert_eq!(tl.cel_at(0, 0), Some(a));
        assert_eq!(tl.cel_at(0, 3), Some(a), "held");
        assert_eq!(tl.cel_at(0, 4), Some(b));
        assert_eq!(tl.cel_at(0, 7), Some(b));
        assert_eq!(tl.cel_at(0, 8), None, "blank");
        tl.set_key(0, 10, Some(0));
        assert_eq!(tl.cel_at(0, 11), Some(a), "cel reused later");
        assert!(tl.is_key(0, 4) && !tl.is_key(0, 5));
    }

    #[test]
    fn apply_toggles_visibility_and_drawing_cel_creates_on_blank() {
        let (mut d, mut tl) = setup();
        let a = tl.new_cel(&mut d, 0, 0).unwrap();
        let b = tl.new_cel(&mut d, 0, 6).unwrap();
        tl.apply(&mut d, 2);
        assert!(d.layer(a).unwrap().props.visible && !d.layer(b).unwrap().props.visible);
        tl.apply(&mut d, 6);
        assert!(!d.layer(a).unwrap().props.visible && d.layer(b).unwrap().props.visible);
        tl.set_key(0, 9, None);
        let c = tl.drawing_cel(&mut d, 0, 9).unwrap();
        assert!(c != a && c != b);
        assert_eq!(tl.drawing_cel(&mut d, 0, 9), Some(c), "second call reuses it");
    }

    #[test]
    fn onion_skips_holds_and_fades() {
        let (mut d, mut tl) = setup();
        let a = tl.new_cel(&mut d, 0, 0).unwrap();
        let b = tl.new_cel(&mut d, 0, 3).unwrap();
        let c = tl.new_cel(&mut d, 0, 6).unwrap();
        let e = tl.new_cel(&mut d, 0, 9).unwrap();
        let o = tl.onion_layers(7);
        let ids: Vec<LayerId> = o.iter().map(|x| x.0).collect();
        assert_eq!(ids, vec![b, a, e], "2 drawings before (skipping the held frames), 1 after");
        assert!(o[0].2 > o[1].2, "farther frames fade");
        assert!(!ids.contains(&c), "current drawing is not onion-skinned");
    }

    #[test]
    fn gif_export_writes_all_frames() {
        let (mut d, mut tl) = setup();
        for f in [0, 4, 8] {
            let id = tl.new_cel(&mut d, 0, f).unwrap();
            for x in 0..50 {
                d.layer_mut(id).unwrap().raster.set(x + f as i32 * 10, 100, Rgba::BLACK);
            }
        }
        let mut buf = Vec::new();
        export_gif(&mut d, &tl, 2, &mut buf).unwrap();
        let mut dec = gif::DecodeOptions::new().read_info(&buf[..]).unwrap();
        let mut n = 0;
        while dec.read_next_frame().unwrap().is_some() {
            n += 1;
        }
        assert_eq!(n, 12);
        assert_eq!((dec.width(), dec.height()), (128, 128));
    }
}
