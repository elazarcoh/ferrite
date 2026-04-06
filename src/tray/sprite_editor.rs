use crate::sprite::{
    animation::AnimationState,
    editor_state::SpriteEditorState,
    sheet::{Frame, FrameTag, SpriteSheet, TagDirection},
};
use eframe::egui;

pub struct SpriteEditorViewport {
    pub state: SpriteEditorState,
    pub texture: Option<egui::TextureHandle>,
    pub anim: AnimationState,
    pub preview_sheet: Option<SpriteSheet>,
    pub dark_mode: bool,        // synced from App each frame
    pub dark_mode_out: Option<bool>,  // set by toggle, read by App
    /// Set to the saved JSON path after a successful save; App reads + clears to trigger hot-reload.
    pub saved_json_path: Option<std::path::PathBuf>,
    pub is_builtin: bool,       // true for embedded sprites — disables Save/Export
    selected_tag_idx: Option<usize>,
    selected_frame_idx: usize,
    dirty: bool,
    sheet_zoom: f32,             // 1.0 = fit to panel; >1.0 = magnified
    tag_drag_start: Option<usize>, // frame index where a tag-range drag began
    show_export_bundle_dialog: bool,    // whether to show the SM picker modal
    selected_sm_name: Option<String>,   // SM selected in the coverage panel combobox
    sm_mappings: std::collections::HashMap<String, std::collections::HashMap<String, String>>,
    chromakey: crate::sprite::sheet::ChromakeyConfig,
    picking_chromakey_color: bool,
}

impl SpriteEditorViewport {
    pub fn new(state: SpriteEditorState) -> Self {
        let tag = state.tags.first().map(|t| t.name.clone()).unwrap_or_default();
        let sm_mappings = state.sm_mappings.clone();
        let chromakey = state.chromakey.clone();
        Self {
            state,
            texture: None,
            anim: AnimationState::new(tag),
            preview_sheet: None,
            dark_mode: true,
            dark_mode_out: None,
            saved_json_path: None,
            is_builtin: false,
            selected_tag_idx: None,
            selected_frame_idx: 0,
            dirty: false,
            sheet_zoom: 1.0,
            tag_drag_start: None,
            show_export_bundle_dialog: false,
            selected_sm_name: None,
            sm_mappings,
            chromakey,
            picking_chromakey_color: false,
        }
    }

    fn do_export_bundle(&self, sm_source: Option<&str>) {
        let bundle_name = crate::sprite::editor_state::sanitize_name(&self.state.sprite_name);

        let json_bytes = self.state.to_json();
        let json_str = match std::str::from_utf8(&json_bytes) {
            Ok(s) => s.to_string(),
            Err(e) => { log::error!("JSON encoding error: {}", e); return; }
        };

        let png_bytes = match std::fs::read(&self.state.png_path) {
            Ok(b) => b,
            Err(e) => { log::error!("Failed to read PNG for bundle: {}", e); return; }
        };

        let file_name = format!("{}.petbundle", bundle_name);
        let path = rfd::FileDialog::new()
            .add_filter("Pet Bundle", &["petbundle"])
            .set_file_name(&file_name)
            .save_file();

        if let Some(path) = path {
            match crate::bundle::export(&bundle_name, None, &json_str, &png_bytes, sm_source, None) {
                Ok(bytes) => {
                    if let Err(e) = std::fs::write(&path, bytes) {
                        log::error!("Failed to write bundle: {}", e);
                    } else {
                        log::info!("Bundle exported to {:?}", path);
                    }
                }
                Err(e) => log::error!("Bundle export failed: {}", e),
            }
        }
    }

    /// Build a SpriteSheet from the current editor state for animation preview.
    fn rebuild_preview_sheet(&mut self) {
        let total = (self.state.rows * self.state.cols) as usize;
        let frames: Vec<Frame> = (0..total)
            .map(|i| {
                let (x, y, w, h) = self.state.frame_rect(i);
                Frame { x, y, w, h, duration_ms: 100 }
            })
            .collect();
        let tags: Vec<FrameTag> = self.state.tags.iter().map(|t| FrameTag {
            name: t.name.clone(),
            from: t.from,
            to: t.to,
            direction: t.direction.clone(),
            flip_h: t.flip_h,
        }).collect();
        let mut keyed = self.state.image.clone();
        crate::sprite::sheet::apply_chromakey(&mut keyed, &self.chromakey);
        self.preview_sheet = Some(SpriteSheet {
            image: keyed,
            frames,
            tags,
            sm_mappings: std::collections::HashMap::new(),
            chromakey: self.chromakey.clone(),
            tight_bboxes: vec![],
            baseline_offset: self.state.baseline_offset,
        });
    }

    /// Total number of frames in the grid.
    fn total_frames(&self) -> usize {
        (self.state.rows * self.state.cols) as usize
    }
}

