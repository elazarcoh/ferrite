use crate::sprite::{
    animation::AnimationState,
    editor_state::SpriteEditorState,
    sheet::{Frame, FrameTag, SpriteSheet, TagDirection},
};
use eframe::egui;
use std::sync::{Arc, Mutex};

pub struct SpriteEditorViewport {
    pub state: SpriteEditorState,
    pub texture: Option<egui::TextureHandle>,
    pub anim: AnimationState,
    pub preview_sheet: Option<SpriteSheet>,
    pub should_close: bool,
    pub dark_mode: bool,        // synced from App each frame
    pub dark_mode_out: Option<bool>,  // set by toggle, read by App
    selected_tag_idx: Option<usize>,
    selected_frame_idx: usize,
    dirty: bool,
}

impl SpriteEditorViewport {
    pub fn new(state: SpriteEditorState) -> Self {
        let tag = state.tags.first().map(|t| t.name.clone()).unwrap_or_default();
        Self {
            state,
            texture: None,
            anim: AnimationState::new(tag),
            preview_sheet: None,
            should_close: false,
            dark_mode: true,
            dark_mode_out: None,
            selected_tag_idx: None,
            selected_frame_idx: 0,
            dirty: false,
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
        }).collect();
        self.preview_sheet = Some(SpriteSheet {
            image: self.state.image.clone(),
            frames,
            tags,
        });
    }

    /// Total number of frames in the grid.
    fn total_frames(&self) -> usize {
        (self.state.rows * self.state.cols) as usize
    }
}

pub fn open_sprite_editor_viewport(
    ctx: &egui::Context,
    state: Arc<Mutex<SpriteEditorViewport>>,
) {
    let viewport_id = egui::ViewportId::from_hash_of("sprite_editor");
    let viewport_builder = egui::ViewportBuilder::default()
        .with_title("Sprite Editor")
        .with_inner_size([900.0, 600.0]);

    ctx.show_viewport_deferred(viewport_id, viewport_builder, move |ctx, _vp_class| {
        if ctx.input(|i| i.viewport().close_requested()) {
            if let Ok(mut s) = state.lock() {
                s.should_close = true;
            }
            return;
        }

        let Ok(mut s) = state.lock() else { return };

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
                ui.label(format!("File: {}", s.state.png_path.display()));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Export PNG\u{2026}").clicked() {
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
                    if ui.add_enabled(s.dirty, save_btn).clicked() {
                        if let Some(dir) = s.state.png_path.parent() {
                            if let Err(e) = s.state.save_to_dir(dir) {
                                log::warn!("save sprite editor: {e}");
                            } else {
                                s.dirty = false;
                            }
                        }
                    }
                });
            });
        });

        // Left panel: tag list and grid settings
        egui::SidePanel::left("tag_panel").min_width(180.0).show(ctx, |ui| {
            ui.heading("Grid");
            ui.horizontal(|ui| {
                ui.label("Cols:");
                let mut cols = s.state.cols;
                if ui.add(egui::DragValue::new(&mut cols).range(1..=64)).changed() {
                    s.state.cols = cols;
                    s.dirty = true;
                    s.preview_sheet = None; // force rebuild
                }
            });
            ui.horizontal(|ui| {
                ui.label("Rows:");
                let mut rows = s.state.rows;
                if ui.add(egui::DragValue::new(&mut rows).range(1..=64)).changed() {
                    s.state.rows = rows;
                    s.dirty = true;
                    s.preview_sheet = None; // force rebuild
                }
            });
            let total = s.total_frames();
            ui.label(format!("{} frames", total));

            ui.separator();
            ui.heading("Tags");

            egui::ScrollArea::vertical().max_height(250.0).show(ui, |ui| {
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
            });

            ui.separator();

            // Selected tag controls
            if let Some(tag_idx) = s.selected_tag_idx {
                if tag_idx < s.state.tags.len() {
                    let total = s.total_frames();

                    // From / To frame range
                    ui.horizontal(|ui| {
                        ui.label("From:");
                        let mut from = s.state.tags[tag_idx].from;
                        if ui.add(egui::DragValue::new(&mut from).range(0..=total.saturating_sub(1))).changed() {
                            s.state.tags[tag_idx].from = from;
                            s.dirty = true;
                            s.preview_sheet = None;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("To:");
                        let mut to = s.state.tags[tag_idx].to;
                        if ui.add(egui::DragValue::new(&mut to).range(0..=total.saturating_sub(1))).changed() {
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
                    color,
                });
                s.dirty = true;
                s.preview_sheet = None;
                new_tag_name.clear();
            }
            ui.data_mut(|d| d.insert_temp(id, new_tag_name));

            ui.separator();
            ui.heading("Tag Map");
            let tag_names: Vec<String> = s.state.tags.iter().map(|t| t.name.clone()).collect();
            if tag_map_ui(ui, &mut s.state.tag_map, &tag_names) {
                s.dirty = true;
            }
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
                s.preview_sheet = Some(sheet);
                frame
            } else {
                None
            };
            if let (Some(tex), Some(f)) = (&tex, preview_frame_data) {
                let tex_size = tex.size_vec2();
                if tex_size.x > 0.0 && tex_size.y > 0.0 && f.w > 0 && f.h > 0 {
                    let uv = egui::Rect::from_min_max(
                        egui::pos2(f.x as f32 / tex_size.x, f.y as f32 / tex_size.y),
                        egui::pos2(
                            (f.x + f.w) as f32 / tex_size.x,
                            (f.y + f.h) as f32 / tex_size.y,
                        ),
                    );
                    let aspect = f.w as f32 / f.h as f32;
                    let preview_w = preview_h * aspect;
                    let (rect, _resp) = ui.allocate_exact_size(
                        egui::vec2(preview_w, preview_h),
                        egui::Sense::hover(),
                    );
                    ui.painter().image(tex.id(), rect, uv, egui::Color32::WHITE);
                }
            }

            // Frame strip (scrollable)
            ui.separator();
            ui.label("Frames:");
            egui::ScrollArea::horizontal().show(ui, |ui| {
                ui.horizontal(|ui| {
                    if let Some(ref tex) = tex {
                        let thumb_h = 64.0_f32;
                        let tex_size = tex.size_vec2();
                        for i in 0..total_frames {
                            let (fx, fy, fw, fh) = s.state.frame_rect(i);
                            if tex_size.x > 0.0 && tex_size.y > 0.0 && fw > 0 && fh > 0 {
                                let uv = egui::Rect::from_min_max(
                                    egui::pos2(fx as f32 / tex_size.x, fy as f32 / tex_size.y),
                                    egui::pos2(
                                        (fx + fw) as f32 / tex_size.x,
                                        (fy + fh) as f32 / tex_size.y,
                                    ),
                                );
                                let thumb_w = thumb_h * (fw as f32 / fh as f32);
                                let (rect, resp) = ui.allocate_exact_size(
                                    egui::vec2(thumb_w, thumb_h),
                                    egui::Sense::click(),
                                );
                                ui.painter().image(tex.id(), rect, uv, egui::Color32::WHITE);
                                if s.selected_frame_idx == i {
                                    ui.painter().rect_stroke(
                                        rect,
                                        0.0,
                                        egui::Stroke::new(2.0, egui::Color32::YELLOW),
                                        egui::StrokeKind::Outside,
                                    );
                                }
                                if resp.clicked() {
                                    s.selected_frame_idx = i;
                                }
                            }
                        }
                    }
                });
            });
        });

        ctx.request_repaint_after(std::time::Duration::from_millis(16));
    });
}

