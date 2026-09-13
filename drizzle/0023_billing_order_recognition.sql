ALTER TABLE "billing_order" ADD COLUMN "recognized_atoms" numeric(39,0) DEFAULT 0 NOT NULL;
ALTER TABLE "billing_order" ADD COLUMN "recognized_at" timestamp with time zone;
ALTER TABLE "billing_order" ADD CONSTRAINT "billing_order_recognized_atoms_valid" CHECK (recognized_atoms >= 0 AND recognized_atoms <= total_atoms);
