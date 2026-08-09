//! Timing collection and end-of-run aggregates.
//!
//! Two things feed the same table:
//!
//! 1. A [`TimingLayer`] attached to the tracing subscriber, which harvests every
//!    `latency_ms` field the server already emits. The application code did not
//!    have to know a test was watching — `#[instrument]` spans and the
//!    `info!(latency_ms = …)` lines in `auth::store` are the instrumentation.
//! 2. [`step`], which the flow test wraps around each leg so the harness's own
//!    view of a step (`step::signin`) sits next to the server-side spans that
//!    ran inside it (`route.signin::signin handled`).
//!
//! Samples accumulate in a process-global registry because the run, not the
//! test function, is the unit being reported on: one binary, many tests, one
//! table at the end.

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use tracing::field::{Field, Visit};
use tracing_subscriber::layer::{Context, Layer};
use tracing_subscriber::registry::LookupSpan;

/// All samples seen so far, keyed by `span::message` (or `step::<name>`).
fn registry() -> &'static Mutex<BTreeMap<String, Vec<f64>>> {
    static REGISTRY: OnceLock<Mutex<BTreeMap<String, Vec<f64>>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(BTreeMap::new()))
}

/// Add one measurement under `key`.
pub fn record(key: impl Into<String>, millis: f64) {
    registry()
        .lock()
        .expect("timing registry not poisoned")
        .entry(key.into())
        .or_default()
        .push(millis);
}

/// Time an async step, record it as `step::<name>`, and return its value.
pub async fn step<F, T>(name: &str, future: F) -> T
where
    F: std::future::Future<Output = T>,
{
    let started = Instant::now();
    let out = future.await;
    let millis = started.elapsed().as_secs_f64() * 1000.0;
    record(format!("step::{name}"), millis);
    tracing::debug!(step = name, latency_ms = millis, "harness step finished");
    out
}

/// Aggregates for one key.
#[derive(Debug, Clone, PartialEq)]
pub struct Aggregate {
    pub key: String,
    pub count: usize,
    pub min_ms: f64,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub max_ms: f64,
    pub mean_ms: f64,
    pub total_ms: f64,
}

/// Nearest-rank percentile over an already-sorted slice.
///
/// Nearest-rank rather than interpolating: with the handful of samples a single
/// flow produces, an interpolated p95 would report a number that was never
/// actually measured.
fn percentile(sorted: &[f64], quantile: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let rank = (quantile * sorted.len() as f64).ceil().max(1.0) as usize;
    sorted[rank.min(sorted.len()) - 1]
}

/// Snapshot every key collected so far, ordered by total time descending.
pub fn aggregates() -> Vec<Aggregate> {
    let guard = registry().lock().expect("timing registry not poisoned");
    let mut out: Vec<Aggregate> = guard
        .iter()
        .filter(|(_, samples)| !samples.is_empty())
        .map(|(key, samples)| {
            let mut sorted = samples.clone();
            sorted.sort_by(|a, b| a.partial_cmp(b).expect("no NaN latencies"));
            let total: f64 = sorted.iter().sum();
            Aggregate {
                key: key.clone(),
                count: sorted.len(),
                min_ms: sorted[0],
                p50_ms: percentile(&sorted, 0.50),
                p95_ms: percentile(&sorted, 0.95),
                max_ms: sorted[sorted.len() - 1],
                mean_ms: total / sorted.len() as f64,
                total_ms: total,
            }
        })
        .collect();
    out.sort_by(|a, b| {
        b.total_ms
            .partial_cmp(&a.total_ms)
            .expect("no NaN latencies")
            .then_with(|| a.key.cmp(&b.key))
    });
    out
}

