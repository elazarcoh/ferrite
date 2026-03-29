// UI tests using egui_kittest — tests render functions directly; no GPU/HWND needed.
#[allow(unused_imports)]
use std::{cell::RefCell, path::PathBuf, rc::Rc};

use egui_kittest::Harness;
use ferrite::{
    config::schema::{Config, PetConfig},
    event::AppEvent,
    tray::{
        app_window::{AppTab, AppWindowState},
        config_window::{render_config_panel, ConfigWindowState},
        sm_editor::{render_sm_panel, SmEditorViewport},
    },
    window::sprite_gallery::SpriteGallery,
};

// ── Helpers ──────────────────────────────────────────────────────────────────

fn make_config_state() -> (ConfigWindowState, crossbeam_channel::Receiver<AppEvent>) {
    let (tx, rx) = crossbeam_channel::unbounded::<AppEvent>();
    let config = Config { pets: vec![PetConfig::default()] };
    let gallery = SpriteGallery::load();
    (ConfigWindowState::new(config, tx, gallery), rx)
}

fn make_sm_state() -> SmEditorViewport {
    let arc = SmEditorViewport::new(true, PathBuf::from("."));
    std::sync::Arc::try_unwrap(arc)
        .unwrap_or_else(|_| panic!("Arc has multiple owners"))
        .into_inner()
        .unwrap()
}

fn make_app_window_state() -> AppWindowState {
    let (tx, _rx) = crossbeam_channel::unbounded::<AppEvent>();
    let config = Config { pets: vec![PetConfig::default()] };
    let gallery = SpriteGallery::load();
    let arc = AppWindowState::new(config, tx, true, PathBuf::from("."), gallery);
    std::sync::Arc::try_unwrap(arc)
        .unwrap_or_else(|_| panic!("Arc has multiple owners"))
        .into_inner()
        .unwrap()
}
