//! Browser-only WebGL2 renderer for the starscape island.
//!
//! Deferred start (rAF + idle) keeps the critical path on HTML/CSS. The star
//! catalog streams in and uploads to the GPU in batches so the sky fills
//! progressively. The Milky Way map fetches at low priority after stars begin.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use leptos::prelude::{document, request_animation_frame, window};
use leptos::task::spawn_local;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    Blob, Event, HtmlCanvasElement, ImageBitmap, MediaQueryList, Response,
    WebGl2RenderingContext as Gl, WebGlBuffer, WebGlProgram, WebGlShader, WebGlTexture,
    WebGlUniformLocation,
};

use super::shaders::{SKY_FRAG, SKY_VERT, STAR_FRAG, STAR_VERT};
use super::star_asset::{HEADER_LEN, STRIDE, validate_header};
use super::telemetry;
use super::tuning;
use super::{sim_time_ms, synced_sim_time_ms, view_matrix};

const GL_STRIDE: i32 = STRIDE as i32;
const STAR_BATCH: usize = 512;
const GALAXY_FADE_MS: f64 = 1_600.0;

struct SkyPass {
    program: WebGlProgram,
    triangle: WebGlBuffer,
    map: WebGlTexture,
    u_view: Option<WebGlUniformLocation>,
    u_f: Option<WebGlUniformLocation>,
    u_aspect: Option<WebGlUniformLocation>,
    u_light: Option<WebGlUniformLocation>,
    u_reveal: Option<WebGlUniformLocation>,
    u_map: Option<WebGlUniformLocation>,
    map_ready: Cell<bool>,
    fade_started_at: Cell<Option<f64>>,
    fade_complete: Cell<bool>,
    tuned: Vec<Option<WebGlUniformLocation>>,
}

struct StarPass {
    program: WebGlProgram,
    stars: WebGlBuffer,
    count: Cell<i32>,
    u_view: Option<WebGlUniformLocation>,
    u_f: Option<WebGlUniformLocation>,
    u_aspect: Option<WebGlUniformLocation>,
    u_dpr: Option<WebGlUniformLocation>,
    u_light: Option<WebGlUniformLocation>,
    tuned: Vec<Option<WebGlUniformLocation>>,
}

struct Scene {
    canvas: HtmlCanvasElement,
    gl: Gl,
    sky: SkyPass,
    stars: StarPass,
}

struct Controller {
    canvas: HtmlCanvasElement,
    scene: RefCell<Option<Rc<Scene>>>,
    running: Cell<bool>,
    animation_generation: Cell<u32>,
    generation: Cell<u32>,
    reduced: Cell<bool>,
    visible: Cell<bool>,
    frozen_at: Cell<Option<f64>>,
    server_epoch_ms: f64,
    client_mount_ms: f64,
}

pub fn start_deferred(canvas: HtmlCanvasElement, server_epoch_ms: f64) {
    let client_mount_ms = js_sys::Date::now();
    let media = window()
        .match_media("(prefers-reduced-motion: reduce)")
        .ok()
        .flatten();
    let reduced = media.as_ref().map(MediaQueryList::matches).unwrap_or(false);
    let controller = Rc::new(Controller {
        canvas,
        scene: RefCell::new(None),
        running: Cell::new(false),
        animation_generation: Cell::new(0),
        generation: Cell::new(0),
        reduced: Cell::new(reduced),
        visible: Cell::new(document().visibility_state() == web_sys::VisibilityState::Visible),
        frozen_at: Cell::new(reduced.then(|| sim_time_ms(server_epoch_ms))),
        server_epoch_ms,
        client_mount_ms,
    });
    install_listeners(&controller, media);
    defer_initialize(controller);
}

