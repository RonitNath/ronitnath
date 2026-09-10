/* Who is here. One question, asked once per request.
 *
 * Better-auth owns the cookie, the token and the row behind it; what this
 * module owns is the translation from a `user` into somebody this site can
 * name. Every page, every command and every audit row talks about a *person*
 * — guests, events, rsvps and photos all reference `person`, never
 * `auth.user` — so a session that cannot be turned into a live person is not
 * a session anything here can use, and resolves to null.
 *
 * Four things make a session unusable and all four are checked in one query:
 * no person linked to the user, a person folded into another one by a merge,
 * a disabled party, and a user better-auth itself has stopped honouring
 * (which it answers before we get here).
 *
 * The answer is cached per request (`React.cache`): a layout, its page and
 * every action rendered inside it ask the same question, and they must not
 * get three different answers or pay for three round trips.
 *
 * `sessionId` is a string now. Better-auth's ids are text, so the audit
 * table's integer `target_id` cannot hold one; where a command used to record
 * a session id as its target it records it in the payload instead. */

import { and, eq, isNull } from 'drizzle-orm';
import { headers } from 'next/headers';
import { cache } from 'react';

import { authSchema, database, schema } from '@/db/client';
import { auth } from './auth';

/** The resource every operator grant is written against: `platform:0`. The
 *  relation table is this site's policy module and stays that way — the
 *  organization plugin's `member.role` is not what an operator is. */
export const OPERATOR_RESOURCE = { kind: 'platform', id: 0 } as const;

export interface Principal {
  personId: number;
  displayName: string;
  /* Better-auth's session id: text, opaque, and never shown to a reader. */
  sessionId: string;
  source: 'local' | 'oidc';
  isOperator: boolean;
  /* Set only while an operator is signed in as somebody else. The principal
   * *is* the target person — nothing in `allows` learns a new case — and this
   * is who is answerable for it: the bar on every page and the second name on
   * every audit row both read it (src/features/platform). */
  actingOperatorId: number | null;
  /* When a password or a fresh round trip was last presented on this session.
   * The commands that destroy or impersonate ask for one inside the window
   * (src/features/platform/reauth.ts). */
  reauthenticatedAt: Date | null;
}

/* The two columns this app added to `auth.session`. They are declared to
 * better-auth as additional fields, so they come back on the session object;
 * naming their shape here keeps the cast to one place instead of one per
 * reader. */
interface FleetSessionFields {
  actingOperatorId?: string | null;
  reauthenticatedAt?: Date | string | null;
}

function asDate(value: Date | string | null | undefined): Date | null {
  if (!value) return null;
  return value instanceof Date ? value : new Date(value);
}

/** A person id that was written into a text column. Anything that is not a
 *  positive integer is treated as absent rather than as an error: a malformed
 *  value must not be able to make every page throw. */
function asPersonId(value: string | null | undefined): number | null {
  if (!value) return null;
  const parsed = Number(value);
  return Number.isInteger(parsed) && parsed > 0 ? parsed : null;
}

async function resolve(): Promise<Principal | null> {
  const found = await auth().api.getSession({ headers: await headers() });
  if (!found) return null;
  const session = found.session as typeof found.session & FleetSessionFields;

  const db = database();
  const rows = await db
    .select({
      personId: schema.person.id,
      displayName: schema.person.displayName,
      disabledAt: schema.party.disabledAt,
    })
    .from(schema.person)
    .innerJoin(schema.party, eq(schema.party.id, schema.person.id))
    .where(and(eq(schema.person.userId, found.user.id), isNull(schema.person.mergedInto)))
    .limit(1);

  const row = rows[0];
  /* A disabled party keeps its rows and loses its door. */
  if (!row || row.disabledAt !== null) return null;

  /* Two small reads rather than one join: which door this user holds, and
   * whether they operate the platform. Both are single-row index lookups and
   * both are wanted by almost every page. */
  const [federated, operator] = await Promise.all([
    db
      .select({ id: authSchema.account.id })
      .from(authSchema.account)
      .where(
        and(
          eq(authSchema.account.userId, found.user.id),
          eq(authSchema.account.providerId, 'zitadel'),
        ),
      )
      .limit(1),
    db
      .select({ id: schema.relation.id })
      .from(schema.relation)
      .where(
        and(
          eq(schema.relation.subjectKind, 'person'),
          eq(schema.relation.subjectId, row.personId),
          eq(schema.relation.verb, 'operator'),
          eq(schema.relation.resourceKind, OPERATOR_RESOURCE.kind),
          eq(schema.relation.resourceId, OPERATOR_RESOURCE.id),
        ),
      )
      .limit(1),
  ]);

  return {
    personId: row.personId,
    displayName: row.displayName,
    sessionId: session.id,
    /* Which door this account holds, not which one this session came through:
     * an account linked to ZITADEL has no password path at all here, so the
     * two cannot disagree in a way a reader would notice. */
    source: federated.length > 0 ? 'oidc' : 'local',
    isOperator: operator.length > 0,
    actingOperatorId: asPersonId(session.actingOperatorId),
    reauthenticatedAt: asDate(session.reauthenticatedAt),
  };
}

export const currentPrincipal = cache(resolve);

/** What a command records about where the request came from. Better-auth
 *  keeps its own copy on the session row; this is for the audit payloads and
 *  the throttles that are ours. */
export async function requestFingerprint(): Promise<{
  userAgent: string | null;
  ip: string | null;
}> {
  const head = await headers();
  const forwarded = head.get('x-forwarded-for');
  return {
    userAgent: head.get('user-agent'),
    ip: forwarded ? (forwarded.split(',')[0]?.trim() ?? null) : null,
  };
}
