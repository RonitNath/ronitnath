'use server';

/* Documents and the shares over them. A document belongs to a party — the
 * member who wrote it, or an organization — and everybody else reaches it
 * through a share: `viewer < commenter < editor`, granted to a person, a
 * group or an organization, and nested in code so that a command asks for the
 * level it needs and never for a list of verbs.
 *
 * Comments are not in this rung. `commenter` exists in the vocabulary because
 * the level between reading and writing is a real one and a share granted
 * today should not have to be regranted when the comments arrive. */

import { and, eq, isNull, sql } from 'drizzle-orm';
import { revalidatePath } from 'next/cache';
import { redirect } from 'next/navigation';
import { z } from 'zod';

import { database, schema } from '@/db/client';
import { recordAudit } from '@/features/auth/audit';
import type { FormState } from '@/features/auth/form-state';
import { currentPrincipal } from '@/features/auth/principal';
import { slugCandidate } from '@/features/events/slug';
import {
  allows,
  grant,
  isLevel,
  registerResource,
  revoke,
  type Subject,
} from '@/lib/authority';
import { encodeId, tryDecodeId } from '@/lib/ids';

const NO_SUCH = 'That is not something you can do here.';
const DOCUMENTS = '/app/documents';

function field(form: FormData, name: string): string {
  const value = form.get(name);
  return typeof value === 'string' ? value : '';
}

async function actor() {
  const principal = await currentPrincipal();
  if (!principal) redirect('/auth/sign-in');
  return { personId: principal.personId, isOperator: principal.isOperator };
}

function documentPath(id: number): string {
  return `${DOCUMENTS}/${encodeId('document', id)}`;
}

const createInput = z.object({ title: z.string().trim().min(1).max(200) });
const editInput = createInput.extend({ body: z.string().max(40_000) });

/** Whichever subject a public id names: a person, a group, or an
 *  organization. One field on the share form, and the prefix says which. */
function subjectOf(raw: string): Subject | null {
  for (const kind of ['person', 'group', 'organization'] as const) {
    const id = tryDecodeId(kind, raw);
    if (id !== null) return { kind, id };
  }
  return null;
}

/** CreateDocument. The slug is derived once, here, so that a rewritten title
 *  does not break a URL somebody already has. */
export async function createDocument(_prev: FormState, form: FormData): Promise<FormState> {
  const me = await actor();
  const parsed = createInput.safeParse({ title: field(form, 'title') });
  if (!parsed.success) return { error: 'A title, up to 200 characters.' };
  const owner = field(form, 'owner');
  const organizationId = owner && owner !== 'me' ? tryDecodeId('organization', owner) : null;
  if (owner && owner !== 'me' && organizationId === null) return { error: NO_SUCH };

  const made = await database().transaction(async (tx) => {
    if (
      organizationId !== null &&
      !(await allows(tx, me, { on: 'organization', id: organizationId, need: 'member' }))
    ) {
      return null;
    }
    const ownerPartyId = organizationId ?? me.personId;
    for (let attempt = 0; attempt < 12; attempt += 1) {
      const slug = slugCandidate(parsed.data.title, attempt);
      const rows = await tx
        .insert(schema.document)
        .values({ ownerPartyId, title: parsed.data.title, slug })
        .onConflictDoNothing({ target: schema.document.slug })
        .returning({ id: schema.document.id });
      const row = rows[0];
      if (!row) continue;
      await registerResource(tx, { kind: 'document', id: row.id, ownerPartyId });
      await recordAudit(tx, {
        actorPersonId: me.personId,
        command: 'create-document',
        targetKind: 'document',
        targetId: row.id,
        payload: { owner: ownerPartyId, slug },
      });
      return row.id;
    }
    return null;
  });

  if (made === null) return { error: NO_SUCH };
  revalidatePath(DOCUMENTS);
  redirect(documentPath(made));
}

/** EditDocument. Editor or above. */
export async function editDocument(_prev: FormState, form: FormData): Promise<FormState> {
  const me = await actor();
  const id = tryDecodeId('document', field(form, 'document'));
  const parsed = editInput.safeParse({ title: field(form, 'title'), body: field(form, 'body') });
  if (id === null) return { error: NO_SUCH };
  if (!parsed.success) return { error: 'A title, and a body under 40,000 characters.' };

  const ok = await database().transaction(async (tx) => {
    if (!(await allows(tx, me, { on: 'document', id, need: 'editor' }))) return false;
    await tx
      .update(schema.document)
      .set({ title: parsed.data.title, body: parsed.data.body, updatedAt: sql`now()` })
      .where(eq(schema.document.id, id));
    await recordAudit(tx, {
      actorPersonId: me.personId,
      command: 'edit-document',
      targetKind: 'document',
      targetId: id,
    });
    return true;
  });

  if (!ok) return { error: NO_SUCH };
  revalidatePath(DOCUMENTS);
  return { notice: 'Saved.' };
}

/** PublishDocument, and the same command unpublishing it. A published
 *  document is readable by anybody at `/d/<slug>`; taking it back makes that
 *  URL decline exactly as one that never existed does. */
export async function setPublication(_prev: FormState, form: FormData): Promise<FormState> {
  const me = await actor();
  const id = tryDecodeId('document', field(form, 'document'));
  if (id === null) return { error: NO_SUCH };
  const publish = field(form, 'publish') === 'yes';

  const outcome = await database().transaction(async (tx) => {
    if (!(await allows(tx, me, { on: 'document', id, need: 'owner' }))) return null;
    const rows = await tx
      .update(schema.document)
      .set({ publishedAt: publish ? sql`now()` : null })
      .where(eq(schema.document.id, id))
      .returning({ slug: schema.document.slug });
    if (rows.length === 0) return null;
    await recordAudit(tx, {
      actorPersonId: me.personId,
      command: publish ? 'publish-document' : 'unpublish-document',
      targetKind: 'document',
      targetId: id,
    });
    return rows[0]!.slug;
  });

  if (outcome === null) return { error: NO_SUCH };
  revalidatePath(DOCUMENTS);
  if (outcome) revalidatePath(`/d/${outcome}`);
  return { notice: publish ? 'Published.' : 'Unpublished.' };
}

/** Share. A level to a person, a group or an organization. */
export async function shareDocument(_prev: FormState, form: FormData): Promise<FormState> {
  const me = await actor();
  const id = tryDecodeId('document', field(form, 'document'));
  const subject = subjectOf(field(form, 'subject'));
  const level = field(form, 'level') || 'viewer';
  if (id === null || subject === null || !isLevel(level)) return { error: NO_SUCH };

  const ok = await database().transaction(async (tx) => {
    if (!(await allows(tx, me, { on: 'document', id, need: 'owner' }))) return false;
    if (subject.kind === 'person') {
      const live = await tx
        .select({ id: schema.person.id })
        .from(schema.person)
        .where(and(eq(schema.person.id, subject.id), isNull(schema.person.mergedInto)))
        .limit(1);
      if (live.length === 0) return false;
    }
    await grant(tx, { subject, verb: level, on: 'document', id });
    await recordAudit(tx, {
      actorPersonId: me.personId,
      command: 'share',
      targetKind: 'document',
      targetId: id,
      payload: { subject: `${subject.kind}:${subject.id}`, level },
    });
    return true;
  });

  if (!ok) return { error: NO_SUCH };
  revalidatePath(DOCUMENTS);
  return { notice: 'Shared.' };
}

/** Revoke. */
export async function revokeShare(_prev: FormState, form: FormData): Promise<FormState> {
  const me = await actor();
  const id = tryDecodeId('document', field(form, 'document'));
  const subject = subjectOf(field(form, 'subject'));
  if (id === null || subject === null) return { error: NO_SUCH };

  const ok = await database().transaction(async (tx) => {
    if (!(await allows(tx, me, { on: 'document', id, need: 'owner' }))) return false;
    await revoke(tx, { subject, on: 'document', id });
    await recordAudit(tx, {
      actorPersonId: me.personId,
      command: 'revoke',
      targetKind: 'document',
      targetId: id,
      payload: { subject: `${subject.kind}:${subject.id}` },
    });
    return true;
  });

  if (!ok) return { error: NO_SUCH };
  revalidatePath(DOCUMENTS);
  return { notice: 'Revoked.' };
}

/** Transfer. The document changes hands: a person or an organization, and
 *  only its owner or a platform operator may say so. */
export async function transferDocument(_prev: FormState, form: FormData): Promise<FormState> {
  const me = await actor();
  const id = tryDecodeId('document', field(form, 'document'));
  const subject = subjectOf(field(form, 'subject'));
  if (id === null || subject === null || subject.kind === 'group') return { error: NO_SUCH };

  const ok = await database().transaction(async (tx) => {
    if (!(await allows(tx, me, { on: 'document', id, need: 'owner' }))) return false;
    const party = await tx
      .select({ id: schema.party.id })
      .from(schema.party)
      .where(and(eq(schema.party.id, subject.id), isNull(schema.party.disabledAt)))
      .limit(1);
    if (party.length === 0) return false;

    await tx
      .update(schema.document)
      .set({ ownerPartyId: subject.id })
      .where(eq(schema.document.id, id));
    await registerResource(tx, { kind: 'document', id, ownerPartyId: subject.id });
    await recordAudit(tx, {
      actorPersonId: me.personId,
      command: 'transfer',
      targetKind: 'document',
      targetId: id,
      payload: { to: `${subject.kind}:${subject.id}` },
    });
    return true;
  });

  if (!ok) return { error: NO_SUCH };
  revalidatePath(DOCUMENTS);
  return { notice: 'Handed over.' };
}
