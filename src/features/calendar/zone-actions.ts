'use server';

import { revalidatePath } from 'next/cache';
import { z } from 'zod';

import { recordAudit } from '@/features/auth/audit';
import { userPath } from '@/lib/paths';
import { calendarDatabase, calendarRequest, ensureCalendarScope } from './server';

const activationSchema = z.object({
  sessionId: z.string().min(1).max(200),
  expectedVersion: z.number().int().nonnegative(),
  idempotencyKey: z.string().min(8).max(200),
});

const inputSchema = z.object({
  timeZone: z.string().min(1).max(100),
  sessionId: z.string().min(1).max(200),
  generation: z.number().int().positive(),
  sequence: z.number().int().positive(),
  foreground: z.literal(true),
  expectedVersion: z.number().int().nonnegative(),
  idempotencyKey: z.string().min(8).max(200),
});

export async function activateCalendarTimeZoneSession(user: string, raw: z.input<typeof activationSchema>): Promise<{ ok: true; sessionId: string; generation: number; version: number } | { ok: false; error: string }> {
  const input = activationSchema.safeParse(raw);
  if (!input.success) return { ok: false, error: 'The device session was invalid.' };
  const request = await calendarRequest(user);
  try {
    let activation: { sessionId: string; generation: number; version: number } | undefined;
    await calendarDatabase().transaction(async (tx) => {
      await ensureCalendarScope(tx, request);
      activation = await request.store.activateTimeZoneSession(tx, {
        scopeId: user, actor: request.actor, idempotencyKey: input.data.idempotencyKey, expectedVersion: input.data.expectedVersion,
      }, input.data.sessionId);
    });
    return { ok: true, ...activation! };
  } catch (error) {
    return { ok: false, error: error instanceof Error ? error.message : 'The device session was not activated.' };
  }
}

export async function reportCalendarTimeZone(user: string, raw: z.input<typeof inputSchema>): Promise<{ ok: true; accepted: boolean; timeZone: string; version: number } | { ok: false; error: string }> {
  const input = inputSchema.safeParse(raw);
  if (!input.success) return { ok: false, error: 'The device time zone report was invalid.' };
  const request = await calendarRequest(user);
  try {
    let report: { accepted: boolean; timeZone: string; version: number } | undefined;
    await calendarDatabase().transaction(async (tx) => {
      await ensureCalendarScope(tx, request);
      const result = await request.store.reportTimeZone(tx, { scopeId: user, actor: request.actor, idempotencyKey: input.data.idempotencyKey, expectedVersion: input.data.expectedVersion }, input.data);
      report = result;
      if (result.accepted) await recordAudit(tx, { actorPersonId: request.subject.personId, command: 'report-calendar-time-zone', targetKind: 'person', targetId: request.subject.subjectPersonId, payload: { timeZone: result.timeZone, version: result.version } });
    });
    if (report!.accepted) revalidatePath(userPath(user, 'calendar'));
    return { ok: true, ...report! };
  } catch (error) {
    return { ok: false, error: error instanceof Error ? error.message : 'The time zone was not updated.' };
  }
}
