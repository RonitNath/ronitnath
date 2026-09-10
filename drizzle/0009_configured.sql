CREATE TABLE "configured" (
	"org_id" text NOT NULL,
	"key" text NOT NULL,
	"state" text NOT NULL,
	"version" integer DEFAULT 0 NOT NULL,
	"body" jsonb NOT NULL,
	"updated_by" text,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "configured_org_id_key_state_pk" PRIMARY KEY("org_id","key","state"),
	CONSTRAINT "configured_state" CHECK ("configured"."state" IN ('draft','published'))
);
--> statement-breakpoint
CREATE TABLE "configured_version" (
	"org_id" text NOT NULL,
	"key" text NOT NULL,
	"version" integer NOT NULL,
	"body" jsonb NOT NULL,
	"published_by" text,
	"at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "configured_version_org_id_key_version_pk" PRIMARY KEY("org_id","key","version")
);
