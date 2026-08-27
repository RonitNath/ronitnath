//! Reconnect timing: full jitter, and the watchdog that decides a silent
//! socket is a dead socket.
//!
//! Full jitter rather than exponential-with-a-fixed-delay because every tab a
//! person has open is a client of the same server: a deploy drops them all at
//! once, and equal delays bring them all back at once. Sleeping a uniform draw
//! from `[0, window]` spreads the return over the window instead.

use std::time::Duration;

/// The first window. Short enough that a blip is invisible.
const BASE_MS: u64 = 250;
/// The ceiling. A tab left open on a dead server retries four times a minute,
/// not four times a second.
const CAP_MS: u64 = 15_000;
/// The server heartbeats every 25 s (`docs/rebuild/plan.md` §API). Two missed
/// beats plus slack is dead; one late beat is not.
const WATCHDOG_MS: u64 = 60_000;

/// The reconnect schedule for one subscription.
#[derive(Debug, Clone, Default)]
pub struct Backoff {
    attempt: u32,
}

impl Backoff {
    /// A schedule that has not failed yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// The upper bound of the next delay: `250 ms · 2^attempt`, capped.
    pub fn window(&self) -> Duration {
        Duration::from_millis(window_ms(self.attempt))
    }

    /// The next delay: a uniform draw from `[0, window]`.
    ///
    /// `random` is a `[0, 1)` sample — the caller passes `Math.random()` in a
    /// browser and a fixed value in a test.
    pub fn next_delay(&mut self, random: f64) -> Duration {
        let window = window_ms(self.attempt);
        self.attempt = self.attempt.saturating_add(1);
        Duration::from_millis((window as f64 * random.clamp(0.0, 1.0)) as u64)
    }

    /// A message arrived, so the connection works: the next failure starts
    /// over at the first window.
    pub fn succeeded(&mut self) {
        self.attempt = 0;
    }

    /// How many consecutive failures this schedule has seen.
    pub fn attempts(&self) -> u32 {
        self.attempt
    }
}

/// How long to wait for any message before treating the socket as dead.
pub const fn watchdog() -> Duration {
    Duration::from_millis(WATCHDOG_MS)
}

fn window_ms(attempt: u32) -> u64 {
    BASE_MS.saturating_mul(1u64 << attempt.min(16)).min(CAP_MS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_doubles_until_it_reaches_the_cap() {
        assert_eq!(window_ms(0), 250);
        assert_eq!(window_ms(1), 500);
        assert_eq!(window_ms(2), 1_000);
        assert_eq!(window_ms(6), 15_000, "16 s would exceed the cap");
        assert_eq!(window_ms(40), CAP_MS, "and it never overflows past it");
    }

    #[test]
    fn a_delay_is_a_draw_from_the_whole_window() {
        let mut backoff = Backoff::new();
        assert_eq!(backoff.next_delay(0.0), Duration::ZERO);
        assert_eq!(backoff.next_delay(1.0), Duration::from_millis(500));
        assert_eq!(backoff.next_delay(0.5), Duration::from_millis(500));
        assert_eq!(backoff.attempts(), 3);
    }

    #[test]
    fn every_delay_stays_inside_its_window() {
        let mut backoff = Backoff::new();
        for step in 0..12 {
            let window = backoff.window();
            let delay = backoff.next_delay(0.999);
            assert!(delay <= window, "step {step}: {delay:?} exceeds {window:?}");
        }
    }

    #[test]
    fn a_message_resets_the_schedule() {
        let mut backoff = Backoff::new();
        for _ in 0..5 {
            backoff.next_delay(0.5);
        }
        assert_eq!(backoff.window(), Duration::from_millis(8_000));
        backoff.succeeded();
        assert_eq!(backoff.attempts(), 0);
        assert_eq!(backoff.window(), Duration::from_millis(BASE_MS));
    }

    #[test]
    fn the_watchdog_outlives_two_heartbeats() {
        assert!(watchdog() > Duration::from_secs(50));
    }
}
