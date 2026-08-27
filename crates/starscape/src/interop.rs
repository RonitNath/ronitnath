//! `window.__rnStarscape` — the sky's state, readable and drivable from JS.
//!
//! This exists for two callers: the end-to-end suite, which has to be able to
//! assert on where the view is rather than on what a canvas looks like, and a
//! later deep-zoom explorer, which will need the same view matrix this bundle
//! draws with. It is deliberately small and deliberately named: an interop
//! surface, not a debug hook that grew.

use std::rc::Rc;

use wasm_bindgen::JsValue;
use wasm_bindgen::closure::Closure;

use crate::app::App;
use crate::dom;
use crate::sky::{sim_time_ms, view_matrix};

const GLOBAL: &str = "__rnStarscape";

pub fn install(app: &Rc<App>) {
    let api = js_sys::Object::new();

    let observer = Rc::clone(app);
    function0(&api, "observer", move || {
        let (lat, lon) = observer.observer_now();
        pair(lat, lon)
    });

    let manual = Rc::clone(app);
    function0(&api, "manualObserver", move || {
        manual
            .manual_observer()
            .map_or(JsValue::NULL, |(lat, lon)| pair(lat, lon))
    });

    let orbit = Rc::clone(app);
    function1(&api, "orbitAt", move |sim_ms| {
        let (lat, lon) = orbit.orbit_at(sim_ms);
        pair(lat, lon)
    });

    let clock = Rc::clone(app);
    function0(&api, "simTimeMs", move || JsValue::from_f64(clock.sim_ms()));

    let matrix = Rc::clone(app);
    function0(&api, "viewMatrix", move || {
        let (lat, lon) = matrix.observer_now();
        view_matrix(matrix.sim_ms(), lat, lon)
            .into_iter()
            .map(|value| JsValue::from_f64(f64::from(value)))
            .collect::<js_sys::Array>()
            .into()
    });

    function1(&api, "simTimeAt", |now_ms| {
        JsValue::from_f64(sim_time_ms(now_ms))
    });

    let set = Rc::clone(app);
    let closure = Closure::<dyn Fn(f64, f64)>::new(move |lat: f64, lon: f64| {
        set.set_observer(lat, lon, true);
        set.draw();
        set.draw_readouts();
    });
    let _ = js_sys::Reflect::set(&api, &"setObserver".into(), closure.as_ref());
    closure.forget();

    let resume = Rc::clone(app);
    let closure = Closure::<dyn Fn()>::new(move || {
        resume.resume_orbit();
        resume.draw();
        resume.draw_readouts();
    });
    let _ = js_sys::Reflect::set(&api, &"resumeOrbit".into(), closure.as_ref());
    closure.forget();

    let _ = js_sys::Reflect::set(&js_sys::global(), &GLOBAL.into(), &api);
    let _ = dom::window();
}

fn pair(lat: f64, lon: f64) -> JsValue {
    let point = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&point, &"lat".into(), &JsValue::from_f64(lat));
    let _ = js_sys::Reflect::set(&point, &"lon".into(), &JsValue::from_f64(lon));
    point.into()
}

fn function0(api: &js_sys::Object, name: &str, body: impl Fn() -> JsValue + 'static) {
    let closure = Closure::<dyn Fn() -> JsValue>::new(body);
    let _ = js_sys::Reflect::set(api, &name.into(), closure.as_ref());
    closure.forget();
}

fn function1(api: &js_sys::Object, name: &str, body: impl Fn(f64) -> JsValue + 'static) {
    let closure = Closure::<dyn Fn(f64) -> JsValue>::new(body);
    let _ = js_sys::Reflect::set(api, &name.into(), closure.as_ref());
    closure.forget();
}
