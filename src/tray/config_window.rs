use crate::{
    config::schema::Config,
    event::AppEvent,
    window::sprite_gallery::SpriteGallery,
};
use crossbeam_channel::Sender;
use eframe::egui;

pub struct ConfigWindowState {
    pub config: Config,
    pub selected_pet_idx: Option<usize>,
    pub gallery: SpriteGallery,
    pub tx: Sender<AppEvent>,
    pub should_close: bool,
    pub open_editor_request: Option<OpenEditorRequest>,
}

pub enum OpenEditorRequest {
    Edit(String),  // sheet_path
    New(std::path::PathBuf), // png_path
}

impl ConfigWindowState {
    pub fn new(config: Config, tx: Sender<AppEvent>) -> Self {
        let selected_pet_idx = if config.pets.is_empty() { None } else { Some(0) };
        let gallery = SpriteGallery::load();
        Self {
            config,
            selected_pet_idx,
            gallery,
            tx,
            should_close: false,
            open_editor_request: None,
        }
    }
}

pub fn open_config_viewport(
    _ctx: &egui::Context,
    _state: std::sync::Arc<std::sync::Mutex<ConfigWindowState>>,
) {
    // Full implementation in Task 3
}