fn defer_initialize(controller: Rc<Controller>) {
    let _ = request_animation_frame(move || {
        let idle_controller = Rc::clone(&controller);
        let run = Closure::<dyn FnMut()>::once(move || {
            initialize(idle_controller);
        });
        if js_sys::Reflect::has(&js_sys::global(), &JsValue::from_str("requestIdleCallback"))
            .unwrap_or(false)
        {
            let opts = js_sys::Object::new();
            let _ = js_sys::Reflect::set(
                &opts,
                &JsValue::from_str("timeout"),
                &JsValue::from_f64(2000.0),
            );
            let idle: js_sys::Function =
                js_sys::Reflect::get(&js_sys::global(), &JsValue::from_str("requestIdleCallback"))
                    .expect("requestIdleCallback")
                    .unchecked_into();
            let _ = idle.call2(&js_sys::global(), run.as_ref(), &opts);
        } else {
            let _ = window().set_timeout_with_callback_and_timeout_and_arguments_0(
                run.as_ref().unchecked_ref(),
                0,
            );
        }
        run.forget();
    });
}

fn initialize(controller: Rc<Controller>) {
    let canvas = controller.canvas.clone();
    let generation = controller.generation.get();
    spawn_local(async move {
        match init(canvas, Rc::clone(&controller)).await {
            Ok(()) if controller.generation.get() == generation => {}
            Ok(()) => {}
            Err(error) if controller.generation.get() == generation => {
                remove_active_class();
                web_sys::console::warn_2(&"starscape unavailable:".into(), &error);
            }
            Err(_) => {}
        }
    });
}

async fn stream_stars(
    gl: &Gl,
    buffer: &WebGlBuffer,
    count_cell: &Cell<i32>,
    on_batch: impl Fn(),
) -> Result<i32, JsValue> {
    let response: Response = JsFuture::from(window().fetch_with_str("/stars/bright.bin"))
        .await?
        .dyn_into()?;
    if !response.ok() {
        return Err(format!("stars HTTP {}", response.status()).into());
    }
    let body = response
        .body()
        .ok_or_else(|| JsValue::from_str("star response has no body"))?;
    let reader: web_sys::ReadableStreamDefaultReader = body.get_reader().dyn_into()?;

    let mut pending = Vec::<u8>::new();
    let mut total_count: Option<usize> = None;
    let mut uploaded = 0usize;
    telemetry::event("star-stream-started");

    loop {
        let result = JsFuture::from(reader.read()).await?;
        let done = js_sys::Reflect::get(&result, &JsValue::from_str("done"))?
            .as_bool()
            .unwrap_or(true);
        if !done {
            let value = js_sys::Reflect::get(&result, &JsValue::from_str("value"))?;
            let chunk = js_sys::Uint8Array::new(&value);
            let mut bytes = vec![0u8; chunk.length() as usize];
            chunk.copy_to(&mut bytes);
            pending.extend_from_slice(&bytes);
            telemetry::increment("stars", "chunks", 1.0);
            telemetry::increment("stars", "bytesReceived", bytes.len() as f64);
            telemetry::set_number_in("stars", "pendingBytes", pending.len() as f64);
        }

        if total_count.is_none() {
            if pending.len() < HEADER_LEN {
                if done {
                    return Err("truncated star header".into());
                }
                continue;
            }
            let (count, _) = validate_header(&pending).map_err(JsValue::from_str)?;
            total_count = Some(count);
            telemetry::set_number_in("stars", "expected", count as f64);
            let payload = count
                .checked_mul(STRIDE)
                .ok_or_else(|| JsValue::from_str("star length overflow"))?;
            telemetry::set_number_in("stars", "gpuBufferBytes", payload as f64);
            gl.bind_buffer(Gl::ARRAY_BUFFER, Some(buffer));
            gl.buffer_data_with_i32(Gl::ARRAY_BUFFER, payload as i32, Gl::DYNAMIC_DRAW);
        }

        let count = total_count.unwrap();
        let available = pending.len().saturating_sub(HEADER_LEN) / STRIDE;
        let target = if done {
            available.min(count)
        } else {
            (uploaded + STAR_BATCH).min(available).min(count)
        };
        // Upload every complete batch (or everything remaining when done).
        while uploaded < target
            && (done || uploaded + STAR_BATCH <= available.min(count) || target > uploaded)
        {
            let end = if done {
                available.min(count)
            } else {
                (uploaded + STAR_BATCH).min(available).min(count)
            };
            if end <= uploaded {
                break;
            }
            let start_byte = HEADER_LEN + uploaded * STRIDE;
            let end_byte = HEADER_LEN + end * STRIDE;
            let slice = &pending[start_byte..end_byte];
            gl.bind_buffer(Gl::ARRAY_BUFFER, Some(buffer));
            gl.buffer_sub_data_with_i32_and_u8_array(
                Gl::ARRAY_BUFFER,
                (uploaded * STRIDE) as i32,
                slice,
            );
            if uploaded == 0 {
                telemetry::set_number_in("stars", "firstBatchAtMs", js_sys::Date::now());
            }
            uploaded = end;
            count_cell.set(uploaded as i32);
            telemetry::increment("stars", "uploadBatches", 1.0);
            telemetry::set_number_in("stars", "uploaded", uploaded as f64);
            on_batch();
            if !done {
                break;
            }
        }

        if done {
            break;
        }
    }

    let count = total_count.ok_or("missing star header")?;
    if uploaded != count {
        telemetry::event("star-stream-incomplete");
        return Err("incomplete star catalog stream".into());
    }
    // Keep one bounded CPU copy (244 KB today) so annotations and the
    // interaction-gated explorer can start without refetching/reassembling the
    // catalog. This replaces, rather than appends to, the prior generation.
    let shared = js_sys::Uint8Array::from(pending.as_slice());
    let _ = js_sys::Reflect::set(
        &js_sys::global(),
        &JsValue::from_str("__rnBrightCatalog"),
        &shared,
    );
    telemetry::set_bool_in("stars", "complete", true);
    telemetry::set_number_in("stars", "pendingBytes", 0.0);
    telemetry::set_number_in("stars", "completedAtMs", js_sys::Date::now());
    telemetry::event("star-stream-complete");
    Ok(count as i32)
}

