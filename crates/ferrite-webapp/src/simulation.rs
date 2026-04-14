use egui;
use ferrite_core::config::schema::Config;
use ferrite_core::geometry::PlatformBounds;
use ferrite_core::sprite::animation::AnimationState;
use ferrite_core::sprite::sheet::SpriteSheet;
use ferrite_core::sprite::sm_runner::{load_default_sm, EnvironmentSnapshot, SMRunner};

pub struct PetSimState {
    pub id: String,
    pub x: i32,
    pub y: i32,
    pub scale: f32,
    pub sheet: SpriteSheet,
    pub sm: SMRunner,
    pub anim: AnimationState,
}

pub struct SimulationState {
    pub pets: Vec<PetSimState>,
}

const SIM_SCREEN_W: i32 = 800;
const SIM_FLOOR_Y: i32 = 500;

impl SimulationState {
    pub fn new(config: Config) -> Self {
        let default_sm = load_default_sm();
        let mut pets = Vec::new();

        for pet_cfg in &config.pets {
            let sheet = match crate::web_storage::load_sheet_for_config(&pet_cfg.sheet_path) {
                Ok(s) => s,
                Err(e) => {
                    log::warn!("SimulationState: failed to load sheet '{}': {e}", pet_cfg.sheet_path);
                    continue;
                }
            };

            let sm = SMRunner::new(default_sm.clone(), pet_cfg.walk_speed);
            let initial_tag = sm.current_state_name().to_string();
            let anim = AnimationState::new(initial_tag);

            let init_h = if let Some(frame) = sheet.frames.first() {
                (frame.h as f32 * pet_cfg.scale) as i32
            } else {
                32
            };

            pets.push(PetSimState {
                id: pet_cfg.id.clone(),
                x: SIM_SCREEN_W / 4,        // start at 25% of sim width
                y: SIM_FLOOR_Y - init_h,    // just above the floor
                scale: pet_cfg.scale,
                sheet,
                sm,
                anim,
            });
        }

        Self { pets }
    }

    pub fn tick(&mut self, delta_ms: u32) {
        let pet_count = self.pets.len() as u32;
        for pet in &mut self.pets {
            // Estimate pet dimensions from the first frame
            let (pet_w, pet_h) = if let Some(frame) = pet.sheet.frames.first() {
                (
                    (frame.w as f32 * pet.scale) as i32,
                    (frame.h as f32 * pet.scale) as i32,
                )
            } else {
                (32, 32)
            };

            pet.sm.update_env(EnvironmentSnapshot {
                pet_count,
                // sim floor is treated as one full-width surface; no virtual-ground distinction.
                surface_w: SIM_SCREEN_W as f32,
                // No cursor, no app focus, no time-of-day in headless sim.
                ..EnvironmentSnapshot::default()
            });

            let bounds = PlatformBounds {
                screen_w: SIM_SCREEN_W,
                screen_h: SIM_FLOOR_Y + 4,  // virtual_ground_y() == SIM_FLOOR_Y
            };

            let tag = pet.sm.tick(
                delta_ms,
                &mut pet.x,
                &mut pet.y,
                &bounds,
                pet_w,
                pet_h,
                SIM_FLOOR_Y,
                &pet.sheet,
            );
            pet.anim.set_tag(tag);
            pet.anim.tick(&pet.sheet, delta_ms);
        }
    }

    pub fn process_event(&mut self, event_json: &str) {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(event_json) {
            let event_type = val["type"].as_str().unwrap_or("");
            let pet_id = val["pet_id"].as_str().unwrap_or("");
            for pet in &mut self.pets {
                if pet.id == pet_id {
                    match event_type {
                        "grab" => pet.sm.interrupt("grabbed", Some((0, 0))),
                        "release" => pet.sm.release((0.0, 0.0)),
                        _ => log::warn!("unknown event type: {event_type}"),
                    }
                    break;
                }
            }
        }
    }

