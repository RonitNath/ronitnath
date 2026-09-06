CREATE TABLE "auth_throttle" (
	"id" serial PRIMARY KEY NOT NULL,
	"scope" text NOT NULL,
	"key" text NOT NULL,
	"window_started_at" timestamp with time zone DEFAULT now() NOT NULL,
	"count" integer DEFAULT 0 NOT NULL
);
--> statement-breakpoint
ALTER TABLE "session" ADD COLUMN "source" "identity_source" DEFAULT 'local' NOT NULL;--> statement-breakpoint
ALTER TABLE "session" ADD COLUMN "oidc_id_token" text;--> statement-breakpoint
CREATE UNIQUE INDEX "auth_throttle_scope_key" ON "auth_throttle" USING btree ("scope","key");