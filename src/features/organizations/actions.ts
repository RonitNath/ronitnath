'use server';

/* The organization's commands. Each is one transaction with its audit row
 * inside it, zod at the door, and one uniform decline: an organization a
 * member may not touch and an organization that does not exist are the same
 * answer.
 *
 * Authorisation is `allows` and nothing else (src/lib/authority.ts). What is
 * written here on top of it are the two rules a rank cannot express: an
 * organization always has an owner, and an admin cannot promote themselves
 * past the person who put them there. */

import { and, eq, isNull, sql } from 'drizzle-orm';
import { revalidatePath } from 'next/cache';
import { redirect } from 'next/navigation';
import { z } from 'zod';

import { database, schema } from '@/db/client';
import { recordAudit } from '@/features/auth/audit';
import type { Transaction } from '@/features/auth/db';
import type { FormState } from '@/features/auth/form-state';
import { createPerson } from '@/features/people/provision';
import { currentPrincipal } from '@/features/auth/principal';
import { CONTACT } from '@/features/people/authority';
import { HANDLE_MAX, normalizeHandle } from '@/features/people/handles';
import { claimUrl, mintClaimLink } from '@/features/people/invitations';
import {
  allows,
  grant,
  heldRank,
  isLastOwner,
  isRole,
  ownerPersonIds,
  rankOf,
  revoke,
} from '@/lib/authority';
import { encodeId, tryDecodeId } from '@/lib/ids';
import { ORG_HANDLE_MAX, normalizeOrgHandle } from './handles';
import { orgPath, userPath } from '@/lib/paths';
import { emit } from '@/lib/fleet/events';

const NO_SUCH = 'That is not something you can do here.';

function field(form: FormData, name: string): string {
  const value = form.get(name);
  return typeof value === 'string' ? value : '';
}

async function actor() {
  const principal = await currentPrincipal();
  if (!principal) redirect('/auth/sign-in');
  return { personId: principal.personId, isOperator: principal.isOperator };
}
/* Where the person who ran this command looks at what it changed. Their
 * surfaces are addressed by their public id, so the path is a function of
 * who is asking and cannot be a constant. */
function home(me: { personId: number }, view = ''): string {
  return userPath(encodeId('person', me.personId), view);
}

/* The stream a person's own changes are published on: their public id, which
 * is also the `[user]` segment of every page that shows them. The event goes
 * in the same transaction as the row it is about and the audit row beside it:
 * an event written after the commit is an event a rollback cannot take back,
 * and one written outside it is a change nobody is told about (§7). */
function streamOf(me: { personId: number }): string {
  return encodeId('person', me.personId);
}

/** The path to revalidate after a command that changed an organization. */
async function handleOf(tx: Transaction, id: number): Promise<string> {
  const rows = await tx
    .select({ handle: schema.organization.handle })
    .from(schema.organization)
    .where(eq(schema.organization.id, id))
    .limit(1);
  return rows[0]?.handle ?? '';
}

const createInput = z.object({
  handle: z.string().trim().max(ORG_HANDLE_MAX),
  name: z.string().trim().min(1).max(120),
});
const profileInput = z.object({ name: z.string().trim().min(1).max(120) });
const inviteInput = z.object({
  handle: z.string().trim().min(1).max(HANDLE_MAX),
  displayName: z.string().trim().max(120),
  role: z.string(),
});

const BAD_HANDLE = 'A handle of letters, digits and hyphens, and a name.';

/** CreateOrganization. The party comes first — an organization is addressable
 *  before it is anything else — and the person who made it owns it. */
export async function createOrganization(_prev: FormState, form: FormData): Promise<FormState> {
  const me = await actor();
  const parsed = createInput.safeParse({
    handle: field(form, 'handle'),
    name: field(form, 'name'),
  });
  if (!parsed.success) return { error: BAD_HANDLE };
  const handle = normalizeOrgHandle(parsed.data.handle);
  if (handle === null) return { error: BAD_HANDLE };

  const made = await database().transaction(async (tx) => {
    const parties = await tx
      .insert(schema.party)
      .values({ kind: 'organization' })
      .returning({ id: schema.party.id });
    const id = parties[0]!.id;
    /* Uniqueness is the index's answer, not a lookup's. */
    const rows = await tx
      .insert(schema.organization)
      .values({ id, handle, name: parsed.data.name })
      .onConflictDoNothing({ target: schema.organization.handle })
      .returning({ id: schema.organization.id });
    if (rows.length === 0) return null;
    await grant(tx, {
      subject: { kind: 'person', id: me.personId },
      verb: 'owner',
      on: 'organization',
      id,
    });
    await recordAudit(tx, {
      actorPersonId: me.personId,
      command: 'create-organization',
      targetKind: 'organization',
      targetId: id,
      payload: { handle },
    });
    await emit(tx, {
      orgId: handle,
      resourceKind: 'organization',
      resourceId: encodeId('organization', id),
      kind: 'created',
    });
    /* The maker's own account page lists the organizations they belong to. */
    await emit(tx, {
      orgId: streamOf(me),
      resourceKind: 'organization',
      resourceId: encodeId('organization', id),
      kind: 'created',
    });
    return id;
  });

  if (made === null) return { error: 'That handle is taken.' };
  revalidatePath(home(me));
  redirect(orgPath(handle));
}

