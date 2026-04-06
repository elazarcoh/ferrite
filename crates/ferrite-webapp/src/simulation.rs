use egui;
use ferrite_core::config::schema::Config;
use ferrite_core::sprite::animation::AnimationState;
use ferrite_core::sprite::sheet::SpriteSheet;
use ferrite_core::sprite::sm_runner::{load_default_sm, SMRunner};

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

            pets.push(PetSimState {
                id: pet_cfg.id.clone(),
                x: pet_cfg.x,
                y: pet_cfg.y,
                scale: pet_cfg.scale,
                sheet,
                sm,
                anim,
            });
        }

        Self { pets }
    }

    pub fn tick(&mut self, delta_ms: u32) {
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

            let tag = pet.sm.tick(
                delta_ms,
                &mut pet.x,
                &mut pet.y,
                SIM_SCREEN_W,
                pet_w,
                pet_h,
                SIM_FLOOR_Y,
                &pet.sheet,
            );
            pet.anim.set_tag(tag);
            pet.anim.tick(&pet.sheet, delta_ms);
        }
    }

    pub fn render(&self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::TopBottomPanel::top("sim_toolbar").show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("Import Bundle\u{2026}").clicked() {
                        wasm_bindgen_futures::spawn_local(async move {
                            if let Some(bytes) = crate::import_export::pick_and_read_bundle().await {
                                log::info!("bundle imported: {} bytes", bytes.len());
                                // Future: register with WebSheetLoader and update config
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
                });
            });

            let panel_rect = ui.available_rect_before_wrap();

            // Draw floor line
            let floor_y = panel_rect.top() + SIM_FLOOR_Y as f32;
            ui.painter().line_segment(
                [
                    egui::pos2(panel_rect.left(), floor_y),
                    egui::pos2(panel_rect.right(), floor_y),
                ],
                egui::Stroke::new(2.0, egui::Color32::from_rgb(100, 100, 100)),
            );

            // Draw each pet using the current animation frame as a texture
            for pet in &self.pets {
                let abs_frame = pet.anim.absolute_frame(&pet.sheet);
                let frame = pet.sheet.frames.get(abs_frame);

                let (frame_w, frame_h) = if let Some(f) = frame {
                    (f.w as f32 * pet.scale, f.h as f32 * pet.scale)
                } else {
                    (32.0, 32.0)
                };

                let px = panel_rect.left() + pet.x as f32;
                let py = panel_rect.top() + pet.y as f32;
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

                    let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
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
