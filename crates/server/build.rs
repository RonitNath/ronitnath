//! Make the four trunk bundle outputs embeddable before the crate compiles.
//!
//! `rust-embed` resolves its folder at macro-expansion time and fails the build
//! if the directory is absent. Every `crates/<bundle>/dist` is produced by
//! `trunk build` and is deliberately not committed, so a fresh checkout — or a
//! `cargo check` run before any bundle has been built — has nothing there. An
//! empty directory embeds as an empty asset set, which is the honest state: the
//! bundle routes then 404 until trunk has run. Creating the directory here
//! keeps that from presenting as a compile error in an unrelated crate.
fn main() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates/")
        .parent()
        .expect("workspace root")
        .to_path_buf();
    for bundle in ["app-member", "app-org", "app-platform", "starscape"] {
        let dist = root.join("crates").join(bundle).join("dist");
        std::fs::create_dir_all(&dist).expect("create bundle dist directory");
        println!("cargo:rerun-if-changed={}", dist.display());
    }
    println!("cargo:rerun-if-changed={}", root.join("static").display());
}
