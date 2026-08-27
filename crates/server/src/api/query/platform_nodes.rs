//! What every node says about itself, and where the feed has got to.
//!
//! `GET /api/q/cluster` is *this node's* account of itself, asked of the node
//! that answers it. Three voters therefore have three of those pages and none
//! of them can see the other two, so "which version is each node running" and
//! "is a rollout half-finished" were not questions the deployment could answer
//! at all. `node_report` is the fix, and it is a table each node upserts its
//! own row into from the observation lane
//! ([`crate::observe::report`]) — an observation, not a command, and not on
//! the change feed.
//!
//! Two things this module refuses to do.
//!
//! It does not report a node's *status*. It reports when the node last spoke,
//! and derives `reporting` from that against the interval. A departed node
//! cannot write "gone", so a status column would be a word left behind by
//! whoever was last able to write it; the absence of a fresh reading is the
//! only honest signal there is, and [`STALE_AFTER`] is how long the screen
//! waits before saying so.
//!
//! It does not extrapolate a rate. B4.4's feed growth is two readings — where
//! the feed ends, and how many commands landed in the last day — and neither
//! is divided by the other into a number the process never measured.

use rn_kernel::bind;
use rn_kernel::store::{Count, Cursor, FromRow, Reads, RowError};
use rn_kernel::{Outcome, Timestamp};
use serde_json::json;

use super::{Params, Row};

/// How long after its last reading a node reads as *not reporting*.
///
/// Three times the report interval (`crate::observe::REPORT_INTERVAL`), so one
/// missed tick — a busy node, an election, a slow disk — is not an outage, and
/// a node that has really gone is named inside a minute.
pub const STALE_AFTER: Timestamp = 45;

/// A day, for the second of the two feed readings.
const DAY: Timestamp = 86_400;

/// Every node's row, in node order. The whole table, and the table has one row
/// per voter: three, on the largest formation this deployment has.
pub(super) const NODES: &str = "SELECT node_id, node, version, raft_role, feed_head, reported_at \
     FROM node_report ORDER BY node_id";

/// Where the change feed ends.
///
/// `max()` over the primary key, which SQLite answers by seeking the last
/// entry of the b-tree rather than by reading it — `SEARCH audit`, one row.
/// The spelling matters: `ORDER BY id DESC LIMIT 1` returns the same answer
/// and plans as a scan, which the gate cannot tell from a real one.
pub(super) const HEAD: &str = "SELECT max(id) AS n FROM audit";

/// How many commands landed in the last day. `audit_at_idx` is what makes it a
/// range seek instead of a pass over the deployment's whole history — which is
/// the same table, and has no retention.
pub(super) const TODAY: &str = "SELECT count(*) AS n FROM audit WHERE at > $1";

struct Report {
    node_id: i64,
    node: String,
    version: String,
    raft_role: String,
    feed_head: i64,
    reported_at: Timestamp,
}

impl FromRow for Report {
    fn from_row(row: &mut impl Cursor) -> Result<Self, RowError> {
        Ok(Self {
            node_id: row.int("node_id")?,
            node: row.text("node")?,
            version: row.text("version")?,
            raft_role: row.text("raft_role")?,
            feed_head: row.int("feed_head")?,
            reported_at: row.int("reported_at")?,
        })
    }
}

/// What the versions in hand add up to.
///
/// Counted from the rows, so it can only say what it counted: one version is
/// settled, two is a rollout in progress, three or more is divergent — a
/// formation where nobody can say what the deployment is running.
#[must_use]
pub fn rollout(versions: &[String]) -> &'static str {
    match versions.len() {
        0 | 1 => "settled",
        2 => "in progress",
        _ => "divergent",
    }
}

/// The distinct versions the reporting rows carry, in order, deduplicated.
///
/// A node that is not reporting is left out: its version is what it was
/// running when it last spoke, and counting it would make a stopped node hold
/// the deployment in "rollout in progress" forever.
fn versions(rows: &[Report], now: Timestamp) -> Vec<String> {
    let mut seen: Vec<String> = rows
        .iter()
        .filter(|row| reporting(row.reported_at, now))
        .map(|row| row.version.clone())
        .collect();
    seen.sort_unstable();
    seen.dedup();
    seen
}

/// Whether a reading is fresh enough to be this node's current state.
const fn reporting(reported_at: Timestamp, now: Timestamp) -> bool {
    now - reported_at <= STALE_AFTER
}

