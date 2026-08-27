//! Wiring: one owner for the clock, the observer and the two canvases.
//!
//! Everything the bundle draws is a function of (server time, the observer).
//! Holding both here is what keeps the sky, the globe's marker and the
//! grounding readout from disagreeing — the failure the old split between a
//! wasm island and two JS modules kept producing.
//!
//! Nothing here is required for the page to be a page. Every mount is
//! best-effort: no canvas, no WebGL, a failed fetch or reduced motion each
//! leave the CSS starfield as the picture and the rest of the document intact.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use wasm_bindgen::JsValue;

use crate::annotate::Placement;
use crate::catalog::{NamedCatalog, NamedStar, position};
use crate::cities::CityCatalog;
use crate::globe::Globe;
use crate::observer::{Observer, TRANSITION_MS};
use crate::render::Scene;
use crate::sky::{sim_time_ms, synced_sim_time_ms, view_matrix};
use crate::{annotate, callouts, dom, label, telemetry, track};

const BRIGHT_URL: &str = "/static/stars/bright.bin";
const NAMED_URL: &str = "/static/stars/named.json";
const CITIES_URL: &str = "/static/cities/cities.bin";
const GALAXY_URL: &str = "/static/sky/milkyway.webp";
const EARTH_URL: &str = "/static/textures/earth/day.jpg";

/// The class the CSS watches to swap the fallback starfield for the canvas.
const ACTIVE_CLASS: &str = "starscape-active";

pub struct App {
    server_epoch_ms: f64,
    client_mount_ms: f64,
    observer: RefCell<Observer>,
    reduced: Cell<bool>,
    visible: Cell<bool>,
    /// The instant the sky is frozen at while motion is reduced.
    frozen_sim_ms: Cell<Option<f64>>,
    running: Cell<bool>,
    generation: Cell<u32>,
    // `Rc` so a loader can take a handle and release the borrow before it
    // awaits: the frame loop draws from the same cell.
    sky: RefCell<Option<Rc<Scene>>>,
    globe: RefCell<Option<Rc<Globe>>>,
    named: RefCell<Vec<NamedStar>>,
    named_vectors: RefCell<Vec<[f64; 3]>>,
    cities: RefCell<Option<CityCatalog>>,
}

impl App {
    #[must_use]
    pub fn new(server_epoch_ms: f64) -> Rc<Self> {
        let reduced = dom::prefers_reduced_motion().is_some_and(|query| query.matches());
        Rc::new(Self {
            server_epoch_ms,
            client_mount_ms: dom::now_ms(),
            observer: RefCell::new(Observer::default()),
            reduced: Cell::new(reduced),
            visible: Cell::new(dom::is_visible()),
            frozen_sim_ms: Cell::new(reduced.then(|| sim_time_ms(server_epoch_ms))),
            running: Cell::new(false),
            generation: Cell::new(0),
            sky: RefCell::new(None),
            globe: RefCell::new(None),
            named: RefCell::new(Vec::new()),
            named_vectors: RefCell::new(Vec::new()),
            cities: RefCell::new(None),
        })
    }

    /// The simulation instant on screen right now.
    #[must_use]
    pub fn sim_ms(&self) -> f64 {
        self.frozen_sim_ms.get().unwrap_or_else(|| {
            synced_sim_time_ms(self.server_epoch_ms, self.client_mount_ms, dom::now_ms())
        })
    }

    /// Where the view is from right now, resolving any move in flight.
    #[must_use]
    pub fn observer_now(&self) -> (f64, f64) {
        self.observer
            .borrow_mut()
            .resolve(self.sim_ms(), dom::now_ms())
    }

    #[must_use]
    pub fn manual_observer(&self) -> Option<(f64, f64)> {
        self.observer.borrow().manual()
    }

    /// A hand-driven move. `travel` is false for a drag, where the point is
    /// that the globe follows the hand rather than chasing it.
    pub fn set_observer(&self, lat: f64, lon: f64, travel: bool) {
        let duration = if travel && !self.reduced.get() {
            TRANSITION_MS
        } else {
            0.0
        };
        self.observer
            .borrow_mut()
            .set(lat, lon, self.sim_ms(), dom::now_ms(), duration);
    }

