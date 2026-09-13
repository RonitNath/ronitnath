DROP INDEX "billing_receipt_external_key";--> statement-breakpoint
ALTER TABLE "billing_receipt_claim" ADD COLUMN "seller_id" text;--> statement-breakpoint
UPDATE "billing_receipt_claim" AS claim
SET "seller_id" = orders."seller_id"
FROM "billing_order" AS orders
WHERE orders."invoice_id" = claim."invoice_id";--> statement-breakpoint
ALTER TABLE "billing_receipt_claim" ALTER COLUMN "seller_id" SET NOT NULL;--> statement-breakpoint
ALTER TABLE "billing_receipt_claim" ADD CONSTRAINT "billing_receipt_claim_seller_id_billing_seller_id_fk" FOREIGN KEY ("seller_id") REFERENCES "public"."billing_seller"("id") ON DELETE no action ON UPDATE no action;--> statement-breakpoint
CREATE UNIQUE INDEX "billing_receipt_external_key" ON "billing_receipt_claim" USING btree ("seller_id","confirmed_external_namespace","confirmed_external_id");
