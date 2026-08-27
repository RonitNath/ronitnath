//! Opt-in browser counters, for diagnosing a long-running sky.
//!
//! Turn it on with `?debug=telemetry` and read `window.__rnTelemetry`. It holds
//! counters and a bounded event ring rather than per-frame samples, so watching
//! the renderer cannot create the retention problem it exists to find. Nothing
//! is installed at all without the query parameter, so the shipped page pays
//! for none of it.

use wasm_bindgen::{JsCast, JsValue};

use crate::dom;

const GLOBAL: &str = "__rnTelemetry";
const MAX_EVENTS: u32 = 100;

pub fn install() {
    if !enabled() || root().is_some() {
        return;
    }
    let telemetry = js_sys::Object::new();
    set(&telemetry, "enabled", &JsValue::TRUE);
    set(&telemetry, "startedAtMs", &JsValue::from_f64(dom::now_ms()));
    for section in ["stars", "sky", "globe", "annotations"] {
        set(&telemetry, section, &js_sys::Object::new());
    }
    set(&telemetry, "events", &js_sys::Array::new());
    set(&js_sys::global(), GLOBAL, &telemetry);
    event("telemetry-installed");
}

pub fn event(name: &str) {
    let Some(events) = root()
        .and_then(|root| js_sys::Reflect::get(&root, &"events".into()).ok())
        .and_then(|events| events.dyn_into::<js_sys::Array>().ok())
    else {
        return;
    };
    let entry = js_sys::Object::new();
    set(&entry, "atMs", &JsValue::from_f64(dom::now_ms()));
    set(&entry, "name", &JsValue::from_str(name));
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
    set(&section, field, &JsValue::from_f64(current + amount));
}

pub fn number(section: &str, field: &str, value: f64) {
    if let Some(section) = section_object(section) {
        set(&section, field, &JsValue::from_f64(value));
    }
}

/// Record a value once and leave the first one standing. Used for the "when did
/// this first happen" marks, where a later frame overwriting the answer would
/// silently turn a latency measurement into a clock reading.
pub fn mark(section: &str, field: &str) {
    let Some(section) = section_object(section) else {
        return;
    };
    if js_sys::Reflect::get(&section, &field.into())
        .ok()
        .and_then(|value| value.as_f64())
        .is_none()
    {
        set(&section, field, &JsValue::from_f64(dom::now_ms()));
    }
}

pub fn flag(section: &str, field: &str, value: bool) {
    if let Some(section) = section_object(section) {
        set(&section, field, &JsValue::from_bool(value));
    }
}

fn enabled() -> bool {
    dom::window()
        .and_then(|window| window.location().search().ok())
        .is_some_and(|search| {
            search
                .trim_start_matches('?')
                .split('&')
                .any(|part| part == "debug=telemetry")
        })
}

fn root() -> Option<JsValue> {
    js_sys::Reflect::get(&js_sys::global(), &GLOBAL.into())
        .ok()
        .filter(|value| !value.is_null() && !value.is_undefined())
}

fn section_object(name: &str) -> Option<JsValue> {
    js_sys::Reflect::get(&root()?, &name.into()).ok()
}

fn set(target: &JsValue, field: &str, value: &JsValue) {
    let _ = js_sys::Reflect::set(target, &field.into(), value);
}
