//! Expose shared track sampling to the mini-globe JS module.

use wasm_bindgen::prelude::*;

use super::{SPEED, TRACK_PERIOD_MS, observer_at, sim_time_ms, synced_sim_time_ms, view_matrix};

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

/// Resolve the observer selected by the browser. A manual globe selection is
/// deliberately tab-local; absent or malformed state falls back to the
/// canonical server-synchronised orbit.
pub fn observer_for(sim_ms: f64) -> (f64, f64) {
    let Ok(api) = js_sys::Reflect::get(&js_sys::global(), &JsValue::from_str("__rnTrack")) else {
        return observer_at(sim_ms);
    };
    let manual = js_sys::Reflect::get(&api, &JsValue::from_str("manualObserver"))
        .ok()
        .filter(|value| !value.is_null() && !value.is_undefined());
    let Some(manual) = manual else {
        return observer_at(sim_ms);
    };
    let lat = js_sys::Reflect::get(&manual, &JsValue::from_str("lat"))
        .ok()
        .and_then(|value| value.as_f64());
    let lon = js_sys::Reflect::get(&manual, &JsValue::from_str("lon"))
        .ok()
        .and_then(|value| value.as_f64());
    let mut target = match (lat, lon) {
        (Some(lat), Some(lon)) if lat.is_finite() && lon.is_finite() => (
            lat.clamp(-90.0, 90.0),
            (lon + 180.0).rem_euclid(360.0) - 180.0,
        ),
        _ => observer_at(sim_ms),
    };
    let transition = js_sys::Reflect::get(&api, &"observerTransition".into())
        .ok()
        .filter(|value| !value.is_null() && !value.is_undefined());
    let Some(transition) = transition else {
        return target;
    };
    let clear_at_end = js_sys::Reflect::get(&transition, &"clearAtEnd".into())
        .ok()
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    if clear_at_end {
        target = observer_at(sim_ms);
    }
    let number = |field: &str| {
        js_sys::Reflect::get(&transition, &field.into())
            .ok()
            .and_then(|value| value.as_f64())
    };
    let Some((start_lat, start_lon, started_at, duration)) = number("startLat")
        .zip(number("startLon"))
        .zip(number("startedAt"))
        .zip(number("duration"))
        .map(|(((lat, lon), started), duration)| (lat, lon, started, duration))
    else {
        return target;
    };
    let progress = if duration <= 0.0 {
        1.0
    } else {
        ((js_sys::Date::now() - started_at) / duration).clamp(0.0, 1.0)
    };
    if progress >= 1.0 {
        let _ = js_sys::Reflect::set(&api, &"observerTransition".into(), &JsValue::NULL);
        if clear_at_end {
            let _ = js_sys::Reflect::set(&api, &"manualObserver".into(), &JsValue::NULL);
        }
        target
    } else {
        great_circle_lerp((start_lat, start_lon), target, progress)
    }
}

pub fn displayed_sim_time(fallback: f64) -> f64 {
    js_sys::Reflect::get(&js_sys::global(), &"__rnTrack".into())
        .ok()
        .and_then(|api| js_sys::Reflect::get(&api, &"viewerState".into()).ok())
        .filter(|state| !state.is_null() && !state.is_undefined())
        .and_then(|state| js_sys::Reflect::get(&state, &"simMs".into()).ok())
        .and_then(|value| value.as_f64())
        .filter(|value| value.is_finite())
        .unwrap_or(fallback)
}

