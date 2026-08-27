//! The sky itself: two WebGL2 passes on a fixed full-viewport canvas.
//!
//! Behind the stars, the Milky Way is painted from `static/sky/milkyway.webp` —
//! Gaia star counts binned onto an equal-area grid, not procedural noise. That
//! map is fixed on the celestial sphere; what changes with the observer is
//! which part of it clears the horizon and how much atmosphere it shines
//! through. Both passes blend additively and leave the canvas alpha where the
//! colour is, which is the only reason the dusk gradient underneath composites
//! into one picture rather than being covered by a black rectangle.
//!
//! The catalog is *streamed* and uploaded in batches, so the sky fills in from
//! the brightest stars down while the rest is still arriving, and the page
//! reveals the canvas on the first batch instead of after the last byte.

use std::cell::Cell;

use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    Blob, HtmlCanvasElement, ImageBitmap, Response, WebGl2RenderingContext as Gl, WebGlBuffer,
    WebGlProgram, WebGlTexture, WebGlUniformLocation,
};

use crate::catalog::{HEADER_LEN, STRIDE, header_count};
use crate::program::link;
use crate::shaders::{SKY_FRAG, SKY_VERT, STAR_FRAG, STAR_VERT};
use crate::sky::{FOCAL, view_matrix};
use crate::{dom, telemetry, tuning};

const STAR_BATCH: usize = 512;
const GALAXY_FADE_MS: f64 = 1_600.0;

pub struct Scene {
    canvas: HtmlCanvasElement,
    gl: Gl,
    sky: SkyPass,
    stars: StarPass,
}

struct SkyPass {
    program: WebGlProgram,
    triangle: WebGlBuffer,
    map: WebGlTexture,
    map_ready: Cell<bool>,
    fade_started_at: Cell<Option<f64>>,
    fade_complete: Cell<bool>,
    uniforms: Uniforms,
    tuned: Vec<Option<WebGlUniformLocation>>,
}

struct StarPass {
    program: WebGlProgram,
    buffer: WebGlBuffer,
    uploaded: Cell<i32>,
    uniforms: Uniforms,
    tuned: Vec<Option<WebGlUniformLocation>>,
}

#[derive(Default)]
struct Uniforms {
    view: Option<WebGlUniformLocation>,
    focal: Option<WebGlUniformLocation>,
    aspect: Option<WebGlUniformLocation>,
    light: Option<WebGlUniformLocation>,
    dpr: Option<WebGlUniformLocation>,
    reveal: Option<WebGlUniformLocation>,
    map: Option<WebGlUniformLocation>,
}

impl Scene {
    /// Compile both passes and allocate their buffers. Fails — rather than
    /// silently drawing nothing — on a browser or a context that cannot run
    /// WebGL2, which is what leaves the CSS starfield as the picture.
    pub fn create(canvas: HtmlCanvasElement) -> Result<Self, JsValue> {
        telemetry::event("sky-init");
        let gl: Gl = canvas
            .get_context("webgl2")?
            .ok_or_else(|| JsValue::from_str("webgl2 is unavailable"))?
            .dyn_into()?;
        gl.enable(Gl::BLEND);
        gl.blend_func(Gl::ONE, Gl::ONE);

        let sky_program = link(&gl, SKY_VERT, SKY_FRAG)?;
        let star_program = link(&gl, STAR_VERT, STAR_FRAG)?;

        // One triangle large enough to cover the viewport: cheaper than a quad
        // and free of the diagonal seam two triangles produce.
        let triangle = gl.create_buffer().ok_or("create_buffer")?;
        gl.bind_buffer(Gl::ARRAY_BUFFER, Some(&triangle));
        let corners: [f32; 6] = [-1.0, -1.0, 3.0, -1.0, -1.0, 3.0];
        unsafe {
            gl.buffer_data_with_array_buffer_view(
                Gl::ARRAY_BUFFER,
                &js_sys::Float32Array::view(&corners),
                Gl::STATIC_DRAW,
            );
        }

        let map = gl.create_texture().ok_or("create_texture")?;
        gl.bind_texture(Gl::TEXTURE_2D, Some(&map));
        gl.tex_image_2d_with_i32_and_i32_and_i32_and_format_and_type_and_opt_u8_array(
            Gl::TEXTURE_2D,
            0,
            Gl::RGBA as i32,
            1,
            1,
            0,
            Gl::RGBA,
            Gl::UNSIGNED_BYTE,
            Some(&[0, 0, 0, 0]),
        )?;
        for (name, value) in [
            (Gl::TEXTURE_WRAP_S, Gl::REPEAT),
            (Gl::TEXTURE_WRAP_T, Gl::CLAMP_TO_EDGE),
            (Gl::TEXTURE_MIN_FILTER, Gl::LINEAR),
            (Gl::TEXTURE_MAG_FILTER, Gl::LINEAR),
        ] {
            gl.tex_parameteri(Gl::TEXTURE_2D, name, value as i32);
        }

        Ok(Self {
            sky: SkyPass {
                uniforms: uniforms(&gl, &sky_program),
                tuned: tuned(&gl, &sky_program),
                program: sky_program,
                triangle,
                map,
                map_ready: Cell::new(false),
                fade_started_at: Cell::new(None),
                fade_complete: Cell::new(false),
            },
            stars: StarPass {
                uniforms: uniforms(&gl, &star_program),
                tuned: tuned(&gl, &star_program),
                program: star_program,
                buffer: gl.create_buffer().ok_or("create_buffer")?,
                uploaded: Cell::new(0),
            },
            canvas,
            gl,
        })
    }

