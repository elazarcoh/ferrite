use dioxus::prelude::*;
use wasm_bindgen::JsCast;
use ferrite_core::sprite::{animation::AnimationState, sm_runner::SMRunner, sheet::SpriteSheet};
use crate::pet::{state::PetWebState, loop_};
use std::{cell::RefCell, rc::Rc};

#[component]
pub fn PetCanvas() -> Element {
    use_effect(move || {
        wasm_bindgen_futures::spawn_local(async move {
            // Fetch JSON and SM files
            let json = gloo_net::http::Request::get("/ferrite/esheep.json")
                .send().await.unwrap().binary().await.unwrap();
            let sm_toml = gloo_net::http::Request::get("/ferrite/default.petstate")
                .send().await.unwrap().text().await.unwrap();

            // Build core objects
            let sheet = SpriteSheet::from_json_bytes(&json).expect("parse sheet");
            let sm_file: ferrite_core::sprite::sm_format::SmFile =
                toml::from_str(&sm_toml).expect("parse sm");
            let compiled = ferrite_core::sprite::sm_compiler::compile(&sm_file)
                .expect("compile sm");
            // Place pet at floor level (canvas height=200, scale=2)
            let pet_h = sheet.frames.first()
                .map(|f| (f.h as f64 * 2.0) as i32).unwrap_or(80);
            let floor_y = 200;
            let state = Rc::new(RefCell::new(PetWebState {
                runner: SMRunner::new(compiled, 80.0),
                anim: AnimationState::new("idle"),
                x: 100, y: floor_y - pet_h,
                last_ts: web_sys::window().unwrap().performance().unwrap().now(),
                sheet,
            }));

            let doc = web_sys::window().unwrap().document().unwrap();
            let canvas: web_sys::HtmlCanvasElement = doc.get_element_by_id("pet-canvas")
                .unwrap().dyn_into().unwrap();
            let img = web_sys::HtmlImageElement::new().unwrap();
            img.set_src("/ferrite/esheep.png");
            loop_::start(state, canvas, img);
        });
    });

    rsx! {
        div { class: "relative w-full bg-sky-50 rounded-xl overflow-hidden",
            canvas { id: "pet-canvas", width: "700", height: "200",
                     class: "w-full" }
        }
    }
}
