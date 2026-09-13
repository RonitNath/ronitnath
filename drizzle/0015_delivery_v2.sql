ALTER TABLE "delivery_run_event" ADD COLUMN "producer_id" text DEFAULT 'legacy' NOT NULL;
ALTER TABLE "delivery_run_event" DROP CONSTRAINT "delivery_run_event_run_id_seq_pk";
ALTER TABLE "delivery_run_event" ADD CONSTRAINT "delivery_run_event_run_id_producer_id_seq_pk" PRIMARY KEY("run_id","producer_id","seq");
CREATE INDEX "delivery_event_run_producer_idx" ON "delivery_run_event" USING btree ("run_id","producer_id","seq");
CREATE TABLE "delivery_acceptance_fixture" (
  "id" uuid PRIMARY KEY,
  "release_id" uuid NOT NULL,
  "created_at" timestamptz NOT NULL DEFAULT now(),
  "expires_at" timestamptz NOT NULL
);
