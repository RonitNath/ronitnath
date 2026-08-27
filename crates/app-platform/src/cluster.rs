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
use rn_ui::PageHead;

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
    let groups = [
        ("sqlite", cluster.raft.sqlite),
        ("cache", cluster.raft.cache),
    ];
    let rows = groups
        .into_iter()
        .map(|(name, group)| view! { <GroupRow name=name group=group /> })
        .collect_view();

    view! {
        <table class="tbl">
            <thead>
                <tr>
                    <th scope="col">"Reading"</th>
                    <th scope="col">"Value"</th>
                </tr>
            </thead>
            <tbody>
                <Reading label="Node" value=cluster.node.clone() />
                <Reading label="Version" value=cluster.version.clone() />
                <Reading label="Feed head" value=cluster.feed_head.to_string() />
                <Reading label="Subscribers" value=cluster.subscribers.to_string() />
                <Reading
                    label="Observations pending"
                    value=cluster.observations_pending.to_string()
                />
            </tbody>
        </table>
        <table class="tbl">
            <thead>
                <tr>
                    <th scope="col">"Raft group"</th>
                    <th scope="col">"State"</th>
                    <th scope="col" class="num">
                        "Voters"
                    </th>
                    <th scope="col" class="num">
                        "Expected"
                    </th>
                    <th scope="col" class="num">
                        "Leader"
                    </th>
                </tr>
            </thead>
            <tbody>{rows}</tbody>
        </table>
    }
}

#[component]
fn Reading(
    /// What was read.
    label: &'static str,
    /// What it said.
    #[prop(into)]
    value: String,
) -> impl IntoView {
    view! {
        <tr>
            <td>{label}</td>
            <td class="mono num">{value}</td>
        </tr>
    }
}

#[component]
fn GroupRow(name: &'static str, group: RaftView) -> impl IntoView {
    // Three states, and they are three different problems: a group that is not
    // answering, one that is answering with the wrong membership, and one that
    // is fine.
    let state = if !group.healthy {
        "disabled"
    } else if group.ready() {
        "active"
    } else {
        "proposed"
    };
    let word = if !group.healthy {
        "unhealthy"
    } else if group.ready() {
        "formed"
    } else {
        "unformed"
    };
    view! {
        <tr>
            <td>{name}</td>
            <td>
                <span class="state" data-state=state>
                    {word}
                </span>
            </td>
            <td class="mono num">{group.voters.to_string()}</td>
            <td class="mono num">{group.expected_voters.to_string()}</td>
            <td class="mono num">
                {group.leader.map_or_else(|| "\u{2014}".to_owned(), |id| id.to_string())}
            </td>
        </tr>
    }
}
