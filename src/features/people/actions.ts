'use server';

/* The member's commands over people and over their own identities. Each is
 * one transaction with its audit row inside it, zod at the door, and one
 * uniform decline: a person a member may not touch and a person who does not
 * exist are the same answer, because the difference is the fact a stranger
 * would like to learn. */

import { and, eq, inArray, isNull, sql } from 'drizzle-orm';
import { revalidatePath } from 'next/cache';
import { redirect } from 'next/navigation';
import { z } from 'zod';

import { database, schema } from '@/db/client';
import { recordAudit } from '@/features/auth/audit';
import { isAllowlisted, looksLikeEmail, normalizeEmail } from '@/features/auth/email-address';
import { CHECK_INBOX, type FormState } from '@/features/auth/form-state';
import { VERIFY_TTL_HOURS, issueLink, linkUrl } from '@/features/auth/links';
import { createPerson } from '@/features/auth/provision';
import { hashPassword } from '@/features/auth/secrets';
import { currentPrincipal, requestFingerprint } from '@/features/auth/session';
import { RESET_LIMIT, overLimit } from '@/features/auth/throttle';
import { tryDecodeId } from '@/lib/ids';
import { deliver, verificationMail } from '@/lib/mail';
import { CONTACT, allows, livePerson } from './authority';
import { HANDLE_MAX, normalizeHandle } from './handles';
import { claimUrl, mintClaimLink, spendClaimLink } from './invitations';
import { mergePersons } from './merge';
import { settleMatchesFor } from './matches';

/* One sentence for everything a member may not do, whether because the row
 * is not theirs or because it is not there. */
const NO_SUCH = 'That is not something you can do here.';
const PEOPLE = '/app/people';

function field(form: FormData, name: string): string {
  const value = form.get(name);
  return typeof value === 'string' ? value : '';
}

const displayName = z.string().trim().min(1).max(120);
const holdInput = z.object({
  handle: z.string().trim().min(1).max(HANDLE_MAX),
  displayName: z.string().trim().max(120),
});
const claimRegisterInput = z.object({
  token: z.string(),
  displayName,
  email: z.string().trim().max(254).transform(normalizeEmail).refine(looksLikeEmail),
  password: z.string().min(10).max(200),
});
const emailInput = z.object({
  email: z.string().trim().max(254).transform(normalizeEmail).refine(looksLikeEmail),
});

async function actor() {
  const principal = await currentPrincipal();
  if (!principal) redirect('/auth');
  return { personId: principal.personId, isOperator: principal.isOperator };
}

/** HoldPerson. A member writes down somebody who has no account: a person row
 *  marked held, one handle identity, and the `contact` edge that says whose
 *  note this is. No factors, so nothing can sign in as them. */
export async function holdPerson(_prev: FormState, form: FormData): Promise<FormState> {
  const me = await actor();
  const parsed = holdInput.safeParse({
    handle: field(form, 'handle'),
    displayName: field(form, 'displayName'),
  });
  if (!parsed.success) return { error: 'An email, a phone number, or a name.' };
  const handle = normalizeHandle(parsed.data.handle);
  if (handle === null) return { error: 'An email, a phone number, or a name.' };
  const name = parsed.data.displayName || handle.raw;

  const outcome = await database().transaction(async (tx) => {
    /* Holding the same handle twice is holding it once. The check is over
     * this member's own contacts, because two members may each hold the same
     * address and neither of them is wrong. */
    const already = await tx
      .select({ personId: schema.identity.personId })
      .from(schema.identity)
      .innerJoin(
        schema.relation,
        and(
          eq(schema.relation.subjectKind, 'person'),
          eq(schema.relation.subjectId, me.personId),
          eq(schema.relation.verb, CONTACT),
          eq(schema.relation.resourceKind, 'person'),
          eq(schema.relation.resourceId, schema.identity.personId),
        ),
      )
      .innerJoin(schema.person, eq(schema.person.id, schema.identity.personId))
      .where(
        and(
          eq(schema.identity.source, 'handle'),
          eq(schema.identity.subject, handle.subject),
          isNull(schema.person.mergedInto),
        ),
      )
      .limit(1);
    if (already.length > 0) return 'held' as const;

    const heldId = await createPerson(tx, { displayName: name, held: true });
    await tx
      .insert(schema.identity)
      .values({ personId: heldId, source: 'handle', subject: handle.subject });
    await tx.insert(schema.relation).values({
      subjectKind: 'person',
      subjectId: me.personId,
      verb: CONTACT,
      resourceKind: 'person',
      resourceId: heldId,
    });
    await recordAudit(tx, {
      actorPersonId: me.personId,
      command: 'hold-person',
      targetKind: 'person',
      targetId: heldId,
      payload: { handle: handle.kind },
    });
    return 'created' as const;
  });

  revalidatePath(PEOPLE);
  return outcome === 'held'
    ? { notice: `You already hold ${handle.raw}.` }
    : { notice: `${name} is yours to invite.` };
}

/** Invite. Mints the claim link and returns the URL exactly once: the row
 *  keeps the hash, and nothing on this side can show it again. */
export async function invite(_prev: FormState, form: FormData): Promise<FormState> {
  const me = await actor();
  const target = tryDecodeId('person', field(form, 'person'));
  if (target === null) return { error: NO_SUCH };

  const minted = await database().transaction(async (tx) => {
    if (!(await allows(tx, me, { on: 'person', id: target }))) return null;
    const person = await livePerson(tx, target);
    if (!person || !person.held) return null;
    const { token, linkId } = await mintClaimLink(tx, {
      personId: target,
      createdBy: me.personId,
    });
    await recordAudit(tx, {
      actorPersonId: me.personId,
      command: 'invite',
      targetKind: 'link',
      targetId: linkId,
      payload: { person: target },
    });
    return claimUrl(token);
  });

  if (minted === null) return { error: NO_SUCH };
  revalidatePath(PEOPLE);
  return { minted, notice: 'Copy it now — it is not stored and cannot be shown again.' };
}

/** RevokeLink. Named by its public id, never by its token. */
export async function revokeLink(_prev: FormState, form: FormData): Promise<FormState> {
  const me = await actor();
  const target = tryDecodeId('link', field(form, 'link'));
  if (target === null) return { error: NO_SUCH };

  const ok = await database().transaction(async (tx) => {
    if (!(await allows(tx, me, { on: 'link', id: target }))) return false;
    const rows = await tx
      .update(schema.link)
      .set({ revokedAt: sql`now()` })
      .where(
        and(
          eq(schema.link.id, target),
          isNull(schema.link.revokedAt),
          /* A claimed link is the record of a merge; taking it back would
           * erase the only trace of where the person came from. */
          isNull(schema.link.claimedAt),
        ),
      )
      .returning({ id: schema.link.id });
    if (rows.length === 0) return false;
    await recordAudit(tx, {
      actorPersonId: me.personId,
      command: 'revoke-link',
      targetKind: 'link',
      targetId: target,
    });
    return true;
  });

  if (!ok) return { error: NO_SUCH };
  revalidatePath(PEOPLE);
  return { notice: 'That link no longer works.' };
}

/** ClaimLink, for a visitor who is already signed in. The token says which
 *  held person; the session says who they are. */
export async function claimLink(_prev: FormState, form: FormData): Promise<FormState> {
  const principal = await currentPrincipal();
  const token = field(form, 'token');
  if (!principal) redirect(`/links/${encodeURIComponent(token)}`);

  const ok = await database().transaction(async (tx) => {
    const spent = await spendClaimLink(tx, token, principal.personId);
    if (!spent) return false;
    await recordAudit(tx, {
      actorPersonId: principal.personId,
      command: 'claim-link',
      targetKind: 'link',
      targetId: spent.linkId,
      payload: { held: spent.personId, person: principal.personId },
    });
    return mergePersons(tx, {
      survivor: principal.personId,
      absorbed: spent.personId,
      actorPersonId: principal.personId,
      method: 'claim',
    });
  });

  if (!ok) return { error: 'That link no longer works.' };
  redirect('/app');
}

/** ConfirmMatch, the member's half: the same merge a claim performs, asked
 *  for from `/app` instead of from a link. */
export async function confirmMatch(_prev: FormState, form: FormData): Promise<FormState> {
  const me = await actor();
  const target = tryDecodeId('match', field(form, 'match'));
  if (target === null) return { error: NO_SUCH };

  const ok = await database().transaction(async (tx) => {
    const rows = await tx
      .select({
        id: schema.match.id,
        identityA: schema.match.identityA,
        identityB: schema.match.identityB,
      })
      .from(schema.match)
      .where(and(eq(schema.match.id, target), eq(schema.match.status, 'proposed')))
      .limit(1);
    const candidate = rows[0];
    if (!candidate) return false;

    const sides = await tx
      .select({ id: schema.identity.id, personId: schema.identity.personId })
      .from(schema.identity)
      .where(inArray(schema.identity.id, [candidate.identityA, candidate.identityB]));
    const mine = sides.find((row) => row.personId === me.personId);
    const other = sides.find((row) => row.personId !== me.personId);
    /* One side has to be the asker's own. Ruling on a pair that is nobody's
     * own is the operator's, in R6. */
    if (!mine || !other) return false;

    await settleMatchesFor(tx, { matchId: candidate.id, personId: me.personId });
    return mergePersons(tx, {
      survivor: me.personId,
      absorbed: other.personId,
      actorPersonId: me.personId,
      method: 'match',
    });
  });

  if (!ok) return { error: NO_SUCH };
  revalidatePath('/app');
  revalidatePath(PEOPLE);
  return { notice: 'Merged. What they held is yours.' };
}

/** SetDisplayName. */
export async function setDisplayName(_prev: FormState, form: FormData): Promise<FormState> {
  const me = await actor();
  const parsed = displayName.safeParse(field(form, 'displayName'));
  if (!parsed.success) return { error: 'A name, up to 120 characters.' };

  await database().transaction(async (tx) => {
    await tx
      .update(schema.person)
      .set({ displayName: parsed.data })
      .where(eq(schema.person.id, me.personId));
    await recordAudit(tx, {
      actorPersonId: me.personId,
      command: 'set-display-name',
      targetKind: 'person',
      targetId: me.personId,
    });
  });
  revalidatePath('/app');
  return { notice: 'Name changed.' };
}

/** AddEmail. A second door, confirmed the same way as the first: the letter
 *  arriving is the proof. */
export async function addEmail(_prev: FormState, form: FormData): Promise<FormState> {
  const me = await actor();
  const parsed = emailInput.safeParse({ email: field(form, 'email') });
  if (!parsed.success) return { error: 'An email address.' };
  const address = parsed.data.email;
  const where = await requestFingerprint();

  const sent = await database().transaction(async (tx) => {
    if (await overLimit(tx, RESET_LIMIT, ['email', address])) return null;
    if (where.ip && (await overLimit(tx, RESET_LIMIT, ['ip', where.ip]))) return null;
    /* An address already spoken for says nothing about who has it. */
    const taken = await tx
      .select({ id: schema.identity.id })
      .from(schema.identity)
      .where(and(eq(schema.identity.source, 'local'), eq(schema.identity.subject, address)))
      .limit(1);
    if (taken.length > 0) return null;

    const rows = await tx
      .insert(schema.identity)
      .values({ personId: me.personId, source: 'local', subject: address })
      .returning({ id: schema.identity.id });
    const identityId = rows[0]!.id;
    await tx.insert(schema.factor).values({ identityId, kind: 'email', meta: { address } });
    const token = await issueLink(tx, {
      kind: 'verify_email',
      targetKind: 'identity',
      targetId: identityId,
      ttlHours: VERIFY_TTL_HOURS,
      createdBy: me.personId,
    });
    await recordAudit(tx, {
      actorPersonId: me.personId,
      command: 'add-email',
      targetKind: 'identity',
      targetId: identityId,
    });
    return token;
  });

  if (sent) {
    await deliver(verificationMail(address, linkUrl('verify_email', sent)), {
      command: 'add-email',
    });
  }
  revalidatePath('/app');
  return { notice: CHECK_INBOX };
}

/** RemoveIdentity. Refuses the last door, because an account nobody can reach
 *  is not an account. */
export async function removeIdentity(_prev: FormState, form: FormData): Promise<FormState> {
  const me = await actor();
  const target = tryDecodeId('identity', field(form, 'identity'));
  if (target === null) return { error: NO_SUCH };

  const outcome = await database().transaction(async (tx) => {
    const rows = await tx
      .select({ id: schema.identity.id, source: schema.identity.source })
      .from(schema.identity)
      .where(and(eq(schema.identity.id, target), eq(schema.identity.personId, me.personId)))
      .limit(1);
    const row = rows[0];
    if (!row) return 'no' as const;

    if (row.source !== 'handle') {
      const doors = await tx
        .select({ id: schema.identity.id })
        .from(schema.identity)
        .where(
          and(
            eq(schema.identity.personId, me.personId),
            inArray(schema.identity.source, ['local', 'oidc']),
          ),
        );
      if (doors.length <= 1) return 'last' as const;
    }

    await tx.delete(schema.identity).where(eq(schema.identity.id, target));
    await recordAudit(tx, {
      actorPersonId: me.personId,
      command: 'remove-identity',
      targetKind: 'identity',
      targetId: target,
      payload: { source: row.source },
    });
    return 'gone' as const;
  });

  if (outcome === 'no') return { error: NO_SUCH };
  if (outcome === 'last') return { error: 'That is the only way in. Add another one first.' };
  revalidatePath('/app');
  return { notice: 'Removed.' };
}

/** ClaimLink for a visitor who has no account yet. Holding the link is what
 *  identifies them — that is what a personal link is for — so the account is
 *  created and the held person folded into it in one transaction. The address
 *  still has to be confirmed before it becomes a door, exactly as it would
 *  from the register form; a bearer link says who somebody is, not that they
 *  own an address. */
export async function claimByRegistering(_prev: FormState, form: FormData): Promise<FormState> {
  const parsed = claimRegisterInput.safeParse({
    token: field(form, 'token'),
    displayName: field(form, 'displayName'),
    email: field(form, 'email'),
    password: field(form, 'password'),
  });
  if (!parsed.success) {
    return { error: 'A name, an address, and a password of at least 10 characters.' };
  }
  const { token, email: address, password: secret } = parsed.data;
  const phc = await hashPassword(secret);

  const outcome = await database().transaction(async (tx) => {
    const spent = await spendClaimLink(tx, token, null);
    if (!spent) return 'gone' as const;
    if (isAllowlisted(address)) return 'taken' as const;
    const taken = await tx
      .select({ id: schema.identity.id })
      .from(schema.identity)
      .where(and(eq(schema.identity.source, 'local'), eq(schema.identity.subject, address)))
      .limit(1);
    if (taken.length > 0) return 'taken' as const;

    const personId = await createPerson(tx, { displayName: parsed.data.displayName });
    const identities = await tx
      .insert(schema.identity)
      .values({ personId, source: 'local', subject: address })
      .returning({ id: schema.identity.id });
    const identityId = identities[0]!.id;
    await tx.insert(schema.factor).values([
      { identityId, kind: 'email', meta: { address } },
      { identityId, kind: 'password', secret: phc },
    ]);
    const verify = await issueLink(tx, {
      kind: 'verify_email',
      targetKind: 'identity',
      targetId: identityId,
      ttlHours: VERIFY_TTL_HOURS,
    });
    await tx
      .update(schema.link)
      .set({ claimedBy: personId })
      .where(eq(schema.link.id, spent.linkId));
    await recordAudit(tx, {
      actorPersonId: personId,
      command: 'claim-link',
      targetKind: 'link',
      targetId: spent.linkId,
      payload: { held: spent.personId, person: personId, registered: true },
    });
    await mergePersons(tx, {
      survivor: personId,
      absorbed: spent.personId,
      actorPersonId: personId,
      method: 'claim',
    });
    return { verify, address } as const;
  });

  if (outcome === 'gone') return { error: 'That link no longer works.' };
  if (outcome === 'taken') {
    return { error: 'That address cannot be used here. Sign in instead.' };
  }
  await deliver(verificationMail(outcome.address, linkUrl('verify_email', outcome.verify)), {
    command: 'claim-link',
  });
  return { notice: CHECK_INBOX };
}
