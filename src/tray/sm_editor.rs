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

fn draw_state_graph(
    ui: &mut egui::Ui,
    sm: &crate::sprite::sm_compiler::CompiledSM,
    active_state: Option<&str>,
    force_out: &mut Option<String>,
) {
    use crate::sprite::sm_compiler::{StateKind, Goto};
    use std::collections::HashMap;

    let (response, painter) = ui.allocate_painter(
        ui.available_size(),
        egui::Sense::click_and_drag(),
    );
    let rect = response.rect;

    // Sort state names for deterministic layout
    let mut state_names: Vec<&str> = sm.states.keys().map(String::as_str).collect();
    state_names.sort_unstable();

    let node_w = 100.0f32;
    let node_h = 30.0f32;
    let gap_x = 20.0f32;
    let gap_y = 20.0f32;
    let cols = ((state_names.len() as f32).sqrt().ceil() as usize).max(1);

    // Compute center positions
    let positions: HashMap<&str, egui::Pos2> = state_names.iter().enumerate().map(|(i, &name)| {
        let col = (i % cols) as f32;
        let row = (i / cols) as f32;
        let x = rect.left() + col * (node_w + gap_x) + node_w / 2.0 + 8.0;
        let y = rect.top() + row * (node_h + gap_y) + node_h / 2.0 + 8.0;
        (name, egui::pos2(x, y))
    }).collect();

    // Draw arrows (behind nodes)
    for (state_name, state) in &sm.states {
        let transitions = match &state.kind {
            StateKind::Atomic { transitions, .. } => transitions.as_slice(),
            StateKind::Composite { transitions, .. } => transitions.as_slice(),
        };
        if let Some(&from) = positions.get(state_name.as_str()) {
            for t in transitions {
                if let Goto::State(to_name) = &t.goto {
                    if let Some(&to) = positions.get(to_name.as_str()) {
                        let delta = to - from;
                        if delta.length() > 0.1 {
                            painter.arrow(from, delta * 0.75, egui::Stroke::new(1.0, egui::Color32::from_gray(120)));
                        }
                    }
                }
            }
        }
    }

    // Draw nodes (and handle hover → Force button)
    for &name in &state_names {
        if let Some(&center) = positions.get(name) {
            let state = &sm.states[name];
            let node_rect = egui::Rect::from_center_size(center, egui::vec2(node_w, node_h));
            let is_active = active_state == Some(name);
            let is_composite = matches!(state.kind, StateKind::Composite { .. });

            let bg = if is_active {
                egui::Color32::from_rgb(50, 100, 200)
            } else if state.required {
                egui::Color32::from_gray(70)
            } else {
                egui::Color32::from_gray(50)
            };

            // Rounding: composite states get more rounding
            let rounding = if is_composite { 10.0 } else { 4.0 };
            painter.rect_filled(node_rect, rounding, bg);

            if is_active {
                painter.rect_stroke(node_rect, rounding, egui::Stroke::new(2.0, egui::Color32::from_rgb(120, 180, 255)), egui::StrokeKind::Outside);
            }

            painter.text(
                center,
                egui::Align2::CENTER_CENTER,
                name,
                egui::FontId::monospace(11.0),
                egui::Color32::WHITE,
            );

            // Hover: show Force button indicator and detect click
            if let Some(hover_pos) = response.hover_pos() {
                if node_rect.contains(hover_pos) {
                    // Show a small Force label at top-right of node
                    let force_pos = node_rect.right_top() + egui::vec2(2.0, -2.0);
                    painter.text(
                        force_pos,
                        egui::Align2::LEFT_TOP,
                        "▶",
                        egui::FontId::monospace(10.0),
                        egui::Color32::YELLOW,
                    );
                    // Check click
                    if response.clicked() {
                        *force_out = Some(name.to_string());
                    }
                }
            }
        }
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
                        vp.save_errors = vec![];
                        vp.has_saved_once = false;
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
                        vp.save_errors = vec![];
                        vp.has_saved_once = false;
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

        // ── Right: state graph ──────────────────────────────────────────────
        egui::SidePanel::right("sm_graph")
            .resizable(true)
            .min_width(240.0)
            .show(ctx, |ui| {
                ui.heading("State Graph");
                ui.add_space(4.0);

                // Force state banner
                if vp.from_app.is_forced {
                    let amber = egui::Color32::from_rgb(255, 180, 0);
                    let state_name = vp.from_app.active_state.clone().unwrap_or_else(|| "?".to_string());
                    ui.horizontal(|ui| {
                        ui.colored_label(amber, format!("⏸ FORCED: {}", state_name));
                        if ui.button("▶ Release").clicked() {
                            vp.from_ui.release_force = true;
                        }
                    });
                    ui.add_space(4.0);
                }

                let compiled_sm = vp.cached_gallery.as_ref()
                    .and_then(|g| vp.selected_sm.as_deref().and_then(|name| g.get(name)));

                if let Some(sm) = compiled_sm {
                    let active = vp.from_app.active_state.as_deref();
                    let mut force_out: Option<String> = None;
                    draw_state_graph(ui, &sm, active, &mut force_out);
                    if let Some(state_name) = force_out {
                        vp.from_ui.force_state = Some(state_name);
                    }
                } else {
                    ui.label("No valid SM selected.");
                }

                ui.separator();

                // Step mode controls
                ui.checkbox(&mut vp.from_ui.step_mode, "Step mode")
                    .on_hover_text("In step mode, transitions are paused until advanced. Interrupts (grab, pet) still fire.");
                if vp.from_ui.step_mode {
                    if ui.button("→ Next transition").clicked() {
                        vp.from_ui.step_advance = true;
                    }
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
                        let Some(gallery) = vp.cached_gallery.as_mut() else {
                            return; // should not happen — lazy-load runs before panels
                        };
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
                                // d) Collect errors for bottom panel (read from gallery entry)
                                // Must be done while gallery borrow is active, before other vp mutations.
                                let save_errors: Vec<crate::sprite::sm_compiler::CompileError> = if !is_valid {
                                    // Read errors from the gallery entry that was just written
                                    gallery.draft_errors(&name).to_vec()
                                } else {
                                    vec![]
                                };
                                // Drop gallery borrow before mutating other vp fields
                                drop(gallery);
                                // c) Update viewport state
                                vp.selected_sm = Some(name.clone());
                                vp.is_dirty = false;
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
            // Status bar (reflects last save attempt)
            if !vp.has_saved_once {
                ui.colored_label(egui::Color32::GRAY, "Not saved yet");
            } else if vp.save_errors.is_empty() {
                ui.colored_label(egui::Color32::GREEN, "✅ Valid");
            } else {
                let n = vp.save_errors.len();
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
