//! `rn-perf` — the parts of the perf harness a load tool cannot do.
//!
//! oha drives every profile whose request is the same every time:
//! `/api/whoami`, `/api/q/documents`, the landing page. This binary owns the
//! three that are not — seeding the world through the real commands, the
//! commit profile (fresh idempotency key and a moving `expected_rev` per
//! request), and the WebSocket fan-out probe — and reports them in the same
//! p50/p99/max/RPS shape so one table can hold both.
//!
//!   rn-perf seed    --host 127.0.0.1:3161 --documents 10000 --subscribers 100
//!                   --workers 8 --out target/perf/seed.json
//!   rn-perf commit  --seed target/perf/seed.json --seconds 30 [--timeline] [--persist]
//!   rn-perf wsprobe --seed target/perf/seed.json --subscribers 100
//!
//! Every subcommand prints its human table on stdout and, with `--json <path>`,
//! writes the same numbers where a report generator can read them.

mod commit;
mod http;
mod key;
mod seed;
mod stats;
mod wsprobe;

use std::collections::HashMap;

fn main() -> std::process::ExitCode {
    let mut args = std::env::args().skip(1);
    let command = args.next().unwrap_or_default();
    let flags = parse(args);

    let runtime = match tokio::runtime::Builder::new_multi_thread()
        // Two worker threads, because the harness shares this machine with
        // whatever it is measuring and a load generator that takes the box is
        // measuring itself.
        .worker_threads(2)
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => return fail(&error.to_string()),
    };

    let result = runtime.block_on(dispatch(&command, &flags));
    match result {
        Ok(Some(report)) => {
            if let Some(path) = flags.get("json") {
                let text = serde_json::to_string_pretty(&report).unwrap_or_default();
                if let Err(error) = std::fs::write(path, text) {
                    return fail(&format!("write {path}: {error}"));
                }
            }
            std::process::ExitCode::SUCCESS
        }
        Ok(None) => std::process::ExitCode::SUCCESS,
        Err(error) => fail(&error),
    }
}

async fn dispatch(
    command: &str,
    flags: &HashMap<String, String>,
) -> Result<Option<serde_json::Value>, String> {
    let host = flags
        .get("host")
        .cloned()
        .unwrap_or_else(|| "127.0.0.1:3161".to_owned());
    match command {
        "seed" => {
            seed::run(seed::Plan {
                host,
                documents: number(flags, "documents", 10_000),
                subscribers: number(flags, "subscribers", 100),
                workers: number(flags, "workers", 8),
                out: flags
                    .get("out")
                    .cloned()
                    .unwrap_or_else(|| "target/perf/seed.json".to_owned()),
            })
            .await?;
            Ok(None)
        }
        "commit" => {
            let seed = read_seed(flags)?;
            let host = host_of(&seed, flags);
            commit::run(commit::Plan {
                host,
                seed,
                seconds: number(flags, "seconds", 30) as u64,
                reads: number(flags, "reads", 4) as u32,
                timeline: flags.contains_key("timeline"),
                persist: flags.contains_key("persist"),
            })
            .await
            .map(Some)
        }
        "wsprobe" => {
            let seed = read_seed(flags)?;
            let host = host_of(&seed, flags);
            wsprobe::run(wsprobe::Plan {
                subscribers: number(flags, "subscribers", 100),
                host,
                seed,
            })
            .await
            .map(Some)
        }
        other => Err(format!(
            "unknown subcommand {other:?} — seed | commit | wsprobe"
        )),
    }
}

/// The host to drive: `--host` wins, otherwise the one the seed was made
/// against, so a drill can point the load at a survivor without re-seeding.
fn host_of(seed: &serde_json::Value, flags: &HashMap<String, String>) -> String {
    flags
        .get("host")
        .cloned()
        .unwrap_or_else(|| seed["host"].as_str().unwrap_or("127.0.0.1:3161").to_owned())
}

fn read_seed(flags: &HashMap<String, String>) -> Result<serde_json::Value, String> {
    let path = flags
        .get("seed")
        .cloned()
        .unwrap_or_else(|| "target/perf/seed.json".to_owned());
    let text = std::fs::read_to_string(&path).map_err(|error| format!("read {path}: {error}"))?;
    serde_json::from_str(&text).map_err(|error| format!("parse {path}: {error}"))
}

fn number(flags: &HashMap<String, String>, name: &str, fallback: usize) -> usize {
    flags
        .get(name)
        .and_then(|value| value.parse().ok())
        .unwrap_or(fallback)
}

/// `--name value` and `--flag`, which is every option this binary takes.
fn parse(args: impl Iterator<Item = String>) -> HashMap<String, String> {
    let mut flags = HashMap::new();
    let mut pending: Option<String> = None;
    for arg in args {
        match arg.strip_prefix("--") {
            Some(name) => {
                if let Some(previous) = pending.take() {
                    flags.insert(previous, String::new());
                }
                match name.split_once('=') {
                    Some((name, value)) => {
                        flags.insert(name.to_owned(), value.to_owned());
                    }
                    None => pending = Some(name.to_owned()),
                }
            }
            None => {
                if let Some(name) = pending.take() {
                    flags.insert(name, arg);
                }
            }
        }
    }
    if let Some(name) = pending {
        flags.insert(name, String::new());
    }
    flags
}

fn fail(message: &str) -> std::process::ExitCode {
    eprintln!("rn-perf: {message}");
    std::process::ExitCode::FAILURE
}
