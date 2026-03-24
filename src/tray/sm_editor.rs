#![allow(dead_code)]
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
    pub pending_delete: Option<String>,
}

const MINIMAL_TEMPLATE: &str = r#"[meta]
name = "My SM"
version = "1.0"
engine_min_version = "1.0"
default_fallback = "idle"

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
            pending_delete: None,
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

    let graph_height = (ui.available_height() - 150.0).max(200.0);
    let (response, painter) = ui.allocate_painter(
        egui::vec2(ui.available_width(), graph_height),
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
                if let Goto::State(to_name) = &t.goto
                    && let Some(&to) = positions.get(to_name.as_str()) {
                        let delta = to - from;
                        if delta.length() > 0.1 {
                            painter.arrow(from, delta * 0.75, egui::Stroke::new(1.0, egui::Color32::from_gray(120)));
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
            if let Some(hover_pos) = response.hover_pos()
                && node_rect.contains(hover_pos) {
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

pub fn render_sm_panel(ctx: &egui::Context, vp: &mut SmEditorViewport) {
    crate::tray::ui_theme::apply_theme(ctx, vp.dark_mode);

    // Lazy-load the gallery once; set cached_gallery = None to trigger reload.
    if vp.cached_gallery.is_none() {
        let config_dir = vp.config_dir.clone();
        vp.cached_gallery = Some(SmGallery::load(&config_dir));
    }
    // Collect data from the gallery into owned Vecs so the borrow ends before
    // the egui closures need &mut vp.
    #[allow(clippy::type_complexity)]
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
                ui.horizontal(|ui| {
                    if ui.selectable_label(selected, name.as_str()).clicked() && !selected {
                        vp.editor_text = src.clone();
                        vp.selected_sm = Some(name.clone());
                        vp.is_dirty = false;
                        vp.save_errors = vec![];
                        vp.has_saved_once = false;
                    }
                    if ui.small_button("🗑").on_hover_text("Delete SM").clicked() {
                        vp.pending_delete = Some(name.clone());
                    }
                });
            }

            ui.separator();

            // Drafts section header
            ui.colored_label(egui::Color32::GRAY, "Drafts");

            for (name, src) in &draft_entries {
                let selected = vp.selected_sm.as_deref() == Some(name.as_str());
                ui.horizontal(|ui| {
                    let label = egui::RichText::new(name.as_str()).color(egui::Color32::GRAY);
                    if ui.selectable_label(selected, label).clicked() && !selected {
                        vp.editor_text = src.clone();
                        vp.selected_sm = Some(name.clone());
                        vp.is_dirty = false;
                        vp.save_errors = vec![];
                        vp.has_saved_once = false;
                    }
                    if ui.small_button("🗑").on_hover_text("Delete draft").clicked() {
                        vp.pending_delete = Some(name.clone());
                    }
                });
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
            if ui.button("📂 Import .petstate").clicked()
                && let Some(path) = rfd::FileDialog::new()
                    .add_filter("Pet State Machine", &["petstate"])
                    .pick_file()
                    && let Ok(source) = std::fs::read_to_string(&path) {
                        vp.editor_text = source;
                        vp.is_dirty = true;
                        vp.selected_sm = None;
                        vp.save_errors = vec![];
                        vp.has_saved_once = false;
                    }
            if ui.button("📦 Import .petbundle").clicked()
                && let Some(path) = rfd::FileDialog::new()
                    .add_filter("Pet Bundle", &["petbundle"])
                    .pick_file()
                {
                    match std::fs::read(&path).map_err(|e| e.to_string()).and_then(|bytes| crate::bundle::import(&bytes)) {
                        Ok(contents) => {
                            if let Some(sm_source) = contents.sm_source {
                                vp.editor_text = sm_source;
                                vp.is_dirty = true;
                                vp.selected_sm = None;
                                vp.save_errors = vec![];
                                vp.has_saved_once = false;
                            }
                        }
                        Err(e) => {
                            vp.save_errors = vec![crate::sprite::sm_compiler::CompileError::ConditionParseError(
                                "(bundle import)".to_string(),
                                e.to_string(),
                            )];
                            vp.has_saved_once = true;
                        }
                    }
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

            egui::ScrollArea::vertical().show(ui, |ui| {
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
                if vp.from_ui.step_mode
                    && ui.button("→ Next transition").clicked() {
                        vp.from_ui.step_advance = true;
                    }

                ui.separator();

                egui::CollapsingHeader::new("🔍 Variables").show(ui, |ui| {
                    let v = &vp.from_app.var_snapshot;
                    egui::Grid::new("vars").striped(true).show(ui, |ui| {
                        ui.label("cursor_dist"); ui.label(format!("{:.0}px", v.cursor_dist)); ui.end_row();
                        ui.label("state_time");  ui.label(format!("{:.1}s", v.state_time_ms as f32 / 1000.0)); ui.end_row();
                        ui.label("on_surface");  ui.label(if v.on_surface { "true" } else { "false" }); ui.end_row();
                        ui.label("near_edge");   ui.label(if v.near_edge { "true" } else { "false" }); ui.end_row();
                        ui.label("pet.x/y");     ui.label(format!("{:.0}, {:.0}", v.pet_x, v.pet_y)); ui.end_row();
                        ui.label("pet.vx/vy/v"); ui.label(format!("{:.0}, {:.0}, {:.0}", v.pet_vx, v.pet_vy, v.pet_v)); ui.end_row();
                        ui.label("time.hour");   ui.label(format!("{}", v.hour)); ui.end_row();
                        ui.label("focused_app"); ui.label(&v.focused_app); ui.end_row();
                    });
                });

                egui::CollapsingHeader::new("📋 Transitions").default_open(true).show(ui, |ui| {
                    for entry in vp.from_app.transition_log.iter().rev() {
                        ui.label(format!("{} → {}  ({})", entry.from_state, entry.to_state, entry.reason));
                    }
                });
            });
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

    // ── Central panel: save button + text editor ────────────────────────
    egui::CentralPanel::default().show(ctx, |ui| {
        // Keyboard shortcut: Ctrl+S to save
        let ctrl_s = ctx.input(|i| i.key_pressed(egui::Key::S) && i.modifiers.command_only());

        let save_label = if vp.is_dirty { "💾 Save*" } else { "💾 Save" };
        if ui.button(save_label).clicked() || ctrl_s {
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
                    // a) Save and collect result/errors while gallery is borrowed
                    let save_result = gallery.save(&name, &editor_text);
                    let save_outcome: Result<(bool, Vec<crate::sprite::sm_compiler::CompileError>), String> =
                        match save_result {
                            Err(io_err) => Err(io_err.to_string()),
                            Ok(is_valid) => {
                                let errs = if !is_valid {
                                    gallery.draft_errors(&name).to_vec()
                                } else {
                                    vec![]
                                };
                                Ok((is_valid, errs))
                            }
                        };
                    // gallery borrow ends here; vp fields are now freely accessible
                    match save_outcome {
                        Err(msg) => {
                            vp.save_errors = vec![
                                crate::sprite::sm_compiler::CompileError::ConditionParseError(
                                    "(io)".to_string(),
                                    msg,
                                ),
                            ];
                        }
                        Ok((is_valid, save_errors)) => {
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

        // Syntax-highlighted TOML editor — scrollable, fills remaining height
        let ppp = ui.ctx().pixels_per_point();
        let theme = if vp.dark_mode {
            egui_extras::syntax_highlighting::CodeTheme::dark(ppp)
        } else {
            egui_extras::syntax_highlighting::CodeTheme::light(ppp)
        };
        let mut layouter = |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap_width: f32| {
            let mut layout_job = egui_extras::syntax_highlighting::highlight(
                ui.ctx(), ui.style(), &theme, buf.as_str(), "toml"
            );
            layout_job.wrap.max_width = wrap_width;
            ui.painter().layout_job(layout_job)
        };

        let scroll_output = egui::ScrollArea::vertical()
            .id_salt("sm_code_editor")
            .show(ui, |ui| {
                egui::TextEdit::multiline(&mut vp.editor_text)
                    .font(egui::TextStyle::Monospace)
                    .desired_width(f32::INFINITY)
                    .desired_rows(50)
                    .layouter(&mut layouter)
                    .show(ui)
            });
        let response = scroll_output.inner;
        if response.response.changed() {
            vp.is_dirty = true;
        }

        // Ctrl+. — toggle line comment (TOML uses `#`)
        let ctrl_period = ctx.input(|i| i.key_pressed(egui::Key::Period) && i.modifiers.command_only());
        if ctrl_period
            && let Some(cursor_range) = response.cursor_range {
                let start_char = cursor_range.primary.index
                    .min(cursor_range.secondary.index);
                let end_char = cursor_range.primary.index
                    .max(cursor_range.secondary.index);

                // Convert char indices to byte offsets
                let text = &vp.editor_text;
                let mut char_iter = text.char_indices();
                let start_byte = char_iter
                    .nth(start_char)
                    .map(|(b, _)| b)
                    .unwrap_or(text.len());
                let end_byte = if end_char == start_char {
                    start_byte
                } else {
                    char_iter
                        .nth(end_char - start_char - 1)
                        .map(|(b, _)| b)
                        .unwrap_or(text.len())
                };

                // Find the line range covering start_byte..end_byte
                let line_start = text[..start_byte].rfind('\n').map(|p| p + 1).unwrap_or(0);
                let line_end = text[end_byte..]
                    .find('\n')
                    .map(|p| end_byte + p)
                    .unwrap_or(text.len());

                let affected = &text[line_start..line_end];

                // If ALL non-empty lines start with `#`, uncomment; otherwise comment
                let all_commented = affected
                    .lines()
                    .filter(|l| !l.trim().is_empty())
                    .all(|l| l.starts_with("# ") || l.starts_with('#'));

                let new_section: String = affected
                    .lines()
                    .map(|line| {
                        if all_commented {
                            if let Some(stripped) = line.strip_prefix("# ") {
                                stripped.to_string()
                            } else if let Some(stripped) = line.strip_prefix('#') {
                                stripped.to_string()
                            } else {
                                line.to_string()
                            }
                        } else {
                            format!("# {}", line)
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n");

                let before = &text[..line_start];
                let after = &text[line_end..];
                vp.editor_text = if after.is_empty() {
                    format!("{}{}", before, new_section)
                } else {
                    format!("{}{}{}", before, new_section, after)
                };
                vp.is_dirty = true;
            }
    });

    // Handle pending deletion — done after panels to avoid borrow conflicts
    if let Some(name) = vp.pending_delete.take()
        && let Some(gallery) = vp.cached_gallery.as_mut() {
            match gallery.delete(&name) {
                Ok(()) => {
                    let config_dir = vp.config_dir.clone();
                    vp.cached_gallery = Some(SmGallery::load(&config_dir));
                    if vp.selected_sm.as_deref() == Some(name.as_str()) {
                        vp.selected_sm = None;
                        vp.editor_text = String::new();
                        vp.is_dirty = false;
                    }
                }
                Err(e) => {
                    vp.save_errors = vec![crate::sprite::sm_compiler::CompileError::ConditionParseError(
                        "(delete)".to_string(),
                        e.to_string(),
                    )];
                    vp.has_saved_once = true;
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

        let Ok(mut guard) = state.lock() else { return };
        render_sm_panel(ctx, &mut guard);
    });
}
