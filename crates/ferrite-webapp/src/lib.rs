use wasm_bindgen::prelude::*;
#[allow(unused_imports)]
use wasm_bindgen::JsCast as _;

mod app;
mod config_store;
mod web_storage;
mod simulation;
mod import_export;
mod bridge;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    console_log::init_with_level(log::Level::Debug).ok();

    let web_options = eframe::WebOptions::default();
    crate::bridge::attach_to_window();
    wasm_bindgen_futures::spawn_local(async {
        let document = web_sys::window()
            .expect("no window")
            .document()
            .expect("no document");
        let canvas = document
            .get_element_by_id("the_canvas_id")
            .expect("canvas element 'the_canvas_id' not found")
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .expect("element is not a canvas");
        eframe::WebRunner::new()
            .start(
                canvas,
                web_options,
                Box::new(|_cc| Ok(Box::new(app::WebApp::new()))),
            )
            .await
            .expect("failed to start eframe");
    });
    Ok(())
}
