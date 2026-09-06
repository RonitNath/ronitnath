/* Sessions. An opaque 256-bit token in a cookie, its SHA-256 in the database:
 * the server can recognise a session it minted and cannot reconstruct one it
 * did not.
 *
 * The window slides. Every resolution that finds a session older than five
 * minutes at the wrist pushes `last_seen_at` and `expires_at` forward, so a
 * person who uses the site keeps it and a person who stops loses it thirty
 * days later. Five minutes is the floor because the alternative is one write
 * per page view for a fact nothing reads at that resolution.
 *
 * Resolution is cached per request (`React.cache`): a page, its layout and
 * every action it renders ask the same question, and they must not get three
 * different answers or pay for three round trips. */

import { and, eq, gt, isNull, sql } from 'drizzle-orm';
import { cookies, headers } from 'next/headers';
import { cache } from 'react';

import { database, schema } from '@/db/client';
import { isProduction, sessionCookieName, sessionTtlDays } from '@/lib/env';
import type { Transaction } from './db';
import { hashToken, mintToken, tokenLooksWellFormed } from './secrets';

export const SLIDE_AFTER_SECONDS = 300;
export const OPERATOR_RESOURCE = { kind: 'platform', id: 0 } as const;

export interface Principal {
  personId: number;
  displayName: string;
  sessionId: number;
  source: 'local' | 'oidc';
  isOperator: boolean;
}

function ttlMs(): number {
  return sessionTtlDays() * 24 * 60 * 60 * 1000;
}

export function expiryFromNow(now = new Date()): Date {
  return new Date(now.getTime() + ttlMs());
}

/** Mint a session row inside a command's transaction and return its token —
 *  the only moment the plaintext exists on this side. */
export async function createSession(
  tx: Transaction,
  input: {
    personId: number;
    source?: 'local' | 'oidc';
    oidcIdToken?: string | null;
    userAgent?: string | null;
    ip?: string | null;
  },
): Promise<{ token: string; sessionId: number }> {
  const token = mintToken();
  const rows = await tx
    .insert(schema.session)
    .values({
      personId: input.personId,
      tokenHash: hashToken(token),
      source: input.source ?? 'local',
      oidcIdToken: input.oidcIdToken ?? null,
      userAgent: input.userAgent ?? null,
      ip: input.ip ?? null,
      expiresAt: expiryFromNow(),
    })
    .returning({ id: schema.session.id });
  return { token, sessionId: rows[0]!.id };
}

/** Whether a session this old at the wrist earns a write. */
export function shouldSlide(lastSeenAt: Date, now = new Date()): boolean {
  return now.getTime() - lastSeenAt.getTime() >= SLIDE_AFTER_SECONDS * 1000;
}

interface CookieOptions {
  httpOnly: true;
  secure: boolean;
  sameSite: 'lax';
  path: '/';
  maxAge?: number;
}

export function sessionCookieOptions(): CookieOptions {
  return {
    httpOnly: true,
    secure: isProduction(),
    sameSite: 'lax',
    path: '/',
    maxAge: Math.floor(ttlMs() / 1000),
  };
}

/** Only callable from a Server Action or a Route Handler. */
export async function setSessionCookie(token: string): Promise<void> {
  const jar = await cookies();
  jar.set(sessionCookieName(), token, sessionCookieOptions());
}

export async function clearSessionCookie(): Promise<void> {
  const jar = await cookies();
  jar.set(sessionCookieName(), '', { ...sessionCookieOptions(), maxAge: 0 });
}

export async function readSessionCookie(): Promise<string | null> {
  const jar = await cookies();
  const value = jar.get(sessionCookieName())?.value;
  return value && tokenLooksWellFormed(value) ? value : null;
}

async function resolve(): Promise<Principal | null> {
  const token = await readSessionCookie();
  if (token === null) return null;

  const db = database();
  const rows = await db
    .select({
      sessionId: schema.session.id,
      personId: schema.session.personId,
      lastSeenAt: schema.session.lastSeenAt,
      source: schema.session.source,
      displayName: schema.person.displayName,
      disabledAt: schema.party.disabledAt,
    })
    .from(schema.session)
    .innerJoin(schema.person, eq(schema.person.id, schema.session.personId))
    .innerJoin(schema.party, eq(schema.party.id, schema.person.id))
    .where(
      and(
        eq(schema.session.tokenHash, hashToken(token)),
        isNull(schema.session.revokedAt),
        gt(schema.session.expiresAt, sql`now()`),
      ),
    )
    .limit(1);

  const row = rows[0];
  /* A disabled party keeps its rows and loses its door. */
  if (!row || row.disabledAt !== null) return null;

  if (shouldSlide(row.lastSeenAt)) {
    await db
      .update(schema.session)
      .set({ lastSeenAt: sql`now()`, expiresAt: expiryFromNow() })
      .where(eq(schema.session.id, row.sessionId));
  }

  const operator = await db
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
    .limit(1);

  return {
    personId: row.personId,
    displayName: row.displayName,
    sessionId: row.sessionId,
    source: row.source,
    isOperator: operator.length > 0,
  };
}

export const currentPrincipal = cache(resolve);

/** What a session row records about where the request came from. */
export async function requestFingerprint(): Promise<{ userAgent: string | null; ip: string | null }> {
  const head = await headers();
  const forwarded = head.get('x-forwarded-for');
  return {
    userAgent: head.get('user-agent'),
    ip: forwarded ? (forwarded.split(',')[0]?.trim() ?? null) : null,
  };
}
