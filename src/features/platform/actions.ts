'use server';

/* The operator's commands. Each is one transaction with its audit row inside
 * it, zod at the door, `requireOperator` before anything is read, and one
 * uniform decline for everything that is not allowed or not there.
 *
 * The exception to uniformity is the re-authentication refusal, and it is
 * deliberate: the caller is already inside the tier (src/features/platform/
 * reauth.ts). */

import { and, eq, isNull, ne, or, sql } from 'drizzle-orm';
import { revalidatePath } from 'next/cache';
import { headers } from 'next/headers';
import { z } from 'zod';

import { authSchema, database, schema } from '@/db/client';
import { recordAudit } from '@/features/auth/audit';
import type { FormState } from '@/features/auth/form-state';
import { auth } from '@/features/auth/auth';
import { currentPrincipal, OPERATOR_RESOURCE, type Principal } from '@/features/auth/principal';
import { mergePersons } from '@/features/people/merge';
import { SCORE } from '@/features/people/matches';
import { encodeId, tryDecodeId } from '@/lib/ids';
import { requireOperator } from '@/lib/tiers';
import { disableCascade, enableRestore } from './disable';
import { grantOperator as writeOperatorEdge, operatorPersonIds, refusesRevoke } from './operators';
import { needsReauth, REAUTH_REQUIRED } from './reauth';
import { splitMerge } from './split';
import { emit } from '@/lib/fleet/events';
import { operatorPath } from '@/lib/paths';

const NO_SUCH = 'That is not something you can do here.';
const PARTIES = operatorPath('parties');

/* An operator's commands are about somebody else, so they are published on
 * *that* person's stream: the point of the event is that the person's own
 * open tabs learn their sessions were ended or their account disabled. The
 * operator console has no stream of its own — `/o/isoastra` is a static
 * segment and does not reach the `/o/[org]` route — and adding one is a leg
 * of its own rather than a side effect of this one. */
function streamOf(personId: number): string {
  return encodeId('person', personId);
}

function field(form: FormData, name: string): string {
  const value = form.get(name);
  return typeof value === 'string' ? value : '';
}

const reason = z.string().trim().min(3).max(2000);

/** Who is asking. A non-operator never gets here: `requireOperator` answers
 *  with the 404 a page that is not theirs gets. */
async function operator(): Promise<Principal> {
  const { principal } = await requireOperator();
  return principal;
}

/** The gate in front of anything that destroys or impersonates. */
function stale(principal: Principal): FormState | null {
  return needsReauth(principal) ? { error: REAUTH_REQUIRED, reauth: true } : null;
}

/* ------------------------------------------------------------ re-auth */

/** ReAuthenticate, the local-account half: the password is presented again
 *  and the session is stamped. The OIDC half is a fresh round trip through
 *  `/auth/oidc/start?reauth=1`, which stamps the session it mints. */
export async function reauthenticate(_prev: FormState, form: FormData): Promise<FormState> {
  const principal = await operator();
  const secret = field(form, 'password');
  if (secret.length === 0) return { error: 'A password.' };

  /* The password is checked by better-auth against the account it belongs to,
   * so this file never sees a hash and never has to know which parameters the
   * stored one was made under. */
  try {
    await auth().api.verifyPassword({ body: { password: secret }, headers: await headers() });
  } catch {
    return { error: 'Authentication failed.' };
  }

  /* The stamp is a column this app added to `auth.session`; better-auth
   * selects it and never lets a client set it, which is what makes the
   * ten-minute window a session fact rather than a second cookie. */
  await database().transaction(async (tx) => {
    await tx
      .update(authSchema.session)
      .set({ reauthenticatedAt: sql`now()` })
      .where(eq(authSchema.session.id, principal.sessionId));
    await recordAudit(tx, {
      actorPersonId: principal.personId,
      command: 'reauthenticate',
      /* `audit.target_id` is an integer and a session id is text now, so the
       * session is named in the payload and the target is the person. */
      targetKind: 'person',
      targetId: principal.personId,
      payload: { source: 'local', session: principal.sessionId },
    });
    await emit(tx, {
      orgId: streamOf(principal.personId),
      resourceKind: 'person',
      resourceId: encodeId('person', principal.personId),
      kind: 'updated',
    });
  });

  revalidatePath(operatorPath());
  return { notice: 'Confirmed. The window is open for ten minutes.' };
}

