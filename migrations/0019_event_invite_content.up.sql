-- Admin-authored invite-page content, both private-tier only (they carry
-- contact info): a notice banner shown at the top of a personal invite,
-- and a highly-abridged "day at a glance" plan. Simple inline HTML, same
-- trust model as entry_instructions.
ALTER TABLE events ADD COLUMN notice_html TEXT NOT NULL DEFAULT '';
ALTER TABLE events ADD COLUMN quick_plan_html TEXT NOT NULL DEFAULT '';
