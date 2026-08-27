-- Migration 9 — what each node says about itself, and the audit's last filter.
--
-- Two additions. The first is the only table in this schema that is not a
-- record of something anybody commanded; the second is the index without which
-- one of the operator's audit filters would read the deployment's whole
-- history.
--
-- Migrations 1–8 are frozen: hiqlite hashes them, so a column appended to one
-- would invalidate every already-migrated node.

-- ------------------------------------------------------------ node_report --
-- One row per node, upserted by that node from the observation lane.
--
-- `/api/q/cluster` is *this node's* account of itself, which is the right
-- answer to "what am I" and the wrong answer to "what is the deployment".
-- Three voters have three of those pages and none of them can see the other
-- two, so "which version is each node running" and "is a rollout half-done"
-- were not questions the deployment could answer at all — and a killed node's
-- absence was invisible from its peers' screens, which is the half of story
-- G20 that was only ever half-true.
--
-- It is an *observation*, not a command and not on the change feed. That is
-- the observation lane's existing contract (`crates/kernel/src/lib.rs`), and
-- it is the only shape that works here: a heartbeat every fifteen seconds on
-- the feed would add 5 760 rows a day per node to `audit`, which is the change
-- feed, is the audit log, and has no retention by ruling
-- (`docs/rebuild/plan.md` §Product contract). A deployment would then be
-- mostly a record of its own pulse.
--
-- The primary key is the hiqlite node id, so a node overwrites its own row and
-- nobody else's, and a restart reuses the row rather than growing the table.
-- Nothing here is nullable: a node that cannot say what version it is running
-- is a node that has not reported, and the *absence* of a fresh
-- `reported_at` is how that is said. There is no `status` column for the same
-- reason — "not reporting" is computed from `reported_at` against the interval
-- by the query that renders it, so the screen says what it witnessed rather
-- than trusting a word a departed node wrote before it left.
CREATE TABLE node_report
(
    -- `RN_SITE_HQL_NODE_ID` — which voter this is, in raft's own numbering.
    node_id     INTEGER PRIMARY KEY,
    -- `RN_SITE_NODE` — what a human calls it. Two different names for two
    -- different readers, and neither derives the other.
    node        TEXT    NOT NULL,
    -- The revision the process is running. The rollout verdict is counted
    -- from the distinct values of this column and from nothing else.
    version     TEXT    NOT NULL,
    -- What this node believes it is in the sqlite raft group: 'leader',
    -- 'follower' or 'unknown' — its own belief, which is why two nodes
    -- claiming leader is a reading and not a contradiction the table forbids.
    raft_role   TEXT    NOT NULL,
    -- Where the change feed ended when the row was written.
    feed_head   INTEGER NOT NULL,
    -- When it was written. The only freshness signal there is.
    reported_at INTEGER NOT NULL
);

-- ------------------------------------------------------- audit by command --
-- Migration 7 gave the operator's audit screen an actor index, a hat index and
-- a time index. The fourth control is `command`, and it had none: filtering
-- for every `sign-in-as` this deployment has ever run was a walk of the whole
-- log.
--
-- `(command, id)` in that order so the filter is a seek and the newest-first
-- walk inside it needs no sort — the same shape, and for the same reason, as
-- `audit_acting_as_idx`.
CREATE INDEX audit_command_idx ON audit (command, id);

-- ------------------------------------------------------ tokens by client --
-- `oidc_token` is indexed by session (the hot bearer lookup), by family (the
-- rotation-reuse rule), by `(person_id, client_id)` and by expiry. The
-- operator's client registry asks the one question none of those answers:
-- everything one client has ever issued, for its *last issued at* and its
-- count of live tokens.
--
-- Leading on `client_id` because that is the seek; `created_at` behind it so
-- the newest issuance is the first row of the range rather than a max() over
-- everything the client ever minted.
CREATE INDEX oidc_token_client_idx ON oidc_token (client_id, created_at);