/* ------------------------------------------------------------ matches */

/** ProposeMatch, the operator's half: two parties they believe are one
 *  person. It writes a question, exactly like the model's own proposals —
 *  nothing merges until somebody rules on it. */
export async function proposeMatch(_prev: FormState, form: FormData): Promise<FormState> {
  const me = await operator();
  const left = tryDecodeId('person', field(form, 'left'));
  const right = tryDecodeId('person', field(form, 'right'));
  if (left === null || right === null || left === right) return { error: NO_SUCH };

  const outcome = await database().transaction(async (tx) => {
    const identities = await tx
      .select({ id: schema.identity.id, personId: schema.identity.personId })
      .from(schema.identity)
      .where(or(eq(schema.identity.personId, left), eq(schema.identity.personId, right)))
      .orderBy(schema.identity.id);
    const a = identities.find((row) => row.personId === left);
    const b = identities.find((row) => row.personId === right);
    if (!a || !b) return 'no-identity' as const;

    const [identityA, identityB] = [a.id, b.id].sort((one, two) => one - two);
    const rows = await tx
      .insert(schema.match)
      .values({
        identityA: identityA!,
        identityB: identityB!,
        signal: 'operator',
        score: SCORE.claimed_link,
        evidence: 'proposed by an operator',
        proposedBy: me.personId,
      })
      .onConflictDoNothing()
      .returning({ id: schema.match.id });
    if (rows.length === 0) return 'already' as const;
    await recordAudit(tx, {
      actorPersonId: me.personId,
      command: 'propose-match',
      targetKind: 'match',
      targetId: rows[0]!.id,
      payload: { signal: 'operator', identity_a: identityA, identity_b: identityB },
    });
    /* Both sides: the question appears on each person's own account page. */
    for (const personId of [left, right]) {
      await emit(tx, {
        orgId: streamOf(personId),
        resourceKind: 'person',
        resourceId: encodeId('person', personId),
        kind: 'updated',
      });
    }
    return 'written' as const;
  });

  if (outcome === 'no-identity') {
    return { error: 'One of those parties has no identity to match against.' };
  }
  revalidatePath(operatorPath('matches'));
  return outcome === 'already'
    ? { notice: 'That question is already open.' }
    : { notice: 'Proposed. Nothing has merged.' };
}

/** RuleMatch: the operator answers a question neither side can answer for
 *  itself. The same merge routine ConfirmMatch runs, with the survivor named
 *  by the operator and the reason in the audit payload. */
