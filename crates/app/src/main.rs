//! The HITO desktop application. For now it opens an empty window.

use eframe::egui;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1280.0, 800.0]),
        ..Default::default()
    };
    eframe::run_native("HITO", options, Box::new(|_cc| Ok(Box::new(Hito))))
}

/// The application state. Empty until the first workspace exists.
struct Hito;

impl eframe::App for Hito {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |_ui| {});
    }
}
