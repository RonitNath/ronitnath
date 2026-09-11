'use server';

/* Groups. A group is a bag of subjects with a name and an owner — a person's
 * own circle, or an organization's team. Membership is the same three roles
 * an organization uses, and a group may hold another group.
 *
 * The one thing that cannot be expressed as a rank is the shape of the graph:
 * a group inside itself, however many hops away, would make `subjectsOf` a
 * walk with no end. The refusal is `wouldCycle`, checked here before the row
 * is written and tested without a database. */

import { and, eq, isNull } from 'drizzle-orm';
import { revalidatePath } from 'next/cache';
import { redirect } from 'next/navigation';
import { z } from 'zod';

import { database, schema } from '@/db/client';
import { recordAudit } from '@/features/auth/audit';
import type { FormState } from '@/features/auth/form-state';
import { currentPrincipal } from '@/features/auth/principal';
import {
  allows,
  grant,
  isRole,
  membershipEdges,
  registerResource,
  revoke,
  wouldCycle,
  type Subject,
} from '@/lib/authority';
import { encodeId, tryDecodeId } from '@/lib/ids';
import { userPath } from '@/lib/paths';
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

const nameInput = z.object({ name: z.string().trim().min(1).max(120) });
const BAD_NAME = 'A name, up to 120 characters.';

/** The member a form names: a person, or another group. One field, and the
 *  prefix says which — that is what the public-id codec is for. */
function subjectOf(raw: string): Subject | null {
  const person = tryDecodeId('person', raw);
  if (person !== null) return { kind: 'person', id: person };
  const group = tryDecodeId('group', raw);
  if (group !== null) return { kind: 'group', id: group };
  return null;
}

/** CreateGroup. Owned by the member who made it, or by an organization they
 *  are an admin of. */
export async function createGroup(_prev: FormState, form: FormData): Promise<FormState> {
  const me = await actor();
  const parsed = nameInput.safeParse({ name: field(form, 'name') });
  if (!parsed.success) return { error: BAD_NAME };
  const owner = field(form, 'owner');
  const organizationId = owner && owner !== 'me' ? tryDecodeId('organization', owner) : null;
  if (owner && owner !== 'me' && organizationId === null) return { error: NO_SUCH };

  const made = await database().transaction(async (tx) => {
    if (
      organizationId !== null &&
      !(await allows(tx, me, { on: 'organization', id: organizationId, need: 'admin' }))
    ) {
      return null;
    }
    const ownerPartyId = organizationId ?? me.personId;
    const rows = await tx
      .insert(schema.group)
      .values({ ownerPartyId, name: parsed.data.name })
      .returning({ id: schema.group.id });
    const id = rows[0]!.id;
    await registerResource(tx, { kind: 'group', id, ownerPartyId });
    await grant(tx, { subject: { kind: 'person', id: me.personId }, verb: 'owner', on: 'group', id });
    await recordAudit(tx, {
      actorPersonId: me.personId,
      command: 'create-group',
      targetKind: 'group',
      targetId: id,
      payload: { owner: ownerPartyId },
    });
    await emit(tx, {
      orgId: streamOf(me),
      resourceKind: 'group',
      resourceId: encodeId('group', id),
      kind: 'created',
    });
    return id;
  });

  if (made === null) return { error: NO_SUCH };
  revalidatePath(home(me, 'groups'));
  return { notice: `${parsed.data.name} exists.` };
}

/** RenameGroup. */
export async function renameGroup(_prev: FormState, form: FormData): Promise<FormState> {
  const me = await actor();
  const id = tryDecodeId('group', field(form, 'group'));
  const parsed = nameInput.safeParse({ name: field(form, 'name') });
  if (id === null) return { error: NO_SUCH };
  if (!parsed.success) return { error: BAD_NAME };

  const ok = await database().transaction(async (tx) => {
    if (!(await allows(tx, me, { on: 'group', id, need: 'admin' }))) return false;
    await tx.update(schema.group).set({ name: parsed.data.name }).where(eq(schema.group.id, id));
    await recordAudit(tx, {
      actorPersonId: me.personId,
      command: 'rename-group',
      targetKind: 'group',
      targetId: id,
    });
    await emit(tx, {
      orgId: streamOf(me),
      resourceKind: 'group',
      resourceId: encodeId('group', id),
      kind: 'updated',
    });
    return true;
  });

  if (!ok) return { error: NO_SUCH };
  revalidatePath(home(me, 'groups'));
  return { notice: 'Renamed.' };
}