export async function ruleMatch(_prev: FormState, form: FormData): Promise<FormState> {
  const me = await operator();
  const blocked = stale(me);
  if (blocked) return blocked;

  const parsed = z
    .object({ match: z.string(), survivor: z.string(), reason })
    .safeParse({
      match: field(form, 'match'),
      survivor: field(form, 'survivor'),
      reason: field(form, 'reason'),
    });
  if (!parsed.success) return { error: 'A survivor, and a reason of at least three characters.' };
  const matchId = tryDecodeId('match', parsed.data.match);
  const survivor = tryDecodeId('person', parsed.data.survivor);
  if (matchId === null || survivor === null) return { error: NO_SUCH };

  const ok = await database().transaction(async (tx) => {
    const rows = await tx
      .select({ identityA: schema.match.identityA, identityB: schema.match.identityB })
      .from(schema.match)
      .where(and(eq(schema.match.id, matchId), eq(schema.match.status, 'proposed')))
      .limit(1);
    const candidate = rows[0];
    if (!candidate) return false;

    const sides = await tx
      .select({ personId: schema.identity.personId })
      .from(schema.identity)
      .where(
        or(
          eq(schema.identity.id, candidate.identityA),
          eq(schema.identity.id, candidate.identityB),
        ),
      );
    const people = [...new Set(sides.map((row) => row.personId))];
    if (!people.includes(survivor)) return false;
    const absorbed = people.find((id) => id !== survivor);
    if (absorbed === undefined) return false;

    await tx
      .update(schema.match)
      .set({ status: 'confirmed', decidedAt: sql`now()`, decidedBy: me.personId })
      .where(eq(schema.match.id, matchId));
    /* One person absorbs the other: the survivor's surfaces gain rows, and
     * the absorbed person's stop being anybody's. */
    for (const personId of [survivor, absorbed]) {
      await emit(tx, {
        orgId: streamOf(personId),
        resourceKind: 'person',
        resourceId: encodeId('person', personId),
        kind: 'updated',
      });
    }
    return mergePersons(tx, {
      survivor,
      absorbed,
      actorPersonId: me.personId,
      method: 'operator',
      reason: parsed.data.reason,
    });
  });

  if (!ok) return { error: NO_SUCH };
  revalidatePath(operatorPath('matches'));
  revalidatePath(PARTIES);
  return { notice: 'Merged. The reason is in the audit.' };
}

/** Split: a merge taken back, from the record the merge itself wrote. */
export async function split(_prev: FormState, form: FormData): Promise<FormState> {
  const me = await operator();
  const blocked = stale(me);
  if (blocked) return blocked;

  const parsed = z.object({ merge: z.string(), reason }).safeParse({
    merge: field(form, 'merge'),
    reason: field(form, 'reason'),
  });
  if (!parsed.success) return { error: 'A reason of at least three characters.' };
  const auditId = tryDecodeId('audit', parsed.data.merge);
  if (auditId === null) return { error: NO_SUCH };
  const confirmed = field(form, 'confirmed') === 'yes';

  const outcome = await database().transaction((tx) =>
    splitMerge(tx, {
      auditId,
      actorPersonId: me.personId,
      reason: parsed.data.reason,
      confirmed,
    }),
  );

  if (outcome.ok) {
    revalidatePath(operatorPath('matches'));
    revalidatePath(PARTIES);
    return { notice: 'Split. The two people are two people again.' };
  }
  if (outcome.reason === 'factors') {
    return {
      error: `The survivor has gained ${outcome.at_risk} factor${
        outcome.at_risk === 1 ? '' : 's'
      } on an identity this would take back. Confirm to split anyway.`,
      confirm: 'yes',
    };
  }
  return { error: NO_SUCH };
}

/* ----------------------------------------------------------- sessions */

export async function revokeSession(_prev: FormState, form: FormData): Promise<FormState> {
  const me = await operator();
  const target = tryDecodeId('session', field(form, 'session'));
  if (target === null) return { error: NO_SUCH };

  await database().transaction(async (tx) => {
    const rows = await tx
      .update(schema.session)
      .set({ revokedAt: sql`now()` })
      .where(and(eq(schema.session.id, target), isNull(schema.session.revokedAt)))
      .returning({ personId: schema.session.personId });
    if (rows.length === 0) return;
    await recordAudit(tx, {
      actorPersonId: me.personId,
      command: 'revoke-session',
      targetKind: 'session',
      targetId: target,
      payload: { person: rows[0]!.personId, by: 'operator' },
    });
    await emit(tx, {
      orgId: streamOf(rows[0]!.personId),
      resourceKind: 'person',
      resourceId: encodeId('person', rows[0]!.personId),
      kind: 'updated',
    });
  });
  revalidatePath(operatorPath('sessions'));
  return { notice: 'Session ended.' };
}

