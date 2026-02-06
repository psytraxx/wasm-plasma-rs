use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen::prelude::*;
use wasm_bindgen::Clamped;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, ImageData};

const WIDTH: u32 = 512;
const HEIGHT: u32 = 512;
const SCALE: f64 = 1.0;

#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
    let window = web_sys::window().unwrap();
    let document = window.document().unwrap();
    let canvas = document
        .get_element_by_id("canvas")
        .unwrap()
        .dyn_into::<HtmlCanvasElement>()?;

    canvas.set_width(WIDTH);
    canvas.set_height(HEIGHT);

    let ctx = canvas
        .get_context("2d")?
        .unwrap()
        .dyn_into::<CanvasRenderingContext2d>()?;

    let performance = window.performance().unwrap();

    let f: Rc<RefCell<Option<Closure<dyn FnMut()>>>> = Rc::new(RefCell::new(None));
    let g = f.clone();

    let mut pixels = vec![0u8; (WIDTH * HEIGHT * 4) as usize];
    let mut last_time = performance.now();
    let mut fps = 0.0_f64;

    *g.borrow_mut() = Some(Closure::new(move || {
        let now = performance.now();
        let dt = now - last_time;
        last_time = now;
        if dt > 0.0 {
            fps = fps * 0.9 + (1000.0 / dt) * 0.1;
        }

        let time = now / 1000.0;

        render_plasma(&mut pixels, WIDTH, HEIGHT, time);

        let data =
            ImageData::new_with_u8_clamped_array_and_sh(Clamped(&pixels), WIDTH, HEIGHT).unwrap();

        ctx.put_image_data(&data, 0.0, 0.0).unwrap();

        ctx.set_font("16px monospace");
        ctx.set_fill_style_str("white");
        ctx.set_shadow_color("black");
        ctx.set_shadow_blur(4.0);
        let _ = ctx.fill_text(&format!("{:.0} FPS", fps), 8.0, 22.0);
        ctx.set_shadow_blur(0.0);

        request_animation_frame(f.borrow().as_ref().unwrap());
    }));

    request_animation_frame(g.borrow().as_ref().unwrap());

    Ok(())
}

fn request_animation_frame(f: &Closure<dyn FnMut()>) {
    web_sys::window()
        .unwrap()
        .request_animation_frame(f.as_ref().unchecked_ref())
        .unwrap();
}

/// Port of the GLSL plasma shader from 4rknova.com/blog/2016/11/01/plasma
///
/// Original GLSL:
///   vec2 c = SCALE * pc.xy / iResolution.xy * a * 4. + time * .3;
///   float k = .1 + cos(c.y + sin(.148 - time)) + 2.4 * time;
///   float w = .9 + sin(c.x + cos(.628 + time)) - 0.7 * time;
///   float d = length(c);
///   float s = 7. * cos(d+w) * sin(k+w);
///   col = vec4(.5 + .5 * cos(s + vec3(.2, .5, .9)), 1);
fn render_plasma(pixels: &mut [u8], width: u32, height: u32, time: f64) {
    let aspect = width as f64 / height as f64;

    for y in 0..height {
        for x in 0..width {
            let uv_x = x as f64 / width as f64;
            let uv_y = y as f64 / height as f64;

            let cx = SCALE * uv_x * aspect * 4.0 + time * 0.3;
            let cy = SCALE * uv_y * 4.0 + time * 0.3;

            let k = 0.1 + (cy + (0.148 - time).sin()).cos() + 2.4 * time;
            let w = 0.9 + (cx + (0.628 + time).cos()).sin() - 0.7 * time;
            let d = (cx * cx + cy * cy).sqrt();
            let s = 7.0 * (d + w).cos() * (k + w).sin();

            let r = (0.5 + 0.5 * (s + 0.8_f64).cos()) * 255.0;
            let g = (0.5 + 0.5 * (s + 0.1_f64).cos()) * 255.0;
            let b = (0.5 + 0.5 * (s + 0.2_f64).cos()) * 255.0;

            let idx = ((y * width + x) * 4) as usize;
            pixels[idx] = r as u8;
            pixels[idx + 1] = g as u8;
            pixels[idx + 2] = b as u8;
            pixels[idx + 3] = 255;
        }
    }
}
