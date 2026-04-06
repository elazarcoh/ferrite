use wasm_bindgen::prelude::*;

mod app;
mod config_store;
mod web_storage;
mod simulation;
mod import_export;
mod bridge;

#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    console_log::init_with_level(log::Level::Debug).ok();

    let web_options = eframe::WebOptions::default();
    wasm_bindgen_futures::spawn_local(async {
        eframe::WebRunner::new()
            .start(
                "the_canvas_id",
                web_options,
                Box::new(|_cc| Ok(Box::new(app::WebApp::new()))),
            )
            .await
            .expect("failed to start eframe");
    });
    Ok(())
}
