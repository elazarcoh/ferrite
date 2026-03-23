// Hide the console window only in release builds. In debug builds the console
// stays open so logs and panic messages are visible.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod assets;
mod config;
mod event;
mod sprite;
mod tray;
mod version;
mod window;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug")).init();

    let app = match app::App::new() {
        Ok(a) => a,
        Err(e) => {
            fatal(&format!("Startup failed:\n\n{e:#}"));
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
        fatal(&format!("eframe failed:\n\n{e}"));
    }
}

/// Show a modal error message box and log the error.
/// In debug builds the message also goes to the console.
fn fatal(msg: &str) {
    log::error!("{msg}");
    // Show a Windows message box so the error is visible even without a console.
    let wide: Vec<u16> = msg.encode_utf16().chain(std::iter::once(0)).collect();
    let caption: Vec<u16> = "my-pet — fatal error\0".encode_utf16().collect();
    unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::MessageBoxW(
            std::ptr::null_mut() as _,
            wide.as_ptr(),
            caption.as_ptr(),
            windows_sys::Win32::UI::WindowsAndMessaging::MB_OK
                | windows_sys::Win32::UI::WindowsAndMessaging::MB_ICONERROR,
        );
    }
}
