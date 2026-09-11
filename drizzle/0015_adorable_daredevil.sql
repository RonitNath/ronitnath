CREATE TABLE "voice_memo_segment" (
	"memo_id" integer NOT NULL,
	"generation" integer NOT NULL,
	"rendition" smallint NOT NULL,
	"sequence" integer NOT NULL,
	"duration_ms" integer NOT NULL,
	"key" text NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "voice_memo_segment_memo_id_generation_rendition_sequence_pk" PRIMARY KEY("memo_id","generation","rendition","sequence"),
	CONSTRAINT "voice_memo_segment_rendition" CHECK ("voice_memo_segment"."rendition" IN (32,64))
);
--> statement-breakpoint
ALTER TABLE "voice_memo" DROP CONSTRAINT "voice_memo_state";--> statement-breakpoint
ALTER TABLE "voice_memo" ALTER COLUMN "state" SET DEFAULT 'uploading';--> statement-breakpoint
ALTER TABLE "voice_memo" ADD COLUMN "upload_id" text;--> statement-breakpoint
ALTER TABLE "voice_memo" ADD COLUMN "upload_complete" boolean DEFAULT false NOT NULL;--> statement-breakpoint
ALTER TABLE "voice_memo" ADD COLUMN "durable_bytes" bigint DEFAULT 0 NOT NULL;--> statement-breakpoint
ALTER TABLE "voice_memo" ADD COLUMN "playable_through_ms" integer DEFAULT 0 NOT NULL;--> statement-breakpoint
ALTER TABLE "voice_memo" ADD COLUMN "processing_mode" text DEFAULT 'probing' NOT NULL;--> statement-breakpoint
ALTER TABLE "voice_memo" ADD COLUMN "generation" integer DEFAULT 0 NOT NULL;--> statement-breakpoint
ALTER TABLE "voice_memo_job" ADD COLUMN "lease_token" uuid;--> statement-breakpoint
UPDATE "voice_memo" SET "upload_complete"=true,"durable_bytes"="source_bytes","playable_through_ms"=coalesce("duration_ms",0),"processing_mode"=CASE WHEN "state"='ready' THEN 'complete' ELSE 'after-upload' END;--> statement-breakpoint
ALTER TABLE "voice_memo_segment" ADD CONSTRAINT "voice_memo_segment_memo_id_voice_memo_id_fk" FOREIGN KEY ("memo_id") REFERENCES "public"."voice_memo"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
CREATE UNIQUE INDEX "voice_memo_upload_key" ON "voice_memo" USING btree ("upload_id");--> statement-breakpoint
ALTER TABLE "voice_memo" ADD CONSTRAINT "voice_memo_processing_mode" CHECK ("voice_memo"."processing_mode" IN ('probing','streaming','after-upload','complete'));--> statement-breakpoint
ALTER TABLE "voice_memo" ADD CONSTRAINT "voice_memo_state" CHECK ("voice_memo"."state" IN ('uploading','processing','playable','ready','failed','deleting'));
