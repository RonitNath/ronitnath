import { sql } from 'drizzle-orm';

import { CalendarStore, type CalendarActor } from '@isoastra/fleet-calendar';

import { database } from '@/db/client';
import type { Transaction } from '@/features/auth/db';
import { encodeId } from '@/lib/ids';
import { requireSubjectPerson, type SubjectContext } from '@/lib/tiers';

export interface CalendarRequest {
  subject: SubjectContext;
  scopeId: string;
  actor: CalendarActor;
  store: CalendarStore;
}

export async function calendarRequest(user: string, view = 'calendar'): Promise<CalendarRequest> {
  const subject = await requireSubjectPerson(user, view);
  const actorId = encodeId('person', subject.principal.personId);
  const actor: CalendarActor = {
    actorId,
    subjectId: user,
    ...(subject.viewingOther ? { actingOperatorId: actorId } : {}),
  };
  const store = new CalendarStore({
    authorize: (attempt) => attempt.scopeId === user
      && attempt.actor.subjectId === user
      && (attempt.actor.actorId === user || attempt.actor.actingOperatorId === attempt.actor.actorId),
  });
  return { subject, scopeId: user, actor, store };
}

export async function ensureCalendarScope(tx: Transaction, request: CalendarRequest): Promise<void> {
  await tx.execute(sql`INSERT INTO calendar_scope(scope_id,owner_id)
    VALUES(${request.scopeId},${request.scopeId}) ON CONFLICT DO NOTHING`);
}

export function calendarDatabase() {
  return database();
}
