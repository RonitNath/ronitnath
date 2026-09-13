ALTER TABLE calendar_mutation ADD COLUMN IF NOT EXISTS command_hash text;
ALTER TABLE calendar_mutation ADD COLUMN IF NOT EXISTS actor_id text;
ALTER TABLE calendar_scope ADD COLUMN IF NOT EXISTS active_zone_session_id text;
ALTER TABLE calendar_scope ADD COLUMN IF NOT EXISTS zone_generation bigint NOT NULL DEFAULT 0;

CREATE TABLE IF NOT EXISTS calendar_zone_session (
  scope_id text NOT NULL REFERENCES calendar_scope(scope_id) ON DELETE CASCADE,
  session_id text NOT NULL,
  generation bigint NOT NULL,
  last_sequence bigint NOT NULL DEFAULT 0,
  activated_at timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY(scope_id,session_id),
  UNIQUE(scope_id,generation)
);