/// UI for editing the AnimTagMap behavior-to-tag mapping.
/// Returns true if any tag mapping changed.
fn tag_map_ui(
    ui: &mut egui::Ui,
    tag_map: &mut crate::sprite::behavior::AnimTagMap,
    tag_names: &[String],
) -> bool {
    let names: Vec<&str> = tag_names.iter().map(|s| s.as_str()).collect();
    let mut changed = false;

    changed |= required_tag_combo_edit(ui, "idle", "tm_idle", &mut tag_map.idle, &names);
    changed |= required_tag_combo_edit(ui, "walk", "tm_walk", &mut tag_map.walk, &names);
    changed |= optional_tag_combo_edit(ui, "run", "tm_run", &mut tag_map.run, &names);
    changed |= optional_tag_combo_edit(ui, "sit", "tm_sit", &mut tag_map.sit, &names);
    changed |= optional_tag_combo_edit(ui, "sleep", "tm_sleep", &mut tag_map.sleep, &names);
    changed |= optional_tag_combo_edit(ui, "wake", "tm_wake", &mut tag_map.wake, &names);
    changed |= optional_tag_combo_edit(ui, "grabbed", "tm_grab", &mut tag_map.grabbed, &names);
    changed |= optional_tag_combo_edit(ui, "petted", "tm_pet", &mut tag_map.petted, &names);
    changed |= optional_tag_combo_edit(ui, "react", "tm_react", &mut tag_map.react, &names);
    changed |= optional_tag_combo_edit(ui, "fall", "tm_fall", &mut tag_map.fall, &names);
    changed |= optional_tag_combo_edit(ui, "thrown", "tm_thrown", &mut tag_map.thrown, &names);
    changed
}

fn required_tag_combo_edit(
    ui: &mut egui::Ui,
    label: &str,
    id: &str,
    value: &mut String,
    tag_names: &[&str],
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(label);
        let display = if value.is_empty() { "(none)" } else { value.as_str() };
        egui::ComboBox::from_id_salt(id)
            .selected_text(display)
            .show_ui(ui, |ui| {
                for name in tag_names {
                    if ui.selectable_label(value.as_str() == *name, *name).clicked() {
                        *value = name.to_string();
                        changed = true;
                    }
                }
            });
    });
    changed
}

fn optional_tag_combo_edit(
    ui: &mut egui::Ui,
    label: &str,
    id: &str,
    value: &mut Option<String>,
    tag_names: &[&str],
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(label);
        let display = value.as_deref().unwrap_or("\u{2014} not set \u{2014}");
        egui::ComboBox::from_id_salt(id)
            .selected_text(display)
            .show_ui(ui, |ui| {
                if ui.selectable_label(value.is_none(), "\u{2014} not set \u{2014}").clicked() {
                    *value = None;
                    changed = true;
                }
                for name in tag_names {
                    let selected = value.as_deref() == Some(*name);
                    if ui.selectable_label(selected, *name).clicked() {
                        *value = Some(name.to_string());
                        changed = true;
                    }
                }
            });
    });
    changed
}