    #[must_use]
    pub fn star_count(&self) -> i32 {
        self.stars.uploaded.get()
    }

    /// Stream `url` into the GPU buffer, calling `on_batch` after each upload.
    /// Returns the whole catalog so the annotations can read positions out of
    /// it without a second fetch.
    pub async fn stream_stars(&self, url: &str, on_batch: impl Fn()) -> Result<Vec<u8>, JsValue> {
        let response = dom::fetch(url).await?;
        let body = response
            .body()
            .ok_or_else(|| JsValue::from_str("the star response has no body"))?;
        let reader: web_sys::ReadableStreamDefaultReader = body.get_reader().dyn_into()?;
        telemetry::event("star-stream-started");

        let mut pending = Vec::<u8>::new();
        let mut expected: Option<usize> = None;
        let mut uploaded = 0usize;

        loop {
            let chunk = JsFuture::from(reader.read()).await?;
            let done = js_sys::Reflect::get(&chunk, &"done".into())?
                .as_bool()
                .unwrap_or(true);
            if !done {
                let value = js_sys::Reflect::get(&chunk, &"value".into())?;
                let bytes = js_sys::Uint8Array::new(&value);
                let start = pending.len();
                pending.resize(start + bytes.length() as usize, 0);
                bytes.copy_to(&mut pending[start..]);
                telemetry::increment("stars", "chunks", 1.0);
            }

            if expected.is_none() {
                if pending.len() < HEADER_LEN {
                    if done {
                        return Err(JsValue::from_str("truncated star header"));
                    }
                    continue;
                }
                let count = header_count(&pending).map_err(JsValue::from_str)?;
                expected = Some(count);
                telemetry::number("stars", "expected", count as f64);
                let payload = count
                    .checked_mul(STRIDE)
                    .ok_or_else(|| JsValue::from_str("star asset length overflow"))?;
                self.gl
                    .bind_buffer(Gl::ARRAY_BUFFER, Some(&self.stars.buffer));
                self.gl
                    .buffer_data_with_i32(Gl::ARRAY_BUFFER, payload as i32, Gl::DYNAMIC_DRAW);
            }

            let count = expected.expect("the header has been read");
            let available = pending.len().saturating_sub(HEADER_LEN) / STRIDE;
            let target = if done {
                available.min(count)
            } else {
                available
                    .min(count)
                    .saturating_sub(available.min(count) % STAR_BATCH)
            };
            if target > uploaded {
                self.gl
                    .bind_buffer(Gl::ARRAY_BUFFER, Some(&self.stars.buffer));
                self.gl.buffer_sub_data_with_i32_and_u8_array(
                    Gl::ARRAY_BUFFER,
                    (uploaded * STRIDE) as i32,
                    &pending[HEADER_LEN + uploaded * STRIDE..HEADER_LEN + target * STRIDE],
                );
                if uploaded == 0 {
                    telemetry::mark("stars", "firstBatchAtMs");
                }
                uploaded = target;
                self.stars.uploaded.set(uploaded as i32);
                telemetry::number("stars", "uploaded", uploaded as f64);
                telemetry::increment("stars", "uploadBatches", 1.0);
                on_batch();
            }
            if done {
                if uploaded != count {
                    telemetry::event("star-stream-incomplete");
                    return Err(JsValue::from_str("incomplete star catalog stream"));
                }
                break;
            }
        }

        telemetry::flag("stars", "complete", true);
        telemetry::event("star-stream-complete");
        Ok(pending)
    }

