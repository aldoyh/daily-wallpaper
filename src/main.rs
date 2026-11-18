#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod image_source;
mod storage;
mod wallpaper;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([520.0, 720.0])
            .with_min_inner_size([440.0, 580.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Daily Wallpaper",
        options,
        Box::new(|cc| Ok(Box::new(app::WallpaperApp::new(cc)))),
    )
}
