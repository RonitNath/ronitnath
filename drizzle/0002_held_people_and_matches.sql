CREATE TYPE "public"."match_signal" AS ENUM('verified_email', 'claimed_link');--> statement-breakpoint
CREATE TYPE "public"."match_status" AS ENUM('proposed', 'confirmed', 'rejected');--> statement-breakpoint
ALTER TYPE "public"."identity_source" ADD VALUE 'handle';--> statement-breakpoint
CREATE TABLE "match" (
	"id" serial PRIMARY KEY NOT NULL,
	"identity_a" integer NOT NULL,
	"identity_b" integer NOT NULL,
	"signal" "match_signal" NOT NULL,
	"score" smallint DEFAULT 0 NOT NULL,
	"status" "match_status" DEFAULT 'proposed' NOT NULL,
	"evidence" text,
	"decided_at" timestamp with time zone,
	"decided_by" integer,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "match_ordered_pair" CHECK ("match"."identity_a" < "match"."identity_b")
);
--> statement-breakpoint
DROP INDEX "identity_source_subject_key";--> statement-breakpoint
ALTER TABLE "link" ADD COLUMN "opened_at" timestamp with time zone;--> statement-breakpoint
ALTER TABLE "link" ADD COLUMN "claimed_at" timestamp with time zone;--> statement-breakpoint
ALTER TABLE "link" ADD COLUMN "claimed_by" integer;--> statement-breakpoint
ALTER TABLE "link" ADD COLUMN "revoked_at" timestamp with time zone;--> statement-breakpoint
ALTER TABLE "person" ADD COLUMN "merged_into" integer;--> statement-breakpoint
ALTER TABLE "match" ADD CONSTRAINT "match_identity_a_identity_id_fk" FOREIGN KEY ("identity_a") REFERENCES "public"."identity"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "match" ADD CONSTRAINT "match_identity_b_identity_id_fk" FOREIGN KEY ("identity_b") REFERENCES "public"."identity"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "match" ADD CONSTRAINT "match_decided_by_person_id_fk" FOREIGN KEY ("decided_by") REFERENCES "public"."person"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
CREATE UNIQUE INDEX "match_pair_signal_key" ON "match" USING btree ("identity_a","identity_b","signal");--> statement-breakpoint
CREATE INDEX "match_status_idx" ON "match" USING btree ("status");--> statement-breakpoint
CREATE INDEX "match_identity_b_idx" ON "match" USING btree ("identity_b");--> statement-breakpoint
ALTER TABLE "link" ADD CONSTRAINT "link_claimed_by_person_id_fk" FOREIGN KEY ("claimed_by") REFERENCES "public"."person"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "person" ADD CONSTRAINT "person_merged_into_person_id_fk" FOREIGN KEY ("merged_into") REFERENCES "public"."person"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "identity_subject_idx" ON "identity" USING btree ("source","subject");--> statement-breakpoint
CREATE INDEX "link_created_by_idx" ON "link" USING btree ("created_by");--> statement-breakpoint
CREATE INDEX "person_merged_into_idx" ON "person" USING btree ("merged_into");--> statement-breakpoint
CREATE UNIQUE INDEX "identity_source_subject_key" ON "identity" USING btree ("source","subject") WHERE source in ('local', 'oidc');