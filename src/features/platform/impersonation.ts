'use server';

/* SignInAs and EndImpersonation.
 *
 * An impersonation is an ordinary session whose user is the target and whose
 * `acting_operator_id` is the operator. Nothing in `allows` learns a new case
 * — the principal really is the target person, with exactly the authority they
 * have — and the operator is named in every audit row the session writes, by
 * `recordAudit` and nowhere else.
 *
 * Three refusals are structural. An operator may not become another operator,
 * because an operator who can would make RevokeOperator meaningless; a person
 * with no account cannot be signed in as, because there is no session to mint
 * — a guest somebody held by email is a person, not a door; and a deployment
 * can switch the whole command off with `RN_IMPERSONATION=off`, in which case
 * the control is not drawn and the command declines. */

import { and, eq, isNull } from 'drizzle-orm';
import { redirect } from 'next/navigation';
import { z } from 'zod';

import { database, schema } from '@/db/client';
import { recordAudit } from '@/features/auth/audit';
import type { FormState } from '@/features/auth/form-state';
import { endSession, mintSession, wearCookieFor } from '@/features/auth/impersonate';
import { currentPrincipal, OPERATOR_RESOURCE } from '@/features/auth/principal';
import { tryDecodeId } from '@/lib/ids';
import { requireOperator } from '@/lib/tiers';
import { impersonationEnabled, needsReauth, REAUTH_REQUIRED } from './reauth';

const NO_SUCH = 'That is not something you can do here.';

function field(form: FormData, name: string): string {
  const value = form.get(name);
  return typeof value === 'string' ? value : '';
}

const input = z.object({ person: z.string(), reason: z.string().trim().min(3).max(2000) });

/** The live person behind an id, and the user they sign in as. Null for a
 *  person who is merged away, disabled, or has never had an account. */
async function doorOf(personId: number): Promise<{ userId: string } | null> {
  const rows = await database()
    .select({ userId: schema.person.userId })
    .from(schema.person)
    .innerJoin(schema.party, eq(schema.party.id, schema.person.id))
    .where(
      and(
        eq(schema.person.id, personId),
        isNull(schema.person.mergedInto),
        isNull(schema.party.disabledAt),
      ),
    )
    .limit(1);
  const userId = rows[0]?.userId;
  return userId ? { userId } : null;
}

async function alsoOperator(personId: number): Promise<boolean> {
  const rows = await database()
    .select({ id: schema.relation.id })
    .from(schema.relation)
    .where(
      and(
        eq(schema.relation.subjectKind, 'person'),
        eq(schema.relation.subjectId, personId),
        eq(schema.relation.verb, 'operator'),
        eq(schema.relation.resourceKind, OPERATOR_RESOURCE.kind),
        eq(schema.relation.resourceId, OPERATOR_RESOURCE.id),
      ),
    )
    .limit(1);
  return rows.length > 0;
}

export async function signInAs(_prev: FormState, form: FormData): Promise<FormState> {
  if (!impersonationEnabled()) return { error: NO_SUCH };
  const { principal } = await requireOperator();
  if (needsReauth(principal)) return { error: REAUTH_REQUIRED, reauth: true };

  const parsed = input.safeParse({ person: field(form, 'person'), reason: field(form, 'reason') });
  if (!parsed.success) return { error: 'A reason of at least three characters.' };
  const target = tryDecodeId('person', parsed.data.person);
  if (target === null || target === principal.personId) return { error: NO_SUCH };

  const door = await doorOf(target);
  if (door === null) return { error: NO_SUCH };
  if (await alsoOperator(target)) {
    return { error: 'An operator cannot be signed in as. Revoke the grant first.' };
  }

  /* Order matters, and the reason is not obvious enough to leave unwritten.
   *
   * Anything that resolves a session — `recordAudit` asks who is acting, which
   * asks `currentPrincipal` — reads the token out of the *request* headers,
   * which still carry the operator's. Once that session row is gone,
   * better-auth answers "no session" and, through `nextCookies`, writes a
   * Set-Cookie that clears the session cookie — overwriting the one this
   * command just wrote, and landing the operator at the door instead of on
   * the page they asked for. Observed, not theorised: a probe cookie set on
   * the same response survived while the session cookie did not.
   *
   * So every read happens first, the audit row is written while the operator's
   * own session is still live, and the two session writes are the last thing
   * before the redirect. */
  const minted = await mintSession(door.userId, principal.personId);
  await database().transaction((tx) =>
    recordAudit(tx, {
      actorPersonId: principal.personId,
      command: 'sign-in-as',
      targetKind: 'person',
      targetId: target,
      /* `audit.target_id` is an integer; a session id is text now, so it goes
       * in the payload. */
      payload: { reason: parsed.data.reason, session: minted },
    }),
  );

  /* The operator's own session ends here and is minted again by
   * EndImpersonation: one browser holds one cookie, and a session nobody can
   * reach is a session that should not still be live. */
  await endSession(principal.sessionId);
  await wearCookieFor(minted);

  redirect('/app');
}

/** EndImpersonation. The worn session ends and the operator gets a fresh one
 *  of their own; the re-authentication window does not come back with it, so
 *  the next sharp command asks again. */
export async function endImpersonation(): Promise<void> {
  const principal = await currentPrincipal();
  if (!principal?.actingOperatorId) redirect('/');
  const operatorId = principal.actingOperatorId;

  const door = await doorOf(operatorId);
  await database().transaction((tx) =>
    recordAudit(tx, {
      actorPersonId: operatorId,
      command: 'end-impersonation',
      targetKind: 'person',
      targetId: principal.personId,
      payload: { person: principal.personId, session: principal.sessionId },
    }),
  );

  /* An operator whose own account has gone in the meantime is simply signed
   * out: there is nothing left to put back on. */
  if (door === null) {
    await endSession(principal.sessionId);
    redirect('/');
  }

  /* Last, and in this order, for the reason SignInAs sets out. */
  const minted = await mintSession(door.userId, null);
  await endSession(principal.sessionId);
  await wearCookieFor(minted);
  redirect('/platform');
}
