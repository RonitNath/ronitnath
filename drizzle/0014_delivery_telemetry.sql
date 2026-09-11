CREATE TABLE "delivery_run" (
  "id" uuid PRIMARY KEY NOT NULL,
  "requested_sha" text NOT NULL,
  "state" text NOT NULL,
  "last_seq" bigint DEFAULT -1 NOT NULL,
  "manifest" jsonb,
  "started_at" timestamp with time zone DEFAULT now() NOT NULL,
  "updated_at" timestamp with time zone DEFAULT now() NOT NULL,
  "finished_at" timestamp with time zone
);
--> statement-breakpoint
CREATE INDEX "delivery_run_started_idx" ON "delivery_run" USING btree ("started_at");
--> statement-breakpoint
CREATE TABLE "delivery_run_event" (
  "run_id" uuid NOT NULL,
  "seq" bigint NOT NULL,
  "event" jsonb NOT NULL,
  "at" timestamp with time zone NOT NULL,
  CONSTRAINT "delivery_run_event_run_id_seq_pk" PRIMARY KEY("run_id","seq")
);
--> statement-breakpoint
ALTER TABLE "delivery_run_event" ADD CONSTRAINT "delivery_run_event_run_id_delivery_run_id_fk" FOREIGN KEY ("run_id") REFERENCES "public"."delivery_run"("id") ON DELETE cascade;
--> statement-breakpoint
CREATE INDEX "delivery_event_at_idx" ON "delivery_run_event" USING btree ("at");
