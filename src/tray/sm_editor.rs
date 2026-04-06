//! Desktop wrapper: provides `DesktopSmStorage` (filesystem-backed) and
//! re-exports the platform-agnostic SM editor from `ferrite-egui`.

#[allow(unused_imports)]
pub use ferrite_egui::sm_editor::{
    render_sm_panel, SmEditorFromApp, SmEditorFromUi, SmEditorViewport, VarSnapshot,
};

use ferrite_egui::sm_storage::SmStorage;
use crate::sprite::sm_gallery::SmGallery;
use std::path::PathBuf;

pub struct DesktopSmStorage {
    pub config_dir: PathBuf,
}

impl SmStorage for DesktopSmStorage {
    fn list_names(&self) -> Vec<String> {
        SmGallery::load(&self.config_dir)
            .valid_names()
            .into_iter()
            .map(|s| s.to_string())
            .collect()
    }

    fn load(&self, name: &str) -> Option<String> {
        SmGallery::load(&self.config_dir).source(name).map(|s| s.to_string())
    }

    fn save(&self, name: &str, source: &str) -> Result<(), String> {
        let mut gallery = SmGallery::load(&self.config_dir);
        gallery.save(name, source).map(|_| ()).map_err(|e| e.to_string())
    }

    fn delete(&self, name: &str) -> Result<(), String> {
        let mut gallery = SmGallery::load(&self.config_dir);
        gallery.delete(name).map_err(|e| e.to_string())
    }
}

/// Desktop constructor: creates `SmEditorViewport` with filesystem-backed storage.
pub fn new_desktop_sm_editor(dark_mode: bool, config_dir: PathBuf) -> SmEditorViewport {
    let storage = Box::new(DesktopSmStorage { config_dir });
    let arc = SmEditorViewport::new(dark_mode, storage);
    match std::sync::Arc::try_unwrap(arc) {
        Ok(mutex) => mutex.into_inner().unwrap_or_else(|e| e.into_inner()),
        Err(_) => panic!("SmEditorViewport Arc has unexpected extra references"),
    }
}

#[cfg(test)]
mod tests {
    use eframe::egui::FontId;
    use ferrite_egui::sm_highlighter::{PetstateTheme, highlight_petstate};

    #[test]
    fn syntax_highlight_produces_multiple_colors() {
        let theme = PetstateTheme::dark(FontId::monospace(14.0));
        let code = "[states.idle]\naction = \"idle\"\n# comment\ntransitions = []\n";
        let job = highlight_petstate(code, &theme);
        assert!(job.sections.len() > 1, "expected multiple sections, got {}", job.sections.len());
        let colors: std::collections::HashSet<_> = job.sections.iter()
            .map(|s| s.format.color)
            .collect();
        assert!(colors.len() > 1, "expected multiple colors in .petstate highlighting, got only {:?}", colors);
    }
}
