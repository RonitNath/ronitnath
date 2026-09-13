CREATE TABLE "billing_command" (
	"operation_key" text PRIMARY KEY NOT NULL,
	"actor_person_id" integer NOT NULL,
	"command" text NOT NULL,
	"request_hash" text NOT NULL,
	"request" jsonb NOT NULL,
	"result" jsonb,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "billing_command_actor_person_id_person_id_fk" FOREIGN KEY ("actor_person_id") REFERENCES "public"."person"("id") ON DELETE no action ON UPDATE no action
);
