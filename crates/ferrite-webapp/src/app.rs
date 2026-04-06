use egui;
use ferrite_egui::app_window::{AppTab, AppWindowState, render_full_window};
use ferrite_egui::config_panel::ConfigPanelState;

pub struct WebApp {
    state: AppWindowState,
    simulation: crate::simulation::SimulationState,
    last_tick_ms: f64,
}

impl WebApp {
    pub fn new() -> Self {
        crate::bridge::init_bridge_state();
        let config = crate::config_store::load();
        let storage = Box::new(crate::web_storage::WebSmStorage::new());
        let loader = Box::new(crate::web_storage::WebSheetLoader::new());
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
        Self { state, simulation, last_tick_ms: 0.0 }
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
