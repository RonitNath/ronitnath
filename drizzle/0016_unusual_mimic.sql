CREATE TABLE "voice_memo_waveform_tile" (
	"memo_id" integer NOT NULL,
	"generation" integer NOT NULL,
	"level" smallint NOT NULL,
	"start_peak" integer NOT NULL,
	"peak_count" integer NOT NULL,
	"key" text NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "voice_memo_waveform_tile_memo_id_generation_level_start_peak_pk" PRIMARY KEY("memo_id","generation","level","start_peak"),
	CONSTRAINT "voice_memo_waveform_level" CHECK ("voice_memo_waveform_tile"."level" IN (1,10,100))
);
--> statement-breakpoint
ALTER TABLE "voice_memo_job" DROP CONSTRAINT "voice_memo_job_kind";--> statement-breakpoint
ALTER TABLE "voice_memo" ADD COLUMN "upload_finalized_at" timestamp with time zone;--> statement-breakpoint
ALTER TABLE "voice_memo" ADD COLUMN "next_generation" integer DEFAULT 1 NOT NULL;--> statement-breakpoint
ALTER TABLE "voice_memo" ADD COLUMN "processing_generation" integer;--> statement-breakpoint
ALTER TABLE "voice_memo" ADD COLUMN "publication_revision" integer DEFAULT 0 NOT NULL;--> statement-breakpoint
ALTER TABLE "voice_memo" ADD COLUMN "source_sha256" text;--> statement-breakpoint
ALTER TABLE "voice_memo_job" ADD COLUMN "generation" integer;--> statement-breakpoint
UPDATE "voice_memo" SET "next_generation"="generation"+1,"upload_finalized_at"=CASE WHEN "upload_complete" THEN "updated_at" ELSE NULL END,"publication_revision"=CASE WHEN "generation">0 THEN 1 ELSE 0 END;--> statement-breakpoint
ALTER TABLE "voice_memo_waveform_tile" ADD CONSTRAINT "voice_memo_waveform_tile_memo_id_voice_memo_id_fk" FOREIGN KEY ("memo_id") REFERENCES "public"."voice_memo"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "voice_memo_job" ADD CONSTRAINT "voice_memo_job_kind" CHECK ("voice_memo_job"."kind" IN ('ingest','process','delete'));
