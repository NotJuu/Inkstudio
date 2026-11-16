//! Inkstudio desktop app.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use inkstudio::InkApp;

struct Shell(InkApp);

impl eframe::App for Shell {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.0.ui(ui);
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
    eframe::run_native("Inkstudio", options, Box::new(|cc| Ok(Box::new(Shell(InkApp::new(&cc.egui_ctx))))))
}
