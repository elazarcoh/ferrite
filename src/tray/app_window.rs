use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use eframe::egui;
use crossbeam_channel::Sender;
use crate::event::AppEvent;
use crate::config::schema::Config;
use crate::window::sprite_gallery::{SpriteGallery, SpriteKey};
use crate::tray::config_window::{DesktopSheetLoader, gallery_entries_from_desktop, OpenEditorRequest};
use crate::tray::sprite_editor::SpriteEditorViewport;
use crate::tray::sm_editor::new_desktop_sm_editor;
use ferrite_egui::config_panel::ConfigPanelState;

#[allow(unused_imports)]
pub use ferrite_egui::app_window::{AppTab, AppWindowState, render_app_tab_bar};

/// Creates a `SpriteEditorViewport` wired for desktop: provides `sprites_dir` and SM storage.
fn make_desktop_sprite_editor(
    state: ferrite_core::sprite::editor_state::SpriteEditorState,
    config_dir: &std::path::Path,
) -> SpriteEditorViewport {
    use crate::tray::sm_editor::DesktopSmStorage;
    use ferrite_egui::sm_storage::SmStorage;
    let mut ed = SpriteEditorViewport::new(state);
    ed.sprites_dir = Some(SpriteGallery::appdata_sprites_dir());
    ed.sm_storage = Some(Box::new(DesktopSmStorage::new(config_dir.to_path_buf())) as Box<dyn SmStorage>);
    ed
}

pub fn new_app_window_state(
    config: Config,
    tx: Sender<AppEvent>,
    dark_mode: bool,
    config_dir: PathBuf,
) -> Arc<Mutex<AppWindowState>> {
    let config_gallery = gallery_entries_from_desktop();
    let config_state = ConfigPanelState::new(config, config_gallery, Box::new(DesktopSheetLoader));
    let sm = new_desktop_sm_editor(dark_mode, config_dir.clone());
    let gallery_entries = gallery_entries_from_desktop();
    let _ = tx; // tx is used in open_app_window, not stored in AppWindowState
    Arc::new(Mutex::new(AppWindowState {
        selected_tab: AppTab::Config,
        should_close: false,
        dark_mode,
        dark_mode_out: None,
        config_state,
        gallery: gallery_entries,
        selected_sprite_key: None,
        sprite_editor: None,
        pending_png_pick: None,
        saved_json_path: None,
        pending_sprite_delete: None,
        wants_png_import: false,
        sm,
        sm_gallery_dirty: false,
        simulation_override: false,
    }))
}

