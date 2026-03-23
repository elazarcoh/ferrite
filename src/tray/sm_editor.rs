use std::sync::{Arc, Mutex};
use crate::sprite::sm_compiler::CompileError;
use crate::sprite::sm_runner::TransitionLogEntry;

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
}

impl SmEditorViewport {
    pub fn new(dark_mode: bool) -> Arc<Mutex<Self>> {
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
        }))
    }
}
