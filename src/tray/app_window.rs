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
use crate::tray::config_window::{render_config_panel, ConfigWindowState};
use crate::tray::sprite_editor::{render_sprite_editor_panel, SpriteEditorViewport};
use crate::tray::sm_editor::{render_sm_panel, SmEditorViewport, new_desktop_sm_editor};

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum AppTab { Config, Sprites, Sm }

pub struct AppWindowState {
    pub selected_tab: AppTab,
    pub should_close: bool,
    pub dark_mode: bool,
    pub dark_mode_out: Option<bool>,

    // ── Config tab ──
    pub config_state: ConfigWindowState,

    // ── Sprites tab ──
    pub sprite_gallery: SpriteGallery,
    pub selected_sprite_key: Option<SpriteKey>,
    pub sprite_editor: Option<SpriteEditorViewport>,
    pub pending_png_pick: Option<crossbeam_channel::Receiver<Option<std::path::PathBuf>>>,
    pub saved_json_path: Option<std::path::PathBuf>,
    pub pending_sprite_delete: Option<SpriteKey>,

    // ── SM tab ──
    pub sm: SmEditorViewport,

    // ── Dirty flags ──
    /// Set when the SM gallery has changed (e.g. after a bundle import).
    /// The config tab clears this each frame after re-loading the gallery.
    pub sm_gallery_dirty: bool,

    // ── Platform paths ──
    pub config_dir: PathBuf,
}

impl AppWindowState {
    pub fn new(config: Config, tx: Sender<AppEvent>, dark_mode: bool, config_dir: PathBuf, gallery: SpriteGallery) -> Arc<Mutex<Self>> {
        // Load a second gallery instance for the config tab (SpriteGallery doesn't impl Clone).
        let config_gallery = SpriteGallery::load();
        let config_state = ConfigWindowState::new(config, tx.clone(), config_gallery);
        let sm = new_desktop_sm_editor(dark_mode, config_dir.clone());
        Arc::new(Mutex::new(Self {
            selected_tab: AppTab::Config,
            should_close: false,
            dark_mode,
            dark_mode_out: None,
            config_state,
            sprite_gallery: gallery,
            selected_sprite_key: None,
            sprite_editor: None,
            pending_png_pick: None,
            saved_json_path: None,
            pending_sprite_delete: None,
            sm,
            sm_gallery_dirty: false,
            config_dir,
        }))
    }
}

/// Creates a `SpriteEditorViewport` wired for desktop: provides `sprites_dir` and SM storage.
fn make_desktop_sprite_editor(
    state: ferrite_core::sprite::editor_state::SpriteEditorState,
    config_dir: &std::path::Path,
) -> SpriteEditorViewport {
    use crate::tray::sm_editor::DesktopSmStorage;
    use ferrite_egui::sm_storage::SmStorage;
    let mut ed = SpriteEditorViewport::new(state);
    ed.sprites_dir = Some(crate::window::sprite_gallery::SpriteGallery::appdata_sprites_dir());
    ed.sm_storage = Some(Box::new(DesktopSmStorage::new(config_dir.to_path_buf())) as Box<dyn SmStorage>);
    ed
}

pub fn render_app_tab_bar(ctx: &egui::Context, s: &mut AppWindowState) {
    egui::TopBottomPanel::top("app_tab_bar").show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut s.selected_tab, AppTab::Config, "⚙ Config");
            ui.selectable_value(&mut s.selected_tab, AppTab::Sprites, "🖼 Sprites");
            ui.selectable_value(&mut s.selected_tab, AppTab::Sm, "🤖 State Machine");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("✕").clicked() {
                    s.should_close = true;
                }
                if crate::tray::ui_theme::dark_light_toggle(ui, &mut s.dark_mode, ctx) {
                    s.dark_mode_out = Some(s.dark_mode);
                }
            });
        });
    });
}

