'use server';

import { z } from 'zod';

import type { CalendarOccurrence } from '@isoastra/calendar-react/model';

import { calendarDatabase, calendarRequest } from './server';

const inputSchema = z.object({
  from: z.string().datetime({ offset: true }),
  to: z.string().datetime({ offset: true }),
  timeZone: z.string().min(1).max(100),
}).refine(({ from, to }) => new Date(from) < new Date(to), 'Window end must follow its start');

export async function loadCalendarWindow(user: string, raw: z.input<typeof inputSchema>): Promise<{ ok: true; occurrences: CalendarOccurrence[] } | { ok: false; error: string }> {
  const input = inputSchema.safeParse(raw);
  if (!input.success) return { ok: false, error: 'The calendar window was invalid.' };
  const request = await calendarRequest(user);
  try {
    const db = calendarDatabase();
    const [items, result] = await Promise.all([
      request.store.listItems(db, request),
      request.store.occurrences(db, request, { from: input.data.from, to: input.data.to, displayTimeZone: input.data.timeZone, maxCandidates: 20_000, maxOccurrences: 5_000 }),
    ]);
    const byId = new Map(items.map((item) => [item.id, item]));
    return { ok: true, occurrences: result.occurrences.filter(({ cancelled }) => !cancelled).flatMap((occurrence) => {
      const item = byId.get(occurrence.seriesId);
      if (!item) return [];
      return [{ ...occurrence, title: item.title, kind: item.kind === 'work-block' ? 'work-block' as const : 'event' as const, editable: true }];
    }) };
  } catch (error) {
    return { ok: false, error: error instanceof Error ? error.message : 'The calendar window could not be loaded.' };
  }
}
