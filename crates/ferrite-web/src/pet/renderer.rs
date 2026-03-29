use wasm_bindgen::JsCast;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, HtmlImageElement};
use ferrite_core::sprite::sm_runner::Facing;

const SCALE: f64 = 2.0;

pub fn tick_and_draw(
    s: &mut super::state::PetWebState,
    canvas: &HtmlCanvasElement,
    img: &HtmlImageElement,
    ts: f64,
) {
    let delta_ms = ((ts - s.last_ts) as u32).min(100);
    s.last_ts = ts;

    let screen_w = canvas.width() as i32;
    let floor_y  = canvas.height() as i32;
    let first = s.sheet.frames.first().cloned();
    let pet_w = first.as_ref().map(|f| (f.w as f64 * SCALE) as i32).unwrap_or(64);
    let pet_h = first.as_ref().map(|f| (f.h as f64 * SCALE) as i32).unwrap_or(64);

    let tag_name = s.runner.tick(delta_ms, &mut s.x, &mut s.y,
                                  screen_w, pet_w, pet_h, floor_y, &s.sheet);
    s.anim.set_tag(tag_name);
    s.anim.tick(&s.sheet, delta_ms);

    let ctx: CanvasRenderingContext2d = canvas
        .get_context("2d").unwrap().unwrap().dyn_into().unwrap();
    ctx.clear_rect(0.0, 0.0, canvas.width() as f64, canvas.height() as f64);

    let abs = s.anim.absolute_frame(&s.sheet);
    let Some(frame) = s.sheet.frames.get(abs) else { return };

    let flip_h = s.sheet.tag(&s.anim.current_tag).map(|t| t.flip_h).unwrap_or(false);
    let should_flip = match s.runner.current_facing() {
        Facing::Left  => !flip_h,
        Facing::Right => flip_h,
    };

    let dst_w = frame.w as f64 * SCALE;
    let dst_h = frame.h as f64 * SCALE;
    let dst_x = s.x as f64;
    let dst_y = s.y as f64;

    let draw = |cx: f64, cy: f64| {
        ctx.draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
            img,
            frame.x as f64, frame.y as f64, frame.w as f64, frame.h as f64,
            cx, cy, dst_w, dst_h,
        ).unwrap();
    };

    if should_flip {
        ctx.save();
        ctx.translate(dst_x + dst_w, dst_y).unwrap();
        ctx.scale(-1.0, 1.0).unwrap();
        draw(0.0, 0.0);
        ctx.restore();
    } else {
        draw(dst_x, dst_y);
    }
}
