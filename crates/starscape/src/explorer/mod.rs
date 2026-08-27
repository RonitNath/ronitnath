//! The deep-zoom celestial atlas: the landing's sky, opened up.
//!
//! The landing draws one view — straight up, from wherever the observer is —
//! and it draws it with the twelve thousand stars the eye can see. The atlas is
//! the other half: a view that can be aimed anywhere and narrowed to a degree,
//! backed by a three-million-star Gaia catalog fetched a sky tile at a time.
//!
//! It opens on what the visitor was already looking at — the zenith, or the
//! star whose label they clicked — so it continues the sky rather than
//! replacing it, and it hands the page back exactly as it found it.
//!
//! This lives in the same wasm bundle as the sky. The old site loaded it as a
//! separate JavaScript module on demand, which bought a smaller first paint at
//! the price of a second renderer with its own copy of the projection and its
//! own opinion about what time it was.

mod atlas;
pub mod camera;
mod frame;
mod input;
pub mod lod;
pub mod paint;
mod shell;
pub mod tiles;

pub use atlas::Atlas;
pub use shell::LAUNCH_ID;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::spawn_local;

use crate::app::App;
use crate::catalog;
use crate::explorer::camera::Camera;
use crate::explorer::lod::Star;
use crate::explorer::paint::Surface;
use crate::explorer::tiles::Tiles;
use crate::{dom, telemetry};

const BRIGHT_URL: &str = "/static/stars/bright.bin";

/// The field the atlas opens at: the whole sky, or a close look at one star.
const SKY_FOV_DEG: f64 = 115.0;
const STAR_FOV_DEG: f64 = 20.0;

/// What asked for the atlas.
pub enum Launch {
    /// The launcher: open on the observer's zenith, wide.
    Sky,
    /// A star label: open tracking that star, close in.
    Star { name: String, position: [f64; 3] },
}

thread_local! {
    static ACTIVE: RefCell<Option<Rc<Atlas>>> = const { RefCell::new(None) };
    static OPENING: Cell<bool> = const { Cell::new(false) };
    /// The bright catalog, kept after the sky uploads it. 244 KB against a
    /// second fetch — and against the atlas opening on an empty frame.
    static CATALOG: RefCell<Option<Rc<Vec<u8>>>> = const { RefCell::new(None) };
}

/// Keep the bytes the sky already fetched, so opening the atlas costs nothing.
pub fn retain_catalog(bytes: Vec<u8>) {
    CATALOG.with(|catalog| catalog.replace(Some(Rc::new(bytes))));
}

#[must_use]
pub fn is_open() -> bool {
    ACTIVE.with(|active| active.borrow().is_some())
}

/// Open the atlas. A second request while one is in flight, or while the atlas
/// is already open, is dropped: there is one sky and one dialog over it.
pub fn open(app: &Rc<App>, launch: Launch) {
    if is_open() || OPENING.with(|opening| opening.replace(true)) {
        return;
    }
    shell::set_launch_state("opening");
    telemetry::event("atlas-requested");
    let app = Rc::clone(app);
    spawn_local(async move {
        if let Err(error) = raise(&app, launch).await {
            dom::warn("the atlas could not open:", &error);
            shell::set_launch_state("idle");
        }
        OPENING.with(|opening| opening.set(false));
    });
}

/// Close it, and put the page back the way it was.
pub fn close() {
    let Some(atlas) = ACTIVE.with(|active| active.borrow_mut().take()) else {
        return;
    };
    atlas.closed.set(true);
    atlas.tiles.clear();
    shell::unmount(&atlas.node);
    shell::set_launch_state("idle");
    telemetry::flag("sky", "pausedForExplorer", false);
    telemetry::event("atlas-closed");

    // The observer the visitor picked before opening the atlas is theirs; the
    // atlas gives the page back, it does not reset it.
    if atlas.app.reduced_motion() {
        atlas.app.redraw_until_drawn();
    } else {
        atlas.app.ensure_animation();
    }
    atlas.app.draw_readouts();
    if let Some(button) = dom::element(shell::LAUNCH_ID)
        && let Some(button) = button.dyn_ref::<web_sys::HtmlElement>()
    {
        let _ = button.focus();
    }
}

/// The atlas's state, for the interop surface. `null` when it is shut.
#[must_use]
pub fn state() -> JsValue {
    ACTIVE.with(|active| {
        active
            .borrow()
            .as_ref()
            .map_or(JsValue::NULL, |atlas| atlas.report())
    })
}

