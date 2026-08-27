//! The landing page's sky: tonight's real sky, spun fast, from a travelling
//! observer, drawn behind the page.
//!
//! The catalog is Gaia/Hipparcos and the Milky Way map is binned Gaia star
//! counts — the whole point is that this is the actual sky and not a
//! procedural one, so nothing here invents a star position. `sky` and `track`
//! hold the astronomy and are plain Rust with tests; `render`, `globe` and
//! `mount` are the browser half.
//!
//! [`start`] mounts on `#starscape` and returns quietly on any page that does
//! not have one, so loading the bundle is never a commitment to it running.

pub mod annotate;
pub mod app;
pub mod callouts;
pub mod catalog;
pub mod cities;
pub mod dom;
pub mod globe;
pub mod interop;
pub mod label;
pub mod mount;
pub mod observer;
pub mod program;
pub mod render;
pub mod shaders;
pub mod sky;
pub mod telemetry;
pub mod track;
pub mod tuning;

/// Mount the sky. Safe to call on a page with no `#starscape` canvas.
pub fn start() {
    mount::start();
}
