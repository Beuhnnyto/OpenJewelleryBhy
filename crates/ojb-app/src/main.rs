//! OpenJewelleryBhy desktop application (Linux & Windows).
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod editors;
mod scene;
mod studio;
mod styles;
mod viewport;

use eframe::egui;

fn main() -> eframe::Result {
    let open = std::env::args_os().nth(1).map(std::path::PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("OpenJewelleryBhy")
            .with_inner_size([1480.0, 920.0])
            .with_min_inner_size([900.0, 600.0])
            .with_app_id("openjewellerybhy")
            .with_icon(std::sync::Arc::new(studio::icon())),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native("OpenJewelleryBhy", options, Box::new(move |cc| Ok(Box::new(app::OjbApp::new(cc, open)))))
}
