pub mod app;
#[cfg(feature = "ssr")]
pub mod auth;
#[cfg(feature = "ssr")]
pub mod operations;
pub mod starscape;

#[cfg(feature = "hydrate")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    // Keep app in the wasm graph so island bindings are generated correctly.
    #[allow(unused_imports)]
    use crate::app::*;
    #[allow(unused_imports)]
    use crate::starscape::*;
    console_error_panic_hook::set_once();
    leptos::mount::hydrate_islands();
}
