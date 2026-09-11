CREATE TABLE realtime_view (
  id uuid PRIMARY KEY, visitor_id uuid NOT NULL, tab_id uuid NOT NULL,
  session_id text, person_id text, path text NOT NULL, title text NOT NULL DEFAULT '',
  visible boolean NOT NULL DEFAULT true, visible_sections jsonb NOT NULL DEFAULT '[]',
  visible_rows jsonb NOT NULL DEFAULT '[]', interacted_at timestamptz,
  connected_at timestamptz NOT NULL DEFAULT now(), last_heartbeat_at timestamptz NOT NULL DEFAULT now(),
  lease_expires_at timestamptz NOT NULL, disconnected_at timestamptz,
  UNIQUE(visitor_id, tab_id)
);
CREATE INDEX realtime_view_lease_idx ON realtime_view (lease_expires_at);
CREATE INDEX realtime_view_person_idx ON realtime_view (person_id, last_heartbeat_at DESC);

CREATE TABLE realtime_subscription (
  view_id uuid NOT NULL REFERENCES realtime_view(id) ON DELETE CASCADE,
  subscription_key text NOT NULL, descriptor jsonb NOT NULL,
  authorized_at timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY(view_id, subscription_key)
);

CREATE TABLE realtime_delivery (
  id uuid PRIMARY KEY, view_id uuid NOT NULL REFERENCES realtime_view(id) ON DELETE CASCADE,
  event_org_id text NOT NULL, event_seq bigint NOT NULL, revision text NOT NULL,
  reason text NOT NULL, selected_at timestamptz NOT NULL DEFAULT now(), sent_at timestamptz,
  received_at timestamptz, applied_at timestamptz,
  UNIQUE(view_id, event_org_id, event_seq)
);
CREATE INDEX realtime_delivery_view_idx ON realtime_delivery (view_id, selected_at DESC);

CREATE OR REPLACE FUNCTION realtime_select_deliveries() RETURNS trigger AS $$
BEGIN
  INSERT INTO realtime_delivery (id, view_id, event_org_id, event_seq, revision, reason)
  SELECT gen_random_uuid(),
         s.view_id, NEW.org_id, NEW.seq, NEW.seq::text,
         CASE s.descriptor->>'type'
           WHEN 'resource' THEN 'displayed resource changed'
           WHEN 'access' THEN 'access to displayed resource changed'
           WHEN 'configuration' THEN coalesce(s.descriptor->>'state', 'published') || ' configuration changed'
           ELSE 'membership of query ' || coalesce(s.descriptor->>'queryKey', '') || ' may have changed'
         END
    FROM realtime_subscription s
    JOIN realtime_view v ON v.id=s.view_id
   WHERE v.lease_expires_at > now()
     AND v.disconnected_at IS NULL
     AND (
       (s.descriptor->>'type' IN ('resource','access') AND s.descriptor->>'resourceKind'=NEW.resource_kind AND s.descriptor->>'resourceId'=NEW.resource_id)
       OR (s.descriptor->>'type'='configuration' AND NEW.resource_kind='configured' AND s.descriptor->>'key'=NEW.resource_id)
       OR (s.descriptor->>'type'='query' AND s.descriptor->>'resourceKind'=NEW.resource_kind)
     )
  ON CONFLICT DO NOTHING;
  PERFORM pg_notify('realtime_delivery', NEW.org_id || '|' || NEW.seq);
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER realtime_select_deliveries
  AFTER INSERT ON domain_event FOR EACH ROW EXECUTE FUNCTION realtime_select_deliveries();

CREATE OR REPLACE FUNCTION realtime_cleanup() RETURNS bigint AS $$
DECLARE removed bigint;
BEGIN
  DELETE FROM realtime_view WHERE last_heartbeat_at < now() - interval '7 days';
  GET DIAGNOSTICS removed = ROW_COUNT;
  RETURN removed;
END;
$$ LANGUAGE plpgsql;

ALTER TABLE document ADD COLUMN draft_version integer NOT NULL DEFAULT 0;
ALTER TABLE document ADD COLUMN title_version integer NOT NULL DEFAULT 0;
ALTER TABLE document ADD COLUMN body_version integer NOT NULL DEFAULT 0;
ALTER TABLE document ADD COLUMN published_title text;
ALTER TABLE document ADD COLUMN published_body text;
ALTER TABLE document ADD COLUMN published_version integer;
UPDATE document SET published_title=title, published_body=body, published_version=draft_version WHERE published_at IS NOT NULL;

CREATE TABLE document_revision (
  id bigserial PRIMARY KEY,
  document_id integer NOT NULL REFERENCES document(id) ON DELETE CASCADE,
  version integer NOT NULL,
  mutation_id text NOT NULL,
  kind text NOT NULL,
  fields jsonb NOT NULL DEFAULT '[]',
  title text NOT NULL,
  body text NOT NULL,
  previous jsonb,
  actor_person_id integer REFERENCES person(id) ON DELETE SET NULL,
  created_at timestamptz NOT NULL DEFAULT now(),
  UNIQUE(document_id, mutation_id)
);
CREATE INDEX document_revision_history_idx ON document_revision(document_id, version DESC);
