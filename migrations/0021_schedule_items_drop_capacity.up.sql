-- No event here has a capacity limit; drop the unused column.
ALTER TABLE schedule_items DROP COLUMN capacity;
