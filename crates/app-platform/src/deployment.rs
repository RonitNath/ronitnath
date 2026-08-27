//! `/platform/deployment` — what the deployment is, as three of its nodes see
//! it rather than as one of them does.
//!
//! No charts. Every number here is a single reading of something a process
//! actually witnessed: which version each node is running, what its raft group
//! told it about the leader, where its feed ended when it last spoke. The
//! rollout verdict is counted from those versions and says only what it
//! counted — *settled*, *in progress*, *divergent* — and a node whose last
//! reading is older than three report intervals reads as **not reporting**,
//! because a node that has gone cannot write "gone".
//!
//! Read on a timer rather than subscribed, and for the same reason
//! `/api/q/cluster` always was: none of it came off the change feed. A node's
//! heartbeat is an observation, and a socket that pushed raft membership would
//! be pushing something no command ever caused.
//!
//! The raft groups are this node's own, from `GET /api/q/cluster`, which is
//! the section `/platform/cluster` used to be a page of. Two readings of the
//! feed sit beside them — where it ends, and how many commands landed in the
//! last day — and neither is divided into the other.

use std::time::Duration;

use leptos::prelude::*;
use leptos::task::spawn_local;
use rn_api::{ClusterView, RaftView};
use rn_ui::{Column, PageHead, Priority, Table};

use crate::keys::KeyTable;
use crate::panel::{Facts, when};
use crate::rows::{Feed, Node};

/// How often the page asks again. Slow enough to be a page and not a poller,
/// quick enough that an election is visible while it is happening and that a
/// node's fifteen-second silence is noticed inside a screenful of time.
const REFRESH: Duration = Duration::from_secs(5);

