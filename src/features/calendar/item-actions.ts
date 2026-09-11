'use server';

import { randomUUID } from 'node:crypto';
import { revalidatePath } from 'next/cache';
import { z } from 'zod';

import { createFollowMeIntent, editRecurringSeries, expandOccurrences, type RecurringSeries, type TemporalIntent } from '@isoastra/calendar-core';
import { CalendarConflictError, type StoredItem } from '@isoastra/fleet-calendar';

import { recordAudit } from '@/features/auth/audit';
import { userPath } from '@/lib/paths';
import { calendarDatabase, calendarRequest, ensureCalendarScope } from './server';

const createInput = z.object({
  idempotencyKey: z.string().min(8).max(200),
  kind: z.enum(['event', 'task', 'work-block']),
  title: z.string().trim().min(1).max(140),
  localStart: z.string().max(40).optional(),
  due: z.string().max(40).optional(),
  durationMinutes: z.number().int().min(1).max(7 * 24 * 60).optional(),
  estimateMinutes: z.number().int().min(1).max(10_000).optional(),
  timeZone: z.string().min(1).max(100),
  followMe: z.boolean().default(false),
  rrule: z.string().max(500).optional(),
  linkedTaskId: z.string().max(200).optional(),
});

function timingOf(input: z.infer<typeof createInput>): TemporalIntent | undefined {
  if (!input.localStart || !input.durationMinutes) return undefined;
  return input.followMe
    ? createFollowMeIntent(input.localStart, input.durationMinutes, input.timeZone, 0)
    : { kind: 'zoned', localStart: input.localStart, timeZone: input.timeZone, durationMinutes: input.durationMinutes };
}

export async function createCalendarItem(user: string, raw: z.input<typeof createInput>): Promise<{ ok: true; id: string } | { ok: false; error: string }> {
  const parsed = createInput.safeParse(raw);
  if (!parsed.success) return { ok: false, error: 'A title and valid time are required.' };
  const request = await calendarRequest(user);
  const input = parsed.data;
  const timing = timingOf(input);
  if (input.kind !== 'task' && !timing) return { ok: false, error: 'Choose when this starts and how long it lasts.' };
  const id = randomUUID();
  try {
    await calendarDatabase().transaction(async (tx) => {
      await ensureCalendarScope(tx, request);
      await request.store.createItem(tx, { scopeId: user, actor: request.actor, idempotencyKey: input.idempotencyKey, expectedVersion: 0 }, {
        id,
        kind: input.kind,
        title: input.title,
        ...(timing ? { timing } : {}),
        ...(input.rrule ? { recurrence: { rrule: input.rrule } } : {}),
        ...(input.kind === 'task' ? { task: { due: input.due, estimateMinutes: input.estimateMinutes, completedOccurrenceIds: [] } } : {}),
        ...(input.linkedTaskId ? { linkedTaskId: input.linkedTaskId } : {}),
        committed: false,
        status: 'active',
      });
      await recordAudit(tx, { actorPersonId: request.subject.personId, command: 'create-calendar-item', targetKind: 'person', targetId: request.subject.subjectPersonId, payload: { calendarItemId: id, kind: input.kind } });
    });
  } catch (error) {
    return { ok: false, error: error instanceof Error ? error.message : 'The item was not saved.' };
  }
  revalidatePath(userPath(user, 'calendar'));
  return { ok: true, id };
}

const editInput = z.object({
  id: z.string().min(1).max(300),
  recurrenceId: z.string().min(1).max(100),
  start: z.string().datetime({ offset: true }),
  end: z.string().datetime({ offset: true }),
  mode: z.enum(['single', 'all', 'future']),
  version: z.number().int().positive(),
  idempotencyKey: z.string().min(8).max(200),
});

function minutes(start: string, end: string): number {
  return (new Date(end).getTime() - new Date(start).getTime()) / 60_000;
}

export async function editCalendarOccurrence(user: string, raw: z.input<typeof editInput>): Promise<{ ok: boolean; error?: string }> {
  const parsed = editInput.safeParse(raw);
  if (!parsed.success || minutes(parsed.data.start, parsed.data.end) <= 0) return { ok: false, error: 'The end must follow the start.' };
  const request = await calendarRequest(user);
  const input = parsed.data;
  try {
    await calendarDatabase().transaction(async (tx) => {
      const item = (await request.store.listItems(tx, request)).find(({ id }) => id === input.id);
      if (!item) throw new CalendarConflictError('Calendar item changed.');
      const moved: TemporalIntent = { kind: 'instant', start: input.start, durationMinutes: minutes(input.start, input.end) };
      if (!item.recurrence?.rrule || input.mode === 'all') {
        await request.store.editItem(tx, { scopeId: user, actor: request.actor, idempotencyKey: input.idempotencyKey, expectedVersion: input.version }, item.id, { timing: moved });
      } else if (input.mode === 'single') {
        await request.store.editItem(tx, { scopeId: user, actor: request.actor, idempotencyKey: input.idempotencyKey, expectedVersion: input.version }, item.id, {
          recurrence: { ...item.recurrence, exceptions: { ...item.recurrence.exceptions, [input.recurrenceId]: { timing: moved } } },
        });
      } else {
        if (!item.timing || item.timing.kind === 'date') throw new CalendarConflictError('This series cannot be split at a time.');
        const series: RecurringSeries = { id: item.id, timing: item.timing, ...item.recurrence };
        const count = expandOccurrences(series, { from: '1900-01-01T00:00Z', to: input.recurrenceId, maxCandidates: 100_000, maxOccurrences: 100_000 }).occurrences.length;
        const [previous, next] = editRecurringSeries(series, { mode: 'future', recurrenceId: input.recurrenceId, nextSeriesId: randomUUID(), nextTiming: moved as Exclude<TemporalIntent, { kind: 'date' }>, consumedOccurrences: count });
        await request.store.editItem(tx, { scopeId: user, actor: request.actor, idempotencyKey: `${input.idempotencyKey}-old`, expectedVersion: input.version }, item.id, { recurrence: recurrenceOf(previous!) });
        await request.store.createItem(tx, { scopeId: user, actor: request.actor, idempotencyKey: `${input.idempotencyKey}-new`, expectedVersion: 0 }, cloneForSplit(item, next!));
      }
      await recordAudit(tx, { actorPersonId: request.subject.personId, command: 'edit-calendar-occurrence', targetKind: 'person', targetId: request.subject.subjectPersonId, payload: { calendarItemId: item.id, occurrenceId: input.recurrenceId, mode: input.mode } });
    });
  } catch (error) {
    return { ok: false, error: error instanceof Error ? error.message : 'The change was not saved.' };
  }
  revalidatePath(userPath(user, 'calendar'));
  return { ok: true };
}

function recurrenceOf(series: RecurringSeries): NonNullable<StoredItem['recurrence']> {
  return {
    ...(series.rrule ? { rrule: series.rrule } : {}),
    ...(series.rdates ? { rdates: series.rdates } : {}),
    ...(series.exdates ? { exdates: series.exdates } : {}),
    ...(series.exceptions ? { exceptions: series.exceptions } : {}),
    ...(series.calendar ? { calendar: series.calendar } : {}),
  };
}

function cloneForSplit(item: StoredItem, series: RecurringSeries): Omit<StoredItem, 'scopeId' | 'version'> {
  return { ...item, id: series.id, timing: series.timing, recurrence: recurrenceOf(series) };
}

export async function completeCalendarTask(user: string, input: { id: string; occurrenceId: string; version: number; idempotencyKey: string }): Promise<{ ok: boolean; error?: string }> {
  const request = await calendarRequest(user);
  try {
    await calendarDatabase().transaction(async (tx) => {
      await request.store.completeTaskOccurrence(tx, { scopeId: user, actor: request.actor, idempotencyKey: input.idempotencyKey, expectedVersion: input.version }, input.id, input.occurrenceId);
      await recordAudit(tx, { actorPersonId: request.subject.personId, command: 'complete-calendar-task', targetKind: 'person', targetId: request.subject.subjectPersonId, payload: { calendarItemId: input.id, occurrenceId: input.occurrenceId } });
    });
  } catch (error) {
    return { ok: false, error: error instanceof Error ? error.message : 'The task was not completed.' };
  }
  revalidatePath(userPath(user, 'calendar'));
  return { ok: true };
}
