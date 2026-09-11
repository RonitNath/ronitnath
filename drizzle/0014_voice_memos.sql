CREATE TABLE "voice_memo" (
	"id" serial PRIMARY KEY NOT NULL,
	"person_id" integer NOT NULL,
	"local_id" uuid NOT NULL,
	"title" text NOT NULL,
	"state" text DEFAULT 'processing' NOT NULL,
	"source_key" text NOT NULL,
	"source_mime_type" text NOT NULL,
	"source_bytes" bigint NOT NULL,
	"source_format" text,
	"source_codec" text,
	"duration_ms" integer,
	"hls_master_key" text,
	"fallback_key" text,
	"waveform_key" text,
	"playback_position_ms" integer DEFAULT 0 NOT NULL,
	"failure" text,
	"trashed_at" timestamp with time zone,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "voice_memo_state" CHECK ("voice_memo"."state" IN ('processing','ready','failed','deleting'))
);
--> statement-breakpoint
CREATE TABLE "voice_memo_job" (
	"memo_id" integer PRIMARY KEY NOT NULL,
	"kind" text DEFAULT 'process' NOT NULL,
	"state" text DEFAULT 'queued' NOT NULL,
	"attempts" integer DEFAULT 0 NOT NULL,
	"available_at" timestamp with time zone DEFAULT now() NOT NULL,
	"lease_until" timestamp with time zone,
	"error" text,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "voice_memo_job_kind" CHECK ("voice_memo_job"."kind" IN ('process','delete')),
	CONSTRAINT "voice_memo_job_state" CHECK ("voice_memo_job"."state" IN ('queued','running','failed'))
);
--> statement-breakpoint
ALTER TABLE "voice_memo" ADD CONSTRAINT "voice_memo_person_id_person_id_fk" FOREIGN KEY ("person_id") REFERENCES "public"."person"("id") ON DELETE cascade ON UPDATE no action;
--> statement-breakpoint
ALTER TABLE "voice_memo_job" ADD CONSTRAINT "voice_memo_job_memo_id_voice_memo_id_fk" FOREIGN KEY ("memo_id") REFERENCES "public"."voice_memo"("id") ON DELETE cascade ON UPDATE no action;
--> statement-breakpoint
CREATE UNIQUE INDEX "voice_memo_person_local_key" ON "voice_memo" USING btree ("person_id","local_id");
--> statement-breakpoint
CREATE INDEX "voice_memo_person_created_idx" ON "voice_memo" USING btree ("person_id","created_at");
--> statement-breakpoint
CREATE INDEX "voice_memo_job_ready_idx" ON "voice_memo_job" USING btree ("state","available_at");