#[component]
pub fn Deployment() -> impl IntoView {
    let cluster = RwSignal::new(None::<ClusterView>);
    let nodes = RwSignal::new(Vec::<Node>::new());
    let feed = RwSignal::new(None::<Feed>);
    let unreachable = RwSignal::new(false);

    let read = move || {
        spawn_local(async move {
            let asked = (
                rn_ui::query::<ClusterView>("cluster", &[]).await,
                rn_ui::query::<Vec<Node>>("platform-nodes", &[]).await,
                rn_ui::query::<Vec<Feed>>("platform-feed", &[]).await,
            );
            match asked {
                (Ok(view), Ok(rows), Ok(readings)) => {
                    unreachable.set(false);
                    cluster.set(Some(view));
                    nodes.set(rows);
                    feed.set(readings.first().copied());
                }
                // The last reading stays on screen. A node that stopped
                // answering has not told us its membership changed, and
                // blanking the page would be this screen inventing an outage.
                _ => unreachable.set(true),
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
        <PageHead title="Deployment">
            <Show when=move || unreachable.get()>
                <span class="state" data-state="disabled">
                    "unreachable"
                </span>
            </Show>
        </PageHead>
        <div class="deployment">
            <Nodes nodes=nodes />
            <div class="readings-pair">
                <Rollout nodes=nodes />
                <FeedReadings feed=feed />
            </div>
            <div class="readings-pair">
                <Raft cluster=cluster />
                <KeyTable />
            </div>
        </div>
    }
}

/// One row per node: what it is, what it is running, and when it last said so.
#[component]
fn Nodes(nodes: RwSignal<Vec<Node>>) -> impl IntoView {
    let rows = Signal::derive(move || nodes.get());
    let columns = vec![
        Column::new("Node", |row: &Node| row.node.clone()),
        Column::new("Version", |row: &Node| row.version.clone()).mono(),
        // A node that has stopped reporting has a raft role that is a
        // memory, so the state column says what the reading is worth rather
        // than repeating a word nobody can vouch for.
        Column::new("Raft", |row: &Node| {
            if row.reporting {
                row.raft_role.clone()
            } else {
                "not reporting".to_owned()
            }
        })
        .state(),
        Column::new("Feed head", |row: &Node| row.feed_head.to_string()).mono(),
        Column::new("Last report", |row: &Node| ago(row.reported_ago))
            .mono()
            .titled(|row: &Node| when(row.reported_at))
            .priority(Priority::Secondary),
        Column::new("Id", |row: &Node| row.node_id.to_string())
            .mono()
            .priority(Priority::Tertiary),
    ];
    view! {
        <Table
            rows=rows
            columns=columns
            empty="No node has reported. The observation lane writes one row per node every fifteen seconds, so this is a deployment that has not yet ticked."
        />
    }
}

/// How long ago, in the unit an operator reads it in.
///
/// Seconds while it is seconds, minutes after that, hours after that. Never a
/// rate and never a fraction: this is one subtraction of two instants the
/// deployment holds.
#[must_use]
pub fn ago(seconds: i64) -> String {
    match seconds.max(0) {
        s if s < 90 => format!("{s}s ago"),
        s if s < 5_400 => format!("{}m ago", s / 60),
        s if s < 172_800 => format!("{}h ago", s / 3_600),
        s => format!("{}d ago", s / 86_400),
    }
}

/// The verdict, and the versions it counted.
#[component]
fn Rollout(nodes: RwSignal<Vec<Node>>) -> impl IntoView {
    let verdict = Signal::derive(move || {
        nodes
            .get()
            .first()
            .map(|row| (row.rollout.clone(), row.versions.clone()))
    });
    view! {
        {move || match verdict.get() {
            None => view! { <p class="none">"No node has reported a version."</p> }.into_any(),
            Some((word, versions)) => {
                let facts = vec![
                    ("Rollout", word),
                    // Named, not counted: "two versions" is a number, and the
                    // question an operator has is which two.
                    ("Versions", versions.join(" \u{00b7} ")),
                ];
                view! { <Facts facts=facts /> }.into_any()
            }
        }}
    }
}

/// The feed's two readings.
#[component]
fn FeedReadings(feed: RwSignal<Option<Feed>>) -> impl IntoView {
    view! {
        {move || match feed.get() {
            None => view! { <p class="none">"The feed has not answered."</p> }.into_any(),
            Some(feed) => {
                let facts = vec![
                    ("Offset", feed.offset.to_string()),
                    ("Commands today", feed.commands_today.to_string()),
                ];
                view! { <Facts facts=facts /> }.into_any()
            }
        }}
    }
}

/// This node's own raft membership, and the two counters beside it.
#[component]
fn Raft(cluster: RwSignal<Option<ClusterView>>) -> impl IntoView {
    view! {
        {move || match cluster.get() {
            None => view! { <p class="none">"Asking the node."</p> }.into_any(),
            Some(view) => {
                let groups = vec![
                    Group::of("sqlite", view.raft.sqlite),
                    Group::of("cache", view.raft.cache),
                ];
                let facts = vec![
                    ("Subscribers", view.subscribers.to_string()),
                    ("Observations pending", view.observations_pending.to_string()),
                ];
                let columns = vec![
                    Column::new("Raft group", |row: &Group| row.name.to_owned()),
                    Column::new("State", |row: &Group| row.word.to_owned()).state(),
                    Column::new("Voters", |row: &Group| row.voters.to_string()).mono(),
                    Column::new("Expected", |row: &Group| row.expected.to_string()).mono(),
                    Column::new("Leader", |row: &Group| row.leader.clone()).mono(),
                ];
                view! {
                    <Table
                        rows=Signal::derive(move || groups.clone())
                        columns=columns
                        empty="This node reports no raft group, which is not a state it can be in."
                    />
                    <Facts facts=facts />
                }
                    .into_any()
            }
        }}
    }
}

/// One raft group, as a row.
///
/// Three states, and they are three different problems: a group that is not
/// answering, one answering with the wrong membership, and one that is fine.
#[derive(Clone, PartialEq, Eq)]
struct Group {
    name: &'static str,
    word: &'static str,
    voters: usize,
    expected: usize,
    leader: String,
}

impl Group {
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
