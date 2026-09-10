CREATE TABLE "domain_event" (
	"id" bigserial PRIMARY KEY NOT NULL,
	"org_id" text NOT NULL,
	"seq" bigint NOT NULL,
	"resource_kind" text NOT NULL,
	"resource_id" text NOT NULL,
	"kind" text NOT NULL,
	"actor_id" text,
	"subject_id" text,
	"acting_operator_id" text,
	"correlation_id" text NOT NULL,
	"published" boolean DEFAULT true NOT NULL,
	"payload" jsonb,
	"at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "domain_event_cursor" (
	"org_id" text PRIMARY KEY NOT NULL,
	"seq" bigint DEFAULT 0 NOT NULL
);
--> statement-breakpoint
CREATE UNIQUE INDEX "domain_event_org_seq" ON "domain_event" USING btree ("org_id","seq");--> statement-breakpoint
CREATE INDEX "domain_event_resource_idx" ON "domain_event" USING btree ("resource_kind","resource_id");--> statement-breakpoint
CREATE INDEX "domain_event_at_idx" ON "domain_event" USING btree ("at");--> statement-breakpoint
-- The gapless per-org sequence. The cursor row is taken with an UPDATE, which
-- is a row lock, so two transactions appending to the same org's stream
-- serialise here and nowhere else: a browser that reconnects with `since=<seq>`
-- can be told, truthfully, that it has missed nothing.
CREATE FUNCTION domain_event_assign_seq() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE next_seq bigint;
BEGIN
  INSERT INTO domain_event_cursor (org_id, seq) VALUES (NEW.org_id, 1)
    ON CONFLICT (org_id) DO UPDATE SET seq = domain_event_cursor.seq + 1
    RETURNING seq INTO next_seq;
  NEW.seq := next_seq;
  RETURN NEW;
END $$;--> statement-breakpoint
CREATE TRIGGER domain_event_assign_seq BEFORE INSERT ON domain_event
  FOR EACH ROW EXECUTE FUNCTION domain_event_assign_seq();--> statement-breakpoint
-- The wake-up. The payload is `<org>|<seq>` and nothing else: a listener reads
-- the row through the same authorized path a page would, so NOTIFY never has
-- to be trusted with anything it would be a leak to overhear.
CREATE FUNCTION domain_event_notify() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  PERFORM pg_notify('domain_event', NEW.org_id || '|' || NEW.seq::text);
  RETURN NULL;
END $$;--> statement-breakpoint
CREATE TRIGGER domain_event_notify AFTER INSERT ON domain_event
  FOR EACH ROW EXECUTE FUNCTION domain_event_notify();--> statement-breakpoint
-- Append-only, in the database. An application can forget; a trigger cannot.
CREATE FUNCTION domain_event_append_only() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  RAISE EXCEPTION 'domain_event is append-only (attempted %)', TG_OP;
END $$;--> statement-breakpoint
CREATE TRIGGER domain_event_append_only BEFORE UPDATE OR DELETE ON domain_event
  FOR EACH ROW EXECUTE FUNCTION domain_event_append_only();
