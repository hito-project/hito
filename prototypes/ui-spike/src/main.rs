fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        viewport: egui::ViewportBuilder::default().with_inner_size([1280.0, 800.0]).with_title("HITO UI spike"),
        ..Default::default()
    };
    eframe::run_native("HITO UI spike", options, Box::new(|cc| Ok(Box::new(ui_spike::HitoApp::new(cc)))))
}