pub fn open_app_window(
    ctx: &egui::Context,
    state: Arc<Mutex<AppWindowState>>,
    window_gen: u64,
    close_flag: Arc<AtomicBool>,
) {
    let viewport_id = egui::ViewportId::from_hash_of(format!("app_window_{window_gen}"));
    let viewport_builder = egui::ViewportBuilder::default()
        .with_title("Ferrite")
        .with_inner_size([1000.0, 640.0]);

    ctx.show_viewport_deferred(viewport_id, viewport_builder, move |ctx, vp_class| {
        if vp_class == egui::ViewportClass::Embedded {
            // egui handles embedded viewports
        }

        // OS close button: signal the main loop via the per-generation flag.
        if ctx.input(|i| i.viewport().close_requested()) {
            close_flag.store(true, Ordering::Relaxed);
            return;
        }

        let mut s = match state.lock() {
            Ok(g) => g,
            Err(_) => return,
        };

        // In-window ✕ button: mirror into the per-generation flag so the main
        // loop can detect it without touching the mutex.
        if s.should_close {
            close_flag.store(true, Ordering::Relaxed);
            return;
        }

        // Sync dark mode into sub-states
        let current_dark = s.dark_mode;
        s.config_state.dark_mode = current_dark;
        s.sm.dark_mode = current_dark;
        if let Some(ref mut ed) = s.sprite_editor {
            ed.dark_mode = current_dark;
        }

        // Apply theme
        crate::tray::ui_theme::apply_theme(ctx, s.dark_mode);

        // Top tab bar
        render_app_tab_bar(ctx, &mut s);

        let tab = s.selected_tab;
        match tab {
            AppTab::Config => render_config_tab(ctx, &mut s),
            AppTab::Sprites => render_sprites_tab(ctx, &mut s),
            AppTab::Sm => render_sm_panel(ctx, &mut s.sm),
        }

        // Sprite deletion confirmation modal
        if s.selected_tab == AppTab::Sprites {
            let mut confirmed = false;
            let mut cancelled = false;
            if let Some(ref key) = s.pending_sprite_delete {
                let display_name = s.sprite_gallery.entries.iter()
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
                if let Err(e) = SpriteGallery::delete_installed(&key) {
                    log::warn!("Failed to delete sprite: {e}");
                } else {
                    s.sprite_gallery = SpriteGallery::load();
                    if s.selected_sprite_key.as_ref() == Some(&key) {
                        s.selected_sprite_key = None;
                        s.sprite_editor = None;
                    }
                }
            }
            if cancelled { s.pending_sprite_delete = None; }
        }

        // Handle "Edit…" / "New from PNG…" requests from Config tab → switch to Sprites tab
        if let Some(req) = s.config_state.open_editor_request.take() {
            s.selected_tab = AppTab::Sprites;
            match req {
                crate::tray::config_window::OpenEditorRequest::Edit(sheet_path) => {
                    if let Ok(es) = load_editor_state_from_sheet(&sheet_path) {
                        let mut ed = make_desktop_sprite_editor(es, &s.config_dir);
                        if sheet_path.starts_with("embedded://") {
                            ed.is_builtin = true;
                        }
                        s.sprite_editor = Some(ed);
                    }
                }
                crate::tray::config_window::OpenEditorRequest::New(png_path) => {
                    if let Ok(es) = load_editor_state_from_png(&png_path) {
                        s.sprite_editor = Some(make_desktop_sprite_editor(es, &s.config_dir));
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
                // Reload gallery so newly saved sprites show up in the list
                s.sprite_gallery = SpriteGallery::load();
                s.selected_sprite_key = Some(SpriteKey::Installed(p));
            }
    });
}

fn render_config_tab(ctx: &egui::Context, s: &mut AppWindowState) {
    render_config_panel(ctx, &mut s.config_state, &mut s.sm_gallery_dirty);
}

fn render_sprites_tab(ctx: &egui::Context, s: &mut AppWindowState) {
    // Poll pending PNG pick
    let mut picked_png: Option<std::path::PathBuf> = None;
    if let Some(rx) = &s.pending_png_pick
        && let Ok(result) = rx.try_recv() {
            picked_png = result;
            s.pending_png_pick = None;
        }
    if let Some(png_path) = picked_png {
        match load_editor_state_from_png(&png_path) {
            Ok(es) => {
                s.sprite_editor = Some(make_desktop_sprite_editor(es, &s.config_dir.clone()));
                s.selected_sprite_key = Some(SpriteKey::Installed(png_path.with_extension("json")));
            }
            Err(e) => log::warn!("Failed to load PNG as sprite: {e}"),
        }
    }

    // Left gallery panel
    egui::SidePanel::left("sprite_gallery_panel")
        .resizable(true)
        .min_width(150.0)
        .show(ctx, |ui| {
            ui.heading("Sprites");
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| {
                let entries: Vec<_> = s.sprite_gallery.entries.iter().map(|e| (e.key.clone(), e.display_name.clone())).collect();
                for (key, display_name) in entries {
                    let selected = s.selected_sprite_key.as_ref() == Some(&key);
                    let is_installed = matches!(key, SpriteKey::Installed(_));
                    ui.horizontal(|ui| {
                        if ui.selectable_label(selected, &display_name).clicked() {
                            s.selected_sprite_key = Some(key.clone());
                            match &key {
                                SpriteKey::Embedded(stem) => {
                                    let sheet_path = format!("embedded://{stem}");
                                    if let Ok(es) = load_editor_state_from_sheet(&sheet_path) {
                                        let mut ed = make_desktop_sprite_editor(es, &s.config_dir.clone());
                                        ed.is_builtin = true;
                                        s.sprite_editor = Some(ed);
                                    }
                                }
                                SpriteKey::Installed(path) => {
                                    let sheet_path = path.to_string_lossy().to_string();
                                    if let Ok(es) = load_editor_state_from_sheet(&sheet_path) {
                                        s.sprite_editor = Some(make_desktop_sprite_editor(es, &s.config_dir.clone()));
                                    }
                                }
                            }
                        }
                        if is_installed && ui.small_button("🗑").clicked() {
                            s.pending_sprite_delete = Some(key.clone());
                        }
                    });
                }
            });
            ui.separator();
            let pick_in_progress = s.pending_png_pick.is_some();
            if ui.add_enabled(!pick_in_progress, egui::Button::new("Import PNG\u{2026}")).clicked() {
                let (tx_pick, rx_pick) = crossbeam_channel::bounded(1);
                std::thread::spawn(move || {
                    let result = rfd::FileDialog::new()
                        .add_filter("PNG", &["png"])
                        .pick_file();
                    tx_pick.send(result).ok();
                });
                s.pending_png_pick = Some(rx_pick);
            }
        });

    // Central area: sprite editor panels or placeholder
    if let Some(ed) = &mut s.sprite_editor {
        render_sprite_editor_panel(ctx, ed);
    } else {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.centered_and_justified(|ui| {
                ui.label("Select a sprite from the list to edit.");
            });
        });
    }
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
