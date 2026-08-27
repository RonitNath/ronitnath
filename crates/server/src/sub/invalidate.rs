//! Throwing away what a command made stale: a warm principal, and this node's
//! belief about which products are on.
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
//! The products projection rides the same task, and for the same reason. A
//! toggle on node 1 has to be answered by node 3, and the change feed is
//! already the thing every node reads: `ProductEnabled` and `ProductDisabled`
//! — and nothing else — make this node re-read the `product` table and replace
//! its [`ProductSet`]. That is one `SELECT` per toggle per node, and a toggle
//! is a human action.
//!
//! Wake-ups are hints: what is acted on comes from the durable read.

use std::sync::Arc;

use futures_util::StreamExt;
use rn_kernel::feed::{ClusterFeed, Feed, READ_LIMIT};
use rn_kernel::principal::PrincipalCache;
use rn_kernel::product::{self, ProductSet};
use rn_kernel::store::Store;
use rn_kernel::{Event, Offset};

use hiqlite::Client;

/// Start the invalidator. It runs for the life of the process.
pub fn spawn(
    feed: Arc<ClusterFeed>,
    principals: Arc<PrincipalCache>,
    store: Arc<Store<Client>>,
    products: Arc<ProductSet>,
) {
    tokio::spawn(async move {
        // Before any wake-up: a node that has just booted holds an empty set,
        // and the products enabled on this deployment were enabled before it
        // started. A read that fails leaves the set empty — every product off
        // until the next toggle or the next boot, which is the safe direction
        // and is loud in the log rather than quiet in a router.
        refresh(&store, &products).await;
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
            let mut toggled = false;
            for committed in events {
                if let Some(identity) = committed.event.touches_identity() {
                    principals.forget_identity(identity);
                }
                if let Some(person) = committed.event.touches_person() {
                    principals.forget_person(person);
                }
                toggled |= matches!(
                    committed.event,
                    Event::ProductEnabled { .. } | Event::ProductDisabled { .. }
                );
                cursor = cursor.max(committed.offset);
            }
            // Once per page of events, not once per event: two toggles in one
            // read are one re-read of the same small table.
            if toggled {
                refresh(&store, &products).await;
            }
        }
    });
}

/// Re-read the enabled set and replace this node's projection.
///
/// The mask holds no truth of its own — it is derived from the table and can
/// always be derived again — so a read that fails keeps what this node has
/// rather than clearing it: stale is recoverable by the next toggle, and
/// wrong-way-open is not.
async fn refresh(store: &Store<Client>, products: &ProductSet) {
    match product::read(&store.reads()).await {
        Ok(bits) => {
            if bits != products.bits() {
                products.replace(bits);
                tracing::info!(
                    enabled = ?products.slugs(),
                    "the enabled products changed on this node"
                );
            }
        }
        Err(error) => tracing::warn!(%error, "the enabled products could not be read"),
    }
}
