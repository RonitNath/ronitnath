/* The gates. Every page and every command names the one it needs by calling
 * one of these, so there is no per-route guard to forget.
 *
 * Two rules run through all of them. A refusal that would teach a stranger
 * something is a 404, not a 403: an operator surface, an organization somebody
 * is not in, and a page that does not exist must all answer the same way. And
 * a visitor is sent to the door carrying where they were going, so signing in
 * returns them there rather than to a lobby.
 *
 * The names and signatures are the fleet contract (payload-removal/contracts.md
 * §gates); `src/lib/tiers.ts` is this site's older vocabulary over the same
 * questions and delegates here where the shapes line up. `requireOrg` resolves
 * against *this* site's `organization` table, keyed by handle, and its
 * membership question is the `relation` table through `allows` — adopting the
 * better-auth organization plugin for ronitnath's sharing model is explicitly
 * out of scope, and the plugin's tables stay empty.
 *
 * `withAction` is the command wrapper: one correlation id per command, one
 * audit row naming actor, subject and path, and one uniform decline for every
 * way a command can refuse. It deliberately lets Next's own control-flow
 * errors — `redirect()` and `notFound()` — through: they are how a command
 * finishes, not how it fails. */

import { randomUUID } from 'node:crypto';
import { headers } from 'next/headers';
import { notFound, permanentRedirect, redirect } from 'next/navigation';

import { database, schema } from '@/db/client';
import { recordAudit } from '@/features/auth/audit';
import { currentPrincipal, type Principal } from '@/features/auth/principal';
import { organizationByHandle, type OrganizationRow } from '@/features/organizations/queries';
import { allows, type Role } from '@/lib/authority';
import { tryDecodeId } from '@/lib/ids';
import { and, eq, isNull } from 'drizzle-orm';

/** The door. Every redirect to it carries where the reader was going. */
export const SIGN_IN = '/auth/sign-in';

/** The one thing a refused command ever says. A field-level hint is a lookup
 *  service for whoever is asking. */
export const DECLINED = 'That is not something you can do here.';

export interface VisitorGate {
  principal: null;
}

export interface UserGate {
  principal: Principal;
  personId: number;
}

export interface OrgGate extends UserGate {
  organization: OrganizationRow;
}

export interface SubjectGate extends OrgGate {
  /* Who the path is about, which is not who is asking. */
  subjectPersonId: number;
  subjectName: string;
}

export function signInPath(next?: string): string {
  return next ? `${SIGN_IN}?next=${encodeURIComponent(next)}` : SIGN_IN;
}

/** Anyone at all, signed in or not. It exists so that a public page states
 *  the tier it is rather than saying nothing. */
export async function requireVisitor(): Promise<VisitorGate> {
  return { principal: null };
}

/** Somebody signed in, as a live person on this site. */
export async function requireUser(next?: string): Promise<UserGate> {
  const principal = await currentPrincipal();
  if (principal === null) redirect(signInPath(next));
  return { principal, personId: principal.personId };
}

/** An organization by slug, and the asker's standing in it. `member` unless
 *  the caller asks for more; somebody who does not hold it gets the 404 that
 *  an organization which does not exist gets, because the difference is the
 *  fact a stranger would like to learn. A platform operator passes every
 *  level — that is what the tier is for. */
export async function requireOrg(slug: string, need: Role = 'member'): Promise<OrgGate> {
  const principal = await currentPrincipal();
  if (principal === null) redirect(signInPath(`/o/${slug}`));
  const organization = await organizationByHandle(slug);
  if (organization === null) notFound();
  const ok = await database().transaction((tx) =>
    allows(
      tx,
      { personId: principal.personId, isOperator: principal.isOperator },
      { on: 'organization', id: organization.id, need },
    ),
  );
  if (!ok) notFound();
  return { principal, personId: principal.personId, organization };
}

/** The long form of an org URL: `/o/{slug}/u/{user}/…`, one member looking at
 *  another's view. Only an admin of the organization or a platform operator
 *  may; a person looking at themselves is on the short form and is sent there
 *  permanently, so that the canonical link is the one that gets shared. */
