import type { ZodType } from 'zod';
import {
  ConfiguredConflict,
  publish as sharedPublish,
  readDraft as sharedReadDraft,
  readPublished as sharedReadPublished,
  saveDraft as sharedSaveDraft,
  type ConfiguredRow,
  type ConfiguredState,
  type FieldDiff,
} from '@isoastra/fleet-configured';
import { withActorContext } from '@isoastra/fleet-events';

import { requestContext } from './context';
import type { Queryable, Transaction } from './events';

export { ConfiguredConflict, type ConfiguredRow, type ConfiguredState, type FieldDiff };
export const CONFIGURED_KIND = 'configured';

export async function saveDraft(
  tx: Transaction,
  orgId: string,
  key: string,
  body: unknown,
  expectedVersion: number,
): Promise<number> {
  const ctx = await requestContext();
  return withActorContext(ctx, () =>
    sharedSaveDraft(tx, orgId, key, body, expectedVersion, ctx.actorId),
  );
}

export async function publish(
  tx: Transaction,
  orgId: string,
  key: string,
  expectedDraftVersion: number,
): Promise<number> {
  const ctx = await requestContext();
  return withActorContext(ctx, () =>
    sharedPublish(tx, orgId, key, expectedDraftVersion, ctx.actorId),
  );
}

export async function readPublished<T>(
  db: Queryable,
  orgId: string,
  key: string,
  schema: ZodType<T>,
): Promise<T | null> {
  return sharedReadPublished(db, orgId, key, schema);
}

export async function readDraft(
  db: Queryable,
  orgId: string,
  key: string,
): Promise<{ version: number; body: unknown }> {
  const row = await sharedReadDraft(db, orgId, key);
  return { version: row.version, body: row.body };
}
