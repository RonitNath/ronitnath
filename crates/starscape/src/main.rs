//! Trunk builds this binary into the bundle the landing page loads; the work
//! lives in the library beside it so the sky maths stays testable off-wasm.

fn main() {
    rn_starscape::start();
}
