//! Desktop re-export + DesktopSheetLoader (wraps App::load_sheet_for_config).

#[allow(unused_imports)]
pub use ferrite_egui::config_panel::{
    render_config_panel, ConfigPanelState, OpenEditorRequest,
};

use ferrite_egui::gallery::SheetLoader;
use ferrite_core::sprite::sheet::SpriteSheet;
use crate::window::sprite_gallery::SpriteGallery;
use ferrite_egui::gallery::GalleryEntry;

pub struct DesktopSheetLoader;

impl SheetLoader for DesktopSheetLoader {
    fn load_sheet(&self, path: &str) -> anyhow::Result<SpriteSheet> {
        crate::app::App::load_sheet_for_config(path)
    }
}

/// Build a `Vec<GalleryEntry>` from the desktop `SpriteGallery`.
pub fn gallery_entries_from_desktop() -> Vec<GalleryEntry> {
    SpriteGallery::load()
        .entries
        .into_iter()
        .map(|e| GalleryEntry {
            key: e.key.to_sheet_path(),
            display_name: e.display_name,
        })
        .collect()
}

/// Alias kept for compatibility — ConfigWindowState is now ConfigPanelState.
pub type ConfigWindowState = ConfigPanelState;
