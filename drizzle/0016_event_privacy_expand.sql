CREATE TABLE personal_privacy_scope (
  application_id text NOT NULL,
  owner_scope text NOT NULL,
  level text NOT NULL CHECK (level IN ('infrastructure','managed','client-only')),
  lifecycle text NOT NULL CHECK (lifecycle IN ('plaintext','migrating','protected','revoked')),
  key_epoch integer NOT NULL CHECK (key_epoch > 0),
  wrapped_root_key bytea,
  state_revision bigint NOT NULL DEFAULT 0,
  migration_cursor text,
  migrated_objects bigint NOT NULL DEFAULT 0,
  updated_at timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (application_id, owner_scope),
  CHECK (level <> 'client-only' OR wrapped_root_key IS NULL)
);

CREATE TABLE personal_protected_object (
  application_id text NOT NULL,
  owner_scope text NOT NULL,
  object_id text NOT NULL,
  schema_id text NOT NULL,
  object_revision text NOT NULL,
  key_epoch integer NOT NULL CHECK (key_epoch > 0),
  storage_version bigint NOT NULL,
  envelope jsonb NOT NULL,
  equality_indexes jsonb NOT NULL DEFAULT '{}',
  byte_length bigint NOT NULL CHECK (byte_length >= 0),
  updated_at timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (application_id, owner_scope, object_id),
  FOREIGN KEY (application_id, owner_scope)
    REFERENCES personal_privacy_scope(application_id, owner_scope)
);
CREATE INDEX personal_protected_object_owner_idx
  ON personal_protected_object(application_id, owner_scope, object_id);

ALTER TABLE event
  ALTER COLUMN title DROP NOT NULL,
  ALTER COLUMN starts_at DROP NOT NULL,
  ALTER COLUMN timezone DROP NOT NULL,
  ALTER COLUMN timezone DROP DEFAULT,
  ALTER COLUMN body DROP NOT NULL,
  ALTER COLUMN body DROP DEFAULT,
  ALTER COLUMN reveal_guests DROP NOT NULL,
  ALTER COLUMN reveal_guests DROP DEFAULT,
  ADD COLUMN privacy_revision integer NOT NULL DEFAULT 0;

ALTER TABLE personal_privacy_scope ENABLE ROW LEVEL SECURITY;
ALTER TABLE personal_privacy_scope FORCE ROW LEVEL SECURITY;
ALTER TABLE personal_protected_object ENABLE ROW LEVEL SECURITY;
ALTER TABLE personal_protected_object FORCE ROW LEVEL SECURITY;
CREATE POLICY personal_privacy_scope_owner ON personal_privacy_scope
  USING (owner_scope = current_setting('grid.authenticated_user_id', true))
  WITH CHECK (owner_scope = current_setting('grid.authenticated_user_id', true));
CREATE POLICY personal_protected_object_owner ON personal_protected_object
  USING (owner_scope = current_setting('grid.authenticated_user_id', true))
  WITH CHECK (owner_scope = current_setting('grid.authenticated_user_id', true));
