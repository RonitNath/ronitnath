//! Make the bundle output directories exist before `rust_embed` looks for them.
//!
//! `assets.rs` embeds each Leptos bundle's `dist/` with `#[derive(Embed)]`,
//! and that derive is a hard error when the folder is absent — not a warning,
//! and not an empty set. The folders are trunk's output and are gitignored, so
//! on a fresh checkout `cargo test --workspace` does not compile at all: the
//! gate manifest's whole fast suite is unreachable until somebody has run
//! `trunk build` four times, and `.forgejo/workflows/deploy.yml` checks out
//! clean and never does.
//!
//! Creating them empty is the honest fix rather than a workaround. In dev the
//! server serves bundles from disk and never consults the embedded set; in a
//! release image the Containerfile builds every bundle before `cargo build`,
//! so the directories are already full and already correct. What this removes
//! is only the case where an *absent* directory is a compile error instead of
//! an empty bundle — which is a build-ordering fact, not a fact about the
//! program.

fn main() {
    for bundle in ["starscape", "app-member", "app-org", "app-platform"] {
        let dist = std::path::Path::new("..").join(bundle).join("dist");
        if let Err(error) = std::fs::create_dir_all(&dist) {
            println!("cargo::warning=could not create {}: {error}", dist.display());
        }
        println!("cargo::rerun-if-changed={}", dist.display());
    }
}
