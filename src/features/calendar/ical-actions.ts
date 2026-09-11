'use server';

import { createHash } from 'node:crypto';
import { revalidatePath } from 'next/cache';

import { applyImport, exportCalendar, previewImport, type ExistingIcalRecord, type IcalRecord, type ImportPreview } from '@isoastra/calendar-ical';
import type { StoredItem } from '@isoastra/fleet-calendar';

import { recordAudit } from '@/features/auth/audit';
import { userPath } from '@/lib/paths';
import { calendarDatabase, calendarRequest, ensureCalendarScope } from './server';

const MAX_ICS_BYTES = 1_000_000;

function imported(item: StoredItem): IcalRecord | undefined {
  const value = item.metadata?.ical;
  return value && typeof value === 'object' ? value as IcalRecord : undefined;
}

function existing(items: StoredItem[]): ExistingIcalRecord[] {
  return items.flatMap((item) => {
    const record = imported(item);
    return record ? [{ key: record.key, localId: item.id, fingerprint: record.fingerprint, sequence: record.sequence }] : [];
  });
}

async function authorizedPreview(user: string, source: string): Promise<{ preview: ImportPreview; items: StoredItem[] }> {
  if (new TextEncoder().encode(source).byteLength > MAX_ICS_BYTES) throw new RangeError('The calendar file is larger than 1 MB.');
  const request = await calendarRequest(user);
  const items = await request.store.listItems(calendarDatabase(), request);
  return { preview: previewImport(source, { existing: existing(items) }), items };
}

export async function previewCalendarImport(user: string, source: string): Promise<{ ok: true; preview: ImportPreview } | { ok: false; error: string }> {
  try {
    return { ok: true, preview: (await authorizedPreview(user, source)).preview };
  } catch (error) {
    return { ok: false, error: error instanceof Error ? error.message : 'The calendar could not be read.' };
  }
}

function localId(record: IcalRecord): string {
  return `ics-${createHash('sha256').update(record.key).digest('hex').slice(0, 32)}`;
}

function mapped(record: IcalRecord, id: string): Omit<StoredItem, 'scopeId' | 'version'> {
  return {
    id,
    kind: record.kind,
    title: record.title,
    ...(record.timing ? { timing: record.timing } : {}),
    ...(record.rrule || record.rdates.length || record.exdates.length ? { recurrence: { ...(record.rrule ? { rrule: record.rrule } : {}), rdates: record.rdates, exdates: record.exdates } } : {}),
    ...(record.kind === 'task' ? { task: { ...(record.due ? { due: record.due } : {}), completedOccurrenceIds: [] } } : {}),
    metadata: { ical: record },
    committed: false,
    status: record.status === 'cancelled' ? 'cancelled' : 'active',
  };
}

export async function applyCalendarImport(user: string, source: string, choices: Record<string, 'replace' | 'skip'>, idempotencyKey: string): Promise<{ ok: boolean; applied?: number; error?: string }> {
  try {
    const request = await calendarRequest(user);
    const { preview, items } = await authorizedPreview(user, source);
    const application = applyImport(preview, choices);
    const byIcalKey = new Map(items.flatMap((item) => {
      const record = imported(item);
      return record ? [[record.key, item] as const] : [];
    }));
    await calendarDatabase().transaction(async (tx) => {
      await ensureCalendarScope(tx, request);
      for (const record of application.upserts) {
        const prior = byIcalKey.get(record.key);
        if (prior) {
          await request.store.editItem(tx, { scopeId: user, actor: request.actor, idempotencyKey: `${idempotencyKey}-${record.fingerprint}`, expectedVersion: prior.version }, prior.id, mapped(record, prior.id));
        } else {
          const id = localId(record);
          await request.store.createItem(tx, { scopeId: user, actor: request.actor, idempotencyKey: `${idempotencyKey}-${record.fingerprint}`, expectedVersion: 0 }, mapped(record, id));
        }
      }
      await recordAudit(tx, { actorPersonId: request.subject.personId, command: 'import-calendar-file', targetKind: 'person', targetId: request.subject.subjectPersonId, payload: { applied: application.upserts.length, skipped: application.skipped.length } });
    });
    revalidatePath(userPath(user, 'calendar'));
    return { ok: true, applied: application.upserts.length };
  } catch (error) {
    return { ok: false, error: error instanceof Error ? error.message : 'The calendar was not imported.' };
  }
}

function exportRecord(item: StoredItem): IcalRecord {
  const original = imported(item);
  if (original) return original;
  return {
    key: `${item.id}::master`,
    uid: `${item.id}@ronitnath.com`,
    kind: item.kind === 'task' ? 'task' : 'event',
    title: item.title,
    status: item.status === 'cancelled' ? 'cancelled' : 'active',
    sequence: item.version,
    ...(item.timing ? { timing: item.timing } : {}),
    ...(item.task?.due ? { due: item.task.due } : {}),
    ...(item.recurrence?.rrule ? { rrule: item.recurrence.rrule } : {}),
    rdates: item.recurrence?.rdates ?? [],
    exdates: item.recurrence?.exdates ?? [],
    fingerprint: '',
    rawJCal: [],
    timeZoneDefinitions: {},
  };
}

export async function exportPersonalCalendar(user: string): Promise<string> {
  const request = await calendarRequest(user, 'calendar/export');
  const items = await request.store.listItems(calendarDatabase(), request);
  return exportCalendar({ name: `${request.subject.subjectName}'s calendar`, records: items.map(exportRecord) });
}
