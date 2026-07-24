//! Exposes `GIT_HASH` to the crate for the `/healthz` endpoint.

use std::process::Command;

fn main() {
    let hash = std::env::var("SOURCE_GIT_HASH")
        .ok()
        .filter(|hash| !hash.trim().is_empty())
        .or_else(|| {
            Command::new("git")
                .args(["rev-parse", "--short", "HEAD"])
                .output()
                .ok()
                .filter(|output| output.status.success())
                .and_then(|output| String::from_utf8(output.stdout).ok())
                .map(|hash| hash.trim().to_string())
        })
        .unwrap_or_else(|| "unknown".to_string());

    println!("cargo:rustc-env=GIT_HASH={hash}");
    println!("cargo:rerun-if-env-changed=SOURCE_GIT_HASH");
    println!("cargo:rerun-if-changed=.git/HEAD");
    // Pulling a new commit onto an already checked-out branch leaves HEAD's
    // contents unchanged; the branch ref is the dependency that moves.
    println!("cargo:rerun-if-changed=.git/refs/heads/main");
    println!("cargo:rerun-if-changed=.git/packed-refs");
}
