CREATE TABLE "billing_seller" (
  "id" text PRIMARY KEY NOT NULL, "name" text NOT NULL, "book_id" text NOT NULL UNIQUE,
  "chart" jsonb NOT NULL, "enabled" boolean DEFAULT false NOT NULL,
  "created_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "billing_offer_version" (
  "namespace" text NOT NULL, "id" text NOT NULL, "version" integer NOT NULL,
  "seller_id" text NOT NULL REFERENCES "billing_seller"("id"), "terms" jsonb NOT NULL,
  "published_at" timestamp with time zone DEFAULT now() NOT NULL,
  CONSTRAINT "billing_offer_version_namespace_id_version_pk" PRIMARY KEY("namespace","id","version")
);
--> statement-breakpoint
CREATE TABLE "billing_offer" (
  "namespace" text NOT NULL, "id" text NOT NULL, "published_version" integer NOT NULL,
  "available" boolean NOT NULL, "updated_at" timestamp with time zone DEFAULT now() NOT NULL,
  CONSTRAINT "billing_offer_namespace_id_pk" PRIMARY KEY("namespace","id")
);
--> statement-breakpoint
CREATE TABLE "billing_order" (
  "id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL, "operation_key" text NOT NULL UNIQUE,
  "customer_person_id" integer NOT NULL REFERENCES "person"("id"),
  "seller_id" text NOT NULL REFERENCES "billing_seller"("id"),
  "offer_namespace" text NOT NULL, "offer_id" text NOT NULL, "offer_version" integer NOT NULL,
  "invoice_id" text NOT NULL UNIQUE, "kind" text NOT NULL, "state" text NOT NULL,
  "version" integer DEFAULT 1 NOT NULL, "total_atoms" numeric(39,0) NOT NULL,
  "paid_atoms" numeric(39,0) DEFAULT 0 NOT NULL, "fulfilled_at" timestamp with time zone,
  "created_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE INDEX "billing_order_customer_idx" ON "billing_order" ("customer_person_id");
--> statement-breakpoint
CREATE INDEX "billing_order_state_idx" ON "billing_order" ("state");
--> statement-breakpoint
CREATE TABLE "billing_membership" (
  "id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
  "order_id" uuid NOT NULL REFERENCES "billing_order"("id"), "state" text NOT NULL,
  "version" integer DEFAULT 1 NOT NULL, "service_starts_at" timestamp with time zone NOT NULL,
  "service_ends_at" timestamp with time zone NOT NULL, "suspended_at" timestamp with time zone,
  "cancel_at" timestamp with time zone, "pending_offer_version" integer,
  "created_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE TABLE "billing_receipt_claim" (
  "id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL, "operation_key" text NOT NULL UNIQUE,
  "customer_person_id" integer NOT NULL REFERENCES "person"("id"), "invoice_id" text NOT NULL,
  "amount_atoms" numeric(39,0) NOT NULL, "evidence" text NOT NULL,
  "state" text DEFAULT 'pending' NOT NULL, "confirmed_at" timestamp with time zone,
  "created_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
CREATE INDEX "billing_receipt_state_idx" ON "billing_receipt_claim" ("state");
--> statement-breakpoint
CREATE TABLE "billing_adjustment" (
  "id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL, "operation_key" text NOT NULL UNIQUE,
  "order_id" uuid NOT NULL REFERENCES "billing_order"("id"), "kind" text NOT NULL,
  "amount_atoms" numeric(39,0) NOT NULL, "evidence" text,
  "created_at" timestamp with time zone DEFAULT now() NOT NULL
);
