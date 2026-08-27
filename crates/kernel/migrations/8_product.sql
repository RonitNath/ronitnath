-- Migration 8 — which products this deployment has turned on.
--
-- The table carries *enablement and nothing else*. What a product is — its
-- name, what it is for, the routes it mounts — is compiled in
-- (`kernel::product::CATALOGUE`), for the reason
-- `crates/kernel/src/relation/vocabulary.rs` gives about unregistered kinds: a
-- slug the binary does not carry admits nothing, so a typo is a refusal rather
-- than a row nothing will ever read.
--
-- **No row means disabled.** Default-deny, so a release that adds a product
-- does not turn it on across every deployment the moment they upgrade. The
-- row exists once somebody has decided, and then it also says who and when —
-- which is the other half of what the products screen renders.
--
-- There is no product *data* here and there never will be: disabling takes the
-- routes away and leaves everything the product wrote exactly where it is
-- (story 5, "a disabled product's routes vanish and its data stays"). Per-
-- product settings are B5.7, deferred until there is a second setting to shape
-- the table around.
CREATE TABLE product
(
    -- The catalogue's slug. TEXT and not a foreign key, because the thing it
    -- names is in the binary rather than in another table.
    slug       TEXT    PRIMARY KEY,
    -- 0 or 1. A row that says 0 is a product somebody turned off, which is a
    -- different fact from a product nobody has ever decided about — the second
    -- is the absent row, and both read as disabled.
    enabled    INTEGER NOT NULL CHECK (enabled IN (0, 1)),
    changed_at INTEGER NOT NULL,
    -- The party the change is attributed to, exactly as `audit.acting_as`
    -- records it. NULL for a row no command wrote.
    changed_by INTEGER REFERENCES party (id)
);
