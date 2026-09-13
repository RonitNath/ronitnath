CREATE TABLE IF NOT EXISTS fleet_notification (
  tenant_id text NOT NULL, id text NOT NULL, recipient_id text NOT NULL, subject jsonb NOT NULL,
  group_key text NOT NULL, conversation jsonb, policy_id text NOT NULL, policy_version text NOT NULL,
  revision bigint NOT NULL DEFAULT 1 CHECK(revision > 0), content jsonb NOT NULL,
  priority integer NOT NULL DEFAULT 0, material boolean NOT NULL DEFAULT true,
  state text NOT NULL DEFAULT 'active' CHECK(state IN ('active','dismissed','expired')),
  first_ingested_at timestamptz NOT NULL, last_ingested_at timestamptz NOT NULL,
  eligible_at timestamptz NOT NULL, force_at timestamptz NOT NULL, expires_at timestamptz,
  last_seen_revision bigint NOT NULL DEFAULT 0 CHECK(last_seen_revision >= 0),
  last_alerted_revision bigint NOT NULL DEFAULT 0 CHECK(last_alerted_revision >= 0),
  dismissed_at timestamptz, created_at timestamptz NOT NULL DEFAULT now(), updated_at timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY(tenant_id,id), UNIQUE(tenant_id,recipient_id,group_key)
);--> statement-breakpoint
CREATE INDEX IF NOT EXISTS fleet_notification_inbox ON fleet_notification(tenant_id,recipient_id,state,updated_at DESC);--> statement-breakpoint
CREATE INDEX IF NOT EXISTS fleet_notification_due ON fleet_notification(eligible_at) WHERE state='active';--> statement-breakpoint
CREATE TABLE IF NOT EXISTS fleet_notification_revision (
  tenant_id text NOT NULL, notification_id text NOT NULL, revision bigint NOT NULL CHECK(revision > 0),
  content jsonb NOT NULL, evidence jsonb NOT NULL DEFAULT '[]'::jsonb, provenance jsonb,
  created_at timestamptz NOT NULL DEFAULT now(), PRIMARY KEY(tenant_id,notification_id,revision),
  FOREIGN KEY(tenant_id,notification_id) REFERENCES fleet_notification(tenant_id,id) ON DELETE CASCADE
);--> statement-breakpoint
CREATE TABLE IF NOT EXISTS fleet_notification_source (
  tenant_id text NOT NULL, notification_id text NOT NULL, source_namespace text NOT NULL,
  stream_id text NOT NULL, sequence numeric(20,0) NOT NULL CHECK(sequence >= 0), occurred_at timestamptz NOT NULL,
  ingested_at timestamptz NOT NULL, correlation_id text, actor_id text,
  PRIMARY KEY(tenant_id,notification_id,source_namespace,stream_id,sequence),
  FOREIGN KEY(tenant_id,notification_id) REFERENCES fleet_notification(tenant_id,id) ON DELETE CASCADE
);--> statement-breakpoint
CREATE TABLE IF NOT EXISTS fleet_notification_outbox (
  tenant_id text NOT NULL, id text NOT NULL, kind text NOT NULL CHECK(kind IN ('aggregate','delivery')),
  generation bigint NOT NULL CHECK(generation > 0), eligible_at timestamptz NOT NULL, payload jsonb NOT NULL,
  state text NOT NULL DEFAULT 'pending' CHECK(state IN ('pending','published','failed')),
  workflow_id text, failure text, created_at timestamptz NOT NULL DEFAULT now(), updated_at timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY(tenant_id,id)
);--> statement-breakpoint
CREATE INDEX IF NOT EXISTS fleet_notification_outbox_pending ON fleet_notification_outbox(eligible_at) WHERE state='pending';--> statement-breakpoint
CREATE TABLE IF NOT EXISTS fleet_delivery_intent (
  tenant_id text NOT NULL, id text NOT NULL, kind text NOT NULL, channel text NOT NULL,
  recipient_id text NOT NULL, address text NOT NULL, correlation_id text, notification_refs jsonb NOT NULL DEFAULT '[]'::jsonb,
  message jsonb, idempotency_key text NOT NULL, generation bigint NOT NULL DEFAULT 1 CHECK(generation > 0),
  state text NOT NULL DEFAULT 'pending' CHECK(state IN ('pending','dispatching','accepted','rejected','expired','suppressed','paused')),
  ambiguous_policy text NOT NULL CHECK(ambiguous_policy IN ('retry','pause','reconcile')),
  provider_deduplication boolean NOT NULL DEFAULT false, deduplication_expires_at timestamptz,
  expires_at timestamptz NOT NULL, workflow_id text, accepted_provider text, accepted_receipt text,
  completed_at timestamptz, created_at timestamptz NOT NULL DEFAULT now(), updated_at timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY(tenant_id,id), UNIQUE(tenant_id,channel,idempotency_key)
);--> statement-breakpoint
CREATE INDEX IF NOT EXISTS fleet_delivery_pending ON fleet_delivery_intent(expires_at) WHERE state IN ('pending','dispatching');--> statement-breakpoint
CREATE TABLE IF NOT EXISTS fleet_delivery_attempt (
  tenant_id text NOT NULL, intent_id text NOT NULL, generation bigint NOT NULL,
  ordinal integer NOT NULL CHECK(ordinal > 0), provider_invocations integer NOT NULL DEFAULT 0 CHECK(provider_invocations >= 0),
  outcome text NOT NULL, code text, retry_after_ms integer, provider text, receipt_id text,
  attempted_at timestamptz NOT NULL DEFAULT now(), PRIMARY KEY(tenant_id,intent_id,generation,ordinal),
  FOREIGN KEY(tenant_id,intent_id) REFERENCES fleet_delivery_intent(tenant_id,id) ON DELETE CASCADE
);
