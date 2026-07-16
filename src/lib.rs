use std::cell::RefCell;
use std::f64::consts::PI;
use std::rc::Rc;

use wasm_bindgen::prelude::*;
use wasm_bindgen::Clamped;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, ImageData, MouseEvent};

const WIDTH: u32 = 512;
const HEIGHT: u32 = 512;
const SCALE: f64 = 1.0;

#[derive(Clone, Copy)]
struct MouseState {
    x: f64,
    y: f64,
}

/// Parse a "#rrggbb" hex string into (r, g, b) floats in [0.0, 1.0].
/// Falls back to the provided default on any parse error.
fn parse_hex_color(hex: &str, default: [f64; 3]) -> [f64; 3] {
    let hex = hex.trim().trim_start_matches('#');
    if hex.len() != 6 {
        return default;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0) as f64 / 255.0;
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0) as f64 / 255.0;
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0) as f64 / 255.0;
    [r, g, b]
}

#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
    let window = web_sys::window().unwrap();
    let document = window.document().unwrap();
    let canvas = document
        .get_element_by_id("canvas")
        .unwrap()
        .dyn_into::<HtmlCanvasElement>()?;

    // Read palette from data attributes; fall back to orange/blue defaults.
    let color_a = parse_hex_color(
        &canvas.get_attribute("data-color-a").unwrap_or_default(),
        [1.0, 0.4, 0.0],
    );
    let color_b = parse_hex_color(
        &canvas.get_attribute("data-color-b").unwrap_or_default(),
        [0.0, 0.2, 1.0],
    );

    canvas.set_width(WIDTH);
    canvas.set_height(HEIGHT);

    let ctx = canvas
        .get_context("2d")?
        .unwrap()
        .dyn_into::<CanvasRenderingContext2d>()?;

    let performance = window.performance().unwrap();

    let mouse_state = Rc::new(RefCell::new(MouseState { x: 0.0, y: 0.0 }));

    let mouse_state_move = mouse_state.clone();
    let canvas_move = canvas.clone();
    let on_mouse_move = Closure::<dyn FnMut(MouseEvent)>::new(move |event: MouseEvent| {
        let rect = canvas_move.get_bounding_client_rect();
        let x = (event.client_x() as f64 - rect.left()) / rect.width();
        let y = (event.client_y() as f64 - rect.top()) / rect.height();
        let nx = (x * 2.0 - 1.0).clamp(-1.0, 1.0);
        let ny = (y * 2.0 - 1.0).clamp(-1.0, 1.0);
        *mouse_state_move.borrow_mut() = MouseState { x: nx, y: ny };
    });
    canvas.add_event_listener_with_callback("mousemove", on_mouse_move.as_ref().unchecked_ref())?;
    on_mouse_move.forget();

    let f: Rc<RefCell<Option<Closure<dyn FnMut()>>>> = Rc::new(RefCell::new(None));
    let g = f.clone();

    let mut pixels = vec![0u8; (WIDTH * HEIGHT * 4) as usize];

    *g.borrow_mut() = Some(Closure::new(move || {
        let now = performance.now();
        let time = now / 1000.0;

        let mouse = *mouse_state.borrow();
        render_plasma(&mut pixels, WIDTH, HEIGHT, time, mouse.x, mouse.y, color_a, color_b);

        let data =
            ImageData::new_with_u8_clamped_array_and_sh(Clamped(&pixels), WIDTH, HEIGHT).unwrap();

        ctx.put_image_data(&data, 0.0, 0.0).unwrap();

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

fn render_plasma(
    pixels: &mut [u8],
    width: u32,
    height: u32,
    time: f64,
    mouse_x: f64,
    mouse_y: f64,
    color_a: [f64; 3],
    color_b: [f64; 3],
) {
    let aspect = width as f64 / height as f64;
    let rot_x = mouse_y * PI * 0.5;
    let rot_y = mouse_x * PI;
    let depth = 1.5 + mouse_y * 0.75;

    let sin_rx = rot_x.sin();
    let cos_rx = rot_x.cos();
    let sin_ry = rot_y.sin();
    let cos_ry = rot_y.cos();

    for y in 0..height {
        for x in 0..width {
            let uv_x = x as f64 / width as f64;
            let uv_y = y as f64 / height as f64;

            let px = (uv_x * 2.0 - 1.0) * aspect * SCALE;
            let py = (uv_y * 2.0 - 1.0) * SCALE;
            let pz = (time * 0.3).sin() * depth;

            let x1 = px;
            let y1 = py * cos_rx - pz * sin_rx;
            let z1 = py * sin_rx + pz * cos_rx;

            let x2 = x1 * cos_ry + z1 * sin_ry;
            let y2 = y1;
            let z2 = -x1 * sin_ry + z1 * cos_ry;

            let d = (x2 * x2 + y2 * y2 + z2 * z2).sqrt();
            let v = (x2 * 2.3 + time * 0.7).sin()
                + (y2 * 2.1 + time * 0.6).sin()
                + (z2 * 2.0 + time * 0.5).sin()
                + (d * 3.2 - time * 0.4).sin();

            // Map plasma value to [0, 1] blend factor between color_a and color_b.
            let t = (0.5 + 0.5 * (v * 0.6).cos()).clamp(0.0, 1.0);

            let r = (color_a[0] * (1.0 - t) + color_b[0] * t) * 255.0;
            let g = (color_a[1] * (1.0 - t) + color_b[1] * t) * 255.0;
            let b = (color_a[2] * (1.0 - t) + color_b[2] * t) * 255.0;

            let idx = ((y * width + x) * 4) as usize;
            pixels[idx] = r as u8;
            pixels[idx + 1] = g as u8;
            pixels[idx + 2] = b as u8;
            pixels[idx + 3] = 255;
        }
    }
}