    pub fn resume_orbit(&self) {
        let duration = if self.reduced.get() {
            0.0
        } else {
            TRANSITION_MS
        };
        self.observer
            .borrow_mut()
            .resume(self.sim_ms(), dom::now_ms(), duration);
    }

    /// Paint both canvases. Returns whether the sky actually drew a frame.
    pub fn draw(&self) -> bool {
        let sim_ms = self.sim_ms();
        let (lat, lon) = self.observer_now();
        if let Some(globe) = self.globe.borrow().as_ref() {
            globe.draw(lat, lon);
        }
        let Some(sky) = self
            .sky
            .borrow()
            .as_ref()
            .map(|scene| scene.draw(sim_ms, lat, lon))
        else {
            return false;
        };
        if sky && self.star_count() > 0 && !dom::has_root_class(ACTIVE_CLASS) {
            dom::set_root_class(ACTIVE_CLASS, true);
            telemetry::mark("stars", "revealedAtMs");
            telemetry::event("first-star-batch-revealed");
        }
        sky
    }

    /// A handle to the sky that outlives the borrow, so a loader can await.
    fn scene(&self) -> Result<Rc<Scene>, JsValue> {
        self.sky
            .borrow()
            .clone()
            .ok_or_else(|| JsValue::from_str("there is no sky mounted"))
    }

    fn star_count(&self) -> i32 {
        self.sky
            .borrow()
            .as_ref()
            .map_or(0, |scene| scene.star_count())
    }

    /// Refresh the grounding line and the star labels.
    pub fn draw_readouts(&self) {
        let (lat, lon) = self.observer_now();
        if let Some(element) = dom::element("grounding") {
            let cities = self.cities.borrow();
            let nearest = cities
                .as_ref()
                .and_then(|catalog| catalog.nearest(lat, lon));
            element.set_text_content(Some(&label::grounding(lat, lon, nearest.as_ref())));
        }
        if let Some(container) = dom::element("star-annotations") {
            let placements = self.placements(lat, lon);
            callouts::render(&container, &self.named.borrow(), &placements);
            telemetry::number("annotations", "labels", placements.len() as f64);
        }
        if let Some(button) = dom::element("resume-orbit") {
            let _ = if self.manual_observer().is_some() {
                button.remove_attribute("hidden")
            } else {
                button.set_attribute("hidden", "")
            };
        }
    }

    fn placements(&self, lat: f64, lon: f64) -> Vec<Placement> {
        let vectors = self.named_vectors.borrow();
        if vectors.is_empty() {
            return Vec::new();
        }
        let aspect = dom::window()
            .and_then(|window| {
                let width = window.inner_width().ok()?.as_f64()?;
                let height = window.inner_height().ok()?.as_f64()?;
                (height > 0.0).then(|| width / height)
            })
            .unwrap_or(1.6);
        annotate::place(
            &vectors,
            view_matrix(self.sim_ms(), lat, lon),
            aspect,
            callouts::MAX_LABELS,
        )
    }

    /// Run the frame loop while the tab is visible and motion is not reduced.
    pub fn ensure_animation(self: &Rc<Self>) {
        if !self.visible.get() || self.reduced.get() || self.running.replace(true) {
            return;
        }
        telemetry::flag("globe", "paused", false);
        let generation = self.generation.get();
        let app = Rc::clone(self);
        dom::on_next_frame(move || app.frame(generation));
    }

    pub fn stop_animation(&self) {
        self.running.set(false);
        self.generation.set(self.generation.get().wrapping_add(1));
        telemetry::flag("globe", "paused", true);
    }

    fn frame(self: Rc<Self>, generation: u32) {
        if !self.running.get() || self.generation.get() != generation {
            return;
        }
        if !self.visible.get() || self.reduced.get() {
            self.stop_animation();
            return;
        }
        self.draw();
        telemetry::increment("sky", "animationFrames", 1.0);
        let app = Rc::clone(&self);
        dom::on_next_frame(move || app.frame(generation));
    }

    /// Draw once, retrying on the next frame until the canvas has a size.
    /// Used when the loop is not running — reduced motion, or a hidden tab that
    /// still owes the page a first picture.
    pub fn redraw_until_drawn(self: &Rc<Self>) {
        if self.draw() {
            self.draw_readouts();
            return;
        }
        let app = Rc::clone(self);
        dom::on_next_frame(move || app.redraw_until_drawn());
    }

