//! Desktop wrapper: provides `DesktopSmStorage` (filesystem-backed) and
//! re-exports the platform-agnostic SM editor from `ferrite-egui`.

#[allow(unused_imports)]
pub use ferrite_egui::sm_editor::{
    render_sm_panel, SmEditorFromApp, SmEditorFromUi, SmEditorViewport, VarSnapshot,
};

use ferrite_egui::sm_storage::SmStorage;
use crate::sprite::sm_gallery::SmGallery;
use std::path::PathBuf;
use std::sync::Mutex;

pub struct DesktopSmStorage {
    pub config_dir: PathBuf,
    cached: Mutex<Option<SmGallery>>,
}

impl DesktopSmStorage {
    pub fn new(config_dir: PathBuf) -> Self {
        Self { config_dir, cached: Mutex::new(None) }
    }

    fn with_gallery<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&SmGallery) -> R,
    {
        let mut guard = self.cached.lock().unwrap_or_else(|e| e.into_inner());
        if guard.is_none() {
            *guard = Some(SmGallery::load(&self.config_dir));
        }
        f(guard.as_ref().unwrap())
    }

    fn with_gallery_mut<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut SmGallery) -> R,
    {
        let mut guard = self.cached.lock().unwrap_or_else(|e| e.into_inner());
        if guard.is_none() {
            *guard = Some(SmGallery::load(&self.config_dir));
        }
        f(guard.as_mut().unwrap())
    }

    fn invalidate(&self) {
        let mut guard = self.cached.lock().unwrap_or_else(|e| e.into_inner());
        *guard = None;
    }
}

impl SmStorage for DesktopSmStorage {
    fn list_names(&self) -> Vec<String> {
        self.with_gallery(|gallery| {
            let mut names: Vec<String> = gallery.valid_names()
                .into_iter()
                .map(|s| s.to_string())
                .collect();
            for draft in gallery.draft_names() {
                let s = draft.to_string();
                if !names.contains(&s) {
                    names.push(s);
                }
            }
            names
        })
    }

    fn load(&self, name: &str) -> Option<String> {
        self.with_gallery(|gallery| {
            gallery.source(name)
                .or_else(|| gallery.draft_source(name))
                .map(|s| s.to_string())
        })
    }

    fn save(&self, name: &str, source: &str) -> Result<(), String> {
        let result = self.with_gallery_mut(|gallery| {
            gallery.save(name, source).map(|_| ()).map_err(|e| e.to_string())
        });
        if result.is_err() {
            self.invalidate();
        }
        result
    }

    fn delete(&self, name: &str) -> Result<(), String> {
        let result = self.with_gallery_mut(|gallery| {
            gallery.delete(name).map_err(|e| e.to_string())
        });
        if result.is_err() {
            self.invalidate();
        }
        result
    }
}

/// Desktop constructor: creates `SmEditorViewport` with filesystem-backed storage.
pub fn new_desktop_sm_editor(dark_mode: bool, config_dir: PathBuf) -> SmEditorViewport {
    let storage: Box<dyn SmStorage> = Box::new(DesktopSmStorage::new(config_dir));
    let arc = SmEditorViewport::new(dark_mode, storage);
    // SmEditorViewport::new wraps in Arc<Mutex<_>> for viewport sharing; unwrap since we hold
    // the only reference at this point.
    match std::sync::Arc::try_unwrap(arc) {
        Ok(mutex) => mutex.into_inner().unwrap_or_else(|e| e.into_inner()),
        Err(_) => panic!("SmEditorViewport::new returned Arc with unexpected extra references"),
    }
}
