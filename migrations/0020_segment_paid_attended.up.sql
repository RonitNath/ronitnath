-- Long-term per-segment bookkeeping, admin-set only (never guest input):
-- whether the person paid for a paid segment (dinner), and whether they
-- actually showed up to it. `attended` is NULL until recorded either way.
ALTER TABLE segment_rsvps ADD COLUMN paid INTEGER NOT NULL DEFAULT 0;
ALTER TABLE segment_rsvps ADD COLUMN attended INTEGER;
