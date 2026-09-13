CREATE TABLE "billing_pilot_account" (
	"person_id" integer PRIMARY KEY NOT NULL,
	"version" integer DEFAULT 1 NOT NULL,
	"enabled" boolean DEFAULT true NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
ALTER TABLE "billing_adjustment" ADD COLUMN "service_effect" text DEFAULT 'preserve_service' NOT NULL;--> statement-breakpoint
ALTER TABLE "billing_adjustment" ALTER COLUMN "service_effect" DROP DEFAULT;--> statement-breakpoint
ALTER TABLE "billing_adjustment" ADD COLUMN "external_namespace" text;--> statement-breakpoint
ALTER TABLE "billing_adjustment" ADD COLUMN "external_id" text;--> statement-breakpoint
ALTER TABLE "billing_membership" ADD COLUMN "anchor_day" integer;--> statement-breakpoint
ALTER TABLE "billing_membership" ADD COLUMN "anchor_time" text;--> statement-breakpoint
ALTER TABLE "billing_membership" ADD COLUMN "time_zone" text DEFAULT 'UTC' NOT NULL;--> statement-breakpoint
UPDATE "billing_membership" SET "anchor_day" = extract(day from "service_starts_at" AT TIME ZONE 'UTC')::integer, "anchor_time" = to_char("service_starts_at" AT TIME ZONE 'UTC', 'HH24:MI');--> statement-breakpoint
ALTER TABLE "billing_membership" ALTER COLUMN "anchor_day" SET NOT NULL;--> statement-breakpoint
ALTER TABLE "billing_membership" ALTER COLUMN "anchor_time" SET NOT NULL;--> statement-breakpoint
ALTER TABLE "billing_order" ADD COLUMN "credited_atoms" numeric(39, 0) DEFAULT 0 NOT NULL;--> statement-breakpoint
ALTER TABLE "billing_order" ADD COLUMN "refunded_atoms" numeric(39, 0) DEFAULT 0 NOT NULL;--> statement-breakpoint
ALTER TABLE "billing_order" ADD COLUMN "fulfilled_operation_key" text;--> statement-breakpoint
ALTER TABLE "billing_receipt_claim" ADD COLUMN "version" integer DEFAULT 1 NOT NULL;--> statement-breakpoint
ALTER TABLE "billing_receipt_claim" ADD COLUMN "confirmed_operation_key" text;--> statement-breakpoint
ALTER TABLE "billing_receipt_claim" ADD COLUMN "confirmed_external_namespace" text;--> statement-breakpoint
ALTER TABLE "billing_receipt_claim" ADD COLUMN "confirmed_external_id" text;--> statement-breakpoint
ALTER TABLE "billing_pilot_account" ADD CONSTRAINT "billing_pilot_account_person_id_person_id_fk" FOREIGN KEY ("person_id") REFERENCES "public"."person"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
CREATE UNIQUE INDEX "billing_adjustment_external_key" ON "billing_adjustment" USING btree ("order_id","kind","external_namespace","external_id");--> statement-breakpoint
CREATE UNIQUE INDEX "billing_receipt_external_key" ON "billing_receipt_claim" USING btree ("invoice_id","confirmed_external_namespace","confirmed_external_id");--> statement-breakpoint
ALTER TABLE "billing_order" ADD CONSTRAINT "billing_order_fulfilled_operation_key_unique" UNIQUE("fulfilled_operation_key");--> statement-breakpoint
ALTER TABLE "billing_receipt_claim" ADD CONSTRAINT "billing_receipt_claim_confirmed_operation_key_unique" UNIQUE("confirmed_operation_key");
--> statement-breakpoint
ALTER TABLE "billing_order" ADD CONSTRAINT "billing_order_amounts_valid" CHECK ("total_atoms" >= 0 AND "paid_atoms" >= 0 AND "credited_atoms" >= 0 AND "refunded_atoms" >= 0 AND "paid_atoms" + "credited_atoms" <= "total_atoms" AND "refunded_atoms" <= "paid_atoms");
--> statement-breakpoint
ALTER TABLE "billing_membership" ADD CONSTRAINT "billing_membership_anchor_valid" CHECK ("anchor_day" BETWEEN 1 AND 31 AND "anchor_time" ~ '^[0-2][0-9]:[0-5][0-9]$');