export async function requireSubject(slug: string, userId: string): Promise<SubjectGate> {
  const gate = await requireOrg(slug, 'admin');
  const subjectPersonId = tryDecodeId('person', userId);
  if (subjectPersonId === null) notFound();
  if (subjectPersonId === gate.personId) permanentRedirect(`/o/${slug}`);

  const rows = await database()
    .select({ id: schema.person.id, displayName: schema.person.displayName })
    .from(schema.person)
    .innerJoin(schema.party, eq(schema.party.id, schema.person.id))
    .where(and(eq(schema.person.id, subjectPersonId), isNull(schema.party.disabledAt)))
    .limit(1);
  const subject = rows[0];
  if (!subject) notFound();

  return { ...gate, subjectPersonId: subject.id, subjectName: subject.displayName };
}

/** The platform tier. Not being one is not an error message: it is a 404. */
export async function requireOperator(): Promise<UserGate> {
  const principal = await currentPrincipal();
  if (principal === null || !principal.isOperator) notFound();
  return { principal, personId: principal.personId };
}

export interface ActionScope {
  /* One id per command, carried into the audit row and into any event the
   * command emits, so that a page load, its command and its consequences can
   * be read back as one story. */
  correlationId: string;
  actor: Principal | null;
  /* Who the command is about, when that is not the actor. A command sets it
   * as soon as it knows. */
  subject: number | null;
  path: string;
}

/* Next signals a redirect and a 404 by throwing. Those are how a command
 * finishes — they must reach the framework, not be turned into a decline. */
function isControlFlow(error: unknown): boolean {
  const digest = (error as { digest?: unknown })?.digest;
  return typeof digest === 'string' && digest.startsWith('NEXT_');
}

/** What page the command was fired from. Next carries the RSC path on
 *  `next-url` and the browser's own on `referer`; either is enough to say
 *  where in the site a command was reached from, and neither is trusted for
 *  anything but the record. */
async function requestPath(): Promise<string> {
  const head = await headers();
  const nextUrl = head.get('next-url');
  if (nextUrl) return nextUrl;
  const referer = head.get('referer');
  if (!referer) return '';
  try {
    return new URL(referer).pathname;
  } catch {
    return '';
  }
}

/** The command wrapper. It opens a correlation id, hands the body a scope to
 *  name its subject in, writes one audit row for the command, and turns every
 *  unexpected failure into the same refusal.
 *
 *  The audit row is written after the body commits, on purpose: a command
 *  that writes its own audit row inside its own transaction (most of them do,
 *  through `recordAudit`) has the stronger guarantee, and this one is the
 *  record that the command was *attempted* — which is the thing an operator
 *  reading a decline needs and which a rolled-back transaction would erase. */
export async function withAction<T>(
  name: string,
  fn: (scope: ActionScope) => Promise<T>,
): Promise<T | { error: string }> {
  const scope: ActionScope = {
    correlationId: randomUUID(),
    actor: await currentPrincipal(),
    subject: null,
    path: await requestPath(),
  };

  try {
    const answer = await fn(scope);
    await note(name, scope, 'ok');
    return answer;
  } catch (error) {
    if (isControlFlow(error)) throw error;
    console.error(
      JSON.stringify({
        level: 'error',
        event: 'action.failed',
        command: name,
        correlation_id: scope.correlationId,
        message: error instanceof Error ? error.message : String(error),
      }),
    );
    await note(name, scope, 'declined');
    return { error: DECLINED };
  }
}

async function note(name: string, scope: ActionScope, outcome: 'ok' | 'declined'): Promise<void> {
  try {
    await database().transaction((tx) =>
      recordAudit(tx, {
        actorPersonId: scope.actor?.personId ?? null,
        command: name,
        targetKind: scope.subject === null ? null : 'person',
        targetId: scope.subject,
        payload: {
          correlation_id: scope.correlationId,
          path: scope.path,
          outcome,
        },
      }),
    );
  } catch {
    /* The record of an attempt must never be the reason the attempt fails. */
  }
}
