use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use eframe::egui;
use crate::sprite::sm_compiler::CompileError;
use crate::sprite::sm_runner::{TransitionLogEntry, DEFAULT_SM_TOML};
use crate::sprite::sm_gallery::SmGallery;

/// Live snapshot of condition variables for the inspector.
#[derive(Clone, Debug, Default)]
pub struct VarSnapshot {
    pub cursor_dist: f32,
    pub state_time_ms: u32,
    pub on_surface: bool,
    pub near_edge: bool,
    pub pet_x: f32,
    pub pet_y: f32,
    pub pet_vx: f32,
    pub pet_vy: f32,
    pub pet_v: f32,
    pub hour: u32,
    pub focused_app: String,
}

/// Fields written by the egui thread, read by the app thread each frame.
pub struct SmEditorFromUi {
    pub saved_sm_name: Option<String>,
    pub force_state: Option<String>,
    pub release_force: bool,
    pub step_mode: bool,
    pub step_advance: bool,
    pub should_close: bool,
}

/// Fields written by the app thread, read by the egui thread each frame.
pub struct SmEditorFromApp {
    pub active_state: Option<String>,
    pub is_forced: bool,
    pub var_snapshot: VarSnapshot,
    pub transition_log: Vec<TransitionLogEntry>,
    pub validation_errors: Vec<CompileError>,
}

pub struct SmEditorViewport {
    pub from_ui: SmEditorFromUi,
    pub from_app: SmEditorFromApp,
    pub selected_sm: Option<String>,
    pub editor_text: String,
    pub is_dirty: bool,
    pub dark_mode: bool,
    pub config_dir: PathBuf,
    pub cached_gallery: Option<SmGallery>,
    pub save_errors: Vec<crate::sprite::sm_compiler::CompileError>,
    pub has_saved_once: bool,
}

const MINIMAL_TEMPLATE: &str = r#"[meta]
name = "My SM"
version = "1.0"
engine_min_version = "1.0"
default_fallback = "idle"

[interrupts]
grabbed = { goto = "grabbed" }
petted  = { goto = "petted" }

[states.idle]
required = true
action   = "idle"
transitions = [
  { goto = "walk", weight = 50, after = "1s..3s" },
]

[states.walk]
required = true
action   = "walk"
dir      = "random"
distance = "200px..600px"
transitions = [{ goto = "idle" }]

[states.grabbed]
required = true
action   = "grabbed"
transitions = []

[states.fall]
required = true
action   = "fall"
transitions = [{ goto = "idle", condition = "on_surface" }]

[states.thrown]
required = true
action   = "thrown"
transitions = [{ goto = "fall", condition = "on_surface" }]
"#;

impl SmEditorViewport {
    pub fn new(dark_mode: bool, config_dir: PathBuf) -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self {
            from_ui: SmEditorFromUi {
                saved_sm_name: None,
                force_state: None,
                release_force: false,
                step_mode: false,
                step_advance: false,
                should_close: false,
            },
            from_app: SmEditorFromApp {
                active_state: None,
                is_forced: false,
                var_snapshot: VarSnapshot::default(),
                transition_log: Vec::new(),
                validation_errors: Vec::new(),
            },
            selected_sm: None,
            editor_text: String::new(),
            is_dirty: false,
            dark_mode,
            config_dir,
            cached_gallery: None,
            save_errors: Vec::new(),
            has_saved_once: false,
        }))
    }
}

