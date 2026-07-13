import type { GuestView } from "../generated/GuestView";
import type { RsvpSubmit } from "../generated/RsvpSubmit";
import type { RsvpResult } from "../generated/RsvpResult";
import { assertShape, csrfToken, errorMessage, isRecord, jsonBody } from "./api";

// Capability-anonymous requests need no synchronizer token. When the page
// exposes an ambient session token, every RSVP endpoint carries it — including
// a capability URL that belongs to a different person.

function isScheduleItem(value: unknown): boolean {
  return (
    isRecord(value) &&
    typeof value.id === "number" &&
    typeof value.time_label === "string" &&
    typeof value.title === "string" &&
    (value.segment_key === null || typeof value.segment_key === "string")
  );
}

function isSegmentCount(value: unknown): boolean {
  return (
    isRecord(value) &&
    typeof value.schedule_item_id === "number" &&
    typeof value.in_count === "number"
  );
}

function isAttendance(value: unknown): boolean {
  return (
    isRecord(value) &&
    typeof value.status === "string" &&
    typeof value.party_size === "number" &&
    typeof value.note === "string"
  );
}

function isSegmentRsvp(value: unknown): boolean {
  return (
    isRecord(value) &&
    typeof value.schedule_item_id === "number" &&
    typeof value.status === "string"
  );
}

function isGuestPerson(value: unknown): boolean {
  return (
    isRecord(value) &&
    typeof value.name === "string" &&
    (value.attendance === null || isAttendance(value.attendance)) &&
    Array.isArray(value.segments) &&
    value.segments.every(isSegmentRsvp)
  );
}

function isGuestView(value: unknown): value is GuestView {
  return (
    isRecord(value) &&
    isRecord(value.event) &&
    Array.isArray(value.schedule) &&
    value.schedule.every(isScheduleItem) &&
    Array.isArray(value.segment_counts) &&
    value.segment_counts.every(isSegmentCount) &&
    (value.person === null || isGuestPerson(value.person))
  );
}

function isRsvpResult(value: unknown): value is RsvpResult {
  return (
    isRecord(value) &&
    typeof value.person_name === "string" &&
    (value.personal_url === null || typeof value.personal_url === "string")
  );
}

export async function fetchGuestView(endpoint: string): Promise<GuestView> {
  const res = await fetch(endpoint);
  if (!res.ok) {
    throw new Error(await errorMessage(res, `Couldn't load this event (${res.status}).`));
  }
  const body = await jsonBody(res, "RSVP view");
  assertShape(body, isGuestView, "RSVP view");
  return body;
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
  const body = await jsonBody(res, "RSVP submission");
  assertShape(body, isRsvpResult, "RSVP submission");
  return body;
}