async fn fetch_sky_map(url: &str) -> Result<ImageBitmap, JsValue> {
    let init = web_sys::RequestInit::new();
    init.set_method("GET");
    let _ = js_sys::Reflect::set(
        &init,
        &JsValue::from_str("priority"),
        &JsValue::from_str("low"),
    );
    let request = web_sys::Request::new_with_str_and_init(url, &init)?;
    let response: Response = JsFuture::from(window().fetch_with_request(&request))
        .await?
        .dyn_into()?;
    if !response.ok() {
        return Err(format!("{url} HTTP {}", response.status()).into());
    }
    let blob: Blob = JsFuture::from(response.blob()?).await?.dyn_into()?;
    JsFuture::from(window().create_image_bitmap_with_blob(&blob)?)
        .await?
        .dyn_into()
}

async fn init(canvas: HtmlCanvasElement, controller: Rc<Controller>) -> Result<(), JsValue> {
    telemetry::event("starscape-init");
    let gl: Gl = canvas
        .get_context("webgl2")?
        .ok_or_else(|| JsValue::from_str("webgl2 unavailable"))?
        .dyn_into()?;

    gl.enable(Gl::BLEND);
    gl.blend_func(Gl::ONE, Gl::ONE);

    let sky_program = link(&gl, SKY_VERT, SKY_FRAG)?;
    let star_program = link(&gl, STAR_VERT, STAR_FRAG)?;

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

    let star_buffer = gl.create_buffer().ok_or("create_buffer")?;
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
    gl.tex_parameteri(Gl::TEXTURE_2D, Gl::TEXTURE_WRAP_S, Gl::REPEAT as i32);
    gl.tex_parameteri(Gl::TEXTURE_2D, Gl::TEXTURE_WRAP_T, Gl::CLAMP_TO_EDGE as i32);
    gl.tex_parameteri(Gl::TEXTURE_2D, Gl::TEXTURE_MIN_FILTER, Gl::LINEAR as i32);
    gl.tex_parameteri(Gl::TEXTURE_2D, Gl::TEXTURE_MAG_FILTER, Gl::LINEAR as i32);

    let scene = Rc::new(Scene {
        sky: SkyPass {
            u_view: gl.get_uniform_location(&sky_program, "u_view"),
            u_f: gl.get_uniform_location(&sky_program, "u_f"),
            u_aspect: gl.get_uniform_location(&sky_program, "u_aspect"),
            u_light: gl.get_uniform_location(&sky_program, "u_light"),
            u_reveal: gl.get_uniform_location(&sky_program, "u_reveal"),
            u_map: gl.get_uniform_location(&sky_program, "u_map"),
            map_ready: Cell::new(false),
            fade_started_at: Cell::new(None),
            fade_complete: Cell::new(false),
            tuned: tuned_locations(&gl, &sky_program),
            program: sky_program,
            triangle,
            map: map.clone(),
        },
        stars: StarPass {
            u_view: gl.get_uniform_location(&star_program, "u_view"),
            u_f: gl.get_uniform_location(&star_program, "u_f"),
            u_aspect: gl.get_uniform_location(&star_program, "u_aspect"),
            u_dpr: gl.get_uniform_location(&star_program, "u_dpr"),
            u_light: gl.get_uniform_location(&star_program, "u_light"),
            tuned: tuned_locations(&gl, &star_program),
            program: star_program,
            stars: star_buffer.clone(),
            count: Cell::new(0),
        },
        canvas,
        gl: gl.clone(),
    });

    controller.scene.replace(Some(Rc::clone(&scene)));
    if controller.reduced.get() {
        redraw_until_drawn(Rc::clone(&controller));
    } else if controller.visible.get() {
        ensure_animation(Rc::clone(&controller));
    }

    let batch_controller = Rc::clone(&controller);
    let final_count = stream_stars(&gl, &star_buffer, &scene.stars.count, || {
        let _ = redraw(&batch_controller);
    })
    .await?;
    scene.stars.count.set(final_count);
    let _ = redraw(&controller);

    match fetch_sky_map("/sky/milkyway.webp").await {
        Ok(bitmap) => {
            gl.bind_texture(Gl::TEXTURE_2D, Some(&scene.sky.map));
            gl.tex_image_2d_with_u32_and_u32_and_image_bitmap(
                Gl::TEXTURE_2D,
                0,
                Gl::RGBA as i32,
                Gl::RGBA,
                Gl::UNSIGNED_BYTE,
                &bitmap,
            )?;
            scene.sky.map_ready.set(true);
            if controller.reduced.get() {
                scene
                    .sky
                    .fade_started_at
                    .set(Some(js_sys::Date::now() - GALAXY_FADE_MS));
            } else if controller.visible.get() {
                scene.sky.fade_started_at.set(Some(js_sys::Date::now()));
            }
            telemetry::set_number_in("stars", "galaxyLoadedAtMs", js_sys::Date::now());
            telemetry::event("galaxy-fade-started");
            let _ = redraw(&controller);
        }
        Err(error) => {
            web_sys::console::warn_2(&"milky way map unavailable:".into(), &error);
        }
    }

    Ok(())
}

