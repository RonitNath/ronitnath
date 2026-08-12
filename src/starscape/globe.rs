//! Deferred mini-globe island — loads globe.gl only after starscape is live.

use leptos::prelude::*;

#[island]
pub fn MiniGlobe(epoch_ms: f64) -> impl IntoView {
    let node_ref = NodeRef::<leptos::html::Div>::new();

    Effect::new(move |_| {
        #[cfg(feature = "hydrate")]
        if let Some(el) = node_ref.get() {
            leptos::task::spawn_local(async move {
                if let Err(error) = start_mini_globe(el, epoch_ms).await {
                    web_sys::console::warn_2(&"mini-globe unavailable:".into(), &error);
                }
            });
        }
        #[cfg(not(feature = "hydrate"))]
        let _ = epoch_ms;
    });

    view! { <div class="mini-globe" node_ref=node_ref aria-hidden="true"></div> }
}

#[cfg(feature = "hydrate")]
async fn start_mini_globe(
    el: web_sys::HtmlDivElement,
    epoch_ms: f64,
) -> Result<(), wasm_bindgen::JsValue> {
    wait_for_starscape_or_timeout(3000).await;
    idle_yield(2000.0).await;
    load_and_mount(&el, epoch_ms).await
}

#[cfg(feature = "hydrate")]
async fn wait_for_starscape_or_timeout(timeout_ms: i32) {
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;

    let start = js_sys::Date::now();
    loop {
        if let Some(root) = document().document_element() {
            if root.class_list().contains("starscape-active") {
                return;
            }
        }
        if js_sys::Date::now() - start > f64::from(timeout_ms) {
            return;
        }
        let promise = js_sys::Promise::new(&mut |resolve, _reject| {
            let cb = wasm_bindgen::closure::Closure::<dyn FnMut()>::once(move || {
                let _ = resolve.call0(&wasm_bindgen::JsValue::UNDEFINED);
            });
            let _ = window().set_timeout_with_callback_and_timeout_and_arguments_0(
                cb.as_ref().unchecked_ref(),
                100,
            );
            cb.forget();
        });
        let _ = JsFuture::from(promise).await;
    }
}

#[cfg(feature = "hydrate")]
async fn idle_yield(timeout_ms: f64) {
    use wasm_bindgen::{JsCast, JsValue};
    use wasm_bindgen_futures::JsFuture;

    let promise = js_sys::Promise::new(&mut |resolve, _reject| {
        let cb = wasm_bindgen::closure::Closure::<dyn FnMut()>::once(move || {
            let _ = resolve.call0(&JsValue::UNDEFINED);
        });
        if js_sys::Reflect::has(&js_sys::global(), &JsValue::from_str("requestIdleCallback"))
            .unwrap_or(false)
        {
            let opts = js_sys::Object::new();
            let _ = js_sys::Reflect::set(
                &opts,
                &JsValue::from_str("timeout"),
                &JsValue::from_f64(timeout_ms),
            );
            let idle: js_sys::Function =
                js_sys::Reflect::get(&js_sys::global(), &JsValue::from_str("requestIdleCallback"))
                    .unwrap()
                    .unchecked_into();
            let _ = idle.call2(&js_sys::global(), cb.as_ref(), &opts);
        } else {
            let _ = window().set_timeout_with_callback_and_timeout_and_arguments_0(
                cb.as_ref().unchecked_ref(),
                0,
            );
        }
        cb.forget();
    });
    let _ = JsFuture::from(promise).await;
}

#[cfg(feature = "hydrate")]
async fn load_and_mount(
    el: &web_sys::HtmlDivElement,
    epoch_ms: f64,
) -> Result<(), wasm_bindgen::JsValue> {
    use wasm_bindgen::{JsCast, JsValue};
    use wasm_bindgen_futures::JsFuture;

    // Dynamically import the ES module (never preloaded). The origin marks
    // stable asset URLs `no-cache`, so the browser revalidates this module.
    let importer = js_sys::Function::new_with_args("u", "return import(u);");
    let url = "/js/mini-globe.js";
    let module = JsFuture::from(js_sys::Promise::resolve(
        &importer.call1(&js_sys::global(), &JsValue::from_str(url))?,
    ))
    .await?;

    let mount = js_sys::Reflect::get(&module, &JsValue::from_str("mountMiniGlobe"))?;
    if !mount.is_function() {
        return Err(JsValue::from_str("mountMiniGlobe missing"));
    }
    let mount_fn: js_sys::Function = mount.unchecked_into();
    // mountMiniGlobe is async — await its Promise so failures surface.
    let result = mount_fn.call2(&JsValue::NULL, el.as_ref(), &JsValue::from_f64(epoch_ms))?;
    JsFuture::from(js_sys::Promise::resolve(&result)).await?;
    Ok(())
}
