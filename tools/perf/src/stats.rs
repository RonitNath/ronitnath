//! Latency samples, and the four numbers the report asks every profile for.
//!
//! Percentiles are read off the sorted samples by nearest rank, which is what
//! oha reports too, so a table can put an oha row and a row from this harness
//! next to each other without a footnote about method.

use std::time::Duration;

/// One profile's samples.
#[derive(Default)]
pub struct Samples {
    latencies: Vec<Duration>,
    pub errors: u64,
    /// Non-2xx statuses, counted by code, so a decline is distinguishable
    /// from a socket that died.
    pub statuses: std::collections::BTreeMap<u16, u64>,
}

impl Samples {
    pub fn push(&mut self, latency: Duration, status: u16) {
        self.latencies.push(latency);
        *self.statuses.entry(status).or_default() += 1;
        if !(200..300).contains(&status) {
            self.errors += 1;
        }
    }

    pub fn fail(&mut self) {
        self.errors += 1;
    }

    pub fn absorb(&mut self, other: Self) {
        self.latencies.extend(other.latencies);
        self.errors += other.errors;
        for (status, count) in other.statuses {
            *self.statuses.entry(status).or_default() += count;
        }
    }

    pub fn len(&self) -> usize {
        self.latencies.len()
    }

    fn percentile(sorted: &[Duration], fraction: f64) -> Duration {
        if sorted.is_empty() {
            return Duration::ZERO;
        }
        let rank = ((sorted.len() as f64) * fraction).ceil() as usize;
        sorted[rank.saturating_sub(1).min(sorted.len() - 1)]
    }

    /// p50, p99, max, and requests per second over `wall`.
    pub fn summary(&self, wall: Duration) -> Summary {
        let mut sorted = self.latencies.clone();
        sorted.sort_unstable();
        Summary {
            count: sorted.len() as u64,
            p50: Self::percentile(&sorted, 0.50),
            p99: Self::percentile(&sorted, 0.99),
            max: sorted.last().copied().unwrap_or(Duration::ZERO),
            rps: if wall.as_secs_f64() > 0.0 {
                sorted.len() as f64 / wall.as_secs_f64()
            } else {
                0.0
            },
            errors: self.errors,
            statuses: self.statuses.clone(),
        }
    }
}

pub struct Summary {
    pub count: u64,
    pub p50: Duration,
    pub p99: Duration,
    pub max: Duration,
    pub rps: f64,
    pub errors: u64,
    pub statuses: std::collections::BTreeMap<u16, u64>,
}

impl Summary {
    /// Milliseconds, to three decimals — the unit every budget in the kernel
    /// report is stated in, or converts to without ambiguity.
    fn ms(value: Duration) -> f64 {
        value.as_secs_f64() * 1000.0
    }

    pub fn line(&self, name: &str) -> String {
        let statuses: Vec<String> = self
            .statuses
            .iter()
            .map(|(code, count)| format!("{code}×{count}"))
            .collect();
        format!(
            "{name:<34} n={:<7} p50 {:>8.3} ms  p99 {:>8.3} ms  max {:>9.3} ms  {:>8.1} rps  errors {}  [{}]",
            self.count,
            Self::ms(self.p50),
            Self::ms(self.p99),
            Self::ms(self.max),
            self.rps,
            self.errors,
            statuses.join(" ")
        )
    }

    pub fn json(&self, name: &str) -> serde_json::Value {
        serde_json::json!({
            "profile": name,
            "count": self.count,
            "p50_ms": Self::ms(self.p50),
            "p99_ms": Self::ms(self.p99),
            "max_ms": Self::ms(self.max),
            "rps": self.rps,
            "errors": self.errors,
            "statuses": self.statuses.iter()
                .map(|(code, count)| (code.to_string(), serde_json::json!(count)))
                .collect::<serde_json::Map<String, serde_json::Value>>(),
        })
    }
}