/// Clamp-step: increment or decrement `val` by 1, clamped to [0, max].
pub fn clamp_step(val: usize, up: bool, max: usize) -> usize {
    if up { val.saturating_add(1).min(max) } else { val.saturating_sub(1) }
}

pub fn render_sprite_editor_panel(ctx: &egui::Context, s: &mut SpriteEditorViewport) {
    crate::tray::ui_theme::apply_theme(ctx, s.dark_mode);

    // Upload texture if not yet uploaded.
    if s.texture.is_none() {
        let image = &s.state.image;
        let size = [image.width() as usize, image.height() as usize];
        let pixels = image.as_raw();
        let color_image = egui::ColorImage::from_rgba_unmultiplied(size, pixels);
        s.texture = Some(ctx.load_texture("sprite_sheet", color_image, egui::TextureOptions::NEAREST));
    }

    // Build preview sheet if missing.
    if s.preview_sheet.is_none() {
        s.rebuild_preview_sheet();
    }

    // Top bar
    egui::TopBottomPanel::top("editor_top").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Name:");
                if ui.add_enabled(
                    !s.is_builtin,
                    egui::TextEdit::singleline(&mut s.state.sprite_name).hint_text("sprite name"),
                ).changed() {
                    s.dirty = true;
                }
                if s.is_builtin {
                    ui.colored_label(egui::Color32::GRAY, "(Built-in \u{2014} read only)");
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add_enabled(!s.is_builtin, egui::Button::new("Export Bundle")).clicked() {
                        s.show_export_bundle_dialog = true;
                    }
                    if ui.add_enabled(!s.is_builtin, egui::Button::new("Export PNG\u{2026}")).clicked() {
                        let image_data = s.state.image.clone();
                        std::thread::spawn(move || {
                            if let Some(path) = rfd::FileDialog::new()
                                .add_filter("PNG", &["png"])
                                .save_file()
                            {
                                image_data.save(path).ok();
                            }
                        });
                    }
                    let save_btn = egui::Button::new("Save");
                    let save_resp = ui.add_enabled(!s.is_builtin && s.dirty, save_btn)
                        .on_disabled_hover_text("Built-in sprites cannot be modified");
                    if save_resp.clicked() {
                        // Sync sm_mappings from viewport into editor state before saving
                        s.state.sm_mappings = s.sm_mappings.clone();
                        s.state.chromakey = s.chromakey.clone();
                        // Always save into the app sprites directory so that new sprites
                        // are registered and don't try to overwrite the source PNG.
                        let sprites_dir = crate::window::sprite_gallery::SpriteGallery::appdata_sprites_dir();
                        let already_installed = s.state.png_path.parent()
                            .map(|p| p == sprites_dir)
                            .unwrap_or(false);
                        let save_dir = if already_installed {
                            s.state.png_path.parent().map(|p| p.to_path_buf())
                        } else {
                            match std::fs::create_dir_all(&sprites_dir) {
                                Ok(()) => Some(sprites_dir),
                                Err(e) => { log::warn!("save sprite editor: create sprites dir: {e}"); None }
                            }
                        };
                        if let Some(dir) = save_dir {
                            if let Err(e) = s.state.save_to_dir(&dir) {
                                log::warn!("save sprite editor: {e}");
                            } else {
                                let stem = crate::sprite::editor_state::sanitize_name(&s.state.sprite_name);
                                // Update png_path so subsequent saves stay in the sprites dir
                                s.state.png_path = dir.join(format!("{stem}.png"));
                                s.saved_json_path = Some(dir.join(format!("{stem}.json")));
                                s.dirty = false;
                            }
                        }
                    }
                });
            });
        });

        // Left panel: tag list and grid settings
        egui::SidePanel::left("tag_panel").min_width(200.0).show(ctx, |ui| {
            ui.add_enabled_ui(!s.is_builtin, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.horizontal(|ui| {
                    ui.heading("Grid");
                    crate::tray::ui_theme::help_icon(ui, "Sets how the PNG is divided into frames. Cols × Rows = total frame count.");
                });
                ui.horizontal(|ui| {
                    ui.label("Cols:");
                    let mut cols = s.state.cols as usize;
                    let mut changed = false;
                    let resp = ui.add(egui::DragValue::new(&mut cols).range(1_usize..=64_usize));
                    if resp.has_focus() {
                        if ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                            cols = clamp_step(cols, true, 64);
                            changed = true;
                        }
                        if ui.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                            cols = clamp_step(cols, false, 64).max(1);
                            changed = true;
                        }
                    }
                    if resp.changed() || changed {
                        s.state.cols = cols as u32;
                        s.dirty = true;
                        s.preview_sheet = None;
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Rows:");
                    let mut rows = s.state.rows as usize;
                    let mut changed = false;
                    let resp = ui.add(egui::DragValue::new(&mut rows).range(1_usize..=64_usize));
                    if resp.has_focus() {
                        if ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                            rows = clamp_step(rows, true, 64);
                            changed = true;
                        }
                        if ui.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                            rows = clamp_step(rows, false, 64).max(1);
                            changed = true;
                        }
                    }
                    if resp.changed() || changed {
                        s.state.rows = rows as u32;
                        let new_frame_h = s.state.image.height() / s.state.rows;
                        s.state.baseline_offset = s.state.baseline_offset.min(new_frame_h.saturating_sub(1));
                        s.dirty = true;
                        s.preview_sheet = None;
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Baseline:");
                    crate::tray::ui_theme::help_icon(
                        ui,
                        "Pixels from the bottom of each frame to the walking floor. \
                         0 = bottom edge is the floor.",
                    );
                    let frame_h = if s.state.rows > 0 {
                        s.state.image.height() / s.state.rows
                    } else {
                        1
                    };
                    let max_offset = frame_h.saturating_sub(1) as usize;
                    let mut offset = s.state.baseline_offset as usize;
                    if ui.add(egui::DragValue::new(&mut offset).range(0_usize..=max_offset)).changed() {
                        s.state.baseline_offset = offset as u32;
                        s.dirty = true;
                    }
                });
                let total = s.total_frames();
                ui.label(format!("{} frames", total));

                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Chromakey").strong());
                    crate::tray::ui_theme::help_icon(
                        ui,
                        "Remove a background color by making matching pixels transparent. \
                         Enable, pick a key color, and adjust tolerance for anti-aliased edges.",
                    );
                });

                let mut ck_changed = false;
                let mut enabled = s.chromakey.enabled;
                if ui.checkbox(&mut enabled, "Enable").changed() {
                    s.chromakey.enabled = enabled;
                    ck_changed = true;
                }

                if s.chromakey.enabled {
                    ui.horizontal(|ui| {
                        ui.label("Key color:");
                        let mut rgb = s.chromakey.color.map(|c| c as f32 / 255.0);
                        if egui::color_picker::color_edit_button_rgb(ui, &mut rgb).changed() {
                            s.chromakey.color = rgb.map(|c| (c * 255.0).round() as u8);
                            ck_changed = true;
                        }
                        if ui.button("Pick").on_hover_text(
                            "Click then click a pixel on the spritesheet to sample its color"
                        ).clicked() {
                            s.picking_chromakey_color = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Tolerance:");
                        let mut tol = s.chromakey.tolerance as u32;
                        if ui.add(egui::Slider::new(&mut tol, 0_u32..=64_u32)).changed() {
                            s.chromakey.tolerance = tol as u8;
                            ck_changed = true;
                        }
                    });
                    ui.label(egui::RichText::new("0 = exact match").weak().small());
                }

                if ck_changed {
                    s.dirty = true;
                    s.preview_sheet = None;
                }

                ui.separator();
                ui.horizontal(|ui| {
                    ui.heading("Tags");
                    crate::tray::ui_theme::help_icon(ui, "Tags group frames into named animations (e.g. 'idle', 'walk'). Select a tag to edit its frame range and direction.");
                });

                let mut clicked_idx = None;
                for (i, tag) in s.state.tags.iter().enumerate() {
                    let selected = s.selected_tag_idx == Some(i);
                    let label = format!("{} [{}-{}]", tag.name, tag.from, tag.to);
                    if ui.selectable_label(selected, &label).clicked() {
                        clicked_idx = Some(i);
                    }
                }
                if let Some(i) = clicked_idx {
                    s.selected_tag_idx = Some(i);
                    s.state.selected_tag = Some(i);
                    let tag_name = s.state.tags[i].name.clone();
                    s.anim.set_tag(tag_name);
                }

                ui.separator();

                // Selected tag controls
                if let Some(tag_idx) = s.selected_tag_idx
                    && tag_idx < s.state.tags.len() {
                        let total = s.total_frames();

                        // From / To frame range
                        let max_frame = total.saturating_sub(1);
                        ui.horizontal(|ui| {
                            ui.label("From:");
                            let mut from = s.state.tags[tag_idx].from;
                            let mut changed = false;
                            let resp = ui.add(egui::DragValue::new(&mut from).range(0..=max_frame));
                            if resp.has_focus() {
                                if ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                                    from = clamp_step(from, true, max_frame);
                                    changed = true;
                                }
                                if ui.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                                    from = clamp_step(from, false, max_frame);
                                    changed = true;
                                }
                            }
                            if resp.changed() || changed {
                                s.state.tags[tag_idx].from = from;
                                s.dirty = true;
                                s.preview_sheet = None;
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label("To:");
                            let mut to = s.state.tags[tag_idx].to;
                            let mut changed = false;
                            let resp = ui.add(egui::DragValue::new(&mut to).range(0..=max_frame));
                            if resp.has_focus() {
                                if ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                                    to = clamp_step(to, true, max_frame);
                                    changed = true;
                                }
                                if ui.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                                    to = clamp_step(to, false, max_frame);
                                    changed = true;
                                }
                            }
                            if resp.changed() || changed {
                                s.state.tags[tag_idx].to = to;
                                s.dirty = true;
                                s.preview_sheet = None;
                            }
                        });

                        // Direction combobox
                        ui.label("Direction:");
                        {
                            let current_dir = s.state.tags[tag_idx].direction.clone();
                            let mut new_dir = current_dir.clone();
                            egui::ComboBox::from_id_salt("tag_direction")
                                .selected_text(current_dir.label())
                                .show_ui(ui, |ui| {
                                    for d in [
                                        TagDirection::Forward,
                                        TagDirection::Reverse,
                                        TagDirection::PingPong,
                                        TagDirection::PingPongReverse,
                                    ] {
                                        ui.selectable_value(&mut new_dir, d.clone(), d.label());
                                    }
                                });
                            if new_dir != current_dir {
                                s.state.tags[tag_idx].direction = new_dir;
                                s.dirty = true;
                                s.preview_sheet = None;
                            }
                        }
                        crate::tray::ui_theme::hint(ui, "Forward plays frames left-to-right. PingPong bounces back and forth.");

                        // Flip H checkbox
                        {
                            let mut flip_h = s.state.tags[tag_idx].flip_h;
                            let resp = ui.checkbox(&mut flip_h, "Flip H")
                                .on_hover_text("Sprite frames face LEFT. Mirror when walking right so the pet faces its direction of travel.");
                            if resp.changed() {
                                s.state.tags[tag_idx].flip_h = flip_h;
                                s.dirty = true;
                                s.preview_sheet = None;
                            }
                        }

                        if ui.button("Delete Tag").clicked() {
                            s.state.tags.remove(tag_idx);
                            let new_idx = if s.state.tags.is_empty() {
                                None
                            } else {
                                Some(tag_idx.min(s.state.tags.len() - 1))
                            };
                            s.selected_tag_idx = new_idx;
                            s.state.selected_tag = new_idx;
                            s.dirty = true;
                            s.preview_sheet = None;
                        }
                    }

                ui.separator();
                ui.label("Add tag:");
                let id = egui::Id::new("new_tag_name_editor");
                let mut new_tag_name: String = ui.data_mut(|d| d.get_temp(id).unwrap_or_default());
                let response = ui.text_edit_singleline(&mut new_tag_name);
                if response.lost_focus()
                    && ui.input(|i| i.key_pressed(egui::Key::Enter))
                    && !new_tag_name.is_empty()
                {
                    let total = s.total_frames();
                    let color = crate::sprite::editor_state::SpriteEditorState::assign_color(s.state.tags.len());
                    s.state.tags.push(crate::sprite::editor_state::EditorTag {
                        name: new_tag_name.clone(),
                        from: 0,
                        to: total.saturating_sub(1),
                        direction: TagDirection::Forward,
                        flip_h: false,
                        color,
                    });
                    s.dirty = true;
                    s.preview_sheet = None;
                    new_tag_name.clear();
                }
                ui.data_mut(|d| d.insert_temp(id, new_tag_name));

                ui.separator();

                // SM selector combobox
                {
                    let config_dir = crate::config::config_path()
                        .parent()
                        .map(|p| p.to_path_buf())
                        .unwrap_or_else(|| std::path::PathBuf::from("."));
                    let gallery = crate::sprite::sm_gallery::SmGallery::load(&config_dir);
                    let sm_names: Vec<String> = gallery.valid_names().into_iter().map(|n| n.to_string()).collect();

                    let selected_text = s.selected_sm_name.clone().unwrap_or_else(|| "(none)".to_string());
                    ui.horizontal(|ui| {
                        ui.label("SM:");
                        egui::ComboBox::from_id_salt("sm_selector_editor")
                            .selected_text(&selected_text)
                            .show_ui(ui, |ui| {
                                if ui.selectable_label(s.selected_sm_name.is_none(), "(none)").clicked() {
                                    s.selected_sm_name = None;
                                }
                                for name in &sm_names {
                                    let is_selected = s.selected_sm_name.as_deref() == Some(name.as_str());
                                    if ui.selectable_label(is_selected, name).clicked() {
                                        s.selected_sm_name = Some(name.clone());
                                    }
                                }
                            });
                    });

                    // SM coverage panel — shown when an SM is selected
                    if let Some(sm_name) = s.selected_sm_name.clone()
                        && let Some(sm) = gallery.get(&sm_name) {
                            ui.separator();
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new("SM Coverage").strong());
                                crate::tray::ui_theme::help_icon(
                                    ui,
                                    "States are matched to spritesheet tags by name. If your tags have \
                                     different names, use the dropdown on each row to map them explicitly. \
                                     '(auto)' means the state name and tag name match — no override needed.",
                                );
                            });

                            let mut sorted_tags: Vec<String> =
                                s.state.tags.iter().map(|t| t.name.clone()).collect();
                            sorted_tags.sort();

                            // Stable display order
                            let mut state_entries: Vec<(&String, &ferrite_core::sprite::sm_compiler::CompiledState)> =
                                sm.states.iter().collect();
                            state_entries.sort_by_key(|(n, _)| n.as_str());

                            let mut mapping_changes: Vec<(String, Option<String>)> = Vec::new();

                            for (state_name, state_def) in &state_entries {
                                let explicit = s.sm_mappings
                                    .get(&sm_name)
                                    .and_then(|m| m.get(state_name.as_str()))
                                    .cloned();
                                let auto_matches = sorted_tags.iter().any(|t| t == *state_name);
                                let resolved = explicit.as_deref().or({
                                    if auto_matches { Some(state_name.as_str()) } else { None }
                                });

                                let (icon, color) = match resolved {
                                    Some(_) => ("✓", egui::Color32::LIGHT_GREEN),
                                    None if state_def.required => ("✗", egui::Color32::LIGHT_RED),
                                    None => ("○", egui::Color32::LIGHT_YELLOW),
                                };

                                let has_explicit = explicit.is_some();
                                let mut selected = explicit.clone().unwrap_or_else(|| "(auto)".to_string());
                                let old_selected = selected.clone();

                                let frame_response = egui::Frame::new()
                                    .inner_margin(egui::Margin::symmetric(4, 1))
                                    .show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            ui.colored_label(color, icon);
                                            let suffix = if resolved.is_none() && state_def.required {
                                                " required"
                                            } else if resolved.is_none() {
                                                " optional"
                                            } else {
                                                ""
                                            };
                                            ui.label(format!("{}{}", state_name, suffix));

                                            ui.with_layout(
                                                egui::Layout::right_to_left(egui::Align::Center),
                                                |ui| {
                                                    let cb = egui::ComboBox::from_id_salt((
                                                        "tag_map",
                                                        sm_name.as_str(),
                                                        state_name.as_str(),
                                                    ))
                                                    .selected_text(selected.as_str())
                                                    .width(110.0)
                                                    .show_ui(ui, |ui| {
                                                        ui.selectable_value(
                                                            &mut selected,
                                                            "(auto)".to_string(),
                                                            "(auto)",
                                                        );
                                                        for tag in &sorted_tags {
                                                            ui.selectable_value(
                                                                &mut selected,
                                                                tag.clone(),
                                                                tag.as_str(),
                                                            );
                                                        }
                                                    });
                                                    if selected == "(auto)" {
                                                        cb.response.on_hover_text(
                                                            "No explicit mapping. Uses the tag named identically \
                                                             to this state. Select a tag to override.",
                                                        );
                                                    }
                                                },
                                            );
                                        });
                                    });

                                if has_explicit {
                                    let rect = frame_response.response.rect;
                                    ui.painter().line_segment(
                                        [rect.left_top(), rect.left_bottom()],
                                        egui::Stroke::new(3.0, egui::Color32::from_rgb(60, 160, 80)),
                                    );
                                }

                                if selected != old_selected {
                                    mapping_changes.push((
                                        state_name.to_string(),
                                        if selected == "(auto)" { None } else { Some(selected) },
                                    ));
                                }
                            }

                            // Apply mapping changes outside the borrow on sm.states
                            for (state_name, tag) in mapping_changes {
                                update_tag_mapping(
                                    &mut s.sm_mappings,
                                    &sm_name,
                                    &state_name,
                                    tag.as_deref(),
                                );
                                s.dirty = true;
                            }

                            // Legend
                            ui.add_space(4.0);
                            ui.horizontal(|ui| {
                                ui.colored_label(egui::Color32::LIGHT_GREEN, "✓");
                                ui.label("resolved");
                                ui.separator();
                                ui.colored_label(egui::Color32::LIGHT_RED, "✗");
                                ui.label("required missing");
                                ui.separator();
                                ui.colored_label(egui::Color32::LIGHT_YELLOW, "○");
                                ui.label("optional missing");
                                ui.separator();
                                ui.label(egui::RichText::new("│ = explicit override").weak().small());
                            });
                        }
                }
            }); // end ScrollArea
            });
        });

        // Central panel: frame controls + preview
        egui::CentralPanel::default().show(ctx, |ui| {
            let total_frames = s.total_frames();
            if total_frames == 0 {
                ui.label("No frames.");
                return;
            }
            let current_frame = s.selected_frame_idx.min(total_frames.saturating_sub(1));
            s.selected_frame_idx = current_frame;

            // Frame navigation
            ui.horizontal(|ui| {
                if ui.button("\u{25C0}").clicked() && current_frame > 0 {
                    s.selected_frame_idx = current_frame - 1;
                }
                ui.label(format!("Frame {}/{}", current_frame + 1, total_frames));
                if ui.button("\u{25B6}").clicked() && current_frame + 1 < total_frames {
                    s.selected_frame_idx = current_frame + 1;
                }
            });

            // Live preview (animated)
            let preview_h = 256.0_f32;
            let tex = s.texture.clone();
            // Tick animation and compute the frame to display.
            // We need to split borrows: tick the anim with the preview sheet,
            // then read the frame data.
            let preview_frame_data = if let Some(sheet) = s.preview_sheet.take() {
                let delta_ms = (ctx.input(|i| i.unstable_dt) * 1000.0).min(200.0) as u32;
                s.anim.tick(&sheet, delta_ms);
                let abs = s.anim.absolute_frame(&sheet);
                let frame = sheet.frames.get(abs).cloned();
                // For the preview we always play forward, so treat flip_h as always-flip.
                let flip_h = sheet.tag(&s.anim.current_tag).is_some_and(|t| t.flip_h);
                s.preview_sheet = Some(sheet);
                frame.map(|f| (f, flip_h))
            } else {
                None
            };
            if let (Some(tex), Some((f, flip_h))) = (&tex, preview_frame_data) {
                let tex_size = tex.size_vec2();
                if tex_size.x > 0.0 && tex_size.y > 0.0 && f.w > 0 && f.h > 0 {
                    let u0 = f.x as f32 / tex_size.x;
                    let u1 = (f.x + f.w) as f32 / tex_size.x;
                    let v0 = f.y as f32 / tex_size.y;
                    let v1 = (f.y + f.h) as f32 / tex_size.y;
                    // Swap U coordinates to mirror the sprite horizontally.
                    let uv = if flip_h {
                        egui::Rect::from_min_max(egui::pos2(u1, v0), egui::pos2(u0, v1))
                    } else {
                        egui::Rect::from_min_max(egui::pos2(u0, v0), egui::pos2(u1, v1))
                    };
                    let aspect = f.w as f32 / f.h as f32;
                    let preview_w = preview_h * aspect;
                    let (rect, _resp) = ui.allocate_exact_size(
                        egui::vec2(preview_w, preview_h),
                        egui::Sense::hover(),
                    );
                    ui.painter().image(tex.id(), rect, uv, egui::Color32::WHITE);
                }
            }

            // Full PNG grid view with zoom + scroll
            ui.separator();
            if let Some(ref tex) = tex {
                let tex_size = tex.size_vec2();
                let cols = s.state.cols as usize;
                let rows = s.state.rows as usize;
                if cols > 0 && rows > 0 && tex_size.x > 0.0 && tex_size.y > 0.0 {
                    // Zoom toolbar
                    ui.horizontal(|ui| {
                        ui.label("Sheet:");
                        if ui.small_button("−").on_hover_text("Zoom out").clicked() {
                            s.sheet_zoom = (s.sheet_zoom / 1.5).clamp(0.25, 8.0);
                        }
                        ui.label(format!("{:.0}%", s.sheet_zoom * 100.0));
                        if ui.small_button("+").on_hover_text("Zoom in").clicked() {
                            s.sheet_zoom = (s.sheet_zoom * 1.5).clamp(0.25, 8.0);
                        }
                        if ui.small_button("Fit").on_hover_text("Reset to fit").clicked() {
                            s.sheet_zoom = 1.0;
                        }
                        crate::tray::ui_theme::hint(ui, "Ctrl+scroll to zoom");
                    });

                    // Ctrl+scroll to zoom (consumes the event before ScrollArea can pan with it).
                    let scroll_zoom = ui.input_mut(|i| {
                        if i.modifiers.ctrl {
                            let delta = i.smooth_scroll_delta.y;
                            i.smooth_scroll_delta = egui::Vec2::ZERO;
                            i.raw_scroll_delta = egui::Vec2::ZERO;
                            delta
                        } else {
                            0.0
                        }
                    });
                    if scroll_zoom != 0.0 {
                        s.sheet_zoom = (s.sheet_zoom * (1.0 + scroll_zoom * 0.005)).clamp(0.25, 8.0);
                    }

                    // Pinch-to-zoom from touchpad gestures.
                    let pinch = ui.input(|i| i.zoom_delta());
                    if pinch != 1.0 {
                        s.sheet_zoom = (s.sheet_zoom * pinch).clamp(0.25, 8.0);
                    }

                    // Compute fit size (fit within available width and height).
                    let sheet_aspect = tex_size.x / tex_size.y;
                    let avail_w = ui.available_width();
                    let avail_h = ui.available_height().max(100.0);
                    let fit_h_for_w = avail_w / sheet_aspect;
                    let (fit_w, fit_h) = if fit_h_for_w <= avail_h {
                        (avail_w, fit_h_for_w)
                    } else {
                        (avail_h * sheet_aspect, avail_h)
                    };
                    let display_w = (fit_w * s.sheet_zoom).max(1.0);
                    let display_h = (fit_h * s.sheet_zoom).max(1.0);

                    // Resolve selected tag info for highlighting/drag.
                    let sel_tag = s.selected_tag_idx.and_then(|idx| s.state.tags.get(idx)).map(|t| {
                        let r = (t.color & 0xFF) as u8;
                        let g = ((t.color >> 8) & 0xFF) as u8;
                        let b = ((t.color >> 16) & 0xFF) as u8;
                        (t.from, t.to, egui::Color32::from_rgba_premultiplied(r, g, b, 35))
                    });
                    egui::ScrollArea::both()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            let (image_rect, resp) = ui.allocate_exact_size(
                                egui::vec2(display_w, display_h),
                                egui::Sense::click_and_drag(),
                            );

                            let cell_w = display_w / cols as f32;
                            let cell_h = display_h / rows as f32;

                            // Helper: pixel pos → clamped frame index.
                            let frame_at = |pos: egui::Pos2| -> usize {
                                let c = ((pos.x - image_rect.left()) / cell_w).floor() as usize;
                                let r = ((pos.y - image_rect.top()) / cell_h).floor() as usize;
                                (r.min(rows - 1) * cols + c.min(cols - 1)).min(total_frames - 1)
                            };

                            // Draw full texture.
                            ui.painter().image(
                                tex.id(),
                                image_rect,
                                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                                egui::Color32::WHITE,
                            );
                            let painter = ui.painter();
                            let grid_color = egui::Color32::from_rgba_premultiplied(200, 200, 200, 60);
                            let accent = egui::Color32::from_rgb(99, 102, 241);

                            // Compute live drag range for preview.
                            let live_range: Option<(usize, usize)> =
                                s.tag_drag_start.zip(resp.interact_pointer_pos().map(&frame_at))
                                .map(|(a, b)| (a.min(b), a.max(b)));

                            // Vertical grid lines
                            for c in 0..=cols {
                                let x = image_rect.left() + c as f32 * cell_w;
                                painter.line_segment(
                                    [egui::pos2(x, image_rect.top()), egui::pos2(x, image_rect.bottom())],
                                    egui::Stroke::new(1.0, grid_color),
                                );
                            }
                            // Horizontal grid lines
                            for r in 0..=rows {
                                let y = image_rect.top() + r as f32 * cell_h;
                                painter.line_segment(
                                    [egui::pos2(image_rect.left(), y), egui::pos2(image_rect.right(), y)],
                                    egui::Stroke::new(1.0, grid_color),
                                );
                            }

                            // Baseline line: one horizontal line per row, showing the walking floor.
                            if s.state.baseline_offset > 0 {
                                let frame_src_h = if s.state.rows > 0 {
                                    s.state.image.height() / s.state.rows
                                } else {
                                    1
                                };
                                let baseline_frac = s.state.baseline_offset as f32 / frame_src_h as f32;
                                let baseline_color = egui::Color32::from_rgba_premultiplied(255, 200, 0, 180); // amber
                                for r in 0..rows {
                                    let y = image_rect.top() + (r as f32 + 1.0 - baseline_frac) * cell_h;
                                    painter.line_segment(
                                        [egui::pos2(image_rect.left(), y), egui::pos2(image_rect.right(), y)],
                                        egui::Stroke::new(1.5, baseline_color),
                                    );
                                }
                            }

                            // Per-cell overlays, labels, borders
                            for i in 0..total_frames {
                                let col = i % cols;
                                let row = i / cols;
                                let cell_rect = egui::Rect::from_min_size(
                                    egui::pos2(
                                        image_rect.left() + col as f32 * cell_w,
                                        image_rect.top() + row as f32 * cell_h,
                                    ),
                                    egui::vec2(cell_w, cell_h),
                                );

                                // Tag range highlight (committed range)
                                if let Some((from, to, color)) = sel_tag
                                    && i >= from && i <= to {
                                        // Brighten during an active drag.
                                        let fill = if live_range.is_some() {
                                            egui::Color32::from_rgba_premultiplied(
                                                color.r(), color.g(), color.b(), 15)
                                        } else {
                                            color
                                        };
                                        painter.rect_filled(cell_rect, 0.0, fill);
                                    }
                                // Live drag preview
                                if let Some((from, to)) = live_range
                                    && i >= from && i <= to
                                        && let Some((_, _, color)) = sel_tag {
                                            painter.rect_filled(
                                                cell_rect,
                                                0.0,
                                                egui::Color32::from_rgba_premultiplied(
                                                    color.r(), color.g(), color.b(), 55),
                                            );
                                        }

                                // Frame number in top-left corner
                                painter.text(
                                    cell_rect.min + egui::vec2(3.0, 2.0),
                                    egui::Align2::LEFT_TOP,
                                    i.to_string(),
                                    egui::FontId::proportional(10.0),
                                    egui::Color32::from_rgba_premultiplied(200, 200, 200, 140),
                                );

                                // Green border on selected frame for preview
                                if s.selected_frame_idx == i {
                                    painter.rect_stroke(
                                        cell_rect,
                                        0.0,
                                        egui::Stroke::new(2.0, accent),
                                        egui::StrokeKind::Outside,
                                    );
                                }
                            }

                            // Eyedropper: pick chromakey color from spritesheet
                            if s.picking_chromakey_color {
                                if resp.hovered() {
                                    ui.output_mut(|o| o.cursor_icon = egui::CursorIcon::Crosshair);
                                }
                                if let Some(pos) = resp.interact_pointer_pos()
                                    && resp.clicked() {
                                        let img_w = s.state.image.width() as f32;
                                        let img_h = s.state.image.height() as f32;
                                        let px_x = ((pos.x - image_rect.left()) / image_rect.width() * img_w)
                                            .floor().clamp(0.0, img_w - 1.0) as u32;
                                        let px_y = ((pos.y - image_rect.top()) / image_rect.height() * img_h)
                                            .floor().clamp(0.0, img_h - 1.0) as u32;
                                        let px = s.state.image.get_pixel(px_x, px_y);
                                        s.chromakey.color = [px.0[0], px.0[1], px.0[2]];
                                        s.chromakey.enabled = true;
                                        s.picking_chromakey_color = false;
                                        s.dirty = true;
                                        s.preview_sheet = None;
                                    }
                            }

                            // Drag → set tag range
                            if s.selected_tag_idx.is_some_and(|idx| idx < s.state.tags.len()) {
                                if resp.drag_started()
                                    && let Some(pos) = resp.interact_pointer_pos() {
                                        s.tag_drag_start = Some(frame_at(pos));
                                    }
                                if resp.dragged() {
                                    // live_range already drives the visual; nothing extra needed
                                }
                                if resp.drag_stopped() {
                                    if let (Some(start), Some(pos)) = (s.tag_drag_start, resp.interact_pointer_pos()) {
                                        let end = frame_at(pos);
                                        let tag_idx = s.selected_tag_idx.unwrap();
                                        s.state.tags[tag_idx].from = start.min(end);
                                        s.state.tags[tag_idx].to = start.max(end);
                                        s.dirty = true;
                                        s.preview_sheet = None;
                                    }
                                    s.tag_drag_start = None;
                                }
                                // Crosshair cursor to hint the sheet is editable
                                if resp.hovered() {
                                    ui.output_mut(|o| o.cursor_icon = egui::CursorIcon::Crosshair);
                                }
                            }

                            // Click (no drag) → select frame for preview
                            if resp.clicked()
                                && let Some(pos) = resp.interact_pointer_pos() {
                                    s.selected_frame_idx = frame_at(pos);
                                }
                        });
                }
            }
        });

        if s.show_export_bundle_dialog {
            egui::Window::new("Export Bundle")
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label("Include state machine:");
                    ui.horizontal(|ui| {
                        if ui.button("Export sprite only").clicked() {
                            s.do_export_bundle(None);
                            s.show_export_bundle_dialog = false;
                        }
                        if ui.button("Cancel").clicked() {
                            s.show_export_bundle_dialog = false;
                        }
                    });
                });
        }

    ctx.request_repaint_after(std::time::Duration::from_millis(16));
}