fn install_listeners(controller: &Rc<Controller>, media: Option<MediaQueryList>) {
    let lost_controller = Rc::clone(controller);
    let lost = Closure::<dyn FnMut(Event)>::new(move |event: Event| {
        event.prevent_default();
        telemetry::increment("starscape", "contextLost", 1.0);
        telemetry::event("webgl-context-lost");
        stop_animation(&lost_controller);
        lost_controller
            .generation
            .set(lost_controller.generation.get().wrapping_add(1));
        lost_controller.scene.replace(None);
        remove_active_class();
    });
    let _ = controller
        .canvas
        .add_event_listener_with_callback("webglcontextlost", lost.as_ref().unchecked_ref());
    lost.forget();

    let restored_controller = Rc::clone(controller);
    let restored = Closure::<dyn FnMut()>::new(move || {
        telemetry::increment("starscape", "contextRestored", 1.0);
        telemetry::event("webgl-context-restored");
        initialize(Rc::clone(&restored_controller));
    });
    let _ = controller.canvas.add_event_listener_with_callback(
        "webglcontextrestored",
        restored.as_ref().unchecked_ref(),
    );
    restored.forget();

    for event_name in ["resize", "starscape-redraw"] {
        let redraw_controller = Rc::clone(controller);
        let redraw_event = Closure::<dyn FnMut()>::new(move || {
            if redraw_controller.reduced.get() {
                redraw(&redraw_controller);
            }
        });
        let _ = window()
            .add_event_listener_with_callback(event_name, redraw_event.as_ref().unchecked_ref());
        redraw_event.forget();
    }

    let explorer_controller = Rc::clone(controller);
    let explorer_changed = Closure::<dyn FnMut()>::new(move || {
        let explorer_open = document()
            .document_element()
            .is_some_and(|root| root.class_list().contains("starscape-explorer-open"));
        let visible =
            !explorer_open && document().visibility_state() == web_sys::VisibilityState::Visible;
        explorer_controller.visible.set(visible);
        telemetry::set_bool_in("starscape", "pausedForExplorer", explorer_open);
        if visible && !explorer_controller.reduced.get() {
            ensure_animation(Rc::clone(&explorer_controller));
        } else {
            stop_animation(&explorer_controller);
        }
    });
    let _ = window().add_event_listener_with_callback(
        "starscape-explorer-state",
        explorer_changed.as_ref().unchecked_ref(),
    );
    explorer_changed.forget();

    let visibility_controller = Rc::clone(controller);
    let visibility_changed = Closure::<dyn FnMut()>::new(move || {
        let explorer_open = document()
            .document_element()
            .is_some_and(|root| root.class_list().contains("starscape-explorer-open"));
        let visible =
            !explorer_open && document().visibility_state() == web_sys::VisibilityState::Visible;
        visibility_controller.visible.set(visible);
        if visible && !visibility_controller.reduced.get() {
            if let Some(scene) = visibility_controller.scene.borrow().as_ref()
                && scene.sky.map_ready.get()
                && scene.sky.fade_started_at.get().is_none()
            {
                scene.sky.fade_started_at.set(Some(js_sys::Date::now()));
            }
            ensure_animation(Rc::clone(&visibility_controller));
        } else {
            stop_animation(&visibility_controller);
        }
    });
    let _ = document().add_event_listener_with_callback(
        "visibilitychange",
        visibility_changed.as_ref().unchecked_ref(),
    );
    visibility_changed.forget();

    if let Some(media) = media {
        let media_controller = Rc::clone(controller);
        let query = media.clone();
        let changed = Closure::<dyn FnMut()>::new(move || {
            let reduced = query.matches();
            media_controller.reduced.set(reduced);
            if let Some(frozen_at) =
                freeze_on_motion_change(reduced, current_sim(&media_controller))
            {
                media_controller.frozen_at.set(Some(frozen_at));
                if let Some(scene) = media_controller.scene.borrow().as_ref()
                    && scene.sky.map_ready.get()
                {
                    scene
                        .sky
                        .fade_started_at
                        .set(Some(js_sys::Date::now() - GALAXY_FADE_MS));
                }
                stop_animation(&media_controller);
                let _ = redraw(&media_controller);
            } else {
                media_controller.frozen_at.set(None);
                if media_controller.visible.get() {
                    ensure_animation(Rc::clone(&media_controller));
                }
            }
        });
        let _ = media.add_event_listener_with_callback("change", changed.as_ref().unchecked_ref());
        changed.forget();
    }
}

