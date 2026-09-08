CREATE TABLE "sky_star_detail" (
	"id" text PRIMARY KEY NOT NULL,
	"payload" jsonb NOT NULL,
	"built_at" timestamp with time zone DEFAULT now() NOT NULL
);
