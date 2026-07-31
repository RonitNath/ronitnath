-- Display-only attendee number for the public landing page (social proof).
-- Never enforced, never derived — set by the admin CLI / seed; NULL hides it.
ALTER TABLE events ADD COLUMN headcount INTEGER;