export async function revokeAllSessions(_prev: FormState, form: FormData): Promise<FormState> {
  const me = await operator();
  const target = tryDecodeId('person', field(form, 'person'));
  if (target === null) return { error: NO_SUCH };

  const ended = await database().transaction(async (tx) => {
    const rows = await tx
      .update(schema.session)
      .set({ revokedAt: sql`now()` })
      .where(and(eq(schema.session.personId, target), isNull(schema.session.revokedAt)))
      .returning({ id: schema.session.id });
    await recordAudit(tx, {
      actorPersonId: me.personId,
      command: 'revoke-sessions',
      targetKind: 'person',
      targetId: target,
      payload: { sessions: rows.length },
    });
    await emit(tx, {
      orgId: streamOf(target),
      resourceKind: 'person',
      resourceId: encodeId('person', target),
      kind: 'updated',
    });
    return rows.length;
  });
  revalidatePath(operatorPath('sessions'));
  revalidatePath(PARTIES);
  return { notice: `${ended} session${ended === 1 ? '' : 's'} ended.` };
}

/* --------------------------------------------------- disable / enable */

/** Disable. A party keeps every row it has and loses every door: sessions
 *  end, outstanding links stop working, and sign-in answers with the same
 *  sentence a wrong password gets. An organization's members keep their own
 *  accounts — what goes is the organization's surface. */
export async function disableParty(_prev: FormState, form: FormData): Promise<FormState> {
  const me = await operator();
  const blocked = stale(me);
  if (blocked) return blocked;
  const parsed = z.object({ reason }).safeParse({ reason: field(form, 'reason') });
  if (!parsed.success) return { error: 'A reason of at least three characters.' };
  const target =
    tryDecodeId('person', field(form, 'party')) ?? tryDecodeId('organization', field(form, 'party'));
  if (target === null || target === me.personId) return { error: NO_SUCH };

  const outcome = await database().transaction(async (tx) => {
    const cascade = await disableCascade(tx, target);
    if (cascade === null) return null;
    await recordAudit(tx, {
      actorPersonId: me.personId,
      command: 'disable-party',
      targetKind: cascade.kind === 'organization' ? 'organization' : 'person',
      targetId: target,
      payload: {
        reason: parsed.data.reason,
        sessions_revoked: cascade.sessions,
        /* The ids, so Enable can put back exactly what this took away. */
        links_revoked: cascade.links,
      },
    });
    await emit(tx, {
      orgId: streamOf(target),
      resourceKind: 'person',
      resourceId: encodeId('person', target),
      kind: 'updated',
    });
    return { sessions: cascade.sessions, links: cascade.links.length };
  });

  if (outcome === null) return { error: NO_SUCH };
  revalidatePath(PARTIES);
  revalidatePath(operatorPath('sessions'));
  return {
    notice: `Disabled. ${outcome.sessions} session${
      outcome.sessions === 1 ? '' : 's'
    } ended and ${outcome.links} link${outcome.links === 1 ? '' : 's'} stopped working.`,
  };
}

/** Enable. The doors come back, and so do the links the matching disable
 *  took: the audit row it wrote names them, which is the only reason this can
 *  be exact rather than approximate. */
export async function enableParty(_prev: FormState, form: FormData): Promise<FormState> {
  const me = await operator();
  const target =
    tryDecodeId('person', field(form, 'party')) ?? tryDecodeId('organization', field(form, 'party'));
  if (target === null) return { error: NO_SUCH };

  const ok = await database().transaction(async (tx) => {
    const restored = await enableRestore(tx, target);
    if (restored === null) return false;
    await recordAudit(tx, {
      actorPersonId: me.personId,
      command: 'enable-party',
      targetKind: 'party',
      targetId: target,
      payload: { links_restored: restored },
    });
    await emit(tx, {
      orgId: streamOf(target),
      resourceKind: 'person',
      resourceId: encodeId('person', target),
      kind: 'updated',
    });
    return true;
  });

  if (!ok) return { error: NO_SUCH };
  revalidatePath(PARTIES);
  return { notice: 'Enabled.' };
}

