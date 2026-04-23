ALTER TABLE events ADD COLUMN approximate_location_name TEXT;

ALTER TABLE event_invitees
ADD COLUMN location_approved INTEGER NOT NULL DEFAULT 1
CHECK (location_approved IN (0, 1));

UPDATE events
SET self_signup_requires_approval = 1
WHERE signup_mode = 'self_signup';