/** SetOrganizationProfile. */
export async function setOrganizationProfile(
  _prev: FormState,
  form: FormData,
): Promise<FormState> {
  const me = await actor();
  const id = tryDecodeId('organization', field(form, 'organization'));
  const parsed = profileInput.safeParse({ name: field(form, 'name') });
  if (id === null) return { error: NO_SUCH };
  if (!parsed.success) return { error: 'A name, up to 120 characters.' };

  const handle = await database().transaction(async (tx) => {
    if (!(await allows(tx, me, { on: 'organization', id, need: 'admin' }))) return null;
    const rows = await tx
      .update(schema.organization)
      .set({ name: parsed.data.name })
      .where(eq(schema.organization.id, id))
      .returning({ handle: schema.organization.handle });
    if (rows.length === 0) return null;
    await recordAudit(tx, {
      actorPersonId: me.personId,
      command: 'set-organization-profile',
      targetKind: 'organization',
      targetId: id,
    });
    /* An organization's stream is keyed by its handle, which is the segment in
     * `/o/<org>` and so the thing its own SSE route listens on. */
    await emit(tx, {
      orgId: await handleOf(tx, id),
      resourceKind: 'organization',
      resourceId: encodeId('organization', id),
      kind: 'updated',
    });
    return rows[0]!.handle;
  });

  if (handle === null) return { error: NO_SUCH };
  revalidatePath(orgPath(handle));
  return { notice: 'Saved.' };
}

/** InviteToOrganization. The invitee is written down as a held person who
 *  already carries the role, and the URL hands that person over: claiming it
 *  is R3's merge, and the role comes across with everything else the held
 *  person had. Nothing new authorises anybody. */
export async function inviteToOrganization(_prev: FormState, form: FormData): Promise<FormState> {
  const me = await actor();
  const id = tryDecodeId('organization', field(form, 'organization'));
  const parsed = inviteInput.safeParse({
    handle: field(form, 'handle'),
    displayName: field(form, 'displayName'),
    role: field(form, 'role') || 'member',
  });
  if (id === null || !parsed.success || !isRole(parsed.data.role)) return { error: NO_SUCH };
  const handle = normalizeHandle(parsed.data.handle);
  if (handle === null) return { error: 'An email, a phone number, or a name.' };
  const role = parsed.data.role;
  const name = parsed.data.displayName || handle.raw;

  const outcome = await database().transaction(async (tx) => {
    if (!(await allows(tx, me, { on: 'organization', id, need: 'admin' }))) return null;
    /* Only an owner hands out ownership. */
    const mine = (await heldRank(tx, me, 'organization', id)) ?? '';
    if (rankOf('organization', role) > rankOf('organization', mine) && !me.isOperator) return null;

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
    await grant(tx, {
      subject: { kind: 'person', id: heldId },
      verb: role,
      on: 'organization',
      id,
    });
    const { token, linkId } = await mintClaimLink(tx, {
      personId: heldId,
      createdBy: me.personId,
    });
    await recordAudit(tx, {
      actorPersonId: me.personId,
      command: 'invite-to-organization',
      targetKind: 'organization',
      targetId: id,
      payload: { person: heldId, role, link: linkId },
    });
    /* An organization's stream is keyed by its handle, which is the segment in
     * `/o/<org>` and so the thing its own SSE route listens on. */
    await emit(tx, {
      orgId: await handleOf(tx, id),
      resourceKind: 'organization',
      resourceId: encodeId('organization', id),
      kind: 'updated',
    });
    const org = await tx
      .select({ handle: schema.organization.handle })
      .from(schema.organization)
      .where(eq(schema.organization.id, id))
      .limit(1);
    return { url: claimUrl(token), handle: org[0]!.handle };
  });

  if (outcome === null) return { error: NO_SUCH };
  revalidatePath(orgPath(outcome.handle));
  return {
    minted: outcome.url,
    notice: 'Copy it now — it is not stored and cannot be shown again.',
  };
}