    /// Fetch and upload the Milky Way map, then start its fade.
    pub async fn load_galaxy(&self, url: &str, immediate: bool) -> Result<(), JsValue> {
        let window = dom::window().ok_or_else(|| JsValue::from_str("no window"))?;
        let init = web_sys::RequestInit::new();
        init.set_method("GET");
        // The map is a background of the background: it must never contend
        // with the star catalog for the connection.
        let _ = js_sys::Reflect::set(&init, &"priority".into(), &"low".into());
        let request = web_sys::Request::new_with_str_and_init(url, &init)?;
        let response: Response = JsFuture::from(window.fetch_with_request(&request))
            .await?
            .dyn_into()?;
        if !response.ok() {
            return Err(JsValue::from_str(&format!(
                "{url} responded {}",
                response.status()
            )));
        }
        let blob: Blob = JsFuture::from(response.blob()?).await?.dyn_into()?;
        let bitmap: ImageBitmap = JsFuture::from(window.create_image_bitmap_with_blob(&blob)?)
            .await?
            .dyn_into()?;

        self.gl.bind_texture(Gl::TEXTURE_2D, Some(&self.sky.map));
        self.gl.tex_image_2d_with_u32_and_u32_and_image_bitmap(
            Gl::TEXTURE_2D,
            0,
            Gl::RGBA as i32,
            Gl::RGBA,
            Gl::UNSIGNED_BYTE,
            &bitmap,
        )?;
        self.sky.map_ready.set(true);
        self.sky.fade_started_at.set(Some(if immediate {
            dom::now_ms() - GALAXY_FADE_MS
        } else {
            dom::now_ms()
        }));
        telemetry::number("stars", "galaxyLoadedAtMs", dom::now_ms());
        telemetry::event("galaxy-fade-started");
        Ok(())
    }

    /// Start the galaxy fade now, if the map is loaded and it has not begun.
    /// A tab that was hidden when the map arrived would otherwise come back to
    /// a fade that had already run its clock down behind a blank screen.
    pub fn start_galaxy_fade_if_pending(&self) {
        if self.sky.map_ready.get() && self.sky.fade_started_at.get().is_none() {
            self.sky.fade_started_at.set(Some(dom::now_ms()));
        }
    }