/* ---------------------------------------------------------- operators */

export async function grantOperator(_prev: FormState, form: FormData): Promise<FormState> {
  const me = await operator();
  const blocked = stale(me);
  if (blocked) return blocked;
  const parsed = z.object({ reason }).safeParse({ reason: field(form, 'reason') });
  if (!parsed.success) return { error: 'A reason of at least three characters.' };
  const target = tryDecodeId('person', field(form, 'person'));
  if (target === null) return { error: NO_SUCH };

  const ok = await database().transaction(async (tx) => {
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
    if (live.length === 0) return false;
    await writeOperatorEdge(tx, target);
    await recordAudit(tx, {
      actorPersonId: me.personId,
      command: 'grant-operator',
      targetKind: 'person',
      targetId: target,
      payload: { reason: parsed.data.reason },
    });
    await emit(tx, {
      orgId: streamOf(target),
      resourceKind: 'person',
      resourceId: encodeId('person', target),
      kind: 'updated',
    });
    return true;
  });

  if (!ok) return { error: NO_SUCH };
  revalidatePath(operatorPath('operators'));
  return { notice: 'Granted.' };
}

/** RevokeOperator. A deployment with no operator is a deployment nobody can
 *  fix, so the last one may not be taken away — not even by themselves. */
export async function revokeOperator(_prev: FormState, form: FormData): Promise<FormState> {
  const me = await operator();
  const blocked = stale(me);
  if (blocked) return blocked;
  const parsed = z.object({ reason }).safeParse({ reason: field(form, 'reason') });
  if (!parsed.success) return { error: 'A reason of at least three characters.' };
  const target = tryDecodeId('person', field(form, 'person'));
  if (target === null) return { error: NO_SUCH };

  const outcome = await database().transaction(async (tx) => {
    const refusal = refusesRevoke(await operatorPersonIds(tx), target);
    if (refusal !== null) return refusal;
    const rows = await tx
      .delete(schema.relation)
      .where(
        and(
          eq(schema.relation.subjectKind, 'person'),
          eq(schema.relation.subjectId, target),
          eq(schema.relation.verb, 'operator'),
          eq(schema.relation.resourceKind, OPERATOR_RESOURCE.kind),
          eq(schema.relation.resourceId, OPERATOR_RESOURCE.id),
        ),
      )
      .returning({ id: schema.relation.id });
    if (rows.length === 0) return 'no' as const;
    /* A revoked operator keeps their account and loses the tier; any session
     * they are impersonating through goes with it. */
    await tx
      .update(schema.session)
      .set({ revokedAt: sql`now()` })
      .where(
        and(
          eq(schema.session.actingOperatorId, target),
          isNull(schema.session.revokedAt),
          ne(schema.session.personId, target),
        ),
      );
    await recordAudit(tx, {
      actorPersonId: me.personId,
      command: 'revoke-operator',
      targetKind: 'person',
      targetId: target,
      payload: { reason: parsed.data.reason },
    });
    await emit(tx, {
      orgId: streamOf(target),
      resourceKind: 'person',
      resourceId: encodeId('person', target),
      kind: 'updated',
    });
    return 'gone' as const;
  });

  if (outcome === 'last') {
    return { error: 'That is the last operator. Grant another one first.' };
  }
  if (outcome === 'no') return { error: NO_SUCH };
  revalidatePath(operatorPath('operators'));
  return { notice: 'Revoked.' };
}

/** Whether the person asking is an operator, for a page that has to decide
 *  whether to draw a control at all. */
export async function amOperator(): Promise<boolean> {
  return (await currentPrincipal())?.isOperator ?? false;
}
