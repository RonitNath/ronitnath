import { z } from "zod";
import type { GuestbookEntry } from "../generated/GuestbookEntry";
import type { NewGuestbookEntry } from "../generated/NewGuestbookEntry";

// `satisfies z.ZodType<Generated>` turns drift between the Rust type and
// this schema into a TS compile error, instead of a silent runtime mismatch.
const guestbookEntrySchema = z.object({
  id: z.number(),
  author: z.string(),
  message: z.string(),
  created_at: z.string(),
}) satisfies z.ZodType<GuestbookEntry>;

const guestbookListSchema = z.array(guestbookEntrySchema);

async function errorMessage(res: Response, fallback: string): Promise<string> {
  const body = await res.json().catch(() => null);
  return typeof body?.error === "string" ? body.error : fallback;
}

// Read once per call rather than cached: a page never changes this after
// load, but re-reading keeps the helper simple and correct if that ever
// changes (e.g. a future SPA-style nav that swaps the meta tag).
function csrfToken(): string {
  return document.querySelector('meta[name="csrf-token"]')?.getAttribute("content") ?? "";
}

export async function fetchGuestbook(): Promise<GuestbookEntry[]> {
  const res = await fetch("/api/guestbook");
  if (!res.ok) {
    throw new Error(await errorMessage(res, `GET /api/guestbook failed: ${res.status}`));
  }
  return guestbookListSchema.parse(await res.json());
}

export async function postGuestbookEntry(
  entry: NewGuestbookEntry,
): Promise<GuestbookEntry> {
  const res = await fetch("/api/guestbook", {
    method: "POST",
    headers: { "content-type": "application/json", "x-csrf-token": csrfToken() },
    body: JSON.stringify(entry),
  });
  if (!res.ok) {
    throw new Error(await errorMessage(res, `POST /api/guestbook failed: ${res.status}`));
  }
  return guestbookEntrySchema.parse(await res.json());
}
