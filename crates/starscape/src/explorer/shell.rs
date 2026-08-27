//! The atlas's chrome: the dialog it lives in, and the launcher that opens it.
//!
//! The markup is built here rather than sitting in the askama landing because
//! a dialog that only exists once someone asks for it cannot be a hidden
//! element in the document — a hidden dialog is one more thing for a screen
//! reader to walk past and for a stylesheet to have to hide correctly.

use wasm_bindgen::JsValue;
use web_sys::{Element, HtmlCanvasElement};

use crate::dom;

/// The class the page wears while the atlas is open. Stylesheet-visible, so the
/// landing's own chrome can stand down without this module knowing about it.
pub const OPEN_CLASS: &str = "starscape-explorer-open";

/// The launcher's id in the landing template, and the states it reports.
pub const LAUNCH_ID: &str = "starscape-launch";

/// Build the dialog and put it in the document. Returns the dialog and its
/// canvas.
pub fn mount() -> Result<(Element, HtmlCanvasElement), JsValue> {
    let document = dom::document().ok_or_else(|| JsValue::from_str("no document"))?;
    let node = document.create_element("section")?;
    node.set_class_name("starscape-explorer");
    node.set_attribute("role", "dialog")?;
    node.set_attribute("aria-modal", "true")?;
    node.set_attribute("aria-label", "Celestial atlas")?;
    node.set_attribute("tabindex", "-1")?;
    node.set_inner_html(MARKUP);

    let body = document
        .body()
        .ok_or_else(|| JsValue::from_str("no body to open the atlas in"))?;
    body.append_child(&node)?;
    dom::set_root_class(OPEN_CLASS, true);

    let canvas = node
        .query_selector("canvas")?
        .and_then(|element| wasm_bindgen::JsCast::dyn_into::<HtmlCanvasElement>(element).ok())
        .ok_or_else(|| JsValue::from_str("the atlas has no canvas"))?;
    Ok((node, canvas))
}

/// Take the dialog back out of the document.
pub fn unmount(node: &Element) {
    node.remove();
    dom::set_root_class(OPEN_CLASS, false);
}

/// The launcher's state: `idle` (openable), `opening`, `open`.
pub fn set_launch_state(state: &str) {
    let Some(button) = dom::element(LAUNCH_ID) else {
        return;
    };
    let _ = button.set_attribute("data-state", state);
    let label = match state {
        "opening" => "Opening",
        _ => "Open atlas",
    };
    button.set_text_content(Some(label));
    let _ = if state == "idle" {
        button.remove_attribute("disabled")
    } else {
        button.set_attribute("disabled", "")
    };
    // While the atlas is open the launcher is behind it; hiding it keeps it out
    // of the tab order rather than leaving a disabled control in the way.
    let _ = if state == "open" {
        button.set_attribute("hidden", "")
    } else {
        button.remove_attribute("hidden")
    };
}

/// Update the readouts the bar carries: what is being tracked, and the field.
pub fn set_readouts(node: &Element, target: &str, fov: f64) {
    if let Ok(Some(element)) = node.query_selector(".atlas-target") {
        element.set_text_content(Some(target));
    }
    if let Ok(Some(element)) = node.query_selector(".atlas-fov") {
        let digits = usize::from(fov < 10.0);
        element.set_text_content(Some(&format!("{fov:.digits$}°")));
    }
}

pub fn set_pause_label(node: &Element, paused: bool) {
    if let Ok(Some(button)) = node.query_selector("[data-action=pause]") {
        button.set_text_content(Some(if paused { "Resume" } else { "Pause" }));
    }
}

/// The dialog's contents. The identity and its links are repeated here because
/// the atlas covers the page that carries them: a visitor who opens the sky
/// must not have to close it to find out whose sky it is.
const MARKUP: &str = r#"<canvas class="atlas-canvas" aria-label="Star map"></canvas>
<header class="atlas-bar">
  <strong>Celestial atlas</strong>
  <span class="atlas-target"></span>
  <span class="atlas-spacer"></span>
  <button type="button" data-action="zoom-out" aria-label="Zoom out">&minus;</button>
  <output class="atlas-fov">115&deg;</output>
  <button type="button" data-action="zoom-in" aria-label="Zoom in">+</button>
  <button type="button" data-action="pause">Pause</button>
  <button type="button" data-action="close" aria-label="Close the atlas">Close</button>
</header>
<nav class="atlas-links" aria-label="Ronit Nath">
  <strong>Ronit Nath</strong>
  <a href="https://isoastra.com" rel="noopener" target="_blank">Isoastra</a>
  <a href="https://github.com/RonitNath" rel="me noopener" target="_blank">GitHub</a>
  <a href="https://instagram.com/ronit_nath" rel="me noopener" target="_blank">Instagram</a>
  <a href="https://linkedin.com/in/ronitn" rel="me noopener" target="_blank">LinkedIn</a>
  <a href="mailto:ronit@isoastra.com">Email</a>
</nav>
<p class="atlas-keys">Drag to pan &middot; scroll to zoom &middot; arrows pan &middot; space pauses</p>"#;