    /// Paint one frame. Returns `false` when the canvas has no area yet — a
    /// layout that has not happened is not an error, it is one frame early.
    pub fn draw(&self, sim_ms: f64, lat_deg: f64, lon_deg: f64) -> bool {
        let Some(window) = dom::window() else {
            return false;
        };
        let dpr = window.device_pixel_ratio().max(1.0);
        let width = (f64::from(self.canvas.client_width()) * dpr) as u32;
        let height = (f64::from(self.canvas.client_height()) * dpr) as u32;
        if width == 0 || height == 0 {
            return false;
        }
        if self.canvas.width() != width || self.canvas.height() != height {
            self.canvas.set_width(width);
            self.canvas.set_height(height);
        }

        let gl = &self.gl;
        gl.viewport(0, 0, width as i32, height as i32);
        gl.clear_color(0.0, 0.0, 0.0, 0.0);
        gl.clear(Gl::COLOR_BUFFER_BIT);

        let view = view_matrix(sim_ms, lat_deg, lon_deg);
        let aspect = width as f32 / height as f32;
        let light = f32::from(u8::from(dom::is_light_theme()));
        let values = tuning::values();

        gl.use_program(Some(&self.sky.program));
        gl.disable_vertex_attrib_array(1);
        gl.disable_vertex_attrib_array(2);
        gl.bind_buffer(Gl::ARRAY_BUFFER, Some(&self.sky.triangle));
        gl.vertex_attrib_pointer_with_i32(0, 2, Gl::FLOAT, false, 0, 0);
        gl.enable_vertex_attrib_array(0);
        gl.uniform_matrix3fv_with_f32_array(self.sky.uniforms.view.as_ref(), false, &view);
        gl.uniform1f(self.sky.uniforms.focal.as_ref(), FOCAL);
        gl.uniform1f(self.sky.uniforms.aspect.as_ref(), aspect);
        gl.uniform1f(self.sky.uniforms.light.as_ref(), light);
        let reveal = self.sky.fade_started_at.get().map_or(0.0, |started| {
            fade_progress(started, dom::now_ms(), GALAXY_FADE_MS)
        });
        gl.uniform1f(self.sky.uniforms.reveal.as_ref(), reveal);
        if reveal >= 1.0 && !self.sky.fade_complete.replace(true) {
            telemetry::number("stars", "galaxyRevealedAtMs", dom::now_ms());
            telemetry::event("galaxy-fade-complete");
        }
        apply(gl, &self.sky.tuned, &values);
        gl.active_texture(Gl::TEXTURE0);
        gl.bind_texture(Gl::TEXTURE_2D, Some(&self.sky.map));
        gl.uniform1i(self.sky.uniforms.map.as_ref(), 0);
        gl.draw_arrays(Gl::TRIANGLES, 0, 3);

        let stars = self.stars.uploaded.get();
        if stars > 0 {
            let stride = STRIDE as i32;
            gl.use_program(Some(&self.stars.program));
            gl.bind_buffer(Gl::ARRAY_BUFFER, Some(&self.stars.buffer));
            for (index, size, kind, normalized, offset) in [
                (0, 3, Gl::FLOAT, false, 0),
                (1, 1, Gl::FLOAT, false, 12),
                (2, 4, Gl::UNSIGNED_BYTE, true, 16),
            ] {
                gl.vertex_attrib_pointer_with_i32(index, size, kind, normalized, stride, offset);
                gl.enable_vertex_attrib_array(index);
            }
            gl.uniform_matrix3fv_with_f32_array(self.stars.uniforms.view.as_ref(), false, &view);
            gl.uniform1f(self.stars.uniforms.focal.as_ref(), FOCAL);
            gl.uniform1f(self.stars.uniforms.aspect.as_ref(), aspect);
            gl.uniform1f(self.stars.uniforms.dpr.as_ref(), dpr as f32);
            gl.uniform1f(self.stars.uniforms.light.as_ref(), light);
            apply(gl, &self.stars.tuned, &values);
            gl.draw_arrays(Gl::POINTS, 0, stars);
        }
        telemetry::increment("sky", "draws", 1.0);
        true
    }
}

/// How far through the galaxy fade `now_ms` is, clamped both ends.
#[must_use]
pub fn fade_progress(started_at_ms: f64, now_ms: f64, duration_ms: f64) -> f32 {
    ((now_ms - started_at_ms) / duration_ms).clamp(0.0, 1.0) as f32
}

fn uniforms(gl: &Gl, program: &WebGlProgram) -> Uniforms {
    let at = |name: &str| gl.get_uniform_location(program, name);
    Uniforms {
        view: at("u_view"),
        focal: at("u_f"),
        aspect: at("u_aspect"),
        light: at("u_light"),
        dpr: at("u_dpr"),
        reveal: at("u_reveal"),
        map: at("u_map"),
    }
}

fn tuned(gl: &Gl, program: &WebGlProgram) -> Vec<Option<WebGlUniformLocation>> {
    tuning::SPEC
        .iter()
        .map(|(field, _)| gl.get_uniform_location(program, &format!("u_{field}")))
        .collect()
}

fn apply(gl: &Gl, locations: &[Option<WebGlUniformLocation>], values: &tuning::Values) {
    for (location, value) in locations.iter().zip(values) {
        gl.uniform1f(location.as_ref(), *value);
    }
}

#[cfg(test)]
mod tests {
    use super::fade_progress;

    #[test]
    fn the_galaxy_fade_is_clamped_at_both_ends() {
        assert_eq!(fade_progress(1_000.0, 500.0, 1_600.0), 0.0);
        assert_eq!(fade_progress(1_000.0, 1_800.0, 1_600.0), 0.5);
        assert_eq!(fade_progress(1_000.0, 3_000.0, 1_600.0), 1.0);
    }
}
