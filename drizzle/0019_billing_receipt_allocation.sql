ALTER TABLE "billing_receipt_claim" ADD COLUMN "applied_atoms" numeric(39, 0);--> statement-breakpoint
ALTER TABLE "billing_receipt_claim" ADD COLUMN "unapplied_atoms" numeric(39, 0);