/** AddToGroup. A person — held or not — or another group. */
export async function addToGroup(_prev: FormState, form: FormData): Promise<FormState> {
  const me = await actor();
  const id = tryDecodeId('group', field(form, 'group'));
  const subject = subjectOf(field(form, 'member'));
  const role = field(form, 'role') || 'member';
  if (id === null || subject === null || !isRole(role)) return { error: NO_SUCH };

  const outcome = await database().transaction(async (tx) => {
    if (!(await allows(tx, me, { on: 'group', id, need: 'admin' }))) return 'no' as const;
    if (subject.kind === 'person') {
      const live = await tx
        .select({ id: schema.person.id })
        .from(schema.person)
        .where(and(eq(schema.person.id, subject.id), isNull(schema.person.mergedInto)))
        .limit(1);
      if (live.length === 0) return 'no' as const;
    } else {
      /* The group being added has to be one the member may hand out, and the
       * graph has to stay a graph. */
      if (!(await allows(tx, me, { on: 'group', id: subject.id, need: 'member' }))) {
        return 'no' as const;
      }
      if (wouldCycle(await membershipEdges(tx), { kind: 'group', id }, subject)) {
        return 'cycle' as const;
      }
    }
    await grant(tx, { subject, verb: role, on: 'group', id });
    await recordAudit(tx, {
      actorPersonId: me.personId,
      command: 'add-to-group',
      targetKind: 'group',
      targetId: id,
      payload: { subject: `${subject.kind}:${subject.id}`, role },
    });
    await emit(tx, {
      orgId: streamOf(me),
      resourceKind: 'group',
      resourceId: encodeId('group', id),
      kind: 'updated',
    });
    return 'done' as const;
  });

  if (outcome === 'cycle') return { error: 'A group cannot contain itself.' };
  if (outcome === 'no') return { error: NO_SUCH };
  revalidatePath(home(me, 'groups'));
  return { notice: 'Added.' };
}

/** RemoveFromGroup. */
export async function removeFromGroup(_prev: FormState, form: FormData): Promise<FormState> {
  const me = await actor();
  const id = tryDecodeId('group', field(form, 'group'));
  const subject = subjectOf(field(form, 'member'));
  if (id === null || subject === null) return { error: NO_SUCH };

  const ok = await database().transaction(async (tx) => {
    if (!(await allows(tx, me, { on: 'group', id, need: 'admin' }))) return false;
    await revoke(tx, { subject, on: 'group', id });
    await recordAudit(tx, {
      actorPersonId: me.personId,
      command: 'remove-from-group',
      targetKind: 'group',
      targetId: id,
      payload: { subject: `${subject.kind}:${subject.id}` },
    });
    await emit(tx, {
      orgId: streamOf(me),
      resourceKind: 'group',
      resourceId: encodeId('group', id),
      kind: 'updated',
    });
    return true;
  });

  if (!ok) return { error: NO_SUCH };
  revalidatePath(home(me, 'groups'));
  return { notice: 'Removed.' };
}

/** DeleteGroup. The rows that named it go with it: a share granted to a group
 *  that no longer exists is a share nobody holds. */
export async function deleteGroup(_prev: FormState, form: FormData): Promise<FormState> {
  const me = await actor();
  const id = tryDecodeId('group', field(form, 'group'));
  if (id === null) return { error: NO_SUCH };

  const ok = await database().transaction(async (tx) => {
    if (!(await allows(tx, me, { on: 'group', id, need: 'owner' }))) return false;
    await tx
      .delete(schema.relation)
      .where(
        and(eq(schema.relation.resourceKind, 'group'), eq(schema.relation.resourceId, id)),
      );
    await tx
      .delete(schema.relation)
      .where(and(eq(schema.relation.subjectKind, 'group'), eq(schema.relation.subjectId, id)));
    await tx
      .delete(schema.resource)
      .where(and(eq(schema.resource.kind, 'group'), eq(schema.resource.refId, id)));
    await tx.delete(schema.group).where(eq(schema.group.id, id));
    await recordAudit(tx, {
      actorPersonId: me.personId,
      command: 'delete-group',
      targetKind: 'group',
      targetId: id,
    });
    await emit(tx, {
      orgId: streamOf(me),
      resourceKind: 'group',
      resourceId: encodeId('group', id),
      kind: 'deleted',
    });
    return true;
  });

  if (!ok) return { error: NO_SUCH };
  revalidatePath(home(me, 'groups'));
  return { notice: 'Gone.' };
}
