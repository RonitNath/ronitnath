//! The change feed: durable in the audit table, woken over the fabric.
//!
//! ## Where `append` is
//!
//! There is no `append` method, and its absence is the design. An event is
//! appended by the command's own audit statement, inside the command's own
//! transaction — so an event cannot exist without the change that produced it,
//! and a change cannot commit without its event. A method here would be a
//! second way to append, reachable without a transaction, and the first thing
//! it would buy is a feed that disagrees with the database.
//!
//! What the trait carries is everything else: [`Feed::notify`], the wake-up
//! after a commit; [`Feed::subscribe`], a stream of offsets to wake on; and
//! [`Feed::read`], the durable read a consumer resumes with. Wake-ups may be
//! lost, duplicated or arrive out of order — they are a hint that there is
//! something to read, never the thing itself. Rung 6 swaps hiqlite's
//! listen/notify for Zenoh behind exactly this trait.

mod cluster;
mod local;
#[cfg(test)]
mod tests;

pub use cluster::ClusterFeed;
pub use local::LocalFeed;

use std::future::Future;

use futures_util::Stream;

use crate::Offset;
use crate::bind;
use crate::error::Outcome;
use crate::event::Committed;
use crate::store::Reads;

/// How many events a single [`Feed::read`] will hand back.
pub const READ_LIMIT: usize = 512;

/// The change feed.
pub trait Feed: Send + Sync {
    /// Announce that everything up to `offset` is committed.
    ///
    /// Called after the transaction, never inside it: a notification sent
    /// before the commit is a subscriber reading rows that may still roll
    /// back. Best effort — a lost wake-up costs latency, not correctness,
    /// because the next one covers it and a consumer holds its own offset.
    fn notify(&self, offset: Offset) -> impl Future<Output = ()> + Send;

    /// A stream of committed offsets.
    fn subscribe(&self) -> impl Stream<Item = Offset> + Send + 'static;

    /// Read events after `offset`, in order, at most `limit` of them.
    fn read(
        &self,
        offset: Offset,
        limit: usize,
    ) -> impl Future<Output = Outcome<Vec<Committed>>> + Send;
}

/// The durable read both implementations share: the audit table, in id order.
///
/// `id > $1` over the primary key is a range scan on the rowid — the cheapest
/// thing SQLite can do, and what `explain_feed_read` asserts.
pub(crate) async fn read_from(
    store: &impl Reads,
    offset: Offset,
    limit: usize,
) -> Outcome<Vec<Committed>> {
    let limit = limit.min(READ_LIMIT) as i64;
    let rows = store
        .query::<crate::audit::Recorded>(
            "SELECT id, request_digest, payload FROM audit WHERE id > $1 ORDER BY id LIMIT $2",
            bind![offset.min(i64::MAX as u64) as i64, limit],
        )
        .await?;
    Ok(rows
        .into_iter()
        .filter_map(|row| {
            // A row this build cannot read is skipped, not guessed at: that is
            // a consumer resuming across a release that added an event kind.
            row.event.map(|event| Committed {
                offset: row.offset,
                event,
            })
        })
        .collect())
}
