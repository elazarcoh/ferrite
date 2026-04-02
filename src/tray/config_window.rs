use crate::{
    app::App,
    config::schema::{Config, PetConfig},
    event::AppEvent,
    sprite::sheet::SpriteSheet,
    window::sprite_gallery::SpriteGallery,
};
use crossbeam_channel::Sender;
use eframe::egui;

pub struct ConfigWindowState {
    pub config: Config,
    pub selected_pet_idx: Option<usize>,
    pub gallery: SpriteGallery,
    pub tx: Sender<AppEvent>,
    pub open_editor_request: Option<OpenEditorRequest>,
    pub dark_mode: bool,        // synced from App each frame
    pub dark_mode_out: Option<bool>,  // set by toggle, read by App
    // internal: cached loaded sheet for tag ComboBoxes
    loaded_sheet: Option<SpriteSheet>,
    loaded_sheet_path: String,
    pending_png_pick: Option<crossbeam_channel::Receiver<Option<std::path::PathBuf>>>,
}

pub enum OpenEditorRequest {
    Edit(String),             // sheet_path
    New(std::path::PathBuf),  // png_path
}

impl ConfigWindowState {
    pub fn new(config: Config, tx: Sender<AppEvent>, gallery: SpriteGallery) -> Self {
        let selected_pet_idx = if config.pets.is_empty() { None } else { Some(0) };
        Self {
            config,
            selected_pet_idx,
            gallery,
            tx,
            open_editor_request: None,
            dark_mode: true,
            dark_mode_out: None,
            loaded_sheet: None,
            loaded_sheet_path: String::new(),
            pending_png_pick: None,
        }
    }

    #[allow(dead_code)]
    pub fn refresh_gallery(&mut self) {
        self.gallery = SpriteGallery::load();
    }
}

