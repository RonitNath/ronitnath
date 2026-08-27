//! Throwing away a warm principal when a command says it is stale.
//!
//! Resolving a cookie is cached (`kernel::principal::cache`) because it is the
//! first three queries of every authenticated request and it answers the same
//! thing until something about that identity changes. "Until something
//! changes" needs a witness, and the change feed is it: every command that
//! touched an identity or a person says so in its event, and this task turns
//! that into an eviction.
//!
//! It is what makes `SignOut` and `RevokeSession` mean anything. Without it a
//! deleted session row keeps answering out of the cache — the lost laptop stays
//! signed in until the entry ages out, which is a revocation that is really
//! only an expiry, and expiry is not revocation.
//!
//! Wake-ups are hints: what is acted on comes from the durable read.

use std::sync::Arc;

use futures_util::StreamExt;
use rn_kernel::Offset;
use rn_kernel::feed::{ClusterFeed, Feed, READ_LIMIT};
use rn_kernel::principal::PrincipalCache;

/// Start the invalidator. It runs for the life of the process.
pub fn spawn(feed: Arc<ClusterFeed>, principals: Arc<PrincipalCache>) {
    tokio::spawn(async move {
        let mut wakes = Box::pin(feed.subscribe());
        // Zero, not the current head: a node that has just started holds no
        // cached principals, so re-reading a bounded page of history on the
        // first wake evicts nothing and costs one indexed range scan.
        let mut cursor: Offset = 0;
        while let Some(offset) = wakes.next().await {
            if offset <= cursor {
                continue;
            }
            let events = match feed.read(cursor, READ_LIMIT).await {
                Ok(events) => events,
                Err(error) => {
                    tracing::warn!(%error, "the change feed could not be read for invalidation");
                    continue;
                }
            };
            for committed in events {
                if let Some(identity) = committed.event.touches_identity() {
                    principals.forget_identity(identity);
                }
                if let Some(person) = committed.event.touches_person() {
                    principals.forget_person(person);
                }
                cursor = cursor.max(committed.offset);
            }
        }
    });
}