interface MemberTarget {
  organizationId: number;
  personId: number;
}

function memberTarget(form: FormData): MemberTarget | null {
  const organizationId = tryDecodeId('organization', field(form, 'organization'));
  const personId = tryDecodeId('person', field(form, 'person'));
  if (organizationId === null || personId === null) return null;
  return { organizationId, personId };
}

/** SetRole. An admin may not act on somebody who outranks them, and may not
 *  hand out a rank above their own; an owner may do both. */
export async function setRole(_prev: FormState, form: FormData): Promise<FormState> {
  const me = await actor();
  const target = memberTarget(form);
  const role = field(form, 'role');
  if (target === null || !isRole(role)) return { error: NO_SUCH };
  const { organizationId, personId } = target;

  const outcome = await database().transaction(async (tx) => {
    if (!(await allows(tx, me, { on: 'organization', id: organizationId, need: 'admin' }))) {
      return 'no' as const;
    }
    const mine = rankOf(
      'organization',
      (await heldRank(tx, me, 'organization', organizationId)) ?? '',
    );
    const theirs = rankOf(
      'organization',
      (await heldRank(tx, { personId, isOperator: false }, 'organization', organizationId)) ?? '',
    );
    if (!me.isOperator && (theirs > mine || rankOf('organization', role) > mine)) {
      return 'no' as const;
    }
    const owners = await ownerPersonIds(tx, 'organization', organizationId);
    if (role !== 'owner' && isLastOwner(owners, personId)) return 'last' as const;

    await grant(tx, {
      subject: { kind: 'person', id: personId },
      verb: role,
      on: 'organization',
      id: organizationId,
    });
    await recordAudit(tx, {
      actorPersonId: me.personId,
      command: 'set-role',
      targetKind: 'organization',
      targetId: organizationId,
      payload: { person: personId, role },
    });
    /* An organization's stream is keyed by its handle, which is the segment in
     * `/o/<org>` and so the thing its own SSE route listens on. */
    await emit(tx, {
      orgId: await handleOf(tx, organizationId),
      resourceKind: 'organization',
      resourceId: encodeId('organization', organizationId),
      kind: 'updated',
    });
    return { at: await handleOf(tx, organizationId) };
  });

  if (outcome === 'last') return { error: 'An organization keeps an owner.' };
  if (outcome === 'no') return { error: NO_SUCH };
  revalidatePath(orgPath(outcome.at));
  return { notice: 'Changed.' };
}

/** RemoveMember. */
export async function removeMember(_prev: FormState, form: FormData): Promise<FormState> {
  const me = await actor();
  const target = memberTarget(form);
  if (target === null) return { error: NO_SUCH };
  const { organizationId, personId } = target;

  const outcome = await database().transaction(async (tx) => {
    if (!(await allows(tx, me, { on: 'organization', id: organizationId, need: 'admin' }))) {
      return 'no' as const;
    }
    const mine = rankOf(
      'organization',
      (await heldRank(tx, me, 'organization', organizationId)) ?? '',
    );
    const theirs = rankOf(
      'organization',
      (await heldRank(tx, { personId, isOperator: false }, 'organization', organizationId)) ?? '',
    );
    if (!me.isOperator && theirs > mine) return 'no' as const;
    const owners = await ownerPersonIds(tx, 'organization', organizationId);
    if (isLastOwner(owners, personId)) return 'last' as const;

    await revoke(tx, {
      subject: { kind: 'person', id: personId },
      on: 'organization',
      id: organizationId,
    });
    await recordAudit(tx, {
      actorPersonId: me.personId,
      command: 'remove-member',
      targetKind: 'organization',
      targetId: organizationId,
      payload: { person: personId },
    });
    /* An organization's stream is keyed by its handle, which is the segment in
     * `/o/<org>` and so the thing its own SSE route listens on. */
    await emit(tx, {
      orgId: await handleOf(tx, organizationId),
      resourceKind: 'organization',
      resourceId: encodeId('organization', organizationId),
      kind: 'updated',
    });
    return { at: await handleOf(tx, organizationId) };
  });

  if (outcome === 'last') return { error: 'An organization keeps an owner.' };
  if (outcome === 'no') return { error: NO_SUCH };
  revalidatePath(orgPath(outcome.at));
  return { notice: 'Removed.' };
}

/** Leave. The last owner cannot: they hand it on first (Transfer), or they
 *  make somebody else an owner. */
