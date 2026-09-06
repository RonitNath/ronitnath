ALTER TABLE "event" ADD COLUMN "timezone" text DEFAULT 'America/Los_Angeles' NOT NULL;--> statement-breakpoint
ALTER TABLE "event" ADD COLUMN "body" text DEFAULT '' NOT NULL;--> statement-breakpoint
ALTER TABLE "event" ADD COLUMN "capacity" integer;--> statement-breakpoint
ALTER TABLE "event" ADD COLUMN "colour" text;--> statement-breakpoint
ALTER TABLE "event" ADD COLUMN "poster_url" text;--> statement-breakpoint
ALTER TABLE "event" ADD COLUMN "reveal_guests" boolean DEFAULT false NOT NULL;--> statement-breakpoint
ALTER TABLE "event" ADD COLUMN "sequence" integer DEFAULT 0 NOT NULL;--> statement-breakpoint
ALTER TABLE "event" ADD COLUMN "updated_at" timestamp with time zone DEFAULT now() NOT NULL;--> statement-breakpoint
ALTER TABLE "event_invite" ADD COLUMN "link_id" integer;--> statement-breakpoint
ALTER TABLE "event_invite" ADD CONSTRAINT "event_invite_link_id_link_id_fk" FOREIGN KEY ("link_id") REFERENCES "public"."link"("id") ON DELETE set null ON UPDATE no action;