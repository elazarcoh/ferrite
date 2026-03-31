use dioxus::prelude::*;
use wasm_bindgen::prelude::*;
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

            // Place pet at floor level using actual viewport height
            let win = web_sys::window().unwrap();
            let win_w = win.inner_width().unwrap().as_f64().unwrap() as i32;
            let win_h = win.inner_height().unwrap().as_f64().unwrap() as i32;
            let pet_h = sheet.frames.first()
                .map(|f| (f.h as f64 * 2.0) as i32).unwrap_or(80);

            let state = Rc::new(RefCell::new(PetWebState {
                runner: SMRunner::new(compiled, 80.0),
                anim: AnimationState::new("idle"),
                x: 100, y: win_h - pet_h,
                last_ts: win.performance().unwrap().now(),
                sheet,
                is_dragging: false,
                drag_offset: (0, 0),
                vel_prev: None,
                vel_cur: None,
            }));

            let doc = win.document().unwrap();
            let canvas: web_sys::HtmlCanvasElement = doc.get_element_by_id("pet-canvas")
                .unwrap().dyn_into().unwrap();
            canvas.set_width(win_w as u32);
            canvas.set_height(win_h as u32);

            let img = web_sys::HtmlImageElement::new().unwrap();
            img.set_src("/ferrite/esheep.png");

            setup_drag(&doc, state.clone());
            loop_::start(state, canvas, img);
        });
    });

    rsx! {
        canvas {
            id: "pet-canvas",
            style: "position:fixed;top:0;left:0;width:100%;height:100%;pointer-events:none;z-index:9999;",
        }
    }
}

fn setup_drag(doc: &web_sys::Document, state: Rc<RefCell<PetWebState>>) {
    // pointerdown — hit-test pet bounding box and start drag
    {
        let state = state.clone();
        let cb = Closure::<dyn FnMut(web_sys::PointerEvent)>::new(move |e: web_sys::PointerEvent| {
            let (mx, my) = (e.client_x(), e.client_y());
            let s = state.borrow();
            let pet_w = s.sheet.frames.first().map(|f| (f.w as f64 * 2.0) as i32).unwrap_or(64);
            let pet_h = s.sheet.frames.first().map(|f| (f.h as f64 * 2.0) as i32).unwrap_or(80);
            let hit = mx >= s.x && mx <= s.x + pet_w && my >= s.y && my <= s.y + pet_h;
            if hit {
                drop(s);
                let mut s = state.borrow_mut();
                let offset = (mx - s.x, my - s.y);
                s.runner.grab(offset);
                s.is_dragging = true;
                s.drag_offset = offset;
                let now = web_sys::window().unwrap().performance().unwrap().now();
                s.vel_prev = None;
                s.vel_cur = Some(((mx, my), now));
            }
        });
        doc.add_event_listener_with_callback("pointerdown", cb.as_ref().unchecked_ref()).unwrap();
        cb.forget();
    }

    // pointermove — update position and track velocity while dragging
    {
        let state = state.clone();
        let cb = Closure::<dyn FnMut(web_sys::PointerEvent)>::new(move |e: web_sys::PointerEvent| {
            let mut s = state.borrow_mut();
            if s.is_dragging {
                let (mx, my) = (e.client_x(), e.client_y());
                s.x = mx - s.drag_offset.0;
                s.y = my - s.drag_offset.1;
                let now = web_sys::window().unwrap().performance().unwrap().now();
                s.vel_prev = s.vel_cur.take();
                s.vel_cur = Some(((mx, my), now));
            }
        });
        doc.add_event_listener_with_callback("pointermove", cb.as_ref().unchecked_ref()).unwrap();
        cb.forget();
    }

    // pointerup — compute velocity and release into physics
    {
        let state = state.clone();
        let cb = Closure::<dyn FnMut(web_sys::PointerEvent)>::new(move |_e: web_sys::PointerEvent| {
            let mut s = state.borrow_mut();
            if s.is_dragging {
                let velocity = match (&s.vel_prev, &s.vel_cur) {
                    (Some(((x0, y0), t0)), Some(((x1, y1), t1))) => {
                        let dt = ((t1 - t0) / 1000.0).max(0.001) as f32;
                        ((x1 - x0) as f32 / dt, (y1 - y0) as f32 / dt)
                    }
                    _ => (0.0, 0.0),
                };
                s.runner.release(velocity);
                s.is_dragging = false;
                s.vel_prev = None;
                s.vel_cur = None;
            }
        });
        doc.add_event_listener_with_callback("pointerup", cb.as_ref().unchecked_ref()).unwrap();
        cb.forget();
    }
}