/// Render the aggregate table.
#[must_use]
pub fn render_report(run: &str) -> String {
    let rows = aggregates();
    let key_width = rows
        .iter()
        .map(|r| r.key.len())
        .chain(std::iter::once(12))
        .max()
        .unwrap_or(12);

    let mut out = String::new();
    out.push('\n');
    out.push_str(&format!(
        "── timing aggregates · {run} · all values ms ──\n"
    ));
    out.push_str(&format!(
        "{:<key_width$}  {:>4}  {:>9}  {:>9}  {:>9}  {:>9}  {:>9}  {:>10}\n",
        "key", "n", "min", "p50", "p95", "max", "mean", "total",
    ));
    out.push_str(&format!("{}\n", "─".repeat(key_width + 68)));
    for row in &rows {
        out.push_str(&format!(
            "{:<key_width$}  {:>4}  {:>9.2}  {:>9.2}  {:>9.2}  {:>9.2}  {:>9.2}  {:>10.2}\n",
            row.key,
            row.count,
            row.min_ms,
            row.p50_ms,
            row.p95_ms,
            row.max_ms,
            row.mean_ms,
            row.total_ms,
        ));
    }
    if rows.is_empty() {
        out.push_str("(no timings captured)\n");
    }
    out.push_str(&format!(
        "{} keys, {} samples\n",
        rows.len(),
        rows.iter().map(|r| r.count).sum::<usize>(),
    ));
    out
}

/// Print the aggregate table.
///
/// Uses `println!` so `cargo test -- --nocapture` shows it; the tracing output
/// goes through the test writer and would otherwise interleave badly with it.
pub fn report(run: &str) {
    println!("{}", render_report(run));
}

/// Print the aggregate table once, when the test binary exits.
///
/// libtest has no "after all tests" hook and statics are never dropped, so the
/// report is registered with `atexit` instead. That is what makes the table
/// cover the *run* rather than whichever test happened to call it last — and it
/// still fires when a test fails, which is exactly when the timings are worth
/// reading.
pub fn report_at_exit(run: &'static str) {
    static RUN_NAME: OnceLock<&'static str> = OnceLock::new();
    static REGISTERED: OnceLock<()> = OnceLock::new();

    let _ = RUN_NAME.set(run);
    REGISTERED.get_or_init(|| {
        extern "C" fn print_report() {
            let run = RUN_NAME.get().copied().unwrap_or("test run");
            // Not `println!`: at atexit time libtest's capture is long gone and
            // a panicking print here would abort the process after a green run.
            let _ =
                std::io::Write::write_all(&mut std::io::stdout(), render_report(run).as_bytes());
        }
        // SAFETY: `print_report` is a plain `extern "C" fn` that only reads a
        // process-global mutex and writes to stdout. No unwinding escapes it.
        unsafe {
            libc::atexit(print_report);
        }
    });
}

/// Drop every sample. Only for tests of the harness itself.
pub fn reset() {
    registry()
        .lock()
        .expect("timing registry not poisoned")
        .clear();
}

/// Pulls `latency_ms` and `message` off an event.
#[derive(Default)]
struct LatencyVisitor {
    latency_ms: Option<f64>,
    message: Option<String>,
}

impl Visit for LatencyVisitor {
    fn record_f64(&mut self, field: &Field, value: f64) {
        if field.name() == "latency_ms" {
            self.latency_ms = Some(value);
        }
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        if field.name() == "latency_ms" {
            self.latency_ms = Some(value as f64);
        }
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        if field.name() == "latency_ms" {
            self.latency_ms = Some(value as f64);
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message = Some(value.to_string());
        }
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = Some(format!("{value:?}"));
        }
    }
}

/// Subscriber layer that files every `latency_ms` event into the registry.
pub struct TimingLayer;

impl<S> Layer<S> for TimingLayer
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_event(&self, event: &tracing::Event<'_>, ctx: Context<'_, S>) {
        let mut visitor = LatencyVisitor::default();
        event.record(&mut visitor);
        let Some(latency_ms) = visitor.latency_ms else {
            return;
        };

        // Key on the enclosing span so two handlers reporting "request
        // completed" stay separate rows, and on the message so several timed
        // stages inside one span (hash, insert, commit) do not collapse.
        let span_name = ctx.lookup_current().map(|span| span.name().to_string());
        let message = visitor.message.unwrap_or_else(|| "event".to_string());
        let key = match span_name {
            Some(span) => format!("{span}::{message}"),
            None => format!("::{message}"),
        };
        record(key, latency_ms);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentile_is_nearest_rank() {
        let sorted = [1.0, 2.0, 3.0, 4.0];
        assert_eq!(percentile(&sorted, 0.0), 1.0);
        assert_eq!(percentile(&sorted, 0.5), 2.0);
        assert_eq!(percentile(&sorted, 0.95), 4.0);
        assert_eq!(percentile(&[], 0.5), 0.0);
    }
}
