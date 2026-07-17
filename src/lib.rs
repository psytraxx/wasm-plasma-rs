use std::cell::RefCell;
use std::f64::consts::PI;
use std::rc::Rc;

use wasm_bindgen::prelude::*;
use wasm_bindgen::Clamped;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, ImageData, MouseEvent};

const SCALE: f64 = 1.0;

/// Render resolution factor. The canvas backing store is rendered at this
/// fraction of the CSS display size and stretched up by the browser, which
/// cuts the pixel count (and CPU cost) dramatically. 0.5 = quarter the pixels.
const RENDER_SCALE: f64 = 0.5;

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

    // Size the canvas buffer to a fraction of its CSS display size for speed.
    let rect = canvas.get_bounding_client_rect();
    let init_w = ((rect.width() * RENDER_SCALE).round() as u32).max(1);
    let init_h = ((rect.height() * RENDER_SCALE).round() as u32).max(1);
    canvas.set_width(init_w);
    canvas.set_height(init_h);

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

    let mut pixels: Vec<u8> = vec![0u8; (init_w * init_h * 4) as usize];

    *g.borrow_mut() = Some(Closure::new(move || {
        let now = performance.now();
        let time = now / 1000.0;

        // Re-read canvas size each frame so it adapts to window resizes.
        let w = canvas.width();
        let h = canvas.height();

        // Sync canvas buffer size to a fraction of its CSS size on resize.
        let rect = canvas.get_bounding_client_rect();
        let css_w = ((rect.width() * RENDER_SCALE).round() as u32).max(1);
        let css_h = ((rect.height() * RENDER_SCALE).round() as u32).max(1);
        let (width, height) = if css_w != w || css_h != h {
            canvas.set_width(css_w);
            canvas.set_height(css_h);
            (css_w, css_h)
        } else {
            (w, h)
        };

        let needed = (width * height * 4) as usize;
        if pixels.len() != needed {
            pixels.resize(needed, 0);
        }

        let mouse = *mouse_state.borrow();
        render_plasma(&mut pixels, width, height, time, mouse.x, mouse.y, color_a, color_b);

        let data =
            ImageData::new_with_u8_clamped_array_and_sh(Clamped(&pixels), width, height).unwrap();

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

    // Mouse drives a rotating "flow" direction and a heat source position.
    let rot = mouse_x * PI;
    let flow_cos = rot.cos();
    let flow_sin = rot.sin();
    let heat_x = mouse_x * aspect; // in canvas space
    let heat_y = mouse_y;

    // Slowly drifting hue offset so the palette breathes over time.
    let hue_shift = time * 0.05;

    // Base rotation is constant per frame — hoist it out of the pixel loop.
    let base_rot = time * 0.08;
    let (br_sin, br_cos) = base_rot.sin_cos();

    // Animation phases, computed once per frame.
    let t_warp = time * 0.35;
    let t_flow = time * 0.3;
    let t_ripple = time * 3.0;

    // Precompute the palette base/amplitude (independent of pixel).
    let pal = PaletteParams::new(color_a, color_b);

    let inv_w = 1.0 / width as f64;
    let inv_h = 1.0 / height as f64;

    for y in 0..height {
        let uv_y = y as f64 * inv_h;
        let cy = (uv_y * 2.0 - 1.0) * SCALE;
        for x in 0..width {
            let uv_x = x as f64 * inv_w;
            let cx = (uv_x * 2.0 - 1.0) * aspect * SCALE;

            // Slow rotation of the sampling space for constant motion.
            let px = cx * br_cos - cy * br_sin;
            let py = cx * br_sin + cy * br_cos;

            // --- Domain warping (single pass) -----------------------------
            // One distortion pass keeps the swirling look at a fraction of
            // the original cost.
            let q_x = fbm(px, py, t_warp);
            let q_y = fbm(px + 5.2, py + 1.3, t_warp);

            let wx = px + 1.6 * q_x + 1.7 * flow_cos;
            let wy = py + 1.6 * q_y + 1.7 * flow_sin;

            let mut v = fbm(wx, wy, t_flow);

            // --- Mouse heat source ----------------------------------------
            let dxm = px - heat_x;
            let dym = py - heat_y;
            let dist2 = dxm * dxm + dym * dym;
            let dist = dist2.sqrt();
            let heat = (-dist * 2.0).exp();
            let ripple = (dist * 8.0 - t_ripple).sin() * (-dist * 1.5).exp();
            v += ripple * 0.35 + heat * 0.5;

            // Normalize roughly to [0, 1].
            let t = (v * 0.5 + 0.5).clamp(0.0, 1.0);

            let [pr, pg, pb] = pal.color(t + hue_shift);

            // Brighten highlights near the heat source.
            let boost = 1.0 + heat * 0.8;

            let r = (pr * boost * 255.0).min(255.0);
            let g = (pg * boost * 255.0).min(255.0);
            let b = (pb * boost * 255.0).min(255.0);

            let idx = ((y * width + x) * 4) as usize;
            pixels[idx] = r as u8;
            pixels[idx + 1] = g as u8;
            pixels[idx + 2] = b as u8;
            pixels[idx + 3] = 255;
        }
    }
}

/// A single smooth "value noise"-ish layer built from sines. Cheap and
/// tileable enough for a plasma. `t` animates the pattern.
fn noise_layer(x: f64, y: f64, t: f64) -> f64 {
    (x * 1.7 + t).sin() * (y * 1.3 - t * 0.8).cos()
        + (x * 0.9 - y * 1.1 + t * 0.6).sin()
}

/// Fractal Brownian motion with 2 octaves — enough layered detail for the
/// plasma while keeping the per-pixel cost low.
fn fbm(x: f64, y: f64, t: f64) -> f64 {
    let mut sum = 0.5 * noise_layer(x, y, t);
    sum += 0.25 * noise_layer(x * 2.0, y * 2.0, t);
    sum
}

/// Precomputed cosine-palette parameters so the per-pixel color lookup is
/// just three cosines instead of rebuilding the palette each pixel.
struct PaletteParams {
    base: [f64; 3],
    amp: [f64; 3],
}

impl PaletteParams {
    fn new(color_a: [f64; 3], color_b: [f64; 3]) -> Self {
        let base = [
            (color_a[0] + color_b[0]) * 0.5,
            (color_a[1] + color_b[1]) * 0.5,
            (color_a[2] + color_b[2]) * 0.5,
        ];
        let amp = [
            (color_b[0] - color_a[0]) * 0.5 + 0.25,
            (color_b[1] - color_a[1]) * 0.5 + 0.25,
            (color_b[2] - color_a[2]) * 0.5 + 0.25,
        ];
        Self { base, amp }
    }

    #[inline]
    fn color(&self, t: f64) -> [f64; 3] {
        let tau = 2.0 * PI;
        [
            (self.base[0] + self.amp[0] * (tau * t).cos()).clamp(0.0, 1.0),
            (self.base[1] + self.amp[1] * (tau * (t + 0.33)).cos()).clamp(0.0, 1.0),
            (self.base[2] + self.amp[2] * (tau * (t + 0.67)).cos()).clamp(0.0, 1.0),
        ]
    }
}