fn freeze_on_motion_change(reduced: bool, current_sim_ms: f64) -> Option<f64> {
    reduced.then_some(current_sim_ms)
}

fn should_animate(reduced: bool, scene_ready: bool, visible: bool) -> bool {
    !reduced && scene_ready && visible
}

fn current_sim(controller: &Controller) -> f64 {
    controller.frozen_at.get().unwrap_or_else(|| {
        synced_sim_time_ms(
            controller.server_epoch_ms,
            controller.client_mount_ms,
            js_sys::Date::now(),
        )
    })
}

fn ensure_animation(controller: Rc<Controller>) {
    if controller.running.replace(true) {
        return;
    }
    let generation = controller.animation_generation.get();
    let _ = request_animation_frame(move || animation_frame(controller, generation));
}

fn stop_animation(controller: &Controller) {
    controller.running.set(false);
    controller
        .animation_generation
        .set(controller.animation_generation.get().wrapping_add(1));
}

fn animation_frame(controller: Rc<Controller>, generation: u32) {
    if !controller.running.get() || controller.animation_generation.get() != generation {
        return;
    }
    if !should_animate(
        controller.reduced.get(),
        controller.scene.borrow().is_some(),
        controller.visible.get(),
    ) {
        stop_animation(&controller);
        return;
    }
    telemetry::increment("starscape", "animationFrames", 1.0);
    telemetry::observe_now("starscape", "lastFrameAtMs", "maxFrameGapMs");
    let _ = redraw(&controller);
    let _ = request_animation_frame(move || animation_frame(controller, generation));
}

