//! `/platform/cluster` — the node's own account of itself.
//!
//! No charts. Every number here is a single reading of something the process
//! actually witnessed, and a chart of one node's raft membership would be a
//! picture of a number that changes twice a year.
//!
//! It is read rather than subscribed, because none of it came off the change
//! feed: raft membership, a socket count and a queue depth are not changes
//! anybody commanded. It is re-read on a timer instead, so an operator
//! watching an election sees it happen rather than sees a stale page.
//!
//! `voters` against `expected_voters` is the whole signal a half-formed
//! cluster gives: the first is what the membership config lists, the second is
//! what the deployment was provisioned for, and the two disagreeing is what
//! `/readyz` refuses on.

use std::time::Duration;

use leptos::prelude::*;
use leptos::task::spawn_local;
use rn_api::{ClusterView, RaftView};

use rn_ui::{Column, PageHead, Table};

use crate::panel::Facts;

/// How often the page asks again. Slow enough to be a page and not a poller,
/// quick enough that an election is visible while it is happening.
const REFRESH: Duration = Duration::from_secs(5);

#[component]
pub fn Cluster() -> impl IntoView {
    let view = RwSignal::new(None::<ClusterView>);
    let failed = RwSignal::new(false);

    let read = move || {
        spawn_local(async move {
            match rn_ui::query::<ClusterView>("cluster", &[]).await {
                Ok(fresh) => {
                    failed.set(false);
                    view.set(Some(fresh));
                }
                // The last reading stays on screen: a node that stopped
                // answering has not told us its membership changed.
                Err(_) => failed.set(true),
            }
        });
    };
    read();
    let handle = set_interval_with_handle(read, REFRESH).ok();
    on_cleanup(move || {
        if let Some(handle) = handle {
            handle.clear();
        }
    });

    view! {
        <PageHead title="Cluster">
            <Show when=move || failed.get()>
                <span class="state" data-state="disabled">
                    "unreachable"
                </span>
            </Show>
        </PageHead>
        {move || match view.get() {
            None => view! { <p class="none">"Asking the node."</p> }.into_any(),
            Some(cluster) => view! { <Readings cluster=cluster /> }.into_any(),
        }}
    }
}

#[component]
fn Readings(cluster: ClusterView) -> impl IntoView {
    // The readings are label-and-value pairs, not a list of rows: sorting them
    // or paging them would be sorting a fact list, so they are drawn as the
    // panel's `Facts` and the raft groups — which *are* rows — as a table.
    let facts = vec![
        ("Node", cluster.node.clone()),
        ("Version", cluster.version.clone()),
        ("Feed head", cluster.feed_head.to_string()),
        ("Subscribers", cluster.subscribers.to_string()),
        (
            "Observations pending",
            cluster.observations_pending.to_string(),
        ),
    ];
    let groups = vec![
        Raft::of("sqlite", cluster.raft.sqlite),
        Raft::of("cache", cluster.raft.cache),
    ];
    let columns = vec![
        Column::new("Raft group", |row: &Raft| row.name.to_owned()),
        Column::new("State", |row: &Raft| row.word.to_owned()).state(),
        Column::new("Voters", |row: &Raft| row.voters.to_string()).mono(),
        Column::new("Expected", |row: &Raft| row.expected.to_string()).mono(),
        Column::new("Leader", |row: &Raft| row.leader.clone()).mono(),
    ];

    view! {
        <div class="readings">
            <Facts facts=facts />
            <Table
                rows=Signal::derive(move || groups.clone())
                columns=columns
                empty="This node reports no raft group, which is not a state it can be in."
            />
        </div>
    }
}

/// One raft group, as a row.
///
/// Three states, and they are three different problems: a group that is not
/// answering, one that is answering with the wrong membership, and one that is
/// fine. The word carries the difference and the state ink colours it.
#[derive(Clone, PartialEq, Eq)]
struct Raft {
    name: &'static str,
    word: &'static str,
    voters: usize,
    expected: usize,
    leader: String,
}

impl Raft {
    fn of(name: &'static str, group: RaftView) -> Self {
        Self {
            name,
            word: if !group.healthy {
                "disabled"
            } else if group.ready() {
                "formed"
            } else {
                "unformed"
            },
            voters: group.voters,
            expected: group.expected_voters,
            leader: group
                .leader
                .map_or_else(|| "\u{2014}".to_owned(), |id| id.to_string()),
        }
    }
}
