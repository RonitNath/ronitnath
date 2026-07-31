use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn track_git_state() {
    let Some(raw_git_dir) = git(&["rev-parse", "--git-dir"]) else {
        return;
    };
    let git_dir = PathBuf::from(raw_git_dir);
    println!("cargo:rerun-if-changed={}", git_dir.join("HEAD").display());
    println!(
        "cargo:rerun-if-changed={}",
        git_dir.join("packed-refs").display()
    );
    if let Some(reference) = git(&["symbolic-ref", "-q", "HEAD"]) {
        println!(
            "cargo:rerun-if-changed={}",
            git_dir.join(reference).display()
        );
    }
}

fn main() {
    println!("cargo:rerun-if-env-changed=SOURCE_DATE_EPOCH");
    track_git_state();
    let revision = git(&["rev-parse", "--verify", "HEAD"])
        .filter(|value| value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .unwrap_or_else(|| "unknown".into());
    let built_at = std::env::var("SOURCE_DATE_EPOCH")
        .ok()
        .filter(|value| !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()))
        .unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("build time must follow the Unix epoch")
                .as_secs()
                .to_string()
        });
    println!("cargo:rustc-env=WEB_BUILD_GIT_REVISION={revision}");
    println!("cargo:rustc-env=WEB_BUILD_UNIX_TIME={built_at}");
}