/// Sets or clears an explicit tag mapping for one SM state.
///
/// - `tag = Some("walk_right")` → inserts `sm_mappings[sm_name][state_name] = "walk_right"`
/// - `tag = None`               → removes the entry (reverts to auto-match)
pub(crate) fn update_tag_mapping(
    sm_mappings: &mut std::collections::HashMap<String, std::collections::HashMap<String, String>>,
    sm_name: &str,
    state_name: &str,
    tag: Option<&str>,
) {
    match tag {
        Some(t) => {
            sm_mappings
                .entry(sm_name.to_string())
                .or_default()
                .insert(state_name.to_string(), t.to_string());
        }
        None => {
            if let Some(m) = sm_mappings.get_mut(sm_name) {
                m.remove(state_name);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::update_tag_mapping;
    use std::collections::HashMap;

    #[test]
    fn tag_mapping_set_explicit_override() {
        let mut mappings: HashMap<String, HashMap<String, String>> = HashMap::new();
        update_tag_mapping(&mut mappings, "default", "walk", Some("walk_right"));
        assert_eq!(mappings["default"]["walk"], "walk_right");
    }

    #[test]
    fn tag_mapping_clear_override_reverts_to_auto() {
        let mut mappings: HashMap<String, HashMap<String, String>> = HashMap::new();
        update_tag_mapping(&mut mappings, "default", "walk", Some("walk_right"));
        update_tag_mapping(&mut mappings, "default", "walk", None);
        assert_eq!(mappings["default"].get("walk"), None);
    }
}