    pub fn set_visible(self: &Rc<Self>, visible: bool) {
        self.visible.set(visible);
        if visible {
            if let Some(sky) = self.sky.borrow().as_ref() {
                sky.start_galaxy_fade_if_pending();
            }
            self.ensure_animation();
        } else {
            self.stop_animation();
        }
    }

    pub fn set_reduced_motion(self: &Rc<Self>, reduced: bool) {
        self.reduced.set(reduced);
        if reduced {
            self.frozen_sim_ms.set(Some(self.sim_ms()));
            self.stop_animation();
            self.redraw_until_drawn();
        } else {
            self.frozen_sim_ms.set(None);
            self.ensure_animation();
        }
    }

    #[must_use]
    pub fn reduced_motion(&self) -> bool {
        self.reduced.get()
    }

    pub fn attach_sky(&self, scene: Scene) {
        self.sky.replace(Some(Rc::new(scene)));
    }

    pub fn attach_globe(&self, globe: Globe) {
        self.globe.replace(Some(Rc::new(globe)));
    }

    pub fn detach_sky(&self) {
        self.sky.replace(None);
        dom::set_root_class(ACTIVE_CLASS, false);
    }

    /// Stream the star catalog into the GPU, revealing on the first batch.
    pub async fn load_stars(self: &Rc<Self>) -> Result<Vec<u8>, JsValue> {
        let app = Rc::clone(self);
        let scene = self.scene()?;
        scene
            .stream_stars(BRIGHT_URL, || {
                app.draw();
            })
            .await
    }

    pub async fn load_named(&self, bright: &[u8]) -> Result<(), JsValue> {
        let text = dom::fetch_text(NAMED_URL).await?;
        let catalog = NamedCatalog::parse(&text)
            .map_err(|error| JsValue::from_str(&format!("named.json: {error}")))?;
        let vectors: Vec<[f64; 3]> = catalog
            .stars
            .iter()
            .filter_map(|star| position(bright, star.bright_index))
            .collect();
        // A catalog whose indices do not all resolve is a mismatched pair of
        // assets, and labelling the wrong stars is worse than labelling none.
        if vectors.len() != catalog.stars.len() {
            return Err(JsValue::from_str(
                "named.json points outside the star catalog",
            ));
        }
        self.named_vectors.replace(vectors);
        self.named.replace(catalog.stars);
        telemetry::flag("annotations", "ready", true);
        Ok(())
    }

    pub async fn load_galaxy(&self) -> Result<(), JsValue> {
        let immediate = self.reduced.get();
        self.scene()?.load_galaxy(GALAXY_URL, immediate).await
    }

    pub async fn load_cities(&self) -> Result<(), JsValue> {
        let bytes = dom::fetch_bytes(CITIES_URL).await?;
        let catalog =
            CityCatalog::parse(bytes).ok_or_else(|| JsValue::from_str("invalid city catalog"))?;
        telemetry::number("globe", "cities", catalog.len() as f64);
        self.cities.replace(Some(catalog));
        Ok(())
    }

    pub async fn load_earth_texture(&self) -> Result<(), JsValue> {
        let globe = self
            .globe
            .borrow()
            .clone()
            .ok_or_else(|| JsValue::from_str("no globe to texture"))?;
        globe.load_texture(EARTH_URL).await
    }

    /// Which point on Earth a pointer event landed on, if it hit the globe.
    #[must_use]
    pub fn globe_point_at(&self, client_x: f64, client_y: f64) -> Option<(f64, f64)> {
        let (lat, lon) = self.observer_now();
        self.globe
            .borrow()
            .as_ref()
            .and_then(|globe| globe.point_at(client_x, client_y, lat, lon))
    }

    /// Apply a drag of `(dx, dy)` pixels from the current viewpoint.
    pub fn drag(&self, dx: f64, dy: f64) {
        let (lat, lon) = self.observer_now();
        let (lat, lon) = crate::globe::drag_to(lat, lon, dx, dy);
        self.set_observer(lat, lon, false);
    }

    /// The canonical orbit position, ignoring any manual override. Exposed for
    /// the interop surface so a caller can ask where the shared sky is.
    #[must_use]
    pub fn orbit_at(&self, sim_ms: f64) -> (f64, f64) {
        track::observer_at(sim_ms)
    }
}