/// `/api/q/platform-nodes` — one row per node, and the deployment's verdict.
///
/// The verdict rides on every row rather than in an envelope of its own,
/// because the wire shape of a query is an array of rows and a synthetic row
/// that was not a node would be a node the table had to know to skip. Every
/// row carries the same words because every row counted the same set.
pub(super) async fn list(reads: &impl Reads, _params: &Params) -> Outcome<Vec<Row>> {
    let now = reads.now();
    let rows = reads.query::<Report>(NODES, bind![]).await?;
    let versions = versions(&rows, now);
    let verdict = rollout(&versions);
    Ok(rows
        .into_iter()
        .map(|row| {
            let fresh = reporting(row.reported_at, now);
            Row {
                // Zero-padded, so a keyed map reads in node order rather than
                // lexicographically wrong at every power of ten.
                key: format!("{:020}", row.node_id),
                value: json!({
                    "node_id": row.node_id,
                    "node": row.node,
                    "version": row.version,
                    // Its own belief about its own group, from its own reading.
                    "raft_role": row.raft_role,
                    "feed_head": row.feed_head,
                    "reported_at": row.reported_at,
                    // How long ago, in seconds, so the screen renders an age
                    // rather than doing arithmetic against a clock that is not
                    // the deployment's.
                    "reported_ago": (now - row.reported_at).max(0),
                    "reporting": fresh,
                    "rollout": verdict,
                    "versions": versions.clone(),
                }),
            }
        })
        .collect())
}

/// `/api/q/platform-feed` — the two readings, and nothing derived from them.
///
/// One row, because it is one set of readings about one thing. `offset` is
/// where the feed ends; `commands_today` is how many rows landed in the last
/// day. A rate would be a third number nothing measured.
pub(super) async fn feed(reads: &impl Reads, _params: &Params) -> Outcome<Vec<Row>> {
    let now = reads.now();
    // An empty log has no head, which is zero rather than an error: a
    // deployment that has run no command has a feed that ends where it began.
    let head = reads
        .query_opt::<Count>(HEAD, bind![])
        .await?
        .map_or(0, |row| row.0);
    let today = reads
        .query_opt::<Count>(TODAY, bind![now - DAY])
        .await?
        .map_or(0, |row| row.0);
    Ok(vec![Row {
        key: "feed".to_owned(),
        value: json!({
            "offset": head,
            "commands_today": today,
        }),
    }])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(version: &str, reported_at: Timestamp) -> Report {
        Report {
            node_id: 1,
            node: "local".to_owned(),
            version: version.to_owned(),
            raft_role: "leader".to_owned(),
            feed_head: 40,
            reported_at,
        }
    }

    #[test]
    fn the_verdict_is_counted_from_the_versions_and_never_guessed() {
        assert_eq!(rollout(&[]), "settled");
        assert_eq!(rollout(&["a".to_owned()]), "settled");
        assert_eq!(rollout(&["a".to_owned(), "b".to_owned()]), "in progress");
        assert_eq!(
            rollout(&["a".to_owned(), "b".to_owned(), "c".to_owned()]),
            "divergent"
        );
    }

    #[test]
    fn a_row_older_than_three_intervals_has_stopped_reporting() {
        let now = 1_800_000_000;
        assert!(reporting(now, now));
        assert!(reporting(now - STALE_AFTER, now));
        assert!(!reporting(now - STALE_AFTER - 1, now));
    }

    #[test]
    fn a_node_that_stopped_reporting_does_not_hold_the_rollout_open() {
        let now = 1_800_000_000;
        let rows = vec![
            report("9f21ac3", now),
            report("9f21ac3", now - 2),
            // The killed one, still carrying the version it left on.
            report("8b0e114", now - 600),
        ];
        let settled = versions(&rows, now);
        assert_eq!(settled, vec!["9f21ac3".to_owned()]);
        assert_eq!(rollout(&settled), "settled");

        // While it is still speaking, the two versions are two versions and
        // both are named.
        let rows = vec![report("9f21ac3", now), report("8b0e114", now - 2)];
        let both = versions(&rows, now);
        assert_eq!(both, vec!["8b0e114".to_owned(), "9f21ac3".to_owned()]);
        assert_eq!(rollout(&both), "in progress");
    }
}