pub fn open_app_window(
    ctx: &egui::Context,
    state: Arc<Mutex<AppWindowState>>,
    tx: Sender<AppEvent>,
    window_gen: u64,
    close_flag: Arc<AtomicBool>,
) {
    let viewport_id = egui::ViewportId::from_hash_of(format!("app_window_{window_gen}"));
    let viewport_builder = egui::ViewportBuilder::default()
        .with_title("Ferrite")
        .with_inner_size([1000.0, 640.0]);

    ctx.show_viewport_deferred(viewport_id, viewport_builder, move |ctx, _vp_class| {
        // OS close button: signal the main loop via the per-generation flag.
        if ctx.input(|i| i.viewport().close_requested()) {
            close_flag.store(true, Ordering::Relaxed);
            return;
        }

        let mut s = match state.lock() {
            Ok(g) => g,
            Err(_) => return,
        };

        // In-window ✕ button: mirror into the per-generation flag.
        if s.should_close {
            close_flag.store(true, Ordering::Relaxed);
            return;
        }

        // Refresh SM names for config tab (desktop-only: from SmGallery on disk)
        {
            let config_dir = crate::config::config_path()
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| std::path::PathBuf::from("."));
            let gallery = crate::sprite::sm_gallery::SmGallery::load(&config_dir);
            let mut sm_names = vec!["embedded://default".to_string()];
            sm_names.extend(gallery.valid_names().into_iter().map(|n| n.to_string()));
            s.config_state.sm_names = sm_names;
        }

        // Render the platform-agnostic tab shell
        ferrite_egui::app_window::render_full_window(ctx, &mut s);

        // ── Desktop-only post-render logic ──

        // Flush config_dirty → AppEvent::ConfigChanged
        if s.config_state.config_dirty {
            s.config_state.config_dirty = false;
            tx.send(AppEvent::ConfigChanged(s.config_state.config.clone())).ok();
        }

        // Poll pending PNG pick
        {
            let mut picked_png: Option<std::path::PathBuf> = None;
            if let Some(rx) = &s.pending_png_pick
                && let Ok(result) = rx.try_recv() {
                    picked_png = result;
                    s.pending_png_pick = None;
                }
            if let Some(png_path) = picked_png {
                let config_dir = crate::config::config_path()
                    .parent()
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|| std::path::PathBuf::from("."));
                match load_editor_state_from_png(&png_path) {
                    Ok(es) => {
                        s.sprite_editor = Some(make_desktop_sprite_editor(es, &config_dir));
                        s.selected_sprite_key = Some(
                            png_path.with_extension("json").to_string_lossy().to_string()
                        );
                    }
                    Err(e) => log::warn!("Failed to load PNG as sprite: {e}"),
                }
            }
        }

        // Sprite deletion confirmation modal
        if s.selected_tab == AppTab::Sprites {
            let mut confirmed = false;
            let mut cancelled = false;
            if let Some(ref key) = s.pending_sprite_delete {
                let display_name = s.gallery.iter()
                    .find(|e| &e.key == key)
                    .map(|e| e.display_name.as_str())
                    .unwrap_or("this sprite");
                egui::Window::new("Remove Sprite?")
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ctx, |ui| {
                        ui.label(format!("Delete \"{}\"? This cannot be undone.", display_name));
                        ui.horizontal(|ui| {
                            if ui.button("Remove").clicked() { confirmed = true; }
                            if ui.button("Cancel").clicked() { cancelled = true; }
                        });
                    });
            }
            if confirmed {
                let key = s.pending_sprite_delete.take().unwrap();
                let sprite_key = SpriteKey::from_sheet_path(&key);
                if let Err(e) = SpriteGallery::delete_installed(&sprite_key) {
                    log::warn!("Failed to delete sprite: {e}");
                } else {
                    s.gallery = gallery_entries_from_desktop();
                    if s.selected_sprite_key.as_ref() == Some(&key) {
                        s.selected_sprite_key = None;
                        s.sprite_editor = None;
                    }
                }
            }
            if cancelled { s.pending_sprite_delete = None; }
        }

        // Wire sprite editor: recreate when a gallery key is selected but no editor is loaded.
        // render_sprites_tab clears sprite_editor on selection change; we rebuild it here.
        if s.sprite_editor.is_none()
            && let Some(ref key) = s.selected_sprite_key.clone()
        {
            let config_dir = crate::config::config_path()
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| std::path::PathBuf::from("."));
            match load_editor_state_from_sheet(key) {
                Ok(es) => {
                    let mut ed = make_desktop_sprite_editor(es, &config_dir);
                    ed.is_builtin = key.starts_with("embedded://");
                    s.sprite_editor = Some(ed);
                }
                Err(e) => log::warn!("Failed to load sprite editor for {key}: {e}"),
            }
        }

        // Handle "Edit…" / "New from PNG…" requests from Config tab → switch to Sprites tab
        if let Some(req) = s.config_state.open_editor_request.take() {
            s.selected_tab = AppTab::Sprites;
            let config_dir = crate::config::config_path()
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| std::path::PathBuf::from("."));
            match req {
                OpenEditorRequest::Edit(sheet_path) => {
                    if let Ok(es) = load_editor_state_from_sheet(&sheet_path) {
                        let mut ed = make_desktop_sprite_editor(es, &config_dir);
                        if sheet_path.starts_with("embedded://") {
                            ed.is_builtin = true;
                        }
                        s.sprite_editor = Some(ed);
                    }
                }
                OpenEditorRequest::New(png_path) => {
                    if let Ok(es) = load_editor_state_from_png(&png_path) {
                        s.sprite_editor = Some(make_desktop_sprite_editor(es, &config_dir));
                    }
                }
            }
        }

        // Collect dark_mode_out from sub-states
        if let Some(new_dark) = s.config_state.dark_mode_out.take() {
            s.dark_mode_out = Some(new_dark);
            s.dark_mode = new_dark;
        }
        if let Some(ref mut ed) = s.sprite_editor
            && let Some(new_dark) = ed.dark_mode_out.take() {
                s.dark_mode_out = Some(new_dark);
                s.dark_mode = new_dark;
            }

        // Collect saved_json_path from sprite editor
        if let Some(ref mut ed) = s.sprite_editor
            && let Some(p) = ed.saved_json_path.take() {
                s.saved_json_path = Some(p.clone());
                s.gallery = gallery_entries_from_desktop();
                s.selected_sprite_key = Some(p.to_string_lossy().to_string());
            }
    });
}

fn load_editor_state_from_sheet(path: &str) -> anyhow::Result<crate::sprite::editor_state::SpriteEditorState> {
    use crate::sprite::editor_state::{EditorTag, SpriteEditorState};
    let sheet = load_sheet_for_path(path)?;
    let json_path = std::path::Path::new(path);
    let png_path = json_path.with_extension("png");
    let (cols, rows) = if let Some(f) = sheet.frames.first() {
        if f.w > 0 && f.h > 0 {
            (sheet.image.width() / f.w, sheet.image.height() / f.h)
        } else {
            (1, 1)
        }
    } else {
        (1, 1)
    };
    let tags: Vec<EditorTag> = sheet.tags.iter().enumerate().map(|(i, t)| EditorTag {
        name: t.name.clone(),
        from: t.from,
        to: t.to,
        direction: t.direction.clone(),
        flip_h: t.flip_h,
        color: SpriteEditorState::assign_color(i),
    }).collect();
    let mut state = SpriteEditorState::new(png_path, sheet.image);
    state.rows = rows;
    state.cols = cols;
    state.tags = tags;
    state.sm_mappings = sheet.sm_mappings;
    state.chromakey = sheet.chromakey.clone();
    state.baseline_offset = sheet.baseline_offset;
    Ok(state)
}

fn load_editor_state_from_png(png_path: &std::path::Path) -> anyhow::Result<crate::sprite::editor_state::SpriteEditorState> {
    let png = std::fs::read(png_path)?;
    let image = image::load_from_memory_with_format(&png, image::ImageFormat::Png)?.into_rgba8();
    Ok(crate::sprite::editor_state::SpriteEditorState::new(png_path.to_path_buf(), image))
}

fn load_sheet_for_path(path: &str) -> anyhow::Result<crate::sprite::sheet::SpriteSheet> {
    if let Some(stem) = path.strip_prefix("embedded://") {
        let (json, png) = crate::assets::embedded_sheet(stem)
            .ok_or_else(|| anyhow::anyhow!("embedded sheet '{stem}' not found"))?;
        return crate::sprite::sheet::load_embedded(&json, &png);
    }
    let json = std::fs::read(path)?;
    let json_path = std::path::Path::new(path);
    let png_path = json_path.with_extension("png");
    let png = std::fs::read(&png_path)?;
    let image = image::load_from_memory_with_format(&png, image::ImageFormat::Png)?.into_rgba8();
    crate::sprite::sheet::SpriteSheet::from_json_and_image(&json, image)
}
