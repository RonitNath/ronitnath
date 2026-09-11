CREATE TABLE IF NOT EXISTS calendar_scope (
  scope_id text PRIMARY KEY,
  owner_id text NOT NULL,
  time_zone text NOT NULL DEFAULT 'UTC',
  time_zone_version bigint NOT NULL DEFAULT 0,
  reported_session_id text,
  reported_at timestamptz,
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS calendar_item (
  scope_id text NOT NULL REFERENCES calendar_scope(scope_id) ON DELETE CASCADE,
  id text NOT NULL,
  kind text NOT NULL CHECK (kind IN ('event','task','work-block')),
  title text NOT NULL,
  timing jsonb,
  recurrence jsonb,
  task jsonb,
  metadata jsonb,
  linked_task_id text,
  committed boolean NOT NULL DEFAULT false,
  status text NOT NULL DEFAULT 'active' CHECK (status IN ('active','cancelled')),
  version bigint NOT NULL DEFAULT 1,
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (scope_id,id)
);
CREATE INDEX IF NOT EXISTS calendar_item_scope_kind ON calendar_item(scope_id,kind);
ALTER TABLE calendar_item ADD COLUMN IF NOT EXISTS metadata jsonb;

CREATE TABLE IF NOT EXISTS calendar_task_completion (
  scope_id text NOT NULL,
  item_id text NOT NULL,
  occurrence_id text NOT NULL,
  completed_at timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (scope_id,item_id,occurrence_id),
  FOREIGN KEY (scope_id,item_id) REFERENCES calendar_item(scope_id,id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS calendar_resource (
  scope_id text NOT NULL REFERENCES calendar_scope(scope_id) ON DELETE CASCADE,
  id text NOT NULL,
  name text NOT NULL,
  capacity integer NOT NULL CHECK (capacity > 0),
  availability jsonb NOT NULL DEFAULT '[]'::jsonb,
  closures jsonb NOT NULL DEFAULT '[]'::jsonb,
  buffer_before_minutes integer NOT NULL DEFAULT 0 CHECK (buffer_before_minutes >= 0),
  buffer_after_minutes integer NOT NULL DEFAULT 0 CHECK (buffer_after_minutes >= 0),
  version bigint NOT NULL DEFAULT 1,
  PRIMARY KEY (scope_id,id)
);

CREATE TABLE IF NOT EXISTS calendar_booking (
  scope_id text NOT NULL REFERENCES calendar_scope(scope_id) ON DELETE CASCADE,
  id text NOT NULL,
  state text NOT NULL CHECK (state IN ('hold','confirmed','cancelled')),
  starts_at timestamptz NOT NULL,
  ends_at timestamptz NOT NULL,
  expires_at timestamptz,
  version bigint NOT NULL DEFAULT 1,
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (scope_id,id),
  CHECK (ends_at > starts_at),
  CHECK ((state = 'hold' AND expires_at IS NOT NULL) OR state <> 'hold')
);

CREATE TABLE IF NOT EXISTS calendar_booking_resource (
  scope_id text NOT NULL,
  booking_id text NOT NULL,
  resource_id text NOT NULL,
  quantity integer NOT NULL CHECK (quantity > 0),
  PRIMARY KEY (scope_id,booking_id,resource_id),
  FOREIGN KEY (scope_id,booking_id) REFERENCES calendar_booking(scope_id,id) ON DELETE CASCADE,
  FOREIGN KEY (scope_id,resource_id) REFERENCES calendar_resource(scope_id,id) ON DELETE RESTRICT
);
CREATE INDEX IF NOT EXISTS calendar_booking_overlap ON calendar_booking(scope_id,starts_at,ends_at)
  WHERE state IN ('hold','confirmed');

CREATE TABLE IF NOT EXISTS calendar_mutation (
  scope_id text NOT NULL,
  operation text NOT NULL,
  idempotency_key text NOT NULL,
  result jsonb,
  created_at timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (scope_id,operation,idempotency_key)
);
