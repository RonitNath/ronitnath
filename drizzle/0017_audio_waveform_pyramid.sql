ALTER TABLE "voice_memo_waveform_tile" DROP CONSTRAINT "voice_memo_waveform_level";--> statement-breakpoint
ALTER TABLE "voice_memo_waveform_tile" ADD CONSTRAINT "voice_memo_waveform_level" CHECK ("voice_memo_waveform_tile"."level" > 0 AND ("voice_memo_waveform_tile"."level" IN (10,100) OR ("voice_memo_waveform_tile"."level" & ("voice_memo_waveform_tile"."level" - 1)) = 0));
