import type { GuestbookEntry } from "../generated/GuestbookEntry";
import type { NewGuestbookEntry } from "../generated/NewGuestbookEntry";

export function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

export function assertShape<T>(
  value: unknown,
  guard: (candidate: unknown) => candidate is T,
  label: string,
): asserts value is T {
  if (!guard(value)) throw new Error(`Invalid ${label} response.`);
}

function hasJsonContentType(res: Response): boolean {
  const contentType = res.headers.get("content-type")?.toLowerCase() ?? "";
  return contentType.includes("application/json") || contentType.includes("+json");
}

export async function jsonBody(res: Response, label: string): Promise<unknown> {
  if (!hasJsonContentType(res)) throw new Error(`${label} returned a non-JSON response.`);
  try {
    return await res.json();
  } catch {
    throw new Error(`${label} returned invalid JSON.`);
  }
}

export async function errorMessage(res: Response, fallback: string): Promise<string> {
  if (!hasJsonContentType(res)) return fallback;
  const body: unknown = await res.json().catch(() => null);
  return isRecord(body) && typeof body.error === "string" ? body.error : fallback;
}

function isGuestbookEntry(value: unknown): value is GuestbookEntry {
  return (
    isRecord(value) &&
    typeof value.id === "number" &&
    typeof value.author === "string" &&
    typeof value.message === "string" &&
    typeof value.created_at === "string"
  );
}

function isGuestbookEntries(value: unknown): value is GuestbookEntry[] {
  return Array.isArray(value) && value.every(isGuestbookEntry);
}

// Read once per call rather than cached: a page never changes this after
// load, but re-reading keeps the helper correct if a future navigation swaps it.
export function csrfToken(): string {
  return document.querySelector('meta[name="csrf-token"]')?.getAttribute("content") ?? "";
}

export async function fetchGuestbook(): Promise<GuestbookEntry[]> {
  const res = await fetch("/api/guestbook");
  if (!res.ok) {
    throw new Error(await errorMessage(res, `GET /api/guestbook failed: ${res.status}`));
  }
  const body = await jsonBody(res, "GET /api/guestbook");
  assertShape(body, isGuestbookEntries, "guestbook list");
  return body;
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
  const body = await jsonBody(res, "POST /api/guestbook");
  assertShape(body, isGuestbookEntry, "guestbook entry");
  return body;
}
