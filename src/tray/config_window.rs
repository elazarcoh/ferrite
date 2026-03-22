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
    pub should_close: bool,
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
            dark_mode: true,
            dark_mode_out: None,
            loaded_sheet: None,
            loaded_sheet_path: String::new(),
            pending_png_pick: None,
        }
    }

    pub fn refresh_gallery(&mut self) {
        self.gallery = SpriteGallery::load();
    }
}

pub fn open_config_viewport(
    ctx: &egui::Context,
    state: std::sync::Arc<std::sync::Mutex<ConfigWindowState>>,
) {
    let viewport_id = egui::ViewportId::from_hash_of("config_window");
    ctx.show_viewport_deferred(
        viewport_id,
        egui::ViewportBuilder::default()
            .with_title("my-pet Config")
            .with_inner_size([600.0, 480.0]),
        move |ctx, _class| {
            let mut guard = match state.lock() {
                Ok(g) => g,
                Err(_) => return,
            };
            let s = &mut *guard;

            if ctx.input(|i| i.viewport().close_requested()) {
                s.should_close = true;
            }

            // Apply theme for this frame.
            crate::tray::ui_theme::apply_theme(ctx, s.dark_mode);

            // Poll pending PNG file pick
            if let Some(ref rx) = s.pending_png_pick {
                if let Ok(maybe_path) = rx.try_recv() {
                    s.pending_png_pick = None;
                    if let Some(path) = maybe_path {
                        s.open_editor_request = Some(OpenEditorRequest::New(path));
                    }
                }
            }

            // Left panel: pet list
            egui::SidePanel::left("pet_list_panel")
                .default_width(180.0)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.heading("Pets");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if crate::tray::ui_theme::dark_light_toggle(ui, &mut s.dark_mode, ctx) {
                                s.dark_mode_out = Some(s.dark_mode);
                            }
                        });
                    });
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
                        if ui.button("Edit\u{2026}").clicked() {
                            if let Some(idx) = s.selected_pet_idx {
                                let path = s.config.pets[idx].sheet_path.clone();
                                s.open_editor_request = Some(OpenEditorRequest::Edit(path));
                            }
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
                            .add(egui::DragValue::new(&mut scale).range(1..=4))
                            .changed()
                        {
                            s.config.pets[idx].scale = scale;
                            s.tx.send(AppEvent::ConfigChanged(s.config.clone())).ok();
                        }
                    });
                    crate::tray::ui_theme::hint(ui, "Pixel upscale factor. 2× is recommended for 32px sprites.");

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
                    ui.horizontal(|ui| {
                        ui.heading("Tag Map");
                        crate::tray::ui_theme::help_icon(ui, "Maps pet behaviors to your tag names. idle and walk are required; others fall back to idle if not set.");
                    });

                    if let Some(ref sheet) = s.loaded_sheet {
                        let tag_names: Vec<String> =
                            sheet.tags.iter().map(|t| t.name.clone()).collect();

                        // Required tags
                        let mut changed = false;
                        changed |= required_tag_combo(
                            ui,
                            "idle (required)",
                            "tag_idle",
                            idx,
                            &mut s.config.pets[idx].tag_map.idle,
                            &tag_names,
                        );
                        changed |= required_tag_combo(
                            ui,
                            "walk (required)",
                            "tag_walk",
                            idx,
                            &mut s.config.pets[idx].tag_map.walk,
                            &tag_names,
                        );

                        // Optional tags
                        changed |= optional_tag_combo(
                            ui, "run", "tag_run", idx,
                            &mut s.config.pets[idx].tag_map.run, &tag_names,
                        );
                        changed |= optional_tag_combo(
                            ui, "sit", "tag_sit", idx,
                            &mut s.config.pets[idx].tag_map.sit, &tag_names,
                        );
                        changed |= optional_tag_combo(
                            ui, "sleep", "tag_sleep", idx,
                            &mut s.config.pets[idx].tag_map.sleep, &tag_names,
                        );
                        changed |= optional_tag_combo(
                            ui, "wake", "tag_wake", idx,
                            &mut s.config.pets[idx].tag_map.wake, &tag_names,
                        );
                        changed |= optional_tag_combo(
                            ui, "grabbed", "tag_grabbed", idx,
                            &mut s.config.pets[idx].tag_map.grabbed, &tag_names,
                        );
                        changed |= optional_tag_combo(
                            ui, "petted", "tag_petted", idx,
                            &mut s.config.pets[idx].tag_map.petted, &tag_names,
                        );
                        changed |= optional_tag_combo(
                            ui, "react", "tag_react", idx,
                            &mut s.config.pets[idx].tag_map.react, &tag_names,
                        );
                        changed |= optional_tag_combo(
                            ui, "fall", "tag_fall", idx,
                            &mut s.config.pets[idx].tag_map.fall, &tag_names,
                        );
                        changed |= optional_tag_combo(
                            ui, "thrown", "tag_thrown", idx,
                            &mut s.config.pets[idx].tag_map.thrown, &tag_names,
                        );

                        if changed {
                            s.tx.send(AppEvent::ConfigChanged(s.config.clone())).ok();
                        }
                    } else {
                        ui.label("(no sheet loaded)");
                    }
                });
            });
        },
    );
}

/// ComboBox for a required tag field (String). Returns true if changed.
fn required_tag_combo(
    ui: &mut egui::Ui,
    label: &str,
    id: &str,
    idx: usize,
    value: &mut String,
    tag_names: &[String],
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(label);
        let display = if value.is_empty() {
            "(none)"
        } else {
            value.as_str()
        };
        egui::ComboBox::from_id_salt((id, idx))
            .selected_text(display)
            .show_ui(ui, |ui| {
                for name in tag_names {
                    if ui.selectable_label(value == name, name).clicked() {
                        *value = name.clone();
                        changed = true;
                    }
                }
            });
    });
    changed
}

/// ComboBox for an optional tag field (Option<String>). Returns true if changed.
fn optional_tag_combo(
    ui: &mut egui::Ui,
    label: &str,
    id: &str,
    idx: usize,
    value: &mut Option<String>,
    tag_names: &[String],
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(label);
        let display = value
            .as_deref()
            .unwrap_or("\u{2014} not set \u{2014}");
        egui::ComboBox::from_id_salt((id, idx))
            .selected_text(display)
            .show_ui(ui, |ui| {
                // "not set" option
                if ui
                    .selectable_label(value.is_none(), "\u{2014} not set \u{2014}")
                    .clicked()
                {
                    *value = None;
                    changed = true;
                }
                for name in tag_names {
                    let selected = value.as_deref() == Some(name.as_str());
                    if ui.selectable_label(selected, name).clicked() {
                        *value = Some(name.clone());
                        changed = true;
                    }
                }
            });
    });
    changed
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
