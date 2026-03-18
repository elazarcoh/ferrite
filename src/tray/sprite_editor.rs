use crate::sprite::{
    animation::AnimationState,
    editor_state::SpriteEditorState,
    sheet::SpriteSheet,
};
use eframe::egui;

pub struct SpriteEditorViewport {
    pub state: SpriteEditorState,
    pub texture: Option<egui::TextureHandle>,
    pub anim: AnimationState,
    pub preview_sheet: Option<SpriteSheet>,
    pub should_close: bool,
}

impl SpriteEditorViewport {
    pub fn new(state: SpriteEditorState) -> Self {
        let tag = state.tags.first()
            .map(|t| t.name.clone())
            .unwrap_or_default();
        Self {
            state,
            texture: None,
            anim: AnimationState::new(tag),
            preview_sheet: None,
            should_close: false,
        }
    }
}

pub fn open_sprite_editor_viewport(
    _ctx: &egui::Context,
    _state: std::sync::Arc<std::sync::Mutex<SpriteEditorViewport>>,
) {
    // Full implementation in Task 4
}
