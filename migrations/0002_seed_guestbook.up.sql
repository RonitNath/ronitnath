-- Demonstrates the seed-migration pattern: a fresh fork gets a welcome row so
-- the guestbook is never empty. Fine to `sqlx database reset` freely — this
-- migration re-runs and restores it.
INSERT INTO guestbook_entries (author, message)
VALUES ('stage_1', 'Welcome! Leave a note below.');
