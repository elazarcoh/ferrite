use std::{cell::RefCell, rc::Rc};
use wasm_bindgen::prelude::*;
use web_sys::{HtmlCanvasElement, HtmlImageElement};

pub fn start(state: Rc<RefCell<super::state::PetWebState>>,
             canvas: HtmlCanvasElement, img: HtmlImageElement) {
    let f: Rc<RefCell<Option<Closure<dyn FnMut(f64)>>>> = Rc::new(RefCell::new(None));
    let g = f.clone();
    *g.borrow_mut() = Some(Closure::wrap(Box::new(move |ts: f64| {
        {
            let mut s = state.borrow_mut();
            super::renderer::tick_and_draw(&mut s, &canvas, &img, ts);
        }
        web_sys::window().unwrap()
            .request_animation_frame(f.borrow().as_ref().unwrap().as_ref().unchecked_ref())
            .unwrap();
    }) as Box<dyn FnMut(f64)>));
    web_sys::window().unwrap()
        .request_animation_frame(g.borrow().as_ref().unwrap().as_ref().unchecked_ref())
        .unwrap();
    std::mem::forget(g);
}
