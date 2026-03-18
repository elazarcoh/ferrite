#![windows_subsystem = "windows"]

mod app;
mod assets;
mod config;
mod event;
mod sprite;
mod tray;
mod window;

fn main() {
    env_logger::init();

    let app = match app::App::new() {
        Ok(a) => a,
        Err(e) => {
            log::error!("startup failed: {e}");
            return;
        }
    };

    let native_options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_visible(false)
            .with_taskbar(false),
        ..Default::default()
    };

    if let Err(e) = eframe::run_native(
        "my-pet",
        native_options,
        Box::new(|_cc| Ok(Box::new(app))),
    ) {
        log::error!("eframe error: {e}");
    }
}
