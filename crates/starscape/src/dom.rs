//! The small set of browser calls the rest of the bundle needs, in one place.
//!
//! Every one of these is a `web-sys` call whose failure mode is "the page is
//! not a page": no window, no document, no element. They return `Option` and
//! the callers treat absence as "there is no sky here", which is what makes the
//! bundle safe to load on a page that does not host it.

use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{Document, Element, HtmlCanvasElement, Response, Window};

#[must_use]
pub fn window() -> Option<Window> {
    web_sys::window()
}

#[must_use]
pub fn document() -> Option<Document> {
    window()?.document()
}

#[must_use]
pub fn element(id: &str) -> Option<Element> {
    document()?.get_element_by_id(id)
}

#[must_use]
pub fn canvas(id: &str) -> Option<HtmlCanvasElement> {
    element(id)?.dyn_into::<HtmlCanvasElement>().ok()
}

/// Whether the visitor has asked the platform for reduced motion.
#[must_use]
pub fn prefers_reduced_motion() -> Option<web_sys::MediaQueryList> {
    window()?
        .match_media("(prefers-reduced-motion: reduce)")
        .ok()
        .flatten()
}

/// The document's current theme attribute. The sky is painted differently
/// against night and against dusk, so this is a render input, not decoration.
#[must_use]
pub fn is_light_theme() -> bool {
    document()
        .and_then(|document| document.document_element())
        .and_then(|root| root.get_attribute("data-theme"))
        .as_deref()
        == Some("light")
}

#[must_use]
pub fn is_visible() -> bool {
    document()
        .is_none_or(|document| document.visibility_state() == web_sys::VisibilityState::Visible)
}

pub fn now_ms() -> f64 {
    js_sys::Date::now()
}

/// Add or remove a class on `<html>` — how the CSS learns that the real sky is
/// live and the fallback starfield can go.
pub fn set_root_class(name: &str, present: bool) {
    let Some(root) = document().and_then(|document| document.document_element()) else {
        return;
    };
    let list = root.class_list();
    let _ = if present {
        list.add_1(name)
    } else {
        list.remove_1(name)
    };
}

#[must_use]
pub fn has_root_class(name: &str) -> bool {
    document()
        .and_then(|document| document.document_element())
        .is_some_and(|root| root.class_list().contains(name))
}

/// Fetch a whole asset as bytes.
pub async fn fetch_bytes(url: &str) -> Result<Vec<u8>, JsValue> {
    let response = fetch(url).await?;
    let buffer = JsFuture::from(response.array_buffer()?).await?;
    Ok(js_sys::Uint8Array::new(&buffer).to_vec())
}

pub async fn fetch_text(url: &str) -> Result<String, JsValue> {
    let response = fetch(url).await?;
    JsFuture::from(response.text()?)
        .await?
        .as_string()
        .ok_or_else(|| JsValue::from_str("response body was not text"))
}

pub async fn fetch(url: &str) -> Result<Response, JsValue> {
    let window = window().ok_or_else(|| JsValue::from_str("no window"))?;
    let response: Response = JsFuture::from(window.fetch_with_str(url))
        .await?
        .dyn_into()?;
    if response.ok() {
        Ok(response)
    } else {
        Err(JsValue::from_str(&format!(
            "{url} responded {}",
            response.status()
        )))
    }
}

/// Schedule a callback for the next frame. The returned unit is deliberate:
/// nothing here cancels a frame, it checks a generation counter instead, so a
/// stale frame retires itself rather than being chased.
pub fn on_next_frame(task: impl FnOnce() + 'static) {
    let Some(window) = window() else { return };
    let closure = Closure::once_into_js(task);
    let _ = window.request_animation_frame(closure.unchecked_ref());
}

/// Run `task` after the browser is idle, or after `timeout_ms` at the latest.
pub fn when_idle(timeout_ms: f64, task: impl FnOnce() + 'static) {
    let Some(window) = window() else { return };
    let closure = Closure::once_into_js(task);
    let global = js_sys::global();
    let idle = js_sys::Reflect::get(&global, &JsValue::from_str("requestIdleCallback")).ok();
    match idle.and_then(|value| value.dyn_into::<js_sys::Function>().ok()) {
        Some(idle) => {
            let options = js_sys::Object::new();
            let _ = js_sys::Reflect::set(
                &options,
                &JsValue::from_str("timeout"),
                &JsValue::from_f64(timeout_ms),
            );
            let _ = idle.call2(&global, &closure, &options);
        }
        None => {
            let _ = window
                .set_timeout_with_callback_and_timeout_and_arguments_0(closure.unchecked_ref(), 0);
        }
    }
}

/// Listen for an event forever. The closure is leaked on purpose: the bundle
/// lives as long as the document, and a handle nobody can drop is one more
/// thing to hold correctly for no benefit.
pub fn listen<T: JsCast + 'static>(
    target: &web_sys::EventTarget,
    event: &str,
    mut handler: impl FnMut(T) + 'static,
) {
    let closure = Closure::<dyn FnMut(JsValue)>::new(move |value: JsValue| {
        if let Ok(event) = value.dyn_into::<T>() {
            handler(event);
        }
    });
    let _ = target.add_event_listener_with_callback(event, closure.as_ref().unchecked_ref());
    closure.forget();
}

pub fn set_interval(interval_ms: i32, task: impl FnMut() + 'static) {
    let Some(window) = window() else { return };
    let closure = Closure::<dyn FnMut()>::new(task);
    let _ = window.set_interval_with_callback_and_timeout_and_arguments_0(
        closure.as_ref().unchecked_ref(),
        interval_ms,
    );
    closure.forget();
}

pub fn warn(message: &str, detail: &JsValue) {
    web_sys::console::warn_2(&JsValue::from_str(message), detail);
}
