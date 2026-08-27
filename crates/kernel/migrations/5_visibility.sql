-- Migration 5 — one index, so "everything I can see" is bounded by its page.
--
-- `resource_owner_idx (owner_party_id, kind, status)` from migration 1 answers
-- "is this mine", which is a seek, and that is all it was ever asked. The
-- visibility statement asks something else — "the newest N of mine" — and
-- without the ordering columns in the index that means reading every resource
-- the party owns and sorting it. At 10⁴ owned documents the endpoint that runs
-- it measured 294 ms against a 5 ms budget (finding 1,
-- `docs/perf/2026-08-27.md`).
--
-- With `(created_at, id)` on the end, in the direction the query reads them,
-- the ownership branch of `relation::LIST_VISIBLE_SQL` becomes an ordered walk
-- that stops at the page. `status` is left out for the reason
-- `resource_kind_created_idx` leaves it out: a column between the prefix and
-- the ordering columns that the query filters with `<>` cannot be a prefix
-- constraint, so it would cost the ordering and buy nothing.
--
-- The old index stays. It is narrower, it is what an ownership *check* wants,
-- and `check()` is the hottest query in the deployment.

CREATE INDEX resource_owner_created_idx
    ON resource (owner_party_id, kind, created_at DESC, id DESC);
