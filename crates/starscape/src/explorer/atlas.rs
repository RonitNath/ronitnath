//! The open atlas: what it is looking at, and what it reports about that.
//!
//! One owner for the view, the clock and the surface. [`super::frame`] drives
//! it and [`super::input`] steers it; nothing else may hold a handle, because
//! the whole of "the atlas is showing the sky it claims to be showing" rests on
//! there being exactly one of these.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use wasm_bindgen::{JsCast, JsValue};
use web_sys::Element;

use crate::app::App;
use crate::explorer::camera::Camera;
use crate::explorer::lod::Star;
use crate::explorer::paint::Surface;
use crate::explorer::shell;
use crate::explorer::tiles::Tiles;
use crate::{dom, telemetry};

pub struct Atlas {
    pub(super) app: Rc<App>,
    pub(super) node: Element,
    pub(super) surface: RefCell<Surface>,
    pub(super) camera: Cell<Camera>,
    /// The naked-eye sky, always drawn.
    pub(super) base: Vec<Star>,
    pub(super) tiles: Rc<Tiles>,
    pub(super) sim_ms: Cell<f64>,
    pub(super) initial_sim_ms: f64,
    pub(super) initial_forward: [f64; 3],
    pub(super) last_frame: Cell<f64>,
    pub(super) last_paint: Cell<f64>,
    pub(super) paused: Cell<bool>,
    pub(super) reduced: bool,
    pub(super) target: String,
    /// Whether the view is still on what it was opened on. Panning gives that
    /// up, and the readout stops naming a direction the view has left.
    pub(super) aimed: Cell<bool>,
    pub(super) visible: Cell<usize>,
    pub(super) first_frame_at: Cell<f64>,
    pub(super) closed: Cell<bool>,
    pub(super) pointers: RefCell<Vec<(i32, f64, f64)>>,
    pub(super) pinch: Cell<Option<(f64, f64)>>,
}

impl Atlas {
    pub(super) fn focus(&self) {
        if let Some(node) = self.node.dyn_ref::<web_sys::HtmlElement>() {
            let _ = node.focus();
        }
    }

    /// Match the drawing surface to the viewport.
    pub fn resize(&self) {
        let Some(window) = dom::window() else { return };
        let width = window.inner_width().ok().and_then(|v| v.as_f64());
        let height = window.inner_height().ok().and_then(|v| v.as_f64());
        let (Some(width), Some(height)) = (width, height) else {
            return;
        };
        self.surface
            .borrow_mut()
            .resize(width, height, window.device_pixel_ratio());
    }

    pub fn camera(&self) -> Camera {
        self.camera.get()
    }

    pub fn paused(&self) -> bool {
        self.paused.get()
    }

    /// The view has been moved off what it was opened on.
    pub fn release_aim(&self) {
        self.aimed.set(false);
    }

    /// Change the view and refresh everything that reports it.
    pub fn steer(&self, change: impl FnOnce(&mut Camera)) {
        let mut camera = self.camera.get();
        change(&mut camera);
        self.camera.set(camera);
        let target = if self.aimed.get() {
            self.target.as_str()
        } else {
            ""
        };
        shell::set_readouts(&self.node, target, camera.fov);
        telemetry::number("explorer", "fov", camera.fov);
        telemetry::number("explorer", "rate", camera.rate());
        self.tiles.request(&camera, self.aspect());
    }

    pub fn toggle_pause(&self) {
        let paused = !self.paused.get();
        self.paused.set(paused);
        shell::set_pause_label(&self.node, paused);
        telemetry::flag("explorer", "paused", paused);
    }

    pub(super) fn aspect(&self) -> f64 {
        let (width, height) = self.surface.borrow().css_size();
        if height > 0.0 { width / height } else { 1.6 }
    }

    /// What the interop surface reports: enough to assert the atlas is showing
    /// the sky it claims to be showing, and nothing about how it draws it.
    pub(super) fn report(&self) -> JsValue {
        let camera = self.camera.get();
        let report = js_sys::Object::new();
        let set = |name: &str, value: JsValue| {
            let _ = js_sys::Reflect::set(&report, &name.into(), &value);
        };
        set("open", JsValue::TRUE);
        set("paused", JsValue::from_bool(self.paused.get()));
        set("simMs", JsValue::from_f64(self.sim_ms.get()));
        set("initialSimMs", JsValue::from_f64(self.initial_sim_ms));
        set("fov", JsValue::from_f64(camera.fov));
        set("rate", JsValue::from_f64(camera.rate()));
        set("ra", JsValue::from_f64(camera.ra));
        set("dec", JsValue::from_f64(camera.dec));
        set("tracking", JsValue::from_bool(camera.tracking));
        set("target", JsValue::from_str(&self.target));
        set("visibleStars", JsValue::from_f64(self.visible.get() as f64));
        set(
            "firstFrameAtMs",
            JsValue::from_f64(self.first_frame_at.get()),
        );
        set(
            "initialForward",
            self.initial_forward
                .iter()
                .map(|value| JsValue::from_f64(*value))
                .collect::<js_sys::Array>()
                .into(),
        );
        report.into()
    }
}
