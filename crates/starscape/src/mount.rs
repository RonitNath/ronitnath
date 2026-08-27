//! Bringing the sky up on a page that already renders without it.
//!
//! Order matters and is the whole of the loading design: the document paints
//! first, then the star catalog streams (revealing the canvas on its first
//! batch), then the annotations, then the Milky Way at low priority, and the
//! globe and its Earth texture last. Every stage is optional — a failure warns
//! and leaves everything earlier standing.

use std::rc::Rc;

use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;
use web_sys::{Event, PointerEvent};

use crate::app::App;
use crate::explorer::{self, Launch};
use crate::globe::Globe;
use crate::label::UPDATE_INTERVAL_MS;
use crate::render::Scene;
use crate::{callouts, dom, interop, telemetry};

/// Movement under this many pixels is a click, not a drag.
const CLICK_SLOP_PX: f64 = 4.0;

pub fn start() {
    console_error_panic_hook::set_once();
    telemetry::install();

    let Some(canvas) = dom::canvas("starscape") else {
        // Not the landing page. The bundle is inert rather than noisy.
        return;
    };
    let app = App::new(server_epoch_ms());
    interop::install(&app);
    listen_to_the_page(&app, &canvas);
    listen_for_the_atlas(&app);

    match Scene::create(canvas) {
        Ok(scene) => app.attach_sky(scene),
        Err(error) => {
            dom::warn("starscape unavailable:", &error);
            return;
        }
    }

    if app.reduced_motion() {
        app.redraw_until_drawn();
    } else {
        app.ensure_animation();
    }
    schedule_readouts(&app);

    let boot = Rc::clone(&app);
    dom::when_idle(2_000.0, move || spawn_local(load(boot)));
}

async fn load(app: Rc<App>) {
    match app.load_stars().await {
        Ok(bright) => {
            app.draw();
            let named = app.load_named(&bright).await;
            if let Err(error) = &named {
                dom::warn("star names unavailable:", error);
            }
            // Star labels are progressive enhancement: without them the sky is
            // unnamed, and the atlas still opens on the launcher.
            if let Some(layer) = dom::element("star-annotations") {
                let _ = layer.set_attribute(
                    "data-annotations-ready",
                    if named.is_ok() { "true" } else { "false" },
                );
            }
            // The atlas opens on these same bytes. 244 KB held is a second
            // fetch, and a first frame, saved.
            explorer::retain_catalog(bright);
        }
        Err(error) => {
            // The CSS starfield is the picture. Nothing else on the page cares.
            dom::warn("star catalog unavailable:", &error);
            app.detach_sky();
            return;
        }
    }

    if let Err(error) = app.load_galaxy().await {
        dom::warn("milky way map unavailable:", &error);
    }
    if let Err(error) = app.load_cities().await {
        dom::warn("city catalog unavailable:", &error);
    }
    mount_globe(&app).await;
    app.draw_readouts();
}

async fn mount_globe(app: &Rc<App>) {
    let Some(canvas) = dom::canvas("mini-globe") else {
        return;
    };
    match Globe::create(canvas.clone()) {
        Ok(globe) => app.attach_globe(globe),
        Err(error) => {
            dom::warn("mini-globe unavailable:", &error);
            return;
        }
    }
    if let Err(error) = app.load_earth_texture().await {
        dom::warn("earth texture unavailable:", &error);
    }
    let _ = canvas.class_list().add_1("is-ready");
    listen_to_the_globe(app, &canvas);
    app.draw();
    telemetry::event("globe-ready");
}

/// Page-level state the sky is a function of: visibility, reduced motion, and
/// the theme, which the toggle announces with a `starscape-redraw` event.
fn listen_to_the_page(app: &Rc<App>, canvas: &web_sys::HtmlCanvasElement) {
    let Some(window) = dom::window() else { return };
    let Some(document) = dom::document() else {
        return;
    };

    let visibility = Rc::clone(app);
    dom::listen::<Event>(document.as_ref(), "visibilitychange", move |_| {
        visibility.set_visible(dom::is_visible());
        telemetry::increment("sky", "visibilityChanges", 1.0);
    });

    for event in ["resize", "starscape-redraw"] {
        let app = Rc::clone(app);
        dom::listen::<Event>(window.as_ref(), event, move |_| {
            app.draw();
            app.draw_readouts();
        });
    }

    if let Some(query) = dom::prefers_reduced_motion() {
        let app = Rc::clone(app);
        let watched = query.clone();
        dom::listen::<Event>(query.as_ref(), "change", move |_| {
            app.set_reduced_motion(watched.matches());
        });
    }

    // A lost context is not an error to report: the browser took the GPU back
    // and will hand it over again. Drop the scene, put the fallback back, and
    // wait for the restore event.
    let lost = Rc::clone(app);
    dom::listen::<Event>(canvas.as_ref(), "webglcontextlost", move |event: Event| {
        event.prevent_default();
        lost.stop_animation();
        lost.detach_sky();
        telemetry::event("webgl-context-lost");
    });
    let restored = Rc::clone(app);
    let restored_canvas = canvas.clone();
    dom::listen::<Event>(canvas.as_ref(), "webglcontextrestored", move |_| {
        telemetry::event("webgl-context-restored");
        if let Ok(scene) = Scene::create(restored_canvas.clone()) {
            restored.attach_sky(scene);
            let reload = Rc::clone(&restored);
            spawn_local(load(reload));
        }
    });
}

