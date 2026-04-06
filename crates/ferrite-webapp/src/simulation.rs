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

            // Draw each pet as a colored rectangle placeholder
            for pet in &self.pets {
                let (frame_w, frame_h) = if let Some(frame) = pet.sheet.frames.first() {
                    (frame.w as f32 * pet.scale, frame.h as f32 * pet.scale)
                } else {
                    (32.0, 32.0)
                };

                let px = panel_rect.left() + pet.x as f32;
                let py = panel_rect.top() + pet.y as f32;
                let rect = egui::Rect::from_min_size(egui::pos2(px, py), egui::vec2(frame_w, frame_h));

                ui.painter().rect_filled(rect, 4.0, egui::Color32::from_rgba_premultiplied(80, 140, 200, 180));
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