export async function leaveOrganization(_prev: FormState, form: FormData): Promise<FormState> {
  const me = await actor();
  const id = tryDecodeId('organization', field(form, 'organization'));
  if (id === null) return { error: NO_SUCH };

  const outcome = await database().transaction(async (tx) => {
    if (!(await allows(tx, me, { on: 'organization', id, need: 'member' }))) return 'no' as const;
    const owners = await ownerPersonIds(tx, 'organization', id);
    if (isLastOwner(owners, me.personId)) return 'last' as const;
    await revoke(tx, { subject: { kind: 'person', id: me.personId }, on: 'organization', id });
    await recordAudit(tx, {
      actorPersonId: me.personId,
      command: 'leave-organization',
      targetKind: 'organization',
      targetId: id,
    });
    await emit(tx, {
      orgId: await handleOf(tx, id),
      resourceKind: 'organization',
      resourceId: encodeId('organization', id),
      kind: 'updated',
    });
    await emit(tx, {
      orgId: streamOf(me),
      resourceKind: 'organization',
      resourceId: encodeId('organization', id),
      kind: 'updated',
    });
    return 'done' as const;
  });

  if (outcome === 'last') {
    return { error: 'You are the last owner. Give it to somebody else first.' };
  }
  if (outcome === 'no') return { error: NO_SUCH };
  revalidatePath(home(me));
  redirect(home(me));
}

/** Transfer, of the organization itself: the new owner is made one, and the
 *  person handing it over stays as an admin. Owner or operator only. */
export async function transferOrganization(_prev: FormState, form: FormData): Promise<FormState> {
  const me = await actor();
  const target = memberTarget(form);
  if (target === null) return { error: NO_SUCH };
  const { organizationId, personId } = target;

  const outcome = await database().transaction(async (tx) => {
    if (!(await allows(tx, me, { on: 'organization', id: organizationId, need: 'owner' }))) {
      return 'no' as const;
    }
    const live = await tx
      .select({ id: schema.person.id })
      .from(schema.person)
      .where(and(eq(schema.person.id, personId), isNull(schema.person.mergedInto)))
      .limit(1);
    if (live.length === 0) return 'no' as const;

    await grant(tx, {
      subject: { kind: 'person', id: personId },
      verb: 'owner',
      on: 'organization',
      id: organizationId,
    });
    if (personId !== me.personId) {
      await grant(tx, {
        subject: { kind: 'person', id: me.personId },
        verb: 'admin',
        on: 'organization',
        id: organizationId,
      });
    }
    await recordAudit(tx, {
      actorPersonId: me.personId,
      command: 'transfer-organization',
      targetKind: 'organization',
      targetId: organizationId,
      payload: { to: personId },
    });
    /* An organization's stream is keyed by its handle, which is the segment in
     * `/o/<org>` and so the thing its own SSE route listens on. */
    await emit(tx, {
      orgId: await handleOf(tx, organizationId),
      resourceKind: 'organization',
      resourceId: encodeId('organization', organizationId),
      kind: 'updated',
    });
    return { at: await handleOf(tx, organizationId) };
  });

  if (outcome === 'no') return { error: NO_SUCH };
  revalidatePath(orgPath(outcome.at));
  return { notice: 'Handed over.' };
}

/** RevokeInvitation, for an organization's own list. The same guard the
 *  people page carries: a claimed link is the record of a merge. */
export async function revokeInvitation(_prev: FormState, form: FormData): Promise<FormState> {
  const me = await actor();
  const id = tryDecodeId('organization', field(form, 'organization'));
  const linkId = tryDecodeId('link', field(form, 'link'));
  if (id === null || linkId === null) return { error: NO_SUCH };

  const ok = await database().transaction(async (tx) => {
    if (!(await allows(tx, me, { on: 'organization', id, need: 'admin' }))) return null;
    const rows = await tx
      .update(schema.link)
      .set({ revokedAt: sql`now()` })
      .where(
        and(
          eq(schema.link.id, linkId),
          isNull(schema.link.revokedAt),
          isNull(schema.link.claimedAt),
        ),
      )
      .returning({ id: schema.link.id });
    if (rows.length === 0) return null;
    await recordAudit(tx, {
      actorPersonId: me.personId,
      command: 'revoke-link',
      targetKind: 'link',
      targetId: linkId,
      payload: { organization: id },
    });
    /* An organization's stream is keyed by its handle, which is the segment in
     * `/o/<org>` and so the thing its own SSE route listens on. */
    await emit(tx, {
      orgId: await handleOf(tx, id),
      resourceKind: 'organization',
      resourceId: encodeId('organization', id),
      kind: 'updated',
    });
    return { at: await handleOf(tx, id) };
  });

  if (ok === null) return { error: NO_SUCH };
  revalidatePath(orgPath(ok.at));
  return { notice: 'That link no longer works.' };
}