    pub fn render(&self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::TopBottomPanel::top("sim_toolbar").show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Bundle:");
                    ui.group(|ui| {
                    if ui.button("Import Bundle\u{2026}").clicked() {
                        wasm_bindgen_futures::spawn_local(async move {
                            if let Some(bytes) = crate::import_export::pick_and_read_bundle().await {
                                match crate::import_export::import_bundle(&bytes) {
                                    Ok(contents) => {
                                        log::info!("imported bundle: {}", contents.bundle_name);
                                        crate::bridge::set_pending_import(contents);
                                    }
                                    Err(e) => log::error!("import failed: {e}"),
                                }
                            }
                        });
                    }
                    if ui.button("Export Bundle\u{2026}").clicked() {
                        crate::import_export::export_bundle(
                            "ferrite-export",
                            r#"{"frames":[],"meta":{"frameTags":[]}}"#,
                            &[],
                            None,
                        );
                    }
                    }); // end group
                });
            });

            let panel_rect = ui.available_rect_before_wrap();
            let panel_w = panel_rect.width();
            let panel_h = panel_rect.height();

            // Scale simulation coords to panel dimensions.
            // Simulation space: SIM_SCREEN_W wide, floor at SIM_FLOOR_Y.
            // Map SIM_FLOOR_Y to 85% of panel height so there's headroom above.
            let x_scale = panel_w / SIM_SCREEN_W as f32;
            let y_scale = (panel_h * 0.85) / SIM_FLOOR_Y as f32;

            // Draw floor line at scaled position
            let floor_y = panel_rect.top() + SIM_FLOOR_Y as f32 * y_scale;
            ui.painter().line_segment(
                [
                    egui::pos2(panel_rect.left(), floor_y),
                    egui::pos2(panel_rect.right(), floor_y),
                ],
                egui::Stroke::new(2.0, egui::Color32::from_rgb(100, 100, 130)),
            );

            // Draw each pet using the current animation frame as a texture
            for pet in &self.pets {
                let abs_frame = pet.anim.absolute_frame(&pet.sheet);
                let frame = pet.sheet.frames.get(abs_frame);

                let (frame_w, frame_h) = if let Some(f) = frame {
                    (f.w as f32 * pet.scale * x_scale, f.h as f32 * pet.scale * y_scale)
                } else {
                    (32.0 * x_scale, 32.0 * y_scale)
                };

                let px = panel_rect.left() + pet.x as f32 * x_scale;
                let py = panel_rect.top() + pet.y as f32 * y_scale;
                let rect = egui::Rect::from_min_size(egui::pos2(px, py), egui::vec2(frame_w, frame_h));

                if let Some(f) = frame {
                    let img_w = pet.sheet.image.width();
                    let img_h = pet.sheet.image.height();

                    // Crop the current frame pixels from the spritesheet
                    let cropped_w = f.w as usize;
                    let cropped_h = f.h as usize;
                    let mut pixels = Vec::with_capacity(cropped_w * cropped_h);
                    for row in 0..f.h {
                        for col in 0..f.w {
                            let sx = (f.x + col).min(img_w.saturating_sub(1));
                            let sy = (f.y + row).min(img_h.saturating_sub(1));
                            let p = pet.sheet.image.get_pixel(sx, sy);
                            pixels.push(egui::Color32::from_rgba_unmultiplied(p[0], p[1], p[2], p[3]));
                        }
                    }

                    let color_image = egui::ColorImage {
                        size: [cropped_w, cropped_h],
                        pixels,
                        source_size: egui::Vec2::new(cropped_w as f32, cropped_h as f32),
                    };

                    let tex_name = format!("pet_{}_{}", pet.id, abs_frame);
                    let tex = ctx.load_texture(
                        tex_name,
                        color_image,
                        egui::TextureOptions::LINEAR,
                    );

                    let flip = pet.sm.compute_flip(&pet.sheet);
                    let (uv_x0, uv_x1) = if flip { (1.0_f32, 0.0_f32) } else { (0.0_f32, 1.0_f32) };
                    let uv = egui::Rect::from_min_max(
                        egui::pos2(uv_x0, 0.0),
                        egui::pos2(uv_x1, 1.0),
                    );
                    ui.painter().image(tex.id(), rect, uv, egui::Color32::WHITE);
                } else {
                    // Fallback: placeholder rectangle
                    ui.painter().rect_filled(rect, 4.0, egui::Color32::from_rgba_premultiplied(80, 140, 200, 180));
                }

                ui.painter().text(
                    rect.center_top(),
                    egui::Align2::CENTER_BOTTOM,
                    format!("{} [{}]", pet.id, pet.sm.current_state_name()),
                    egui::FontId::proportional(10.0),
                    egui::Color32::WHITE,
                );
            }
        });
    }

    pub fn snapshot_pets(&self) -> Vec<crate::bridge::PetStateSnapshot> {
        self.pets.iter().map(|pet| crate::bridge::PetStateSnapshot {
            id: pet.id.clone(),
            x: pet.x as f32,
            y: pet.y as f32,
            sm_state: pet.sm.current_state_name().to_string(),
            animation_tag: pet.anim.current_tag.clone(),
        }).collect()
    }
}
