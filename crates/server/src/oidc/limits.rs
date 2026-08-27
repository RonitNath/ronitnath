//! A bucket per client, counting failed authentications at the token endpoint.
//!
//! A client secret is 256 bits from the OS and cannot be guessed. This is not
//! about that. It is about the endpoint being a *free* oracle: without a limit,
//! a caller may try a secret as fast as the deployment can hash one, forever,
//! and the only thing standing between that and a compromise is the entropy of
//! a secret somebody might one day have set by hand.
//!
//! Only failures count, and a success clears the count — so a working client
//! is never slowed down by a broken one sharing the process, and a client that
//! has been fixed works on its next attempt rather than after a wait.
//!
//! In-process, per node, and that is honest rather than ideal: a three-node
//! cluster gives a caller three buckets. The alternative is a counter that
//! goes through raft, which would put a write on the failure path of an
//! endpoint whose failure path is the one being attacked.

use std::collections::HashMap;
use std::sync::Mutex;

/// How many failures a client may accumulate before it waits.
pub const BURST: u32 = 20;

/// How long the window is. Twenty failures a minute is far more than a working
/// client produces and far less than a search.
pub const WINDOW: i64 = 60;

/// One client's recent failures.
#[derive(Debug, Clone, Copy)]
struct Bucket {
    failures: u32,
    since: i64,
}

/// The buckets, keyed by client id.
#[derive(Debug, Default)]
pub struct Limiter {
    buckets: Mutex<HashMap<String, Bucket>>,
}

impl Limiter {
    /// A fresh limiter.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether this client may be tried again.
    #[must_use]
    pub fn allow(&self, client: &str, now: i64) -> bool {
        let mut buckets = self.buckets.lock().unwrap_or_else(|poisoned| {
            // A panic in a handler holding this lock must not take the token
            // endpoint down with it: the bucket is a rate limit, not a ledger.
            self.buckets.clear_poison();
            poisoned.into_inner()
        });
        match buckets.get(client) {
            Some(bucket) if now - bucket.since >= WINDOW => {
                buckets.remove(client);
                true
            }
            Some(bucket) => bucket.failures < BURST,
            None => true,
        }
    }

    /// Record a failure.
    pub fn failed(&self, client: &str, now: i64) {
        let mut buckets = self.buckets.lock().unwrap_or_else(|poisoned| {
            self.buckets.clear_poison();
            poisoned.into_inner()
        });
        // Unbounded growth is the obvious worry and this is where it is
        // bounded: an attacker naming a fresh client id every time never gets
        // a second failure under any of them, so the map is swept whenever it
        // is larger than the number of clients a deployment plausibly has.
        if buckets.len() > 4_096 {
            buckets.retain(|_, bucket| now - bucket.since < WINDOW);
        }
        let bucket = buckets.entry(client.to_owned()).or_insert(Bucket {
            failures: 0,
            since: now,
        });
        if now - bucket.since >= WINDOW {
            *bucket = Bucket {
                failures: 0,
                since: now,
            };
        }
        bucket.failures = bucket.failures.saturating_add(1);
    }

    /// Forget a client's failures: it has just proved itself.
    pub fn forgive(&self, client: &str) {
        let mut buckets = self.buckets.lock().unwrap_or_else(|poisoned| {
            self.buckets.clear_poison();
            poisoned.into_inner()
        });
        buckets.remove(client);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_client_is_allowed_until_it_has_failed_enough_and_then_waits() {
        let limiter = Limiter::new();
        assert!(limiter.allow("c_one", 0));
        for _ in 0..BURST {
            limiter.failed("c_one", 0);
        }
        assert!(!limiter.allow("c_one", 0));
        assert!(limiter.allow("c_two", 0), "and nobody else is affected");
        assert!(
            limiter.allow("c_one", WINDOW),
            "the window is what it waits for"
        );
    }

    #[test]
    fn proving_itself_clears_the_count() {
        let limiter = Limiter::new();
        for _ in 0..BURST {
            limiter.failed("c_one", 0);
        }
        assert!(!limiter.allow("c_one", 0));
        limiter.forgive("c_one");
        assert!(limiter.allow("c_one", 0));
    }

    #[test]
    fn a_stream_of_invented_client_ids_does_not_grow_the_map_without_bound() {
        let limiter = Limiter::new();
        for n in 0..5_000 {
            limiter.failed(&format!("c_{n}"), WINDOW * 2);
        }
        let held = limiter.buckets.lock().expect("the lock").len();
        assert!(held <= 5_000, "every one of them is recent");
        // Advance past the window: the next failure sweeps them all.
        limiter.failed("c_last", WINDOW * 4);
        let held = limiter.buckets.lock().expect("the lock").len();
        assert_eq!(held, 1, "only the recent one is kept");
    }
}