fn redraw(controller: &Controller) -> bool {
    let Some(scene) = controller.scene.borrow().clone() else {
        return false;
    };
    if !draw(&scene, current_sim(controller)) {
        return false;
    }
    telemetry::increment("starscape", "draws", 1.0);
    telemetry::set_number_in("starscape", "starCount", f64::from(scene.stars.count.get()));
    if should_reveal(scene.stars.count.get()) {
        if let Some(root) = document().document_element() {
            let first_reveal = !root.class_list().contains("starscape-active");
            let _ = root.class_list().add_1("starscape-active");
            if first_reveal {
                telemetry::set_number_in("stars", "revealedAtMs", js_sys::Date::now());
                telemetry::event("first-star-batch-revealed");
            }
        }
    }
    true
}

fn should_reveal(star_count: i32) -> bool {
    star_count > 0
}

fn fade_progress(started_at_ms: f64, now_ms: f64, duration_ms: f64) -> f32 {
    ((now_ms - started_at_ms) / duration_ms).clamp(0.0, 1.0) as f32
}

fn redraw_until_drawn(controller: Rc<Controller>) {
    if redraw(&controller) {
        return;
    }
    let _ = request_animation_frame(move || redraw_until_drawn(controller));
}

fn remove_active_class() {
    if let Some(root) = document().document_element() {
        let _ = root.class_list().remove_1("starscape-active");
    }
}

fn draw(scene: &Scene, sim_ms: f64) -> bool {
    let dpr = window().device_pixel_ratio().max(1.0);
    let w = (f64::from(scene.canvas.client_width()) * dpr) as u32;
    let h = (f64::from(scene.canvas.client_height()) * dpr) as u32;
    if w == 0 || h == 0 {
        return false;
    }
    let light = document()
        .document_element()
        .and_then(|root| root.get_attribute("data-theme"))
        .as_deref()
        == Some("light");
    if scene.canvas.width() != w || scene.canvas.height() != h {
        scene.canvas.set_width(w);
        scene.canvas.set_height(h);
    }

    let gl = &scene.gl;
    gl.viewport(0, 0, w as i32, h as i32);
    gl.clear_color(0.0, 0.0, 0.0, 0.0);
    gl.clear(Gl::COLOR_BUFFER_BIT);

    let (lat_deg, lon_deg) = super::bridge::observer_for(sim_ms);
    let view = view_matrix(sim_ms, lat_deg, lon_deg);
    let aspect = w as f32 / h as f32;
    let theme = if light { 1.0 } else { 0.0 };
    let tuning = tuning::values();
    const FOCAL: f32 = 0.8391;

    gl.use_program(Some(&scene.sky.program));
    gl.disable_vertex_attrib_array(1);
    gl.disable_vertex_attrib_array(2);
    gl.bind_buffer(Gl::ARRAY_BUFFER, Some(&scene.sky.triangle));
    gl.vertex_attrib_pointer_with_i32(0, 2, Gl::FLOAT, false, 0, 0);
    gl.enable_vertex_attrib_array(0);
    gl.uniform_matrix3fv_with_f32_array(scene.sky.u_view.as_ref(), false, &view);
    gl.uniform1f(scene.sky.u_f.as_ref(), FOCAL);
    gl.uniform1f(scene.sky.u_aspect.as_ref(), aspect);
    gl.uniform1f(scene.sky.u_light.as_ref(), theme);
    let galaxy_reveal = scene.sky.fade_started_at.get().map_or(0.0, |started| {
        fade_progress(started, js_sys::Date::now(), GALAXY_FADE_MS)
    });
    gl.uniform1f(scene.sky.u_reveal.as_ref(), galaxy_reveal);
    if galaxy_reveal >= 1.0 && !scene.sky.fade_complete.replace(true) {
        telemetry::set_number_in("stars", "galaxyRevealedAtMs", js_sys::Date::now());
        telemetry::event("galaxy-fade-complete");
    }
    apply_tuning(gl, &scene.sky.tuned, &tuning);
    gl.active_texture(Gl::TEXTURE0);
    gl.bind_texture(Gl::TEXTURE_2D, Some(&scene.sky.map));
    gl.uniform1i(scene.sky.u_map.as_ref(), 0);
    gl.draw_arrays(Gl::TRIANGLES, 0, 3);

    let star_count = scene.stars.count.get();
    if star_count > 0 {
        gl.use_program(Some(&scene.stars.program));
        gl.bind_buffer(Gl::ARRAY_BUFFER, Some(&scene.stars.stars));
        gl.vertex_attrib_pointer_with_i32(0, 3, Gl::FLOAT, false, GL_STRIDE, 0);
        gl.enable_vertex_attrib_array(0);
        gl.vertex_attrib_pointer_with_i32(1, 1, Gl::FLOAT, false, GL_STRIDE, 12);
        gl.enable_vertex_attrib_array(1);
        gl.vertex_attrib_pointer_with_i32(2, 4, Gl::UNSIGNED_BYTE, true, GL_STRIDE, 16);
        gl.enable_vertex_attrib_array(2);
        gl.uniform_matrix3fv_with_f32_array(scene.stars.u_view.as_ref(), false, &view);
        gl.uniform1f(scene.stars.u_f.as_ref(), FOCAL);
        gl.uniform1f(scene.stars.u_aspect.as_ref(), aspect);
        gl.uniform1f(scene.stars.u_dpr.as_ref(), dpr as f32);
        gl.uniform1f(scene.stars.u_light.as_ref(), theme);
        apply_tuning(gl, &scene.stars.tuned, &tuning);
        gl.draw_arrays(Gl::POINTS, 0, star_count);
    }
    true
}

