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
    pending_png_bytes: Option<crossbeam_channel::Receiver<(String, Vec<u8>)>>,
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
            wants_png_import: false,
            sm,
            sm_gallery_dirty: false,
            simulation_override: true,
        };
        let simulation = crate::simulation::SimulationState::new(config);
        Self { state, simulation, sheet_loader, last_tick_ms: 0.0, pending_png_bytes: None }
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

        // Consume wants_png_import: spawn async file picker (wasm only)
        if self.state.wants_png_import && self.pending_png_bytes.is_none() {
            self.state.wants_png_import = false;
            let (tx, rx) = crossbeam_channel::bounded(1);
            wasm_bindgen_futures::spawn_local(async move {
                if let Some(fh) = rfd::AsyncFileDialog::new()
                    .add_filter("PNG", &["png"])
                    .pick_file()
                    .await
                {
                    let name = fh.file_name();
                    let bytes = fh.read().await;
                    tx.send((name, bytes)).ok();
                }
            });
            self.pending_png_bytes = Some(rx);
        }

        // Poll PNG import result
        if let Some(ref rx) = self.pending_png_bytes {
            match rx.try_recv() {
                Ok((filename, png_bytes)) => {
                    self.pending_png_bytes = None;
                    let key = format!("user://{filename}");
                    match make_single_frame_json_stub(&png_bytes) {
                        Ok(json_bytes) => {
                            self.sheet_loader.register(key.clone(), json_bytes, png_bytes);
                            // Avoid duplicate gallery entries for the same key
                            if !self.state.gallery.iter().any(|e| e.key == key) {
                                self.state.gallery.push(ferrite_egui::gallery::GalleryEntry {
                                    key: key.clone(),
                                    display_name: filename,
                                });
                            }
                            self.state.selected_tab = AppTab::Sprites;
                            self.state.selected_sprite_key = Some(key);
                            self.state.sprite_editor = None; // auto-recreated next frame
                        }
                        Err(e) => log::warn!("Failed to generate stub JSON for imported PNG: {e}"),
                    }
                }
                Err(crossbeam_channel::TryRecvError::Disconnected) => {
                    self.pending_png_bytes = None;
                }
                Err(crossbeam_channel::TryRecvError::Empty) => {}
            }
        }

        // Handle "Edit…" from Config tab → switch to Sprites tab and select the sheet
        if let Some(req) = self.state.config_state.open_editor_request.take() {
            use ferrite_egui::config_panel::OpenEditorRequest;
            if let OpenEditorRequest::Edit(sheet_path) = req {
                self.state.selected_tab = AppTab::Sprites;
                self.state.selected_sprite_key = Some(sheet_path);
                self.state.sprite_editor = None; // will be (re)created below
            }
            // OpenEditorRequest::New is cfg(not(wasm)) so can't appear here
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

/// Generate minimal single-frame Aseprite-compatible JSON from raw PNG bytes.
/// The frame covers the entire image. Used when importing a bare PNG with no sheet metadata.
fn make_single_frame_json_stub(png_bytes: &[u8]) -> anyhow::Result<Vec<u8>> {
    use image::GenericImageView as _;
    let img = image::load_from_memory_with_format(png_bytes, image::ImageFormat::Png)?;
    let (w, h) = img.dimensions();
    let json = format!(
        r#"{{"frames":[{{"frame":{{"x":0,"y":0,"w":{w},"h":{h}}},"duration":100}}],"meta":{{"frameTags":[]}}}}"#
    );
    Ok(json.into_bytes())
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
