use egui;
use std::sync::Arc;
use ferrite_egui::app_window::{AppTab, AppWindowState, render_full_window};
use ferrite_egui::config_panel::ConfigPanelState;
use ferrite_core::sprite::{editor_state::{EditorTag, SpriteEditorState}, sheet::SpriteSheet};
use ferrite_egui::gallery::SheetLoader;

pub struct WebApp {
    state: AppWindowState,
    simulation: crate::simulation::SimulationState,
    sheet_loader: Arc<crate::web_storage::WebSheetLoader>,
    last_tick_ms: f64,
}

impl WebApp {
    pub fn new() -> Self {
        crate::bridge::init_bridge_state();
        let config = crate::config_store::load();
        let storage = Box::new(crate::web_storage::WebSmStorage::new());
        let sheet_loader = Arc::new(crate::web_storage::WebSheetLoader::new());
        let loader: Box<dyn ferrite_egui::gallery::SheetLoader> =
            Box::new(crate::web_storage::SharedWebSheetLoader(sheet_loader.clone()));
        let gallery = crate::web_storage::build_gallery();
        let config_state = ConfigPanelState::new(config.clone(), gallery.clone(), loader);

        // SmEditorViewport::new wraps in Arc<Mutex<_>>; unwrap since we hold the only reference.
        let sm_arc = ferrite_egui::sm_editor::SmEditorViewport::new(true, storage);
        let sm = match std::sync::Arc::try_unwrap(sm_arc) {
            Ok(mutex) => mutex.into_inner().unwrap_or_else(|e| e.into_inner()),
            Err(_) => panic!("SmEditorViewport::new returned Arc with unexpected extra references"),
        };

        let state = AppWindowState {
            selected_tab: AppTab::Simulation,
            should_close: false,
            dark_mode: true,
            dark_mode_out: None,
            config_state,
            gallery,
            selected_sprite_key: None,
            sprite_editor: None,
            pending_png_pick: None,
            saved_json_path: None,
            pending_sprite_delete: None,
            sm,
            sm_gallery_dirty: false,
            simulation_override: true,
        };
        let simulation = crate::simulation::SimulationState::new(config);
        Self { state, simulation, sheet_loader, last_tick_ms: 0.0 }
    }
}

impl eframe::App for WebApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Persist config changes
        if self.state.config_state.config_dirty {
            self.state.config_state.config_dirty = false;
            crate::config_store::save(&self.state.config_state.config);
        }

        // Tick simulation
        let now = js_sys::Date::now();
        let delta_ms = if self.last_tick_ms == 0.0 {
            16.0
        } else {
            (now - self.last_tick_ms).min(100.0)
        };
        self.last_tick_ms = now;
        self.simulation.tick(delta_ms as u32);

        // Process injected events from JS bridge
        for event_json in crate::bridge::drain_events() {
            self.simulation.process_event(&event_json);
        }

        // Process any pending bundle import
        if let Some(contents) = crate::bridge::take_pending_import() {
            let path = format!("bundle://{}", contents.bundle_name);
            self.sheet_loader.register(path, contents.sprite_json.into_bytes(), contents.sprite_png);
        }

        // Wire sprite editor: create when a sprite is selected but editor is not loaded yet
        if self.state.sprite_editor.is_none() {
            if let Some(key) = self.state.selected_sprite_key.clone() {
                if let Ok(sheet) = self.sheet_loader.load_sheet(&key) {
                    let state = sprite_editor_state_from_sheet(&key, sheet);
                    let mut ed = ferrite_egui::sprite_editor::SpriteEditorViewport::new(state);
                    ed.is_builtin = key.starts_with("embedded://");
                    ed.dark_mode = self.state.dark_mode;
                    self.state.sprite_editor = Some(ed);
                }
            }
        }

        // Render tabs (simulation tab body skipped due to simulation_override=true)
        render_full_window(ctx, &mut self.state);

        // Render simulation tab if selected
        if self.state.selected_tab == AppTab::Simulation {
            self.simulation.render(ctx);
        }

        // Update JS bridge state
        crate::bridge::update_bridge_state(
            self.simulation.snapshot_pets(),
            self.state.dark_mode,
        );

        ctx.request_repaint_after(std::time::Duration::from_millis(16));
    }
}

fn sprite_editor_state_from_sheet(key: &str, sheet: SpriteSheet) -> SpriteEditorState {
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

    let png_path = std::path::PathBuf::from(key);
    let mut state = SpriteEditorState::new(png_path, sheet.image);
    state.rows = rows;
    state.cols = cols;
    state.tags = tags;
    state.sm_mappings = sheet.sm_mappings;
    state.chromakey = sheet.chromakey;
    state.baseline_offset = sheet.baseline_offset;
    state
}
