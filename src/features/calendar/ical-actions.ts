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
  return importedRecords(item).find((record) => !record.recurrenceId);
}

function importedRecords(item: StoredItem): IcalRecord[] {
  const records = item.metadata?.icalRecords;
  if (Array.isArray(records)) return records.filter((record) => record && typeof record === 'object') as IcalRecord[];
  const value = item.metadata?.ical;
  return value && typeof value === 'object' ? [value as IcalRecord] : [];
}

function existing(items: StoredItem[]): ExistingIcalRecord[] {
  return items.flatMap((item) => {
    return importedRecords(item).map((record) => ({ key: record.key, localId: item.id, fingerprint: record.fingerprint, sequence: record.sequence }));
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

function mapped(record: IcalRecord, records: IcalRecord[], id: string): Omit<StoredItem, 'scopeId' | 'version'> {
  const exceptions = Object.fromEntries(records.flatMap((candidate) => candidate.recurrenceId ? [[candidate.occurrenceKey ?? candidate.recurrenceId.replace(/Z$/, ''), {
    ...(candidate.status === 'cancelled' ? { cancelled: true } : {}),
    ...(candidate.timing ? { timing: candidate.timing } : {}),
  }]] : []));
  return {
    id,
    kind: record.kind,
    title: record.title,
    ...(record.timing ? { timing: record.timing } : {}),
    ...(record.rrule || record.rdates.length || record.exdates.length || Object.keys(exceptions).length ? { recurrence: { ...(record.rrule ? { rrule: record.rrule } : {}), rdates: record.rdates, exdates: record.exdates, ...(Object.keys(exceptions).length ? { exceptions } : {}) } } : {}),
    ...(record.kind === 'task' ? { task: { ...(record.due ? { due: record.due } : {}), completedOccurrenceIds: [] } } : {}),
    metadata: { ical: record, icalRecords: records },
    committed: false,
    status: record.status === 'cancelled' ? 'cancelled' : 'active',
  };
}

export async function applyCalendarImport(user: string, source: string, choices: Record<string, 'replace' | 'skip'>, idempotencyKey: string): Promise<{ ok: boolean; applied?: number; error?: string }> {
  try {
    const request = await calendarRequest(user);
    const { preview, items } = await authorizedPreview(user, source);
    const application = applyImport(preview, choices);
    const byIcalKey = new Map(items.flatMap((item) => importedRecords(item).map((record) => [record.key, item] as const)));
    const groups = new Map<string, IcalRecord[]>();
    for (const record of application.upserts) groups.set(record.uid, [...(groups.get(record.uid) ?? []), record]);
    await calendarDatabase().transaction(async (tx) => {
      await ensureCalendarScope(tx, request);
      for (const [uid, changed] of groups) {
        const prior = byIcalKey.get(`${uid}::master`) ?? changed.map(({ key }) => byIcalKey.get(key)).find(Boolean);
        const records = new Map((prior ? importedRecords(prior) : []).map((record) => [record.key, record]));
        for (const record of changed) records.set(record.key, record);
        const master = records.get(`${uid}::master`);
        if (!master) throw new Error(`Calendar series ${uid} has no master component.`);
        const combined = [...records.values()];
        const fingerprint = createHash('sha256').update(combined.map(({ fingerprint: value }) => value).sort().join(':')).digest('hex').slice(0, 32);
        if (prior) {
          await request.store.editItem(tx, { scopeId: user, actor: request.actor, idempotencyKey: `${idempotencyKey}-${fingerprint}`, expectedVersion: prior.version }, prior.id, mapped(master, combined, prior.id));
        } else {
          const id = localId(master);
          await request.store.createItem(tx, { scopeId: user, actor: request.actor, idempotencyKey: `${idempotencyKey}-${fingerprint}`, expectedVersion: 0 }, mapped(master, combined, id));
        }
      }
      await recordAudit(tx, { actorPersonId: request.subject.personId, command: 'import-calendar-file', targetKind: 'person', targetId: request.subject.subjectPersonId, payload: { applied: application.upserts.length, skipped: application.skipped.length } });
    });
    revalidatePath(userPath(user, 'calendar'));
    return { ok: true, applied: groups.size };
  } catch (error) {
    return { ok: false, error: error instanceof Error ? error.message : 'The calendar was not imported.' };
  }
}

function exportRecord(item: StoredItem): IcalRecord {
  const original = imported(item);
  const current: IcalRecord = {
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
  if (!original) return current;
  const merged: IcalRecord = {
    ...original,
    title: current.title,
    status: current.status,
    sequence: current.sequence,
    rdates: current.rdates,
    exdates: current.exdates,
  };
  if (current.timing) merged.timing = current.timing; else delete merged.timing;
  if (current.due) merged.due = current.due; else delete merged.due;
  if (current.rrule) merged.rrule = current.rrule; else delete merged.rrule;
  return merged;
}

function exportRecords(item: StoredItem): IcalRecord[] {
  const master = exportRecord(item);
  const originalExceptions = new Map(importedRecords(item).filter(({ recurrenceId }) => recurrenceId).map((record) => [record.occurrenceKey ?? record.recurrenceId!.replace(/Z$/, ''), record]));
  const exceptions = Object.entries(item.recurrence?.exceptions ?? {}).map(([recurrenceId, exception]) => {
    const original = originalExceptions.get(recurrenceId);
    return {
      ...(original ?? { ...master, rawJCal: [], rdates: [], exdates: [], rrule: undefined }),
      key: `${master.uid}::${recurrenceId}`,
      recurrenceId,
      occurrenceKey: recurrenceId,
      sequence: item.version,
      status: exception.cancelled ? 'cancelled' as const : 'active' as const,
      ...(exception.timing ? { timing: exception.timing } : {}),
    } as IcalRecord;
  });
  return [master, ...exceptions];
}

export async function exportPersonalCalendar(user: string): Promise<string> {
  const request = await calendarRequest(user, 'calendar/export');
  const items = await request.store.listItems(calendarDatabase(), request);
  return exportCalendar({ name: `${request.subject.subjectName}'s calendar`, records: items.flatMap(exportRecords) });
}
