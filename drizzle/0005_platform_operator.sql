ALTER TYPE "public"."match_signal" ADD VALUE 'operator';--> statement-breakpoint
ALTER TABLE "match" ADD COLUMN "proposed_by" integer;--> statement-breakpoint
ALTER TABLE "session" ADD COLUMN "acting_operator_id" integer;--> statement-breakpoint
ALTER TABLE "session" ADD COLUMN "reauthenticated_at" timestamp with time zone;--> statement-breakpoint
ALTER TABLE "match" ADD CONSTRAINT "match_proposed_by_person_id_fk" FOREIGN KEY ("proposed_by") REFERENCES "public"."person"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "session" ADD CONSTRAINT "session_acting_operator_id_person_id_fk" FOREIGN KEY ("acting_operator_id") REFERENCES "public"."person"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "session_acting_operator_idx" ON "session" USING btree ("acting_operator_id");