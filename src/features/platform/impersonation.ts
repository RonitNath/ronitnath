'use server';

/* SignInAs and EndImpersonation.
 *
 * An impersonation is an ordinary session whose person is the target and
 * whose `acting_operator_id` is the operator. Nothing in `allows` learns a
 * new case — the principal really is the target person, with exactly the
 * authority they have — and the operator is named in every audit row the
 * session writes, by `recordAudit` and nowhere else.
 *
 * Two refusals are structural. An operator may not become another operator,
 * because an operator who can would make RevokeOperator meaningless; and a
 * deployment can switch the whole command off with `RN_IMPERSONATION=off`,
 * in which case the control is not drawn and the command declines. */

import { and, eq, isNull, sql } from 'drizzle-orm';
import { redirect } from 'next/navigation';
import { z } from 'zod';

import { database, schema } from '@/db/client';
import { recordAudit } from '@/features/auth/audit';
import type { FormState } from '@/features/auth/form-state';
import {
  createSession,
  currentPrincipal,
  OPERATOR_RESOURCE,
  requestFingerprint,
  setSessionCookie,
} from '@/features/auth/session';
import { tryDecodeId } from '@/lib/ids';
import { requireOperator } from '@/lib/tiers';
import { impersonationEnabled, needsReauth, REAUTH_REQUIRED } from './reauth';

const NO_SUCH = 'That is not something you can do here.';

function field(form: FormData, name: string): string {
  const value = form.get(name);
  return typeof value === 'string' ? value : '';
}

const input = z.object({ person: z.string(), reason: z.string().trim().min(3).max(2000) });

export async function signInAs(_prev: FormState, form: FormData): Promise<FormState> {
  if (!impersonationEnabled()) return { error: NO_SUCH };
  const { principal } = await requireOperator();
  if (needsReauth(principal)) return { error: REAUTH_REQUIRED, reauth: true };

  const parsed = input.safeParse({ person: field(form, 'person'), reason: field(form, 'reason') });
  if (!parsed.success) return { error: 'A reason of at least three characters.' };
  const target = tryDecodeId('person', parsed.data.person);
  if (target === null || target === principal.personId) return { error: NO_SUCH };

  const where = await requestFingerprint();
  const minted = await database().transaction(async (tx) => {
    const live = await tx
      .select({ id: schema.person.id })
      .from(schema.person)
      .innerJoin(schema.party, eq(schema.party.id, schema.person.id))
      .where(
        and(
          eq(schema.person.id, target),
          isNull(schema.person.mergedInto),
          isNull(schema.party.disabledAt),
        ),
      )
      .limit(1);
    if (live.length === 0) return null;

    const alsoOperator = await tx
      .select({ id: schema.relation.id })
      .from(schema.relation)
      .where(
        and(
          eq(schema.relation.subjectKind, 'person'),
          eq(schema.relation.subjectId, target),
          eq(schema.relation.verb, 'operator'),
          eq(schema.relation.resourceKind, OPERATOR_RESOURCE.kind),
          eq(schema.relation.resourceId, OPERATOR_RESOURCE.id),
        ),
      )
      .limit(1);
    if (alsoOperator.length > 0) return 'operator' as const;

    /* The operator's own session ends here and is minted again by
     * EndImpersonation: one browser holds one cookie, and a session nobody
     * can reach is a session that should not still be live. */
    await tx
      .update(schema.session)
      .set({ revokedAt: sql`now()` })
      .where(eq(schema.session.id, principal.sessionId));

    const session = await createSession(tx, {
      personId: target,
      source: 'local',
      userAgent: where.userAgent,
      ip: where.ip,
      actingOperatorId: principal.personId,
    });
    await recordAudit(tx, {
      actorPersonId: principal.personId,
      command: 'sign-in-as',
      targetKind: 'person',
      targetId: target,
      payload: { reason: parsed.data.reason, session: session.sessionId },
    });
    return session.token;
  });

  if (minted === null) return { error: NO_SUCH };
  if (minted === 'operator') {
    return { error: 'An operator cannot be signed in as. Revoke the grant first.' };
  }
  await setSessionCookie(minted);
  redirect('/app');
}

/** EndImpersonation. The impersonation session ends and the operator gets a
 *  fresh one of their own; the re-authentication window does not come back
 *  with it, so the next sharp command asks again. */
export async function endImpersonation(): Promise<void> {
  const principal = await currentPrincipal();
  if (!principal?.actingOperatorId) redirect('/');
  const operatorId = principal.actingOperatorId;
  const where = await requestFingerprint();

  const token = await database().transaction(async (tx) => {
    await tx
      .update(schema.session)
      .set({ revokedAt: sql`now()` })
      .where(eq(schema.session.id, principal.sessionId));
    await recordAudit(tx, {
      actorPersonId: operatorId,
      command: 'end-impersonation',
      targetKind: 'person',
      targetId: principal.personId,
      payload: { person: principal.personId },
    });
    const session = await createSession(tx, {
      personId: operatorId,
      /* The door they came through is the door they are still standing in. */
      source: principal.source,
      userAgent: where.userAgent,
      ip: where.ip,
    });
    return session.token;
  });

  await setSessionCookie(token);
  redirect('/platform');
}