pub fn open_sm_editor_viewport(ctx: &egui::Context, state: Arc<Mutex<SmEditorViewport>>) {
    let viewport_id = egui::ViewportId::from_hash_of("sm_editor");
    let viewport_builder = egui::ViewportBuilder::default()
        .with_title("SM Editor")
        .with_inner_size([900.0, 600.0]);

    ctx.show_viewport_deferred(viewport_id, viewport_builder, move |ctx, _vp_class| {
        if ctx.input(|i| i.viewport().close_requested()) {
            if let Ok(mut s) = state.lock() {
                s.from_ui.should_close = true;
            }
            return;
        }

        let Ok(mut vp) = state.lock() else { return };

        crate::tray::ui_theme::apply_theme(ctx, vp.dark_mode);

        // Lazy-load the gallery once; set cached_gallery = None to trigger reload.
        if vp.cached_gallery.is_none() {
            let config_dir = vp.config_dir.clone();
            vp.cached_gallery = Some(SmGallery::load(&config_dir));
        }
        // Collect data from the gallery into owned Vecs so the borrow ends before
        // the egui closures need &mut vp.
        let (valid_entries, draft_entries): (Vec<(String, String)>, Vec<(String, String)>) = {
            let g = vp.cached_gallery.as_ref().unwrap();
            let valid = g.valid_names().into_iter()
                .map(|n| (n.to_string(), g.source(n).unwrap_or("").to_string()))
                .collect();
            let drafts = g.draft_names().into_iter()
                .map(|n| (n.to_string(), g.draft_source(n).unwrap_or("").to_string()))
                .collect();
            (valid, drafts)
        };

        // ── Left browser panel ──────────────────────────────────────────────
        egui::SidePanel::left("sm_browser")
            .resizable(true)
            .min_width(160.0)
            .show(ctx, |ui| {
                ui.heading("State Machines");
                ui.add_space(4.0);

                // Valid SMs
                for (name, src) in &valid_entries {
                    let selected = vp.selected_sm.as_deref() == Some(name.as_str());
                    if ui.selectable_label(selected, name.as_str()).clicked() && !selected {
                        vp.editor_text = src.clone();
                        vp.selected_sm = Some(name.clone());
                        vp.is_dirty = false;
                    }
                }

                ui.separator();

                // Drafts section header
                ui.colored_label(egui::Color32::GRAY, "Drafts");

                for (name, src) in &draft_entries {
                    let selected = vp.selected_sm.as_deref() == Some(name.as_str());
                    let label = egui::RichText::new(name.as_str()).color(egui::Color32::GRAY);
                    if ui.selectable_label(selected, label).clicked() && !selected {
                        vp.editor_text = src.clone();
                        vp.selected_sm = Some(name.clone());
                        vp.is_dirty = false;
                    }
                }

                ui.separator();

                // Action buttons
                if ui.button("📄 New SM").clicked() {
                    vp.editor_text = MINIMAL_TEMPLATE.to_string();
                    vp.selected_sm = None;
                    vp.is_dirty = true;
                }
                if ui.button("📋 Copy Built-in Default").clicked() {
                    vp.editor_text = DEFAULT_SM_TOML.to_string();
                    vp.selected_sm = None;
                    vp.is_dirty = true;
                }
            });

        // ── Bottom error bar ────────────────────────────────────────────────
        egui::TopBottomPanel::bottom("sm_errors")
            .min_height(24.0)
            .show(ctx, |ui| {
                if !vp.has_saved_once {
                    ui.label("Ready.");
                } else if vp.save_errors.is_empty() {
                    ui.colored_label(egui::Color32::DARK_GREEN, "✅ Saved.");
                } else {
                    egui::ScrollArea::vertical().max_height(80.0).show(ui, |ui| {
                        for e in &vp.save_errors {
                            ui.colored_label(egui::Color32::RED, e.to_string());
                        }
                    });
                }
            });

        // ── Central panel: save button + text editor placeholder ────────────
        egui::CentralPanel::default().show(ctx, |ui| {
            let save_label = if vp.is_dirty { "💾 Save*" } else { "💾 Save" };
            if ui.button(save_label).clicked() {
                let editor_text = vp.editor_text.clone();
                // a) Extract SM name from TOML
                let name_result: Result<String, String> =
                    toml::from_str::<crate::sprite::sm_format::SmFile>(&editor_text)
                        .map(|f| f.meta.name)
                        .map_err(|e| e.to_string());

                match name_result {
                    Err(parse_err) => {
                        vp.save_errors = vec![
                            crate::sprite::sm_compiler::CompileError::ConditionParseError(
                                "(parse)".to_string(),
                                parse_err,
                            ),
                        ];
                    }
                    Ok(name) => {
                        let gallery = vp.cached_gallery.as_mut().unwrap();
                        match gallery.save(&name, &editor_text) {
                            Err(io_err) => {
                                vp.save_errors = vec![
                                    crate::sprite::sm_compiler::CompileError::ConditionParseError(
                                        "(io)".to_string(),
                                        io_err.to_string(),
                                    ),
                                ];
                            }
                            Ok(is_valid) => {
                                // c) Update viewport state
                                vp.selected_sm = Some(name.clone());
                                vp.is_dirty = false;
                                // d) Collect errors for bottom panel
                                let save_errors = if !is_valid {
                                    match toml::from_str::<crate::sprite::sm_format::SmFile>(
                                        &editor_text,
                                    ) {
                                        Ok(sm_file) => {
                                            match crate::sprite::sm_compiler::compile(&sm_file) {
                                                Err(errs) => errs,
                                                Ok(_) => vec![],
                                            }
                                        }
                                        Err(e) => vec![
                                            crate::sprite::sm_compiler::CompileError::ConditionParseError(
                                                "(parse)".to_string(),
                                                e.to_string(),
                                            ),
                                        ],
                                    }
                                } else {
                                    vec![]
                                };
                                vp.save_errors = save_errors;
                                // e) Signal app thread to hot-reload if valid
                                if is_valid {
                                    vp.from_ui.saved_sm_name = Some(name);
                                }
                                // f) Reload gallery from disk
                                let config_dir = vp.config_dir.clone();
                                vp.cached_gallery = Some(SmGallery::load(&config_dir));
                            }
                        }
                    }
                }
                vp.has_saved_once = true;
            }
            ui.add_space(4.0);
            // Status bar
            if vp.from_app.validation_errors.is_empty() {
                ui.colored_label(egui::Color32::GREEN, "✅ Valid");
            } else {
                let n = vp.from_app.validation_errors.len();
                ui.colored_label(egui::Color32::RED, format!("❌ {} error(s)", n));
            }
            ui.add_space(4.0);
            // Monospace multiline text editor (20 rows fits a typical 1080p window at initial size)
            let response = egui::TextEdit::multiline(&mut vp.editor_text)
                .font(egui::TextStyle::Monospace)
                .desired_width(f32::INFINITY)
                .desired_rows(20)
                .show(ui);
            if response.response.changed() {
                vp.is_dirty = true;
            }
        });
    });
}
