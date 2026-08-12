//! Opt-in browser telemetry for diagnosing long-running starscape sessions.
//!
//! Enable with `?debug=telemetry`, then inspect `window.__rnTelemetry` in the
//! browser console. The object is intentionally bounded and contains counters
//! rather than per-frame samples, so observing the renderer does not create the
//! same kind of retention problem we are trying to diagnose.

use leptos::prelude::{document, window};
use wasm_bindgen::{JsCast, JsValue, closure::Closure};

const GLOBAL: &str = "__rnTelemetry";
const MAX_EVENTS: u32 = 100;

pub fn install() {
    if !enabled() || telemetry().is_some() {
        return;
    }

    let root = js_sys::Object::new();
    set_value(&root, "enabled", &JsValue::TRUE);
    set_number(&root, "startedAtMs", js_sys::Date::now());
    set_value(&root, "visibility", &document().visibility_state().into());

    for section in ["stars", "starscape", "globe"] {
        set_value(&root, section, &js_sys::Object::new());
    }
    set_value(&root, "events", &js_sys::Array::new());
    set_value(&js_sys::global(), GLOBAL, &root);

    event("telemetry-installed");
    let changed = Closure::<dyn FnMut()>::new(move || {
        if let Some(root) = telemetry() {
            set_value(&root, "visibility", &document().visibility_state().into());
        }
        increment("starscape", "visibilityChanges", 1.0);
        event("visibility-change");
    });
    let _ = document()
        .add_event_listener_with_callback("visibilitychange", changed.as_ref().unchecked_ref());
    changed.forget();
}

pub fn event(name: &str) {
    let Some(root) = telemetry() else {
        return;
    };
    let Ok(events) = js_sys::Reflect::get(&root, &"events".into()) else {
        return;
    };
    let Ok(events) = events.dyn_into::<js_sys::Array>() else {
        return;
    };
    let entry = js_sys::Object::new();
    set_number(&entry, "atMs", js_sys::Date::now());
    set_value(&entry, "name", &name.into());
    events.push(&entry);
    while events.length() > MAX_EVENTS {
        events.shift();
    }
}

pub fn increment(section: &str, field: &str, amount: f64) {
    let Some(section) = section_object(section) else {
        return;
    };
    let current = js_sys::Reflect::get(&section, &field.into())
        .ok()
        .and_then(|value| value.as_f64())
        .unwrap_or(0.0);
    set_number(&section, field, current + amount);
}

pub fn set_number_in(section: &str, field: &str, value: f64) {
    if let Some(section) = section_object(section) {
        set_number(&section, field, value);
    }
}

pub fn set_bool_in(section: &str, field: &str, value: bool) {
    if let Some(section) = section_object(section) {
        set_value(&section, field, &JsValue::from_bool(value));
    }
}

pub fn observe_now(section: &str, last_field: &str, max_gap_field: &str) {
    let Some(section) = section_object(section) else {
        return;
    };
    let now = js_sys::Date::now();
    if let Some(previous) = js_sys::Reflect::get(&section, &last_field.into())
        .ok()
        .and_then(|value| value.as_f64())
    {
        let gap = now - previous;
        let prior_max = js_sys::Reflect::get(&section, &max_gap_field.into())
            .ok()
            .and_then(|value| value.as_f64())
            .unwrap_or(0.0);
        set_number(&section, max_gap_field, gap.max(prior_max));
    }
    set_number(&section, last_field, now);
}

fn enabled() -> bool {
    window().location().search().is_ok_and(|search| {
        search
            .split('&')
            .any(|part| part.trim_start_matches('?') == "debug=telemetry")
    })
}

fn telemetry() -> Option<JsValue> {
    js_sys::Reflect::get(&js_sys::global(), &GLOBAL.into())
        .ok()
        .filter(|value| !value.is_null() && !value.is_undefined())
}

fn section_object(name: &str) -> Option<JsValue> {
    let root = telemetry()?;
    js_sys::Reflect::get(&root, &name.into()).ok()
}

fn set_number(target: &JsValue, field: &str, value: f64) {
    set_value(target, field, &JsValue::from_f64(value));
}

fn set_value(target: &JsValue, field: &str, value: &JsValue) {
    let _ = js_sys::Reflect::set(target, &field.into(), value);
}