async fn raise(app: &Rc<App>, launch: Launch) -> Result<(), JsValue> {
    let bytes = match CATALOG.with(|catalog| catalog.borrow().clone()) {
        Some(bytes) => bytes,
        None => {
            let fetched = Rc::new(dom::fetch_bytes(BRIGHT_URL).await?);
            CATALOG.with(|catalog| catalog.replace(Some(Rc::clone(&fetched))));
            fetched
        }
    };
    let base = base_stars(&bytes).map_err(JsValue::from_str)?;

    let (node, canvas) = shell::mount()?;
    let surface = Surface::create(canvas)?;
    let (target, camera) = aim(app, launch);

    let atlas = Rc::new(Atlas {
        app: Rc::clone(app),
        node,
        surface: RefCell::new(surface),
        camera: Cell::new(camera),
        base,
        tiles: Tiles::new(),
        sim_ms: Cell::new(app.sim_ms()),
        initial_sim_ms: app.sim_ms(),
        initial_forward: camera.forward(),
        last_frame: Cell::new(dom::now_ms()),
        last_paint: Cell::new(0.0),
        paused: Cell::new(app.reduced_motion()),
        reduced: app.reduced_motion(),
        target,
        aimed: Cell::new(true),
        visible: Cell::new(0),
        first_frame_at: Cell::new(0.0),
        closed: Cell::new(false),
        pointers: RefCell::new(Vec::new()),
        pinch: Cell::new(None),
    });

    // The sky behind the atlas is covered; leaving its loop running would spend
    // a GPU on a picture nobody can see.
    app.stop_animation();
    telemetry::flag("sky", "pausedForExplorer", true);

    shell::set_readouts(&atlas.node, &atlas.target, camera.fov);
    shell::set_pause_label(&atlas.node, atlas.paused.get());
    input::listen(&atlas);
    ACTIVE.with(|active| active.borrow_mut().replace(Rc::clone(&atlas)));
    shell::set_launch_state("open");

    let index = Rc::clone(&atlas.tiles);
    spawn_local(async move { index.load_manifest().await });

    atlas.focus();
    atlas.resize();
    atlas.schedule();
    telemetry::event("atlas-open");
    Ok(())
}

/// Where the atlas opens: on a star, or on what the page was looking at.
fn aim(app: &Rc<App>, launch: Launch) -> (String, Camera) {
    match launch {
        Launch::Star { name, position } => {
            let camera = Camera::looking_at(position, STAR_FOV_DEG, true);
            (name, camera)
        }
        Launch::Sky => {
            let (lat, lon) = app.observer_now();
            let matrix = crate::sky::view_matrix(app.sim_ms(), lat, lon);
            // Row three of the view matrix is the zenith, expressed in the
            // catalog's own frame — the direction the landing draws at its
            // centre, and so the direction the atlas must open on.
            let zenith = [
                f64::from(matrix[2]),
                f64::from(matrix[5]),
                f64::from(matrix[8]),
            ];
            (
                "Overhead".to_string(),
                Camera::looking_at(zenith, SKY_FOV_DEG, false),
            )
        }
    }
}

/// The bright catalog in the atlas's own record shape.
fn base_stars(bytes: &[u8]) -> Result<Vec<Star>, &'static str> {
    let count = catalog::header_count(bytes)?;
    let records = bytes
        .get(catalog::HEADER_LEN..)
        .ok_or("a star catalog with no records")?;
    let stars: Vec<Star> = records
        .chunks_exact(catalog::STRIDE)
        .map(|record| {
            let float =
                |at: usize| f32::from_le_bytes(record[at..at + 4].try_into().expect("four bytes"));
            Star {
                pos: [float(0), float(4), float(8)],
                mag: float(12),
                color: [record[16], record[17], record[18]],
            }
        })
        .collect();
    if stars.len() != count {
        return Err("star asset length mismatch");
    }
    Ok(stars)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BRIGHT: &[u8] = include_bytes!("../../../../static/stars/bright.bin");

    #[test]
    fn the_bright_catalog_reads_into_the_atlas_record_shape() {
        let stars = base_stars(BRIGHT).expect("the shipped catalog");
        assert_eq!(stars.len(), 12_191);
        for star in [
            stars.first().expect("a star"),
            stars.last().expect("a star"),
        ] {
            let norm = f64::from(star.pos[0])
                .hypot(f64::from(star.pos[1]))
                .hypot(f64::from(star.pos[2]));
            assert!((norm - 1.0).abs() < 0.02, "{norm}");
        }
        // Brightest first: the atlas inherits the ordering the sky relies on.
        assert!(stars[0].mag < stars[stars.len() - 1].mag);
    }

    #[test]
    fn a_catalog_whose_header_lies_about_its_length_is_refused() {
        let mut truncated = BRIGHT[..BRIGHT.len() - catalog::STRIDE].to_vec();
        assert!(base_stars(&truncated).is_err());
        truncated.truncate(4);
        assert!(base_stars(&truncated).is_err());
    }
}
