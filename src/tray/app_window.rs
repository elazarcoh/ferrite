use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use eframe::egui;
use crossbeam_channel::Sender;
use crate::event::AppEvent;
use crate::config::schema::Config;
use crate::window::sprite_gallery::{SpriteGallery, SpriteKey};
use crate::tray::config_window::{render_config_panel, ConfigWindowState};
use crate::tray::sprite_editor::{render_sprite_editor_panel, SpriteEditorViewport};
use crate::tray::sm_editor::{render_sm_panel, SmEditorViewport};

#[derive(PartialEq, Clone, Copy)]
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

    // ── SM tab ──
    pub sm: SmEditorViewport,
}

impl AppWindowState {
    pub fn new(config: Config, tx: Sender<AppEvent>, dark_mode: bool, config_dir: PathBuf) -> Arc<Mutex<Self>> {
        let config_state = ConfigWindowState::new(config, tx.clone());
        let sm_arc = SmEditorViewport::new(dark_mode, config_dir.clone());
        let sm = match Arc::try_unwrap(sm_arc) {
            Ok(mutex) => mutex.into_inner().unwrap_or_else(|e| e.into_inner()),
            Err(_) => panic!("SmEditorViewport Arc has unexpected extra references"),
        };
        let sprite_gallery = SpriteGallery::load();
        Arc::new(Mutex::new(Self {
            selected_tab: AppTab::Config,
            should_close: false,
            dark_mode,
            dark_mode_out: None,
            config_state,
            sprite_gallery,
            selected_sprite_key: None,
            sprite_editor: None,
            pending_png_pick: None,
            saved_json_path: None,
            sm,
        }))
    }
}

pub fn open_app_window(ctx: &egui::Context, state: Arc<Mutex<AppWindowState>>) {
    let viewport_id = egui::ViewportId::from_hash_of("app_window");
    let viewport_builder = egui::ViewportBuilder::default()
        .with_title("My Pet")
        .with_inner_size([1000.0, 640.0]);

    ctx.show_viewport_deferred(viewport_id, viewport_builder, move |ctx, vp_class| {
        if vp_class == egui::ViewportClass::Embedded {
            // egui handles embedded viewports
        }

        if ctx.input(|i| i.viewport().close_requested()) {
            if let Ok(mut s) = state.lock() {
                s.should_close = true;
            }
            return;
        }

        let mut s = match state.lock() {
            Ok(g) => g,
            Err(_) => return,
        };

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

        let tab = s.selected_tab;
        match tab {
            AppTab::Config => render_config_tab(ctx, &mut s),
            AppTab::Sprites => render_sprites_tab(ctx, &mut s),
            AppTab::Sm => render_sm_panel(ctx, &mut s.sm),
        }

        // Handle "Edit…" / "New from PNG…" requests from Config tab → switch to Sprites tab
        if let Some(req) = s.config_state.open_editor_request.take() {
            s.selected_tab = AppTab::Sprites;
            match req {
                crate::tray::config_window::OpenEditorRequest::Edit(sheet_path) => {
                    if let Ok(es) = load_editor_state_from_sheet(&sheet_path) {
                        let mut ed = crate::tray::sprite_editor::SpriteEditorViewport::new(es);
                        if sheet_path.starts_with("embedded://") {
                            ed.is_builtin = true;
                        }
                        s.sprite_editor = Some(ed);
                    }
                }
                crate::tray::config_window::OpenEditorRequest::New(png_path) => {
                    if let Ok(es) = load_editor_state_from_png(&png_path) {
                        s.sprite_editor = Some(crate::tray::sprite_editor::SpriteEditorViewport::new(es));
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
                s.saved_json_path = Some(p);
            }
    });
}

fn render_config_tab(ctx: &egui::Context, s: &mut AppWindowState) {
    render_config_panel(ctx, &mut s.config_state);
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
                s.sprite_editor = Some(SpriteEditorViewport::new(es));
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
                    if ui.selectable_label(selected, &display_name).clicked() {
                        s.selected_sprite_key = Some(key.clone());
                        match &key {
                            SpriteKey::Embedded(stem) => {
                                let sheet_path = format!("embedded://{stem}");
                                if let Ok(es) = load_editor_state_from_sheet(&sheet_path) {
                                    let mut ed = SpriteEditorViewport::new(es);
                                    ed.is_builtin = true;
                                    s.sprite_editor = Some(ed);
                                }
                            }
                            SpriteKey::Installed(path) => {
                                let sheet_path = path.to_string_lossy().to_string();
                                if let Ok(es) = load_editor_state_from_sheet(&sheet_path) {
                                    s.sprite_editor = Some(SpriteEditorViewport::new(es));
                                }
                            }
                        }
                    }
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