fn great_circle_lerp(a: (f64, f64), b: (f64, f64), t: f64) -> (f64, f64) {
    let vector = |(lat, lon): (f64, f64)| {
        let (sin_lat, cos_lat) = lat.to_radians().sin_cos();
        let (sin_lon, cos_lon) = lon.to_radians().sin_cos();
        [cos_lat * cos_lon, cos_lat * sin_lon, sin_lat]
    };
    let (av, bv) = (vector(a), vector(b));
    let dot = av
        .iter()
        .zip(bv)
        .map(|(x, y)| x * y)
        .sum::<f64>()
        .clamp(-1.0, 1.0);
    let angle = dot.acos();
    let out = if angle < 1e-9 {
        av
    } else {
        let scale = angle.sin();
        let wa = ((1.0 - t) * angle).sin() / scale;
        let wb = (t * angle).sin() / scale;
        [
            av[0] * wa + bv[0] * wb,
            av[1] * wa + bv[1] * wb,
            av[2] * wa + bv[2] * wb,
        ]
    };
    (
        out[2].asin().to_degrees(),
        out[1].atan2(out[0]).to_degrees(),
    )
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
        let (lat, lon) = observer_for(sim_ms);
        let point = js_sys::Array::new();
        point.push(&JsValue::from_f64(lat));
        point.push(&JsValue::from_f64(lon));
        point.into()
    });
    let _ = js_sys::Reflect::set(&api, &JsValue::from_str("observerAt"), observer.as_ref());
    observer.forget();

    let matrix = Closure::<dyn Fn(f64) -> JsValue>::new(|sim_ms: f64| {
        let (lat, lon) = observer_for(sim_ms);
        view_matrix(sim_ms, lat, lon)
            .into_iter()
            .map(|value| JsValue::from_f64(f64::from(value)))
            .collect::<js_sys::Array>()
            .into()
    });
    let _ = js_sys::Reflect::set(&api, &JsValue::from_str("viewMatrix"), matrix.as_ref());
    matrix.forget();

    let set_manual = Closure::<dyn Fn(f64, f64)>::new(|lat: f64, lon: f64| {
        let Ok(api) = js_sys::Reflect::get(&js_sys::global(), &JsValue::from_str("__rnTrack"))
        else {
            return;
        };
        let viewer_sim = js_sys::Reflect::get(&api, &"viewerState".into())
            .ok()
            .and_then(|state| js_sys::Reflect::get(&state, &"simMs".into()).ok())
            .and_then(|value| value.as_f64());
        let server_epoch = js_sys::Reflect::get(&api, &"serverEpochMs".into())
            .ok()
            .and_then(|value| value.as_f64())
            .unwrap_or_else(js_sys::Date::now);
        let client_mount = js_sys::Reflect::get(&api, &"clientMountMs".into())
            .ok()
            .and_then(|value| value.as_f64())
            .unwrap_or_else(js_sys::Date::now);
        let sim_ms = viewer_sim
            .unwrap_or_else(|| synced_sim_time_ms(server_epoch, client_mount, js_sys::Date::now()));
        let (start_lat, start_lon) = observer_for(sim_ms);
        let reduced = web_sys::window()
            .and_then(|window| {
                window
                    .match_media("(prefers-reduced-motion: reduce)")
                    .ok()
                    .flatten()
            })
            .is_some_and(|query| query.matches());
        let transition = js_sys::Object::new();
        for (field, value) in [
            ("startLat", start_lat),
            ("startLon", start_lon),
            ("startedAt", js_sys::Date::now()),
            ("duration", if reduced { 0.0 } else { 800.0 }),
        ] {
            let _ = js_sys::Reflect::set(&transition, &field.into(), &JsValue::from_f64(value));
        }
        let point = js_sys::Object::new();
        let _ = js_sys::Reflect::set(
            &point,
            &"lat".into(),
            &JsValue::from_f64(lat.clamp(-90.0, 90.0)),
        );
        let _ = js_sys::Reflect::set(
            &point,
            &"lon".into(),
            &JsValue::from_f64((lon + 180.0).rem_euclid(360.0) - 180.0),
        );
        let _ = js_sys::Reflect::set(&api, &"manualObserver".into(), &point);
        let _ = js_sys::Reflect::set(&api, &"observerTransition".into(), &transition);
        if let Ok(event) = web_sys::CustomEvent::new("starscape-observer-changed") {
            let _ = web_sys::window().unwrap().dispatch_event(&event);
        }
        if let Ok(event) = web_sys::Event::new("starscape-redraw") {
            let _ = web_sys::window().unwrap().dispatch_event(&event);
        }
    });
    let _ = js_sys::Reflect::set(
        &api,
        &JsValue::from_str("setManualObserver"),
        set_manual.as_ref(),
    );
    set_manual.forget();

    let resume = Closure::<dyn Fn()>::new(|| {
        if let Ok(api) = js_sys::Reflect::get(&js_sys::global(), &JsValue::from_str("__rnTrack")) {
            let viewer_sim = js_sys::Reflect::get(&api, &"viewerState".into())
                .ok()
                .and_then(|state| js_sys::Reflect::get(&state, &"simMs".into()).ok())
                .and_then(|value| value.as_f64());
            let server_epoch = js_sys::Reflect::get(&api, &"serverEpochMs".into())
                .ok()
                .and_then(|value| value.as_f64())
                .unwrap_or_else(js_sys::Date::now);
            let client_mount = js_sys::Reflect::get(&api, &"clientMountMs".into())
                .ok()
                .and_then(|value| value.as_f64())
                .unwrap_or_else(js_sys::Date::now);
            let sim_ms = viewer_sim.unwrap_or_else(|| {
                synced_sim_time_ms(server_epoch, client_mount, js_sys::Date::now())
            });
            let (start_lat, start_lon) = observer_for(sim_ms);
            let (end_lat, end_lon) = observer_at(sim_ms);
            let point = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&point, &"lat".into(), &JsValue::from_f64(end_lat));
            let _ = js_sys::Reflect::set(&point, &"lon".into(), &JsValue::from_f64(end_lon));
            let reduced = web_sys::window()
                .and_then(|window| {
                    window
                        .match_media("(prefers-reduced-motion: reduce)")
                        .ok()
                        .flatten()
                })
                .is_some_and(|query| query.matches());
            let transition = js_sys::Object::new();
            for (field, value) in [
                ("startLat", start_lat),
                ("startLon", start_lon),
                ("startedAt", js_sys::Date::now()),
                ("duration", if reduced { 0.0 } else { 800.0 }),
            ] {
                let _ = js_sys::Reflect::set(&transition, &field.into(), &JsValue::from_f64(value));
            }
            let _ = js_sys::Reflect::set(&transition, &"clearAtEnd".into(), &JsValue::TRUE);
            let _ = js_sys::Reflect::set(&api, &"manualObserver".into(), &point);
            let _ = js_sys::Reflect::set(&api, &"observerTransition".into(), &transition);
        }
        if let Ok(event) = web_sys::CustomEvent::new("starscape-observer-changed") {
            let _ = web_sys::window().unwrap().dispatch_event(&event);
        }
        if let Ok(event) = web_sys::Event::new("starscape-redraw") {
            let _ = web_sys::window().unwrap().dispatch_event(&event);
        }
    });
    let _ = js_sys::Reflect::set(&api, &JsValue::from_str("resumeOrbit"), resume.as_ref());
    resume.forget();

    let sim = Closure::<dyn Fn(f64) -> f64>::new(sim_time_ms);
    let _ = js_sys::Reflect::set(&api, &JsValue::from_str("simTimeMs"), sim.as_ref());
    sim.forget();

    let synced = Closure::<dyn Fn(f64, f64, f64) -> f64>::new(synced_sim_time_ms);
    let _ = js_sys::Reflect::set(&api, &JsValue::from_str("syncedSimTimeMs"), synced.as_ref());
    synced.forget();

    let _ = js_sys::Reflect::set(&js_sys::global(), &JsValue::from_str("__rnTrack"), &api);
}

#[cfg(test)]
mod tests {
    use super::great_circle_lerp;

    #[test]
    fn observer_transition_follows_the_shortest_great_circle() {
        let midpoint = great_circle_lerp((0.0, 0.0), (0.0, 90.0), 0.5);
        assert!(midpoint.0.abs() < 1e-9);
        assert!((midpoint.1 - 45.0).abs() < 1e-9);

        let across_antimeridian = great_circle_lerp((0.0, 170.0), (0.0, -170.0), 0.5);
        assert!((across_antimeridian.1.abs() - 180.0).abs() < 1e-9);
    }
}
