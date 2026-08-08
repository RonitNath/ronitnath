//! Expose shared track sampling to the mini-globe JS module.

use wasm_bindgen::prelude::*;

use super::{SPEED, TRACK_PERIOD_MS, observer_at, sim_time_ms, synced_sim_time_ms};

#[wasm_bindgen]
pub struct TrackSample {
    pub lat: f64,
    pub lon: f64,
}

#[wasm_bindgen]
pub fn rn_observer_at(sim_ms: f64) -> TrackSample {
    let (lat, lon) = observer_at(sim_ms);
    TrackSample { lat, lon }
}

#[wasm_bindgen]
pub fn rn_sim_time_ms(now_ms: f64) -> f64 {
    sim_time_ms(now_ms)
}

#[wasm_bindgen]
pub fn rn_synced_sim_time_ms(
    server_epoch_ms: f64,
    client_mount_ms: f64,
    client_now_ms: f64,
) -> f64 {
    synced_sim_time_ms(server_epoch_ms, client_mount_ms, client_now_ms)
}

#[wasm_bindgen]
pub fn rn_track_period_ms() -> f64 {
    TRACK_PERIOD_MS
}

#[wasm_bindgen]
pub fn rn_speed() -> f64 {
    SPEED
}

/// Install `window.__rnTrack` helpers the mini-globe module can call.
pub fn install_track_api(server_epoch_ms: f64) {
    let client_mount_ms = js_sys::Date::now();
    let api = js_sys::Object::new();
    let _ = js_sys::Reflect::set(
        &api,
        &JsValue::from_str("serverEpochMs"),
        &JsValue::from_f64(server_epoch_ms),
    );
    let _ = js_sys::Reflect::set(
        &api,
        &JsValue::from_str("clientMountMs"),
        &JsValue::from_f64(client_mount_ms),
    );
    let _ = js_sys::Reflect::set(
        &api,
        &JsValue::from_str("trackPeriodMs"),
        &JsValue::from_f64(TRACK_PERIOD_MS),
    );
    let _ = js_sys::Reflect::set(&api, &JsValue::from_str("speed"), &JsValue::from_f64(SPEED));

    let observer = Closure::<dyn Fn(f64) -> JsValue>::new(|sim_ms: f64| {
        let (lat, lon) = observer_at(sim_ms);
        let point = js_sys::Array::new();
        point.push(&JsValue::from_f64(lat));
        point.push(&JsValue::from_f64(lon));
        point.into()
    });
    let _ = js_sys::Reflect::set(&api, &JsValue::from_str("observerAt"), observer.as_ref());
    observer.forget();

    let sim = Closure::<dyn Fn(f64) -> f64>::new(sim_time_ms);
    let _ = js_sys::Reflect::set(&api, &JsValue::from_str("simTimeMs"), sim.as_ref());
    sim.forget();

    let synced = Closure::<dyn Fn(f64, f64, f64) -> f64>::new(synced_sim_time_ms);
    let _ = js_sys::Reflect::set(&api, &JsValue::from_str("syncedSimTimeMs"), synced.as_ref());
    synced.forget();

    let _ = js_sys::Reflect::set(&js_sys::global(), &JsValue::from_str("__rnTrack"), &api);
}
