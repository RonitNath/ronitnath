CREATE TABLE workflow_dispatch_cursor (
  namespace text NOT NULL,
  consumer text NOT NULL,
  org_id text NOT NULL,
  seq bigint NOT NULL DEFAULT 0 CHECK(seq >= 0),
  updated_at timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY(namespace, consumer, org_id)
);--> statement-breakpoint

CREATE TABLE mail_intent (
  id uuid PRIMARY KEY,
  deduplication_key text NOT NULL UNIQUE,
  kind text NOT NULL,
  recipient text NOT NULL,
  message jsonb,
  correlation_id text NOT NULL,
  state text NOT NULL DEFAULT 'pending' CHECK(state IN ('pending','dispatching','accepted','failed','expired')),
  workflow_id text,
  dispatch_generation integer NOT NULL DEFAULT 0,
  created_at timestamptz NOT NULL DEFAULT now(),
  expires_at timestamptz NOT NULL,
  completed_at timestamptz
);--> statement-breakpoint
CREATE INDEX mail_intent_pending_idx ON mail_intent(state, created_at);--> statement-breakpoint

CREATE TABLE mail_attempt (
  id bigserial PRIMARY KEY,
  intent_id uuid NOT NULL REFERENCES mail_intent(id) ON DELETE CASCADE,
  ordinal smallint NOT NULL,
  outcome text NOT NULL,
  detail text,
  attempted_at timestamptz NOT NULL DEFAULT now(),
  UNIQUE(intent_id, ordinal)
);
