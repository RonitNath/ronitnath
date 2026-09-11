'use server';

import { revalidatePath } from 'next/cache';
import { z } from 'zod';

import { recordAudit } from '@/features/auth/audit';
import { userPath } from '@/lib/paths';
import { calendarDatabase, calendarRequest, ensureCalendarScope } from './server';

const inputSchema = z.object({
  timeZone: z.string().min(1).max(100),
  sessionId: z.string().min(1).max(200),
  activatedAt: z.string().datetime({ offset: true }),
  foreground: z.literal(true),
  expectedVersion: z.number().int().nonnegative(),
  idempotencyKey: z.string().min(8).max(200),
});

export async function reportCalendarTimeZone(user: string, raw: z.input<typeof inputSchema>): Promise<{ ok: boolean; accepted?: boolean; error?: string }> {
  const input = inputSchema.safeParse(raw);
  if (!input.success) return { ok: false, error: 'The device time zone report was invalid.' };
  const request = await calendarRequest(user);
  try {
    let accepted = false;
    await calendarDatabase().transaction(async (tx) => {
      await ensureCalendarScope(tx, request);
      const result = await request.store.reportTimeZone(tx, { scopeId: user, actor: request.actor, idempotencyKey: input.data.idempotencyKey, expectedVersion: input.data.expectedVersion }, input.data);
      accepted = result.accepted;
      if (accepted) await recordAudit(tx, { actorPersonId: request.subject.personId, command: 'report-calendar-time-zone', targetKind: 'person', targetId: request.subject.subjectPersonId, payload: { timeZone: result.timeZone, version: result.version } });
    });
    if (accepted) revalidatePath(userPath(user, 'calendar'));
    return { ok: true, accepted };
  } catch (error) {
    return { ok: false, error: error instanceof Error ? error.message : 'The time zone was not updated.' };
  }
}
