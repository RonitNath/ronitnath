//! Confirmed reads: a command waits for the offset its caller already holds.
//!
//! A command reads its preconditions from the local state machine — the
//! document's revision, the session behind the cookie, whether the person
//! exists at all — and on a follower that has not yet applied the last entry
//! those reads answer about a world one commit old. The visible result is a
//! decline for a request that is perfectly well formed: `create-document`
//! posted to a follower straight after the `register` that made the person is
//! refused every time, and one retry 50 ms later succeeds (finding 2,
//! `docs/perf/2026-08-27.md`).
//!
//! The fix is the one the kernel report already names for the log group:
//! *acks and reads wait for the offset they need*. A command reply carries the
//! offset its event landed at; the caller sends that offset back on its next
//! command, as `after`; this node waits until its own feed head has reached it
//! before anything reads anything. What that buys is read-your-writes across
//! nodes — a caller never sees a world older than one it has already been
//! told about — which is exactly the property the declines were violating.
//!
//! ## Why not a consistent read instead
//!
//! hiqlite offers `query_consistent`, which runs on the leader behind a
//! linearizable read barrier, and routing every precondition through it would
//! need no client cooperation at all. It would also put a leader round trip
//! and a raft read barrier in front of *each* of the four or five reads a
//! command makes, on a commit path that is already 47× over its budget
//! (finding 4) — paying for global linearizability to fix a per-caller
//! staleness. The waiting side is one local `max(id)` read, taken only when
//! the caller presents an offset, and skipped entirely when this node is
//! already past it.
//!
//! ## What it does not promise
//!
//! A caller that presents no offset gets what it always got: this node's
//! current state. The barrier is a caller saying "I have seen offset N", not
//! this node claiming to be current — and it is bounded, because a node that
//! cannot catch up within [`WAIT`] is a node whose answer, late, is worth more
//! than a request held forever.

use std::time::{Duration, Instant};

use rn_kernel::Offset;
use rn_kernel::feed::Feed;
use serde::Deserialize;

use crate::state::AppState;

/// The header a caller that cannot carry an envelope field uses instead — the
/// `/auth` form posts, which are `application/x-www-form-urlencoded`.
pub const AFTER_HEADER: &str = "x-rn-after";

/// The header every command reply stamps with the offset it landed at, so a
/// form post — whose reply is a redirect, not a [`CommandReply`] — can still
/// tell its caller where it got to.
///
/// [`CommandReply`]: rn_api::CommandReply
pub const OFFSET_HEADER: &str = "x-rn-offset";

/// How long a node will wait to catch up before answering from where it is.
///
/// Well inside the command timeout (`crate::http::COMMAND_TIMEOUT`), so a
/// caller that is waiting on a node which will never catch up is answered by
/// this rather than by the timeout.
pub const WAIT: Duration = Duration::from_secs(2);

/// How often it re-reads its own feed head while waiting. Two milliseconds is
/// two orders of magnitude under the wait and one over the replication it is
/// waiting for.
const POLL: Duration = Duration::from_millis(2);

/// How far ahead of this node an offset may be and still be worth waiting for.
///
/// A caller presents an offset it has *been told about*, so a legitimate one
/// is at most this node's replication lag ahead — a handful of entries, and
/// generously a batch of them. An offset far beyond that is not a world
/// anything has committed, so waiting [`WAIT`] for it buys nothing and costs
/// a request slot: `x-rn-after: 9999999999` is two seconds of this node,
/// taken before the cookie is even looked at, by anybody who can set a
/// header. Past this lead the barrier is skipped and the command answers from
/// where the node is, which is the same thing the wait would have done two
/// seconds later.
const MAX_LEAD: Offset = 10_000;

/// The `after` field of a command envelope, read without knowing which command
/// this is.
///
/// The body is parsed twice — once here, once as the typed envelope the
/// dispatch table names — and that is the price of reading a field before
/// knowing the type it sits beside. Command bodies are kilobytes at most.
#[derive(Debug, Deserialize)]
struct Envelope {
    #[serde(default)]
    after: Option<Offset>,
}

/// The offset a caller says it has already been told about: the envelope's
/// `after`, or the header for a caller that posts a form.
#[must_use]
pub fn requested(body: &str, headers: &axum::http::HeaderMap) -> Option<Offset> {
    serde_json::from_str::<Envelope>(body)
        .ok()
        .and_then(|envelope| envelope.after)
        .or_else(|| {
            headers
                .get(AFTER_HEADER)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.trim().parse().ok())
        })
        .filter(|after| *after > 0)
}

/// What waiting for an offset came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Caught {
    /// This node was already at or past it. The usual answer.
    Already,
    /// It arrived, after this long.
    Waited(Duration),
    /// It did not arrive within [`WAIT`]; the command runs anyway.
    Gave(Duration),
}

/// Block until this node's own state machine has applied `after`.
pub async fn catch_up(state: &AppState, after: Offset) -> Caught {
    let started = Instant::now();
    let mut first = true;
    loop {
        if let Ok(head) = state.feed.head().await
            && after > head.saturating_add(MAX_LEAD)
        {
            tracing::warn!(
                after,
                head,
                "an offset further ahead than this cluster has ever been was not waited for"
            );
            return Caught::Gave(started.elapsed());
        }
        // A feed that cannot answer reads as zero and would spin here, so a
        // failed read is treated as "cannot know", which ends the wait.
        match state.feed.head().await {
            Ok(head) if head >= after => {
                return if first {
                    Caught::Already
                } else {
                    Caught::Waited(started.elapsed())
                };
            }
            Ok(_) => {}
            Err(error) => {
                tracing::warn!(%error, "the feed head could not be read while catching up");
                return Caught::Gave(started.elapsed());
            }
        }
        first = false;
        if started.elapsed() >= WAIT {
            tracing::warn!(
                after,
                waited_ms = started.elapsed().as_millis() as u64,
                "this node did not catch up to the offset its caller holds"
            );
            return Caught::Gave(started.elapsed());
        }
        tokio::time::sleep(POLL).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderMap;

    #[test]
    fn the_envelope_field_is_read_beside_a_commands_own_arguments() {
        let body = r#"{"key":"x","after":42,"document":"r_A","expected_rev":3}"#;
        assert_eq!(requested(body, &HeaderMap::new()), Some(42));
    }

    #[test]
    fn a_caller_that_says_nothing_asks_for_nothing() {
        assert_eq!(
            requested(r#"{"key":"x","display_name":"Ronit"}"#, &HeaderMap::new()),
            None
        );
        assert_eq!(requested("not json", &HeaderMap::new()), None);
        assert_eq!(requested(r#"{"after":0}"#, &HeaderMap::new()), None);
    }

    #[test]
    fn the_lead_bound_is_generous_about_lag_and_not_about_a_forged_header() {
        // Replication lag is entries, not billions of them. The bound has to
        // sit above anything a real follower is behind by and below what a
        // caller can type into a header to buy two seconds of a node.
        assert!(
            MAX_LEAD >= 1_000,
            "a batch of entries must still be waited for"
        );
        assert!(u64::from(u32::MAX) > MAX_LEAD, "and a forged one must not");
    }

    #[test]
    fn a_form_post_carries_the_offset_in_a_header_because_it_has_no_envelope() {
        let mut headers = HeaderMap::new();
        headers.insert(AFTER_HEADER, "17".parse().expect("a header value"));
        assert_eq!(requested("display_name=Ronit", &headers), Some(17));
        // The envelope wins where both are present: it is the contract.
        assert_eq!(requested(r#"{"after":9}"#, &headers), Some(9));
    }
}
