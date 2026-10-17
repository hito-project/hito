//! Starts the HITO desktop application.

// Release builds on Windows open no console window next to the app.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;
use hito::Shell;
use hito::i18n::{I18n, Locale};

/// Names the folder the layout is saved in, such as `~/.local/share/hito`.
const APP_ID: &str = "hito";

fn main() -> eframe::Result {
    let title = I18n::new(Locale::default()).tr("app-title");
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_app_id(APP_ID)
            .with_title(&title)
            .with_inner_size([1280.0, 800.0]),
        ..Default::default()
    };
    eframe::run_native(APP_ID, options, Box::new(|cc| Ok(Box::new(Shell::new(cc)))))
}
