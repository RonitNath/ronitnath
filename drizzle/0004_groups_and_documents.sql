-- R5. Additive on a live database: the two `document` columns are nullable,
-- and `group` has never had a row written to it (no code before this rung
-- touched the table), so the NOT NULL column it gains needs no default and
-- the two it loses take nothing with them.
ALTER TABLE "group" DROP CONSTRAINT "group_organization_id_organization_id_fk";
--> statement-breakpoint
DROP INDEX "group_org_handle_key";--> statement-breakpoint
ALTER TABLE "document" ADD COLUMN "slug" text;--> statement-breakpoint
ALTER TABLE "document" ADD COLUMN "published_at" timestamp with time zone;--> statement-breakpoint
ALTER TABLE "group" ADD COLUMN "owner_party_id" integer NOT NULL;--> statement-breakpoint
ALTER TABLE "group" ADD CONSTRAINT "group_owner_party_id_party_id_fk" FOREIGN KEY ("owner_party_id") REFERENCES "public"."party"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
CREATE UNIQUE INDEX "document_slug_key" ON "document" USING btree ("slug");--> statement-breakpoint
CREATE INDEX "group_owner_idx" ON "group" USING btree ("owner_party_id");--> statement-breakpoint
ALTER TABLE "group" DROP COLUMN "organization_id";--> statement-breakpoint
ALTER TABLE "group" DROP COLUMN "handle";