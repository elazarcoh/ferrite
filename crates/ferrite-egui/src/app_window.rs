use egui;

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum AppTab { Config, Sprites, Sm, Simulation }

pub struct AppWindowState {
    pub selected_tab: AppTab,
    pub should_close: bool,
    pub dark_mode: bool,
    pub dark_mode_out: Option<bool>,

    // ── Config tab ──
    pub config_state: crate::config_panel::ConfigPanelState,

    // ── Sprites tab ──
    pub gallery: Vec<crate::gallery::GalleryEntry>,
    pub selected_sprite_key: Option<String>,
    pub sprite_editor: Option<crate::sprite_editor::SpriteEditorViewport>,
    pub pending_png_pick: Option<crossbeam_channel::Receiver<Option<std::path::PathBuf>>>,
    pub saved_json_path: Option<std::path::PathBuf>,
    pub pending_sprite_delete: Option<String>,

    // ── SM tab ──
    pub sm: crate::sm_editor::SmEditorViewport,

    // ── Dirty flags ──
    pub sm_gallery_dirty: bool,

    /// When `true`, `render_full_window` skips rendering the Simulation tab body so the
    /// caller can render it separately (e.g. `WebApp`).
    pub simulation_override: bool,
}

pub fn render_app_tab_bar(ctx: &egui::Context, s: &mut AppWindowState) {
    egui::TopBottomPanel::top("app_tab_bar").show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut s.selected_tab, AppTab::Config, "⚙ Config");
            ui.selectable_value(&mut s.selected_tab, AppTab::Sprites, "🖼 Sprites");
            ui.selectable_value(&mut s.selected_tab, AppTab::Sm, "🤖 State Machine");
            ui.selectable_value(&mut s.selected_tab, AppTab::Simulation, "▶ Simulation");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                #[cfg(not(target_arch = "wasm32"))]
                if ui.button("✕").clicked() {
                    s.should_close = true;
                }
                if crate::ui_theme::dark_light_toggle(ui, &mut s.dark_mode, ctx) {
                    s.dark_mode_out = Some(s.dark_mode);
                }
            });
        });
    });
}

pub fn render_full_window(ctx: &egui::Context, s: &mut AppWindowState) {
    let current_dark = s.dark_mode;
    s.config_state.dark_mode = current_dark;
    s.sm.dark_mode = current_dark;
    if let Some(ref mut ed) = s.sprite_editor {
        ed.dark_mode = current_dark;
    }
    crate::ui_theme::apply_theme(ctx, s.dark_mode);
    render_app_tab_bar(ctx, s);
    let tab = s.selected_tab;
    match tab {
        AppTab::Config => render_config_tab(ctx, s),
        AppTab::Sprites => render_sprites_tab(ctx, s),
        AppTab::Sm => crate::sm_editor::render_sm_panel(ctx, &mut s.sm),
        AppTab::Simulation => {
            if !s.simulation_override {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.label("Simulation not available in this context.");
                });
            }
            // If simulation_override=true, caller renders simulation after this call
        }
    }
}

fn render_config_tab(ctx: &egui::Context, s: &mut AppWindowState) {
    crate::config_panel::render_config_panel(ctx, &mut s.config_state, &mut s.sm_gallery_dirty);
}

fn render_sprites_tab(ctx: &egui::Context, s: &mut AppWindowState) {
    // Left gallery panel
    egui::SidePanel::left("sprite_gallery_panel")
        .resizable(true)
        .min_width(150.0)
        .show(ctx, |ui| {
            ui.heading("Sprites");
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| {
                let entries: Vec<_> = s.gallery.iter()
                    .map(|e| (e.key.clone(), e.display_name.clone()))
                    .collect();
                for (key, display_name) in entries {
                    let selected = s.selected_sprite_key.as_deref() == Some(key.as_str());
                    if ui.selectable_label(selected, &display_name).clicked() {
                        s.selected_sprite_key = Some(key.clone());
                    }
                }
            });
        });

    // Central area: sprite editor panels or placeholder
    if let Some(ed) = &mut s.sprite_editor {
        crate::sprite_editor::render_sprite_editor_panel(ctx, ed);
    } else {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.centered_and_justified(|ui| {
                ui.label("Select a sprite from the list to edit.");
            });
        });
    }
}
