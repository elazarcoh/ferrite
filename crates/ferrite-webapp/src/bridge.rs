use wasm_bindgen::prelude::*;
use serde::Serialize;
use std::sync::Mutex;

#[derive(Serialize, Clone)]
pub struct PetStateSnapshot {
    pub id: String,
    pub x: f32,
    pub y: f32,
    pub sm_state: String,
    pub animation_tag: String,
}

#[derive(Serialize)]
pub struct AppStateSnapshot {
    pub pets: Vec<PetStateSnapshot>,
    pub dark_mode: bool,
}

static APP_HANDLE: std::sync::OnceLock<Mutex<BridgeState>> = std::sync::OnceLock::new();

pub struct BridgeState {
    pub pets: Vec<PetStateSnapshot>,
    pub dark_mode: bool,
}

pub fn init_bridge_state() {
    APP_HANDLE.get_or_init(|| Mutex::new(BridgeState { pets: Vec::new(), dark_mode: true }));
}

pub fn update_bridge_state(pets: Vec<PetStateSnapshot>, dark_mode: bool) {
    if let Some(lock) = APP_HANDLE.get() {
        if let Ok(mut state) = lock.lock() {
            state.pets = pets;
            state.dark_mode = dark_mode;
        }
    }
}

#[wasm_bindgen]
pub struct FerriteBridge;

#[wasm_bindgen]
impl FerriteBridge {
    pub fn get_pet_state(&self, id: &str) -> JsValue {
        let Some(lock) = APP_HANDLE.get() else { return JsValue::NULL };
        let state = lock.lock().unwrap();
        match state.pets.iter().find(|p| p.id == id) {
            Some(p) => serde_wasm_bindgen::to_value(p).unwrap_or(JsValue::NULL),
            None => JsValue::NULL,
        }
    }

    pub fn get_state(&self) -> JsValue {
        let Some(lock) = APP_HANDLE.get() else { return JsValue::NULL };
        let state = lock.lock().unwrap();
        let snap = AppStateSnapshot { pets: state.pets.clone(), dark_mode: state.dark_mode };
        serde_wasm_bindgen::to_value(&snap).unwrap_or(JsValue::NULL)
    }

    pub fn inject_event(&self, event_json: &str) {
        log::debug!("inject_event: {event_json}");
        // Future: parse JSON and route to appropriate pet's SMRunner
    }
}

pub fn attach_to_window() {
    let bridge = FerriteBridge;
    let js_val = wasm_bindgen::JsValue::from(bridge);
    let window = web_sys::window().expect("no window");
    js_sys::Reflect::set(&window, &JsValue::from_str("__ferrite"), &js_val)
        .expect("failed to set window.__ferrite");
}
