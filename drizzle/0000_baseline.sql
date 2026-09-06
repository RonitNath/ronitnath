CREATE TYPE "public"."factor_kind" AS ENUM('email', 'password', 'oidc');--> statement-breakpoint
CREATE TYPE "public"."identity_source" AS ENUM('local', 'oidc');--> statement-breakpoint
CREATE TYPE "public"."invite_kind" AS ENUM('personal', 'open');--> statement-breakpoint
CREATE TYPE "public"."link_kind" AS ENUM('verify_email', 'reset_password', 'invitation', 'claim', 'event_personal', 'event_open');--> statement-breakpoint
CREATE TYPE "public"."party_kind" AS ENUM('person', 'organization', 'service');--> statement-breakpoint
CREATE TYPE "public"."rsvp_response" AS ENUM('yes', 'maybe', 'no');--> statement-breakpoint
CREATE TABLE "audit" (
	"id" bigserial PRIMARY KEY NOT NULL,
	"actor_person_id" integer,
	"command" text NOT NULL,
	"target_kind" text,
	"target_id" integer,
	"payload" jsonb,
	"at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "document" (
	"id" serial PRIMARY KEY NOT NULL,
	"owner_party_id" integer NOT NULL,
	"title" text NOT NULL,
	"body" text DEFAULT '' NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "event" (
	"id" serial PRIMARY KEY NOT NULL,
	"host_person_id" integer NOT NULL,
	"slug" text NOT NULL,
	"title" text NOT NULL,
	"summary" text,
	"starts_at" timestamp with time zone NOT NULL,
	"ends_at" timestamp with time zone,
	"location" text,
	"address" text,
	"published_at" timestamp with time zone,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "event_invite" (
	"id" serial PRIMARY KEY NOT NULL,
	"event_id" integer NOT NULL,
	"person_id" integer,
	"kind" "invite_kind" NOT NULL,
	"plus_one_allowed" boolean DEFAULT false NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "factor" (
	"id" serial PRIMARY KEY NOT NULL,
	"identity_id" integer NOT NULL,
	"kind" "factor_kind" NOT NULL,
	"secret" text,
	"meta" jsonb,
	"used_at" timestamp with time zone,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "group" (
	"id" serial PRIMARY KEY NOT NULL,
	"organization_id" integer NOT NULL,
	"handle" text NOT NULL,
	"name" text NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "identity" (
	"id" serial PRIMARY KEY NOT NULL,
	"person_id" integer NOT NULL,
	"source" "identity_source" NOT NULL,
	"subject" text NOT NULL,
	"verified_at" timestamp with time zone,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "link" (
	"id" serial PRIMARY KEY NOT NULL,
	"token_hash" text NOT NULL,
	"kind" "link_kind" NOT NULL,
	"target_kind" text NOT NULL,
	"target_id" integer NOT NULL,
	"created_by" integer,
	"expires_at" timestamp with time zone,
	"used_at" timestamp with time zone,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "organization" (
	"id" integer PRIMARY KEY NOT NULL,
	"handle" text NOT NULL,
	"name" text NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "party" (
	"id" serial PRIMARY KEY NOT NULL,
	"kind" "party_kind" NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"disabled_at" timestamp with time zone
);
--> statement-breakpoint
CREATE TABLE "person" (
	"id" integer PRIMARY KEY NOT NULL,
	"display_name" text NOT NULL,
	"held" boolean DEFAULT false NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "relation" (
	"id" bigserial PRIMARY KEY NOT NULL,
	"subject_kind" text NOT NULL,
	"subject_id" integer NOT NULL,
	"verb" text NOT NULL,
	"resource_kind" text NOT NULL,
	"resource_id" integer NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "resource" (
	"id" serial PRIMARY KEY NOT NULL,
	"kind" text NOT NULL,
	"ref_id" integer NOT NULL,
	"owner_party_id" integer,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "rsvp" (
	"id" serial PRIMARY KEY NOT NULL,
	"event_id" integer NOT NULL,
	"person_id" integer NOT NULL,
	"response" "rsvp_response" NOT NULL,
	"plus_one" smallint DEFAULT 0 NOT NULL,
	"note" text,
	"answered_at" timestamp with time zone DEFAULT now() NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "session" (
	"id" serial PRIMARY KEY NOT NULL,
	"person_id" integer NOT NULL,
	"token_hash" text NOT NULL,
	"user_agent" text,
	"ip" text,
	"last_seen_at" timestamp with time zone DEFAULT now() NOT NULL,
	"expires_at" timestamp with time zone NOT NULL,
	"revoked_at" timestamp with time zone,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
ALTER TABLE "audit" ADD CONSTRAINT "audit_actor_person_id_person_id_fk" FOREIGN KEY ("actor_person_id") REFERENCES "public"."person"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "document" ADD CONSTRAINT "document_owner_party_id_party_id_fk" FOREIGN KEY ("owner_party_id") REFERENCES "public"."party"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "event" ADD CONSTRAINT "event_host_person_id_person_id_fk" FOREIGN KEY ("host_person_id") REFERENCES "public"."person"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "event_invite" ADD CONSTRAINT "event_invite_event_id_event_id_fk" FOREIGN KEY ("event_id") REFERENCES "public"."event"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "event_invite" ADD CONSTRAINT "event_invite_person_id_person_id_fk" FOREIGN KEY ("person_id") REFERENCES "public"."person"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "factor" ADD CONSTRAINT "factor_identity_id_identity_id_fk" FOREIGN KEY ("identity_id") REFERENCES "public"."identity"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "group" ADD CONSTRAINT "group_organization_id_organization_id_fk" FOREIGN KEY ("organization_id") REFERENCES "public"."organization"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "identity" ADD CONSTRAINT "identity_person_id_person_id_fk" FOREIGN KEY ("person_id") REFERENCES "public"."person"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "link" ADD CONSTRAINT "link_created_by_person_id_fk" FOREIGN KEY ("created_by") REFERENCES "public"."person"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "organization" ADD CONSTRAINT "organization_id_party_id_fk" FOREIGN KEY ("id") REFERENCES "public"."party"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "person" ADD CONSTRAINT "person_id_party_id_fk" FOREIGN KEY ("id") REFERENCES "public"."party"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "resource" ADD CONSTRAINT "resource_owner_party_id_party_id_fk" FOREIGN KEY ("owner_party_id") REFERENCES "public"."party"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "rsvp" ADD CONSTRAINT "rsvp_event_id_event_id_fk" FOREIGN KEY ("event_id") REFERENCES "public"."event"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "rsvp" ADD CONSTRAINT "rsvp_person_id_person_id_fk" FOREIGN KEY ("person_id") REFERENCES "public"."person"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "session" ADD CONSTRAINT "session_person_id_person_id_fk" FOREIGN KEY ("person_id") REFERENCES "public"."person"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "audit_at_idx" ON "audit" USING btree ("at");--> statement-breakpoint
CREATE INDEX "audit_actor_idx" ON "audit" USING btree ("actor_person_id");--> statement-breakpoint
CREATE INDEX "audit_target_idx" ON "audit" USING btree ("target_kind","target_id");--> statement-breakpoint
CREATE INDEX "document_owner_idx" ON "document" USING btree ("owner_party_id");--> statement-breakpoint
CREATE UNIQUE INDEX "event_slug_key" ON "event" USING btree ("slug");--> statement-breakpoint
CREATE INDEX "event_host_idx" ON "event" USING btree ("host_person_id");--> statement-breakpoint
CREATE UNIQUE INDEX "event_invite_person_key" ON "event_invite" USING btree ("event_id","person_id");--> statement-breakpoint
CREATE INDEX "event_invite_event_idx" ON "event_invite" USING btree ("event_id");--> statement-breakpoint
CREATE UNIQUE INDEX "factor_identity_kind_key" ON "factor" USING btree ("identity_id","kind");--> statement-breakpoint
CREATE UNIQUE INDEX "group_org_handle_key" ON "group" USING btree ("organization_id","handle");--> statement-breakpoint
CREATE UNIQUE INDEX "identity_source_subject_key" ON "identity" USING btree ("source","subject");--> statement-breakpoint
CREATE INDEX "identity_person_idx" ON "identity" USING btree ("person_id");--> statement-breakpoint
CREATE UNIQUE INDEX "link_token_hash_key" ON "link" USING btree ("token_hash");--> statement-breakpoint
CREATE INDEX "link_target_idx" ON "link" USING btree ("target_kind","target_id");--> statement-breakpoint
CREATE UNIQUE INDEX "organization_handle_key" ON "organization" USING btree ("handle");--> statement-breakpoint
CREATE UNIQUE INDEX "relation_edge_key" ON "relation" USING btree ("subject_kind","subject_id","verb","resource_kind","resource_id");--> statement-breakpoint
CREATE INDEX "relation_resource_idx" ON "relation" USING btree ("resource_kind","resource_id");--> statement-breakpoint
CREATE INDEX "relation_subject_idx" ON "relation" USING btree ("subject_kind","subject_id");--> statement-breakpoint
CREATE UNIQUE INDEX "resource_kind_ref_key" ON "resource" USING btree ("kind","ref_id");--> statement-breakpoint
CREATE INDEX "resource_owner_idx" ON "resource" USING btree ("owner_party_id");--> statement-breakpoint
CREATE UNIQUE INDEX "rsvp_event_person_key" ON "rsvp" USING btree ("event_id","person_id");--> statement-breakpoint
CREATE INDEX "rsvp_event_idx" ON "rsvp" USING btree ("event_id");--> statement-breakpoint
CREATE UNIQUE INDEX "session_token_hash_key" ON "session" USING btree ("token_hash");--> statement-breakpoint
CREATE INDEX "session_person_idx" ON "session" USING btree ("person_id");