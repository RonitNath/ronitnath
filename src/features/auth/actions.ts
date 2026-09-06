'use server';

/* The commands. Each is one transaction with its audit row inside it, typed
 * input through zod, and one uniform answer for every way it can be refused.
 *
 * Two rules run through all of them. Registration and password reset answer
 * the same way whether or not the address is known, so neither form is a
 * lookup service for who has an account here; and an address on
 * `OIDC_ALLOWLIST` has no password path at all — Ronit's person is reached
 * through ZITADEL or not at all — which is also refused without saying so. */

import { and, eq, isNull, sql } from 'drizzle-orm';
import { revalidatePath } from 'next/cache';
import { redirect } from 'next/navigation';
import { z } from 'zod';

import { database, schema } from '@/db/client';
import { tryDecodeId } from '@/lib/ids';
import { deliver, passwordResetMail, verificationMail } from '@/lib/mail';
import { proposeMatchesFor } from '@/features/people/matches';
import { recordAudit } from './audit';
import { isAllowlisted, looksLikeEmail, normalizeEmail } from './email-address';
import { AUTH_FAILED, CHECK_INBOX, RESET_SENT, type FormState } from './form-state';
import { RESET_TTL_HOURS, VERIFY_TTL_HOURS, issueLink, linkUrl, spendLink } from './links';
import { createPerson, findIdentity } from './provision';
import { hashPassword, spendVerificationTime, verifyPassword } from './secrets';
import {
  clearSessionCookie,
  createSession,
  currentPrincipal,
  requestFingerprint,
  setSessionCookie,
} from './session';
import { RESET_LIMIT, SIGN_IN_LIMIT, VERIFY_LIMIT, clearLimit, overLimit } from './throttle';

/* Sign-in has three answers, not two: a session, a refusal, and an address
 * whose password is right and whose confirmation never landed. */
type SignInOutcome = { token: string } | { unconfirmed: true };
const UNCONFIRMED: SignInOutcome = { unconfirmed: true };

const PASSWORD_MIN = 10;
const PASSWORD_MAX = 200;

const email = z.string().trim().max(254).transform(normalizeEmail).refine(looksLikeEmail);
const password = z.string().min(PASSWORD_MIN).max(PASSWORD_MAX);

const registerInput = z.object({
  displayName: z.string().trim().min(1).max(120),
  email,
  password,
});
const signInInput = z.object({ email, password: z.string().max(PASSWORD_MAX), next: z.string() });
const resetRequestInput = z.object({ email });
const resetInput = z.object({ token: z.string(), password });

function field(form: FormData, name: string): string {
  const value = form.get(name);
  return typeof value === 'string' ? value : '';
}

/** Where a `next=` may send someone: this site, by path, and never `/auth`. */
function safeNext(raw: string): string {
  return /^\/(?!\/|auth(\/|$))[A-Za-z0-9\-._~/]*$/.test(raw) ? raw : '/app';
}

export async function register(_prev: FormState, form: FormData): Promise<FormState> {
  const parsed = registerInput.safeParse({
    displayName: field(form, 'displayName'),
    email: field(form, 'email'),
    password: field(form, 'password'),
  });
  if (!parsed.success) {
    return {
      form: 'register',
      error: `A name, an address, and a password of at least ${PASSWORD_MIN} characters.`,
    };
  }
  const { displayName, email: address, password: secret } = parsed.data;

  /* Hashed before the transaction: argon2 is tens of milliseconds and no
   * transaction is held open across it. */
  const phc = await hashPassword(secret);

  const sent = await database().transaction(async (tx) => {
    /* Allowlisted addresses sign in through ZITADEL and never hold a
     * password. Refused here, silently, exactly like a taken address. */
    if (isAllowlisted(address)) return null;
    if (await findIdentity(tx, 'local', address)) return null;

    const personId = await createPerson(tx, { displayName });
    const identities = await tx
      .insert(schema.identity)
      .values({ personId, source: 'local', subject: address })
      .returning({ id: schema.identity.id });
    const identityId = identities[0]!.id;
    await tx.insert(schema.factor).values([
      { identityId, kind: 'email', meta: { address } },
      { identityId, kind: 'password', secret: phc },
    ]);
    const token = await issueLink(tx, {
      kind: 'verify_email',
      targetKind: 'identity',
      targetId: identityId,
      ttlHours: VERIFY_TTL_HOURS,
    });
    await recordAudit(tx, {
      actorPersonId: personId,
      command: 'register',
      targetKind: 'identity',
      targetId: identityId,
      payload: { source: 'local' },
    });
    return { token, address };
  });

  /* Mail goes out after the commit — a letter cannot be rolled back — and a
   * letter that fails to go out must not take the account with it. The
   * visitor sees the same page either way and can ask for another one from
   * the door; the failure is a line in the log, where it can be acted on. */
  if (sent) {
    await deliver(verificationMail(sent.address, linkUrl('verify_email', sent.token)), {
      command: 'register',
    });
  }
  return { form: 'register', notice: CHECK_INBOX };
}

export async function verifyEmail(_prev: FormState, form: FormData): Promise<FormState> {
  const token = field(form, 'token');
  const ok = await database().transaction(async (tx) => {
    const spent = await spendLink(tx, 'verify_email', token);
    if (!spent || spent.targetKind !== 'identity') return false;
    const rows = await tx
      .update(schema.identity)
      .set({ verifiedAt: sql`now()` })
      .where(and(eq(schema.identity.id, spent.targetId), isNull(schema.identity.verifiedAt)))
      .returning({ personId: schema.identity.personId, subject: schema.identity.subject });
    const personId = rows[0]?.personId;
    /* A newly confirmed address may be the address somebody else wrote on a
     * contact card. Proposing is not merging: what this writes is a question
     * for the person who just proved the address (src/features/people). */
    if (rows[0]) {
      await proposeMatchesFor(tx, {
        identityId: spent.targetId,
        personId: rows[0].personId,
        subject: rows[0].subject,
      });
    }
    await tx
      .update(schema.factor)
      .set({ usedAt: sql`now()` })
      .where(and(eq(schema.factor.identityId, spent.targetId), eq(schema.factor.kind, 'email')));
    await recordAudit(tx, {
      actorPersonId: personId ?? null,
      command: 'verify-email',
      targetKind: 'identity',
      targetId: spent.targetId,
    });
    return true;
  });
  return ok
    ? { notice: 'Address confirmed. You can sign in.' }
    : { error: 'That link has been used or has expired.' };
}

export async function signIn(_prev: FormState, form: FormData): Promise<FormState> {
  const parsed = signInInput.safeParse({
    email: field(form, 'email'),
    password: field(form, 'password'),
    next: field(form, 'next'),
  });
  if (!parsed.success) return { form: 'sign-in', error: AUTH_FAILED };
  const { email: address, password: secret, next } = parsed.data;
  const where = await requestFingerprint();

  const outcome = await database().transaction<SignInOutcome | null>(async (tx) => {
    if (await overLimit(tx, SIGN_IN_LIMIT, ['email', address])) return null;
    if (where.ip && (await overLimit(tx, SIGN_IN_LIMIT, ['ip', where.ip]))) return null;

    const rows = await tx
      .select({
        identityId: schema.identity.id,
        personId: schema.identity.personId,
        verifiedAt: schema.identity.verifiedAt,
        phc: schema.factor.secret,
        disabledAt: schema.party.disabledAt,
      })
      .from(schema.identity)
      .innerJoin(schema.party, eq(schema.party.id, schema.identity.personId))
      .leftJoin(
        schema.factor,
        and(eq(schema.factor.identityId, schema.identity.id), eq(schema.factor.kind, 'password')),
      )
      .where(and(eq(schema.identity.source, 'local'), eq(schema.identity.subject, address)))
      .limit(1);

    /* Every refusal costs one argon2 verification, so an address nobody
     * registered does not answer faster than a wrong password. */
    const row = rows[0];
    if (!row || row.disabledAt !== null || !row.phc) {
      await spendVerificationTime(secret);
      return null;
    }
    if (!(await verifyPassword(secret, row.phc))) return null;

    /* The password is right and the address was never confirmed. Saying so
     * here reveals nothing the person asking does not already hold. */
    if (row.verifiedAt === null) {
      await clearLimit(tx, SIGN_IN_LIMIT, ['email', address]);
      if (where.ip) await clearLimit(tx, SIGN_IN_LIMIT, ['ip', where.ip]);
      await recordAudit(tx, {
        actorPersonId: row.personId,
        command: 'sign-in-unconfirmed',
        targetKind: 'identity',
        targetId: row.identityId,
      });
      return UNCONFIRMED;
    }

    const { token } = await createSession(tx, {
      personId: row.personId,
      source: 'local',
      userAgent: where.userAgent,
      ip: where.ip,
    });
    await clearLimit(tx, SIGN_IN_LIMIT, ['email', address]);
    if (where.ip) await clearLimit(tx, SIGN_IN_LIMIT, ['ip', where.ip]);
    await recordAudit(tx, {
      actorPersonId: row.personId,
      command: 'sign-in',
      targetKind: 'identity',
      targetId: row.identityId,
      payload: { source: 'local' },
    });
    return { token };
  });

  if (outcome === null) return { form: 'sign-in', error: AUTH_FAILED };
  if ('unconfirmed' in outcome) return { form: 'sign-in', unverified: true, email: address };
  await setSessionCookie(outcome.token);
  redirect(safeNext(next));
}

export async function signOut(): Promise<void> {
  const principal = await currentPrincipal();
  let endSession: string | null = null;
  if (principal) {
    const idToken = await database().transaction(async (tx) => {
      const rows = await tx
        .update(schema.session)
        .set({ revokedAt: sql`now()` })
        .where(and(eq(schema.session.id, principal.sessionId), isNull(schema.session.revokedAt)))
        .returning({ oidcIdToken: schema.session.oidcIdToken });
      await recordAudit(tx, {
        actorPersonId: principal.personId,
        command: 'sign-out',
        targetKind: 'session',
        targetId: principal.sessionId,
      });
      return rows[0]?.oidcIdToken ?? null;
    });
    /* A session that came through ZITADEL ends there too, or the next sign-in
     * is silent and this button did nothing a reader would recognise. */
    if (principal.source === 'oidc') {
      const { endSessionUrl } = await import('./oidc');
      endSession = await endSessionUrl(idToken);
    }
  }
  await clearSessionCookie();
  redirect(endSession ?? '/');
}

export async function revokeSession(_prev: FormState, form: FormData): Promise<FormState> {
  const principal = await currentPrincipal();
  if (!principal) redirect('/auth');
  const target = tryDecodeId('session', field(form, 'session'));
  if (target === null || target === principal.sessionId) {
    return { error: 'That session is not one you can end here.' };
  }
  await database().transaction(async (tx) => {
    const rows = await tx
      .update(schema.session)
      .set({ revokedAt: sql`now()` })
      .where(
        and(
          eq(schema.session.id, target),
          eq(schema.session.personId, principal.personId),
          isNull(schema.session.revokedAt),
        ),
      )
      .returning({ id: schema.session.id });
    if (rows.length === 0) return;
    await recordAudit(tx, {
      actorPersonId: principal.personId,
      command: 'revoke-session',
      targetKind: 'session',
      targetId: target,
    });
  });
  revalidatePath('/app/sessions');
  return { notice: 'Session ended.' };
}

export async function requestPasswordReset(
  _prev: FormState,
  form: FormData,
): Promise<FormState> {
  const parsed = resetRequestInput.safeParse({ email: field(form, 'email') });
  if (!parsed.success) return { form: 'reset', notice: RESET_SENT };
  const address = parsed.data.email;
  const where = await requestFingerprint();

  const sent = await database().transaction(async (tx) => {
    if (await overLimit(tx, RESET_LIMIT, ['email', address])) return null;
    if (where.ip && (await overLimit(tx, RESET_LIMIT, ['ip', where.ip]))) return null;
    if (isAllowlisted(address)) return null;
    const identity = await findIdentity(tx, 'local', address);
    if (!identity) return null;
    const token = await issueLink(tx, {
      kind: 'reset_password',
      targetKind: 'identity',
      targetId: identity.id,
      ttlHours: RESET_TTL_HOURS,
      createdBy: identity.personId,
    });
    await recordAudit(tx, {
      actorPersonId: identity.personId,
      command: 'request-password-reset',
      targetKind: 'identity',
      targetId: identity.id,
    });
    return token;
  });

  if (sent) {
    await deliver(passwordResetMail(address, linkUrl('reset_password', sent)), {
      command: 'request-password-reset',
    });
  }
  return { form: 'reset', notice: RESET_SENT };
}

export async function resetPassword(_prev: FormState, form: FormData): Promise<FormState> {
  const parsed = resetInput.safeParse({
    token: field(form, 'token'),
    password: field(form, 'password'),
  });
  if (!parsed.success) {
    return { error: `A password of at least ${PASSWORD_MIN} characters.` };
  }
  const phc = await hashPassword(parsed.data.password);

  const ok = await database().transaction(async (tx) => {
    const spent = await spendLink(tx, 'reset_password', parsed.data.token);
    if (!spent || spent.targetKind !== 'identity') return false;
    const identities = await tx
      .select({ personId: schema.identity.personId })
      .from(schema.identity)
      .where(eq(schema.identity.id, spent.targetId))
      .limit(1);
    const personId = identities[0]?.personId;
    if (personId === undefined) return false;

    await tx
      .insert(schema.factor)
      .values({ identityId: spent.targetId, kind: 'password', secret: phc })
      .onConflictDoUpdate({
        target: [schema.factor.identityId, schema.factor.kind],
        set: { secret: phc },
      });
    /* A reset is what someone locked out does, so every open session goes:
     * whoever held one may be the reason for the reset. */
    await tx
      .update(schema.session)
      .set({ revokedAt: sql`now()` })
      .where(and(eq(schema.session.personId, personId), isNull(schema.session.revokedAt)));
    /* The address is proven by the fact that the letter arrived. */
    await tx
      .update(schema.identity)
      .set({ verifiedAt: sql`now()` })
      .where(and(eq(schema.identity.id, spent.targetId), isNull(schema.identity.verifiedAt)));
    await recordAudit(tx, {
      actorPersonId: personId,
      command: 'reset-password',
      targetKind: 'identity',
      targetId: spent.targetId,
    });
    return true;
  });

  if (!ok) return { error: 'That link has been used or has expired.' };
  await clearSessionCookie();
  return { notice: 'Password set. Every other session has been signed out.' };
}

/** RequestVerification. The letter register sent may never have arrived — the
 *  transport can fail after the account exists — so the door can send another
 *  one. Throttled like a reset, and answered the same way whether or not the
 *  address is one that could be confirmed. */
export async function requestVerification(_prev: FormState, form: FormData): Promise<FormState> {
  const parsed = resetRequestInput.safeParse({ email: field(form, 'email') });
  if (!parsed.success) return { form: 'sign-in', unverified: true, notice: CHECK_INBOX };
  const address = parsed.data.email;
  const where = await requestFingerprint();

  const sent = await database().transaction(async (tx) => {
    if (await overLimit(tx, VERIFY_LIMIT, ['email', address])) return null;
    if (where.ip && (await overLimit(tx, VERIFY_LIMIT, ['ip', where.ip]))) return null;
    if (isAllowlisted(address)) return null;
    const identity = await findIdentity(tx, 'local', address);
    if (!identity || identity.verifiedAt !== null) return null;
    const token = await issueLink(tx, {
      kind: 'verify_email',
      targetKind: 'identity',
      targetId: identity.id,
      ttlHours: VERIFY_TTL_HOURS,
      createdBy: identity.personId,
    });
    await recordAudit(tx, {
      actorPersonId: identity.personId,
      command: 'request-verification',
      targetKind: 'identity',
      targetId: identity.id,
    });
    return token;
  });

  if (sent) {
    await deliver(verificationMail(address, linkUrl('verify_email', sent)), {
      command: 'request-verification',
    });
  }
  return { form: 'sign-in', unverified: true, email: address, notice: CHECK_INBOX };
}
