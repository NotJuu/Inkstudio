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
            egui::Frame::new()
                .fill(p.bg)
                .inner_margin(24)
                .show(ui, |ui| gallery::show(ui, &mut state));
        });
        // Workspace preview (S03).
        let ws_ctx = egui::Context::default();
        theme::apply(&ws_ctx, mode);
        let mut ws = ink_ui::preview::WorkspaceState::default();
        let ws_img = headless::screenshot(&ws_ctx, egui::vec2(1600.0, 960.0), scale, p.bg, |ui| {
            ink_ui::preview::show(ui, &mut ws)
        });
        let ws_name = match mode {
            theme::Mode::Dark => "workspace-dark.png",
            theme::Mode::Light => "workspace-light.png",
        };
        write_png(&out_dir.join(ws_name), &ws_img);

        let name = match mode {
            theme::Mode::Dark => "gallery-dark.png",
            theme::Mode::Light => "gallery-light.png",
        };
        write_png(&out_dir.join(name), &img);
    }
}

fn write_png(path: &std::path::Path, img: &headless::Image) {
    let mut enc = png::Encoder::new(
        BufWriter::new(File::create(path).expect("create png")),
        img.width as u32,
        img.height as u32,
    );
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()
        .unwrap()
        .write_image_data(&img.rgba)
        .unwrap();
    println!("wrote {} ({}x{})", path.display(), img.width, img.height);
}
