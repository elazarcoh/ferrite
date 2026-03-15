#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod assets;
mod config;
mod event;
mod sprite;
mod tray;
mod window;

use anyhow::Result;

fn main() -> Result<()> {
    #[cfg(debug_assertions)]
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Debug)
        .init();
    #[cfg(not(debug_assertions))]
    env_logger::init();
    let mut application = app::App::new()?;
    application.run()
}
