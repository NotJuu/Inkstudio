//! Inkstudio desktop app.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use inkstudio::InkApp;

struct Shell {
    app: InkApp,
    /// Pen tablet backend for macOS / Linux (Windows pens come through winit).
    tablet: Option<ink_tablet::Tablet>,
}

impl eframe::App for Shell {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.app.ui(ui);
    }

    fn raw_input_hook(&mut self, ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        if let Some(t) = &self.tablet {
            let samples = t.drain();
            if !samples.is_empty() {
                let ppp = ctx.pixels_per_point();
                raw_input.events.extend(inkstudio::tablet::to_events(
                    &samples,
                    ppp,
                    t.drives_pointer(),
                ));
            }
        }
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Inkstudio")
            .with_inner_size([1600.0, 1000.0])
            .with_min_inner_size([1024.0, 640.0]),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "Inkstudio",
        options,
        Box::new(|cc| {
            let tablet = ink_tablet::Tablet::attach(cc);
            let mut app = InkApp::new(&cc.egui_ctx);
            app.tablet_name = tablet.as_ref().map(|t| t.name());
            Ok(Box::new(Shell { app, tablet }))
        }),
    )
}