/// The globe's two gestures. A press that moves is a drag; a press that does
/// not is a pick, and a pick travels rather than snapping so the viewer can see
/// where they went.
fn listen_to_the_globe(app: &Rc<App>, canvas: &web_sys::HtmlCanvasElement) {
    let drag: Rc<std::cell::Cell<Option<(f64, f64, f64)>>> = Rc::new(std::cell::Cell::new(None));

    let down = Rc::clone(&drag);
    let down_canvas = canvas.clone();
    dom::listen::<PointerEvent>(canvas.as_ref(), "pointerdown", move |event| {
        let _ = down_canvas.set_pointer_capture(event.pointer_id());
        down.set(Some((
            event.client_x().into(),
            event.client_y().into(),
            0.0,
        )));
    });

    let moved = Rc::clone(&drag);
    let move_app = Rc::clone(app);
    dom::listen::<PointerEvent>(canvas.as_ref(), "pointermove", move |event| {
        let Some((x, y, travelled)) = moved.get() else {
            return;
        };
        let (dx, dy) = (
            f64::from(event.client_x()) - x,
            f64::from(event.client_y()) - y,
        );
        moved.set(Some((
            event.client_x().into(),
            event.client_y().into(),
            travelled + dx.abs() + dy.abs(),
        )));
        move_app.drag(dx, dy);
        move_app.draw();
        move_app.draw_readouts();
    });

    let up = Rc::clone(&drag);
    let up_app = Rc::clone(app);
    dom::listen::<PointerEvent>(canvas.as_ref(), "pointerup", move |event| {
        let travelled = up.replace(None).map_or(0.0, |(_, _, travelled)| travelled);
        if travelled <= CLICK_SLOP_PX
            && let Some((lat, lon)) =
                up_app.globe_point_at(event.client_x().into(), event.client_y().into())
        {
            up_app.set_observer(lat, lon, true);
        }
        up_app.draw();
        up_app.draw_readouts();
    });

    let cancel = Rc::clone(&drag);
    dom::listen::<PointerEvent>(canvas.as_ref(), "pointercancel", move |_| {
        cancel.set(None);
    });

    if let Some(button) = dom::element("resume-orbit") {
        let app = Rc::clone(app);
        dom::listen::<Event>(button.as_ref(), "click", move |_| {
            app.resume_orbit();
            app.draw();
            app.draw_readouts();
        });
    }
}

fn schedule_readouts(app: &Rc<App>) {
    let readouts = Rc::clone(app);
    dom::set_interval(UPDATE_INTERVAL_MS, move || {
        // Nothing behind the atlas is visible, and relaying out labels that
        // cannot be seen is the cost of the sky without the picture.
        if readouts.reduced_motion() || explorer::is_open() {
            return;
        }
        readouts.draw_readouts();
    });
}

/// The two ways into the atlas: the launcher, and any star label.
///
/// The labels are handled by one listener on their container rather than one
/// per label, because the set of labels is rebuilt whenever the sky turns far
/// enough to change which stars are overhead.
fn listen_for_the_atlas(app: &Rc<App>) {
    if let Some(button) = dom::element(crate::explorer::LAUNCH_ID) {
        let app = Rc::clone(app);
        dom::listen::<Event>(button.as_ref(), "click", move |_| {
            explorer::open(&app, Launch::Sky);
        });
    }
    if let Some(layer) = dom::element("star-annotations") {
        let app = Rc::clone(app);
        dom::listen::<Event>(layer.as_ref(), "click", move |event: Event| {
            let Some(label) = event
                .target()
                .and_then(|target| target.dyn_into::<web_sys::Element>().ok())
                .and_then(|element| element.closest(".star-callout").ok().flatten())
            else {
                return;
            };
            let position = label
                .get_attribute(callouts::POSITION_ATTRIBUTE)
                .as_deref()
                .and_then(callouts::parse_position);
            let (Some(position), Some(name)) = (position, label.get_attribute("data-name")) else {
                return;
            };
            explorer::open(&app, Launch::Star { name, position });
        });
    }
}

/// The server's clock at render, published by the askama landing so every
/// browser draws the same instant regardless of how wrong its own clock is.
fn server_epoch_ms() -> f64 {
    dom::document()
        .and_then(|document| {
            document
                .query_selector("meta[name=rn-server-epoch-ms]")
                .ok()
                .flatten()
        })
        .and_then(|meta| meta.get_attribute("content"))
        .and_then(|value| value.parse::<f64>().ok())
        .filter(|value| value.is_finite() && *value > 0.0)
        .unwrap_or_else(dom::now_ms)
}
