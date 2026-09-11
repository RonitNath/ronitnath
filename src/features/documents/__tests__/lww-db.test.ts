import { randomUUID } from 'node:crypto';
import { eq } from 'drizzle-orm';
import { afterAll, describe, expect, it } from 'vitest';

import { schema } from '@/db/client';
import { closeDatabase, database, reachable } from '@/lib/fleet/__tests__/db';
import { snapshotDocument, writeDocumentDraft } from '../lww';

afterAll(closeDatabase);

describe.skipIf(!reachable)('document LWW and publication', () => {
  it('merges different fields, orders same-field writes, deduplicates retries, and freezes publication', async () => {
    const db = database();
    const personId = await db.transaction(async (tx) => {
      const [party] = await tx
        .insert(schema.party)
        .values({ kind: 'person' })
        .returning({ id: schema.party.id });
      await tx.insert(schema.person).values({ id: party!.id, displayName: 'Realtime test' });
      return party!.id;
    });
    const [made] = await db
      .insert(schema.document)
      .values({
        ownerPartyId: personId,
        title: 'Alpha',
        body: 'One',
        slug: `lww-${randomUUID()}`,
      })
      .returning({ id: schema.document.id });
    const documentId = made!.id;
    const titleMutation = randomUUID();
    const bodyMutation = randomUUID();

    await db.transaction((tx) =>
      writeDocumentDraft(tx, {
        documentId,
        actorPersonId: personId,
        mutationId: titleMutation,
        title: 'Beta',
        body: 'One',
        originalTitle: 'Alpha',
        originalBody: 'One',
      }),
    );
    await db.transaction((tx) =>
      writeDocumentDraft(tx, {
        documentId,
        actorPersonId: personId,
        mutationId: bodyMutation,
        title: 'Alpha',
        body: 'Two',
        originalTitle: 'Alpha',
        originalBody: 'One',
      }),
    );
    await db.transaction((tx) =>
      writeDocumentDraft(tx, {
        documentId,
        actorPersonId: personId,
        mutationId: randomUUID(),
        title: 'Gamma',
        body: 'One',
        originalTitle: 'Alpha',
        originalBody: 'One',
      }),
    );
    await db.transaction((tx) =>
      writeDocumentDraft(tx, {
        documentId,
        actorPersonId: personId,
        mutationId: bodyMutation,
        title: 'Alpha',
        body: 'Two',
        originalTitle: 'Alpha',
        originalBody: 'One',
      }),
    );

    let [row] = await db
      .select()
      .from(schema.document)
      .where(eq(schema.document.id, documentId));
    expect(row).toMatchObject({
      title: 'Gamma',
      body: 'Two',
      draftVersion: 3,
      titleVersion: 3,
      bodyVersion: 2,
    });
    expect(
      await db
        .select()
        .from(schema.documentRevision)
        .where(eq(schema.documentRevision.documentId, documentId)),
    ).toHaveLength(3);

    await db.transaction((tx) =>
      snapshotDocument(tx, {
        documentId,
        actorPersonId: personId,
        mutationId: randomUUID(),
        publish: true,
      }),
    );
    await db.transaction((tx) =>
      writeDocumentDraft(tx, {
        documentId,
        actorPersonId: personId,
        mutationId: randomUUID(),
        title: 'Delta',
        body: 'Two',
        originalTitle: 'Gamma',
        originalBody: 'Two',
      }),
    );
    [row] = await db.select().from(schema.document).where(eq(schema.document.id, documentId));
    expect(row).toMatchObject({
      title: 'Delta',
      publishedTitle: 'Gamma',
      publishedBody: 'Two',
      publishedVersion: 3,
    });
  });
});
