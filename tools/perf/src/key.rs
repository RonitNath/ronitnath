//! Idempotency keys, without a uuid crate.
//!
//! A command's `key` is the thing that makes a retry safe, which means the
//! load generator must mint a *fresh* one per request: posting one body with
//! one key repeatedly measures the replay cache, not the commit path, and that
//! is the trap that rules oha out for `POST /api/cmd/*` in the first place.
//! The server only parses these as UUIDs and stores them; uniqueness within a
//! run is the whole requirement, so a counter mixed with the process id and
//! the clock is enough, and being visibly a v4 keeps the parser happy.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_nanos())
        .unwrap_or_default()
}

/// A fresh key, unique for the life of this process and across the handful of
/// processes one run starts.
pub fn uuid() -> String {
    let count = COUNTER.fetch_add(1, Ordering::Relaxed);
    let high = (nanos() as u64) ^ (u64::from(std::process::id()) << 40);
    let low =
        count.wrapping_mul(0x9E37_79B9_7F4A_7C15).rotate_left(17) ^ u64::from(std::process::id());
    let a = (high >> 32) as u32;
    let b = (high >> 16) as u16;
    let c = 0x4000 | (high as u16 & 0x0FFF);
    let d = 0x8000 | (low >> 48) as u16 & 0x3FFF;
    let e = low & 0xFFFF_FFFF_FFFF;
    format!("{a:08x}-{b:04x}-{c:04x}-{d:04x}-{e:012x}")
}

/// A short run stamp, so two seeds on one cluster do not collide on the
/// deployment-unique email index.
pub fn stamp() -> String {
    format!("{:x}", nanos() as u64 & 0xFFFF_FFFF_FFFF)
}
