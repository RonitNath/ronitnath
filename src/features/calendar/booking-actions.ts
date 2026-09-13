'use server';

import { randomUUID } from 'node:crypto';
import { revalidatePath } from 'next/cache';
import { z } from 'zod';

import { recordAudit } from '@/features/auth/audit';
import { userPath } from '@/lib/paths';
import { calendarDatabase, calendarRequest, ensureCalendarScope } from './server';

const command = z.object({ idempotencyKey: z.string().min(8).max(200), version: z.number().int().nonnegative() });
const resources = z.array(z.object({ resourceId: z.string().min(1).max(200), quantity: z.number().int().positive() })).min(1).max(20);

function failure(error: unknown): { ok: false; error: string } {
  return { ok: false, error: error instanceof Error ? error.message : 'The booking change was not saved.' };
}

export async function createCalendarResource(user: string, raw: { name: string; capacity: number; idempotencyKey: string }): Promise<{ ok: true } | { ok: false; error: string }> {
  const input = z.object({ name: z.string().trim().min(1).max(100), capacity: z.number().int().positive().max(10_000), idempotencyKey: z.string().min(8).max(200) }).safeParse(raw);
  if (!input.success) return { ok: false, error: 'Enter a resource name and positive capacity.' };
  const request = await calendarRequest(user);
  try {
    await calendarDatabase().transaction(async (tx) => {
      await ensureCalendarScope(tx, request);
      const id = randomUUID();
      await request.store.upsertResource(tx, { scopeId: user, actor: request.actor, idempotencyKey: input.data.idempotencyKey, expectedVersion: 0 }, { id, name: input.data.name, capacity: input.data.capacity });
      await recordAudit(tx, { actorPersonId: request.subject.personId, command: 'create-calendar-resource', targetKind: 'person', targetId: request.subject.subjectPersonId, payload: { resourceId: id, capacity: input.data.capacity } });
    });
    revalidatePath(userPath(user, 'calendar'));
    return { ok: true };
  } catch (error) { return failure(error); }
}

export async function createCalendarHold(user: string, raw: { start: string; end: string; resources: Array<{ resourceId: string; quantity: number }>; idempotencyKey: string }): Promise<{ ok: true; id: string } | { ok: false; error: string }> {
  const input = z.object({ start: z.string().datetime({ offset: true }), end: z.string().datetime({ offset: true }), resources, idempotencyKey: z.string().min(8).max(200) }).safeParse(raw);
  if (!input.success) return { ok: false, error: 'Choose a valid interval and at least one resource.' };
  const request = await calendarRequest(user);
  const id = randomUUID();
  try {
    await calendarDatabase().transaction(async (tx) => {
      await ensureCalendarScope(tx, request);
      await request.store.createHold(tx, { scopeId: user, actor: request.actor, idempotencyKey: input.data.idempotencyKey, expectedVersion: 0 }, { id, start: input.data.start, end: input.data.end, resources: input.data.resources });
      await recordAudit(tx, { actorPersonId: request.subject.personId, command: 'create-calendar-hold', targetKind: 'person', targetId: request.subject.subjectPersonId, payload: { bookingId: id } });
    });
    revalidatePath(userPath(user, 'calendar'));
    return { ok: true, id };
  } catch (error) { return failure(error); }
}

export async function changeCalendarBooking(user: string, raw: { id: string; action: 'confirm' | 'cancel'; version: number; idempotencyKey: string }): Promise<{ ok: true } | { ok: false; error: string }> {
  const input = command.extend({ id: z.string().min(1).max(200), action: z.enum(['confirm', 'cancel']) }).safeParse(raw);
  if (!input.success) return { ok: false, error: 'The booking command was invalid.' };
  const request = await calendarRequest(user);
  try {
    await calendarDatabase().transaction(async (tx) => {
      const context = { scopeId: user, actor: request.actor, idempotencyKey: input.data.idempotencyKey, expectedVersion: input.data.version };
      if (input.data.action === 'confirm') await request.store.confirmHold(tx, context, input.data.id);
      else await request.store.cancelBooking(tx, context, input.data.id);
      await recordAudit(tx, { actorPersonId: request.subject.personId, command: `${input.data.action}-calendar-booking`, targetKind: 'person', targetId: request.subject.subjectPersonId, payload: { bookingId: input.data.id } });
    });
    revalidatePath(userPath(user, 'calendar'));
    return { ok: true };
  } catch (error) { return failure(error); }
}
