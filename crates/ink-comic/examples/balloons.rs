//! Render every balloon style with vertical Japanese and horizontal text.
//!   cargo run --release -p ink-comic --example balloons -- out.png
use ink_comic::{balloon::render_all, Balloon, BalloonStyle, Fonts, TextDir};
use ink_core::{Document, Layer};
use ink_vector::Pt;

fn main() {
    let out = std::env::args().nth(1).unwrap_or_else(|| "balloons.png".into());
    let fonts = Fonts::system();
    println!("fonts: {:?}", fonts.names);
    let texts = ["ちょっと\n待って！", "Hello\nthere!", "ラーメン\n食べたい…", "うるさい！！", "あのね、\n秘密だよ", "こ、こわい…", "今日は\n雨だった。", "Narration\nbox", "ピピッ\n受信中", "ドン"];
    let (w, h) = (2600u32, 1500u32);
    let mut balloons = Vec::new();
    for (i, style) in BalloonStyle::ALL.into_iter().enumerate() {
        let c = Pt::new(260.0 + (i % 5) as f32 * 520.0, 360.0 + (i / 5) as f32 * 720.0);
        let dir = if matches!(style, BalloonStyle::Rounded | BalloonStyle::Narration) { TextDir::Horizontal } else { TextDir::Vertical };
        let mut b = Balloon::new(i as u64 + 1, style, c, 350, texts[i], dir);
        b.fit_to_text(&fonts);
        b.tail_tip = Pt::new(c.x - 120.0, c.y + 300.0);
        balloons.push(b);
    }
    let raster = render_all(&balloons, &fonts, (w, h));
    let mut d = Document::new(w, h, 350);
    d.push_layer(Layer { props: ink_core::LayerProps::named("b"), raster });
    let rgba = d.flatten_rgba8();
    let f = std::fs::File::create(&out).unwrap();
    let mut e = png::Encoder::new(std::io::BufWriter::new(f), w, h);
    e.set_color(png::ColorType::Rgba);
    e.set_depth(png::BitDepth::Eight);
    e.write_header().unwrap().write_image_data(&rgba).unwrap();
}