fn tuned_locations(gl: &Gl, program: &WebGlProgram) -> Vec<Option<WebGlUniformLocation>> {
    tuning::SPEC
        .iter()
        .map(|(field, _)| gl.get_uniform_location(program, &format!("u_{field}")))
        .collect()
}

fn apply_tuning(gl: &Gl, locations: &[Option<WebGlUniformLocation>], values: &tuning::Values) {
    for (location, value) in locations.iter().zip(values.iter()) {
        gl.uniform1f(location.as_ref(), *value);
    }
}

fn link(gl: &Gl, vert: &str, frag: &str) -> Result<WebGlProgram, JsValue> {
    let program = gl.create_program().ok_or("create_program")?;
    gl.attach_shader(&program, &compile(gl, Gl::VERTEX_SHADER, vert)?);
    gl.attach_shader(&program, &compile(gl, Gl::FRAGMENT_SHADER, frag)?);
    gl.link_program(&program);
    if gl
        .get_program_parameter(&program, Gl::LINK_STATUS)
        .as_bool()
        .unwrap_or(false)
    {
        Ok(program)
    } else {
        Err(gl.get_program_info_log(&program).unwrap_or_default().into())
    }
}

fn compile(gl: &Gl, kind: u32, src: &str) -> Result<WebGlShader, JsValue> {
    let shader = gl.create_shader(kind).ok_or("create_shader")?;
    gl.shader_source(&shader, src);
    gl.compile_shader(&shader);
    if gl
        .get_shader_parameter(&shader, Gl::COMPILE_STATUS)
        .as_bool()
        .unwrap_or(false)
    {
        Ok(shader)
    } else {
        Err(gl.get_shader_info_log(&shader).unwrap_or_default().into())
    }
}

#[cfg(test)]
mod tests {
    use super::{fade_progress, freeze_on_motion_change, should_animate, should_reveal};

    #[test]
    fn reduced_motion_freezes_time_and_disables_continuous_animation() {
        assert_eq!(freeze_on_motion_change(true, 42.0), Some(42.0));
        assert!(!should_animate(true, true, true));
        assert!(should_animate(false, true, true));
        assert!(!should_animate(false, false, true));
        assert!(!should_animate(false, true, false));
    }

    #[test]
    fn css_fallback_stays_visible_until_the_first_star_batch() {
        assert!(!should_reveal(0));
        assert!(should_reveal(1));
    }

    #[test]
    fn galaxy_fade_progress_is_clamped() {
        assert_eq!(fade_progress(1_000.0, 500.0, 1_600.0), 0.0);
        assert_eq!(fade_progress(1_000.0, 1_800.0, 1_600.0), 0.5);
        assert_eq!(fade_progress(1_000.0, 3_000.0, 1_600.0), 1.0);
    }
}
