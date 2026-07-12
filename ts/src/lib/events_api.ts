import { z } from "zod";
import type { GuestView } from "../generated/GuestView";
import type { RsvpSubmit } from "../generated/RsvpSubmit";
import type { RsvpResult } from "../generated/RsvpResult";
import { csrfToken } from "./api";

// Capability-anonymous requests need no synchronizer token. When the page
// exposes an ambient session token, every RSVP endpoint carries it — including
// a capability URL that belongs to a different person.

const scheduleItemSchema = z.object({
  id: z.number(),
  sort_order: z.number(),
  time_label: z.string(),
  title: z.string(),
  detail: z.string(),
  tier: z.string(),
  segment_key: z.string().nullable(),
});

const guestViewSchema = z.object({
  event: z.object({
    title: z.string(),
    tagline: z.string(),
    starts_at: z.string(),
    ends_at: z.string().nullable(),
    timezone: z.string(),
    status: z.string(),
    summary: z.string(),
    area_name: z.string(),
    address: z.string().nullable(),
    entry_instructions: z.string().nullable(),
    private_details: z.string().nullable(),
  }),
  schedule: z.array(scheduleItemSchema),
  segment_counts: z.array(
    z.object({
      schedule_item_id: z.number(),
      in_count: z.number(),
      maybe_count: z.number(),
    }),
  ),
  person: z
    .object({
      name: z.string(),
      attendance: z
        .object({
          person_id: z.number(),
          status: z.string(),
          party_size: z.number(),
          note: z.string(),
          updated_at: z.string(),
        })
        .nullable(),
      segments: z.array(
        z.object({ schedule_item_id: z.number(), status: z.string() }),
      ),
    })
    .nullable(),
}) satisfies z.ZodType<GuestView>;

const rsvpResultSchema = z.object({
  person_name: z.string(),
  personal_url: z.string().nullable(),
}) satisfies z.ZodType<RsvpResult>;

async function errorMessage(res: Response, fallback: string): Promise<string> {
  const body = await res.json().catch(() => null);
  return typeof body?.error === "string" ? body.error : fallback;
}

export async function fetchGuestView(endpoint: string): Promise<GuestView> {
  const res = await fetch(endpoint);
  if (!res.ok) {
    throw new Error(await errorMessage(res, `Couldn't load this event (${res.status}).`));
  }
  return guestViewSchema.parse(await res.json());
}

export async function postRsvp(endpoint: string, submit: RsvpSubmit): Promise<RsvpResult> {
  const csrf = csrfToken();
  const res = await fetch(`${endpoint}/rsvp`, {
    method: "POST",
    headers: {
      "content-type": "application/json",
      ...(csrf ? { "x-csrf-token": csrf } : {}),
    },
    body: JSON.stringify(submit),
  });
  if (!res.ok) {
    throw new Error(await errorMessage(res, `RSVP failed (${res.status}).`));
  }
  return rsvpResultSchema.parse(await res.json());
}
