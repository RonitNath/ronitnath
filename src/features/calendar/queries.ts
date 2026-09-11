import { sql } from 'drizzle-orm';

import { expandOccurrences, resolveIntent, type Occurrence } from '@isoastra/calendar-core';
import type { StoredItem } from '@isoastra/fleet-calendar';
import type { CalendarOccurrence, CalendarTaskView } from '@isoastra/calendar-react/model';

import { database } from '@/db/client';
import { calendarRequest } from './server';

export interface PersonalCalendarData {
  activeTimeZone: string;
  timeZoneVersion: number;
  occurrences: CalendarOccurrence[];
  tasks: CalendarTaskView[];
  items: StoredItem[];
  viewingOther: boolean;
}

function eventOccurrences(item: StoredItem, zone: string, from: string, to: string): Occurrence[] {
  if (!item.timing) return [];
  if (item.timing.kind === 'date') {
    const interval = resolveIntent(item.timing, zone);
    return [{ id: `${item.id}/${item.timing.date}`, seriesId: item.id, recurrenceId: item.timing.date, start: interval.start, end: interval.end, originalStart: interval.start, cancelled: item.status === 'cancelled' }];
  }
  return expandOccurrences({ id: item.id, timing: item.timing, ...item.recurrence }, { from, to, maxCandidates: 20_000, maxOccurrences: 5_000 }).occurrences;
}

export async function loadPersonalCalendar(user: string): Promise<PersonalCalendarData> {
  const request = await calendarRequest(user);
  const db = database();
  const scopeRows = await db.execute<{ time_zone: string; time_zone_version: string }>(sql`
    SELECT time_zone,time_zone_version FROM calendar_scope WHERE scope_id=${request.scopeId}`);
  const activeTimeZone = scopeRows.rows[0]?.time_zone ?? 'UTC';
  const timeZoneVersion = Number(scopeRows.rows[0]?.time_zone_version ?? 0);
  const items = await request.store.listItems(db, request);
  const completions = await request.store.taskCompletions(db, request);
  const completed = new Set(completions.map(({ itemId, occurrenceId }) => `${itemId}/${occurrenceId}`));
  const now = new Date();
  const from = new Date(Date.UTC(now.getUTCFullYear() - 1, 0, 1)).toISOString();
  const to = new Date(Date.UTC(now.getUTCFullYear() + 3, 0, 1)).toISOString();
  const occurrences = items
    .filter((item) => item.kind !== 'task' && item.status !== 'cancelled')
    .flatMap((item) => eventOccurrences(item, activeTimeZone, from, to).map((occurrence) => ({
      ...occurrence,
      title: item.title,
      kind: item.kind === 'work-block' ? 'work-block' as const : 'event' as const,
      editable: true,
    })));
  const tasks = items.filter((item) => item.kind === 'task' && item.status !== 'cancelled').flatMap((item) => {
    if (item.timing && item.timing.kind !== 'date' && item.recurrence?.rrule) {
      return eventOccurrences(item, activeTimeZone, from, to).map((occurrence) => ({
        id: item.id,
        occurrenceId: occurrence.recurrenceId,
        title: item.title,
        due: occurrence.start,
        estimateMinutes: item.task?.estimateMinutes,
        completed: completed.has(`${item.id}/${occurrence.recurrenceId}`),
      }));
    }
    const occurrenceId = item.task?.due ?? item.id;
    return [{
      id: item.id,
      occurrenceId,
      title: item.title,
      due: item.task?.due,
      estimateMinutes: item.task?.estimateMinutes,
      completed: completed.has(`${item.id}/${occurrenceId}`),
    }];
  });
  return { activeTimeZone, timeZoneVersion, occurrences, tasks, items, viewingOther: request.subject.viewingOther };
}