pub fn render_config_panel(ctx: &egui::Context, s: &mut ConfigWindowState, sm_gallery_dirty: &mut bool) {
    // Apply theme for this frame.
    crate::tray::ui_theme::apply_theme(ctx, s.dark_mode);

    // Poll pending PNG file pick
    if let Some(ref rx) = s.pending_png_pick
        && let Ok(maybe_path) = rx.try_recv() {
            s.pending_png_pick = None;
            if let Some(path) = maybe_path {
                s.open_editor_request = Some(OpenEditorRequest::New(path));
            }
        }

    // Left panel: pet list
    egui::SidePanel::left("pet_list_panel")
        .default_width(180.0)
        .show(ctx, |ui| {
            ui.heading("Pets");
            ui.separator();

            egui::ScrollArea::vertical().show(ui, |ui| {
                for i in 0..s.config.pets.len() {
                    let label = s.config.pets[i].id.clone();
                    if ui
                        .selectable_value(&mut s.selected_pet_idx, Some(i), &label)
                        .clicked()
                    {
                        // selection changed — sheet will reload below
                    }
                }
            });

            ui.separator();

            if ui.button("Add Pet").clicked() {
                let new_id = format!("pet_{}", s.config.pets.len());
                s.config.pets.push(PetConfig {
                    id: new_id,
                    ..PetConfig::default()
                });
                s.selected_pet_idx = Some(s.config.pets.len() - 1);
                s.tx.send(AppEvent::ConfigChanged(s.config.clone())).ok();
            }

            let has_selection = s.selected_pet_idx.is_some();

            ui.add_enabled_ui(has_selection, |ui| {
                if ui.button("Remove").clicked() {
                    remove_selected(&mut s.config, &mut s.selected_pet_idx);
                    s.tx.send(AppEvent::ConfigChanged(s.config.clone())).ok();
                }
            });

            let has_sheet = has_selection
                && s.selected_pet_idx
                    .and_then(|i| s.config.pets.get(i))
                    .map(|p| !p.sheet_path.is_empty())
                    .unwrap_or(false);

            ui.add_enabled_ui(has_sheet, |ui| {
                if ui.button("Edit\u{2026}").clicked()
                    && let Some(idx) = s.selected_pet_idx {
                        let path = s.config.pets[idx].sheet_path.clone();
                        s.open_editor_request = Some(OpenEditorRequest::Edit(path));
                    }
            });

            let pick_in_progress = s.pending_png_pick.is_some();
            if ui.add_enabled(!pick_in_progress, egui::Button::new("New from PNG\u{2026}")).clicked() {
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

    // Right panel: pet settings
    egui::CentralPanel::default().show(ctx, |ui| {
        let Some(idx) = s.selected_pet_idx else {
            ui.label("Select a pet on the left.");
            return;
        };
        if idx >= s.config.pets.len() {
            ui.label("Select a pet on the left.");
            return;
        }

        // Reload sheet if path changed
        {
            let sheet_path = &s.config.pets[idx].sheet_path;
            if s.loaded_sheet_path != *sheet_path {
                s.loaded_sheet = App::load_sheet_for_config(sheet_path).ok();
                s.loaded_sheet_path = sheet_path.clone();
            }
        }

        ui.heading("Pet Settings");
        ui.separator();

        egui::ScrollArea::vertical().show(ui, |ui| {
            // Sheet ComboBox
            let mut sheet_changed = false;
            {
                let current_path = s.config.pets[idx].sheet_path.clone();
                let current_label = s.gallery.entries.iter()
                    .find(|e| e.key.to_sheet_path() == current_path)
                    .map(|e| e.display_name.clone())
                    .unwrap_or_else(|| {
                        if current_path.is_empty() {
                            "(none)".to_string()
                        } else {
                            current_path.clone()
                        }
                    });

                ui.horizontal(|ui| {
                    ui.label("Sheet:");
                    crate::tray::ui_theme::help_icon(ui, "Choose a sprite sheet from your installed library, or use 'New from PNG' to import one.");
                    egui::ComboBox::from_id_salt(("sheet_combo", idx))
                        .selected_text(&current_label)
                        .show_ui(ui, |ui| {
                            for entry in &s.gallery.entries {
                                let path = entry.key.to_sheet_path();
                                if ui
                                    .selectable_label(
                                        path == current_path,
                                        &entry.display_name,
                                    )
                                    .clicked()
                                {
                                    s.config.pets[idx].sheet_path = path;
                                    sheet_changed = true;
                                }
                            }
                        });
                });
            }
            if sheet_changed {
                let sheet_path = &s.config.pets[idx].sheet_path;
                s.loaded_sheet = App::load_sheet_for_config(sheet_path).ok();
                s.loaded_sheet_path = sheet_path.clone();
                s.tx.send(AppEvent::ConfigChanged(s.config.clone())).ok();
            }

            // Scale
            ui.horizontal(|ui| {
                ui.label("Scale:");
                let mut scale = s.config.pets[idx].scale;
                if ui
                    .add(egui::DragValue::new(&mut scale).range(0.25_f32..=4.0_f32).speed(0.05))
                    .changed()
                {
                    s.config.pets[idx].scale = scale;
                    s.tx.send(AppEvent::ConfigChanged(s.config.clone())).ok();
                }
            });
            crate::tray::ui_theme::hint(ui, "Pixel upscale factor. 2× is recommended for 32px sprites. Fractional values (e.g. 1.5) are supported.");

            // Walk speed
            ui.horizontal(|ui| {
                ui.label("Walk speed:");
                let mut speed = s.config.pets[idx].walk_speed;
                if ui
                    .add(
                        egui::DragValue::new(&mut speed)
                            .range(1.0..=500.0)
                            .suffix(" px/s"),
                    )
                    .changed()
                {
                    s.config.pets[idx].walk_speed = speed;
                    s.tx.send(AppEvent::ConfigChanged(s.config.clone())).ok();
                }
            });
            crate::tray::ui_theme::hint(ui, "How fast the pet walks across the screen (pixels/second).");

            // X position
            ui.horizontal(|ui| {
                ui.label("X:");
                let mut x = s.config.pets[idx].x;
                if ui.add(egui::DragValue::new(&mut x)).changed() {
                    s.config.pets[idx].x = x;
                    s.tx.send(AppEvent::ConfigChanged(s.config.clone())).ok();
                }
            });

            // Y position
            ui.horizontal(|ui| {
                ui.label("Y:");
                let mut y = s.config.pets[idx].y;
                if ui.add(egui::DragValue::new(&mut y)).changed() {
                    s.config.pets[idx].y = y;
                    s.tx.send(AppEvent::ConfigChanged(s.config.clone())).ok();
                }
            });

            ui.separator();

            // SM selector — gallery is reloaded from disk every frame (cheap for typical
            // gallery sizes). sm_gallery_dirty is cleared here so the flag doesn't
            // accumulate; a future optimization could cache the gallery and only reload
            // when this flag is set.
            {
                *sm_gallery_dirty = false;
                let config_dir = crate::config::config_path()
                    .parent()
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|| std::path::PathBuf::from("."));
                let gallery = crate::sprite::sm_gallery::SmGallery::load(&config_dir);
                let mut sm_names = vec!["embedded://default".to_string()];
                sm_names.extend(gallery.valid_names().into_iter().map(|s| s.to_string()));

                let current = s.config.pets[idx].state_machine.clone();
                let mut new_sm = current.clone();
                ui.horizontal(|ui| {
                    ui.label("State Machine:");
                    crate::tray::ui_theme::help_icon(ui, "Choose which state machine drives this pet's behaviour. 'embedded://default' is the built-in eSheep behaviour.");
                    egui::ComboBox::from_id_salt(("sm_selector", idx))
                        .selected_text(&current)
                        .show_ui(ui, |ui| {
                            for name in &sm_names {
                                let selected = current == *name;
                                if ui.selectable_label(selected, name).clicked() {
                                    new_sm = name.clone();
                                }
                            }
                        });
                });
                if new_sm != current {
                    s.config.pets[idx].state_machine = new_sm;
                    s.tx.send(AppEvent::ConfigChanged(s.config.clone())).ok();
                }
            }
        });
    });
}

fn remove_selected(config: &mut Config, selected: &mut Option<usize>) {
    if let Some(idx) = *selected {
        config.pets.remove(idx);
        *selected = if config.pets.is_empty() {
            None
        } else if idx >= config.pets.len() {
            Some(config.pets.len() - 1)
        } else {
            Some(idx)
        };
    }
}
