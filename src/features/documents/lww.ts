import { and, eq, sql } from 'drizzle-orm';
import { schema } from '@/db/client';
import type { Transaction } from '@/lib/fleet/events';

export interface DraftWrite {
  documentId: number;
  mutationId: string;
  actorPersonId: number;
  title: string;
  body: string;
  originalTitle: string;
  originalBody: string;
}

export async function writeDocumentDraft(tx: Transaction, input: DraftWrite) {
  const duplicate = await tx
    .select({ version: schema.documentRevision.version })
    .from(schema.documentRevision)
    .where(
      and(
        eq(schema.documentRevision.documentId, input.documentId),
        eq(schema.documentRevision.mutationId, input.mutationId),
      ),
    )
    .limit(1);
  if (duplicate[0]) return { changed: false, version: duplicate[0].version };
  const rows = await tx
    .select()
    .from(schema.document)
    .where(eq(schema.document.id, input.documentId))
    .limit(1)
    .for('update');
  const current = rows[0];
  if (!current) return null;
  const fields = [
    input.title !== input.originalTitle ? 'title' : null,
    input.body !== input.originalBody ? 'body' : null,
  ].filter((name): name is string => name !== null);
  if (!fields.length) return { changed: false, version: current.draftVersion };
  const title = fields.includes('title') ? input.title : current.title;
  const body = fields.includes('body') ? input.body : current.body;
  const version = current.draftVersion + 1;
  await tx
    .update(schema.document)
    .set({
      title,
      body,
      draftVersion: version,
      ...(fields.includes('title') ? { titleVersion: version } : {}),
      ...(fields.includes('body') ? { bodyVersion: version } : {}),
      updatedAt: sql`now()`,
    })
    .where(eq(schema.document.id, input.documentId));
  await tx
    .insert(schema.documentRevision)
    .values({
      documentId: input.documentId,
      version,
      mutationId: input.mutationId,
      kind: 'draft',
      fields,
      title,
      body,
      previous: {
        title: current.title,
        body: current.body,
        titleVersion: current.titleVersion,
        bodyVersion: current.bodyVersion,
      },
      actorPersonId: input.actorPersonId,
    });
  return { changed: true, version };
}

export async function snapshotDocument(
  tx: Transaction,
  input: { documentId: number; mutationId: string; actorPersonId: number; publish: boolean },
) {
  const rows = await tx
    .select()
    .from(schema.document)
    .where(eq(schema.document.id, input.documentId))
    .limit(1)
    .for('update');
  const current = rows[0];
  if (!current) return null;
  const duplicate = await tx
    .select({ id: schema.documentRevision.id })
    .from(schema.documentRevision)
    .where(
      and(
        eq(schema.documentRevision.documentId, input.documentId),
        eq(schema.documentRevision.mutationId, input.mutationId),
      ),
    )
    .limit(1);
  if (duplicate.length) return { changed: false, slug: current.slug };
  await tx
    .update(schema.document)
    .set(
      input.publish
        ? {
            publishedAt: sql`now()`,
            publishedTitle: current.title,
            publishedBody: current.body,
            publishedVersion: current.draftVersion,
          }
        : { publishedAt: null },
    )
    .where(eq(schema.document.id, input.documentId));
  await tx
    .insert(schema.documentRevision)
    .values({
      documentId: input.documentId,
      version: current.draftVersion,
      mutationId: input.mutationId,
      kind: input.publish ? 'published' : 'unpublished',
      fields: [],
      title: current.title,
      body: current.body,
      actorPersonId: input.actorPersonId,
    });
  return { changed: true, slug: current.slug };
}
