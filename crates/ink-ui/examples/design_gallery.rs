//! Render the design-system gallery to PNG without a GPU.
//!
//!   cargo run -p ink-ui --example design_gallery -- <out-dir> [scale]
//!
//! Writes gallery-dark.png and gallery-light.png. Review them after any token or widget change.

use ink_ui::{gallery, headless, theme};
use std::{fs::File, io::BufWriter, path::PathBuf};

fn main() {
    let mut args = std::env::args().skip(1);
    let out_dir = PathBuf::from(args.next().unwrap_or_else(|| "replica/design".into()));
    let scale: f32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(2.0);
    std::fs::create_dir_all(&out_dir).expect("create out dir");
    let size = egui::vec2(1280.0, 860.0);

    for mode in [theme::Mode::Dark, theme::Mode::Light] {
        let ctx = egui::Context::default();
        theme::apply(&ctx, mode);
        let mut state = gallery::GalleryState::default();
        let p = mode.palette();
        let img = headless::screenshot(&ctx, size, scale, p.bg, |ui| {
            egui::Frame::new().fill(p.bg).inner_margin(24).show(ui, |ui| gallery::show(ui, &mut state));
        });
        let name = match mode {
            theme::Mode::Dark => "gallery-dark.png",
            theme::Mode::Light => "gallery-light.png",
        };
        let path = out_dir.join(name);
        let mut enc = png::Encoder::new(BufWriter::new(File::create(&path).expect("create png")), img.width as u32, img.height as u32);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(&img.rgba).unwrap();
        println!("wrote {} ({}x{})", path.display(), img.width, img.height);
    }
}
