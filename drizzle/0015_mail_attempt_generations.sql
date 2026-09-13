ALTER TABLE mail_attempt ADD COLUMN generation integer NOT NULL DEFAULT 0;--> statement-breakpoint
ALTER TABLE mail_attempt ADD COLUMN provider_invocations integer NOT NULL DEFAULT 0;--> statement-breakpoint
ALTER TABLE mail_attempt DROP CONSTRAINT mail_attempt_intent_id_ordinal_key;--> statement-breakpoint
ALTER TABLE mail_attempt ADD CONSTRAINT mail_attempt_intent_generation_ordinal UNIQUE(intent_id,generation,ordinal);
