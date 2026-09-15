import 'server-only';

import {
  base64url,
  decryptProtectedJson,
  encryptProtectedJson,
  type ProtectedObjectEnvelopeV1,
  type ProtectionContext,
  type PrivacySchema,
  unbase64url,
  unwrapKey,
  wrapKey,
} from '@isoastra/privacy';
import { AwsKmsManagedKeyProvider } from '@isoastra/privacy-aws-kms';
import { RequestKeyCache, type ManagedKeyProvider } from '@isoastra/privacy/server';
import { sql } from 'drizzle-orm';

import type { Transaction } from '@/features/auth/db';
import { encodeId } from '@/lib/ids';

export const EVENT_PRIVACY_APPLICATION = 'ronitnath-events';
export const EVENT_CONTENT_SCHEMA = 'ronitnath/event-content/v1';

export interface EventContent {
  title: string;
  summary: string | null;
  startsAt: string;
  endsAt: string | null;
  location: string | null;
  address: string | null;
  timezone: string;
  body: string;
  capacity: number | null;
  colour: string | null;
  posterUrl: string | null;
  revealGuests: boolean;
}

export const EVENT_CONTENT_CLASSIFICATION = {
  title: 'protected',
  summary: 'protected',
  startsAt: 'protected',
  endsAt: 'protected',
  location: 'protected',
  address: 'protected',
  timezone: 'protected',
  body: 'protected',
  capacity: 'protected',
  colour: 'protected',
  posterUrl: 'protected',
  revealGuests: 'protected',
} as const satisfies PrivacySchema<EventContent>;

export interface EventRouting {
  id: number;
  hostPersonId: number;
  slug: string;
  sequence: number;
  privacyRevision: number;
  publishedAt: Date | null;
  updatedAt: Date;
  createdAt: Date;
}

type ScopeRow = { key_epoch: number; wrapped_root_key: Buffer | null };
type ObjectRow = { envelope: ProtectedObjectEnvelopeV1; storage_version: string | number };

class LocalManagedKeyProvider implements ManagedKeyProvider {
  readonly #key: Uint8Array;
  constructor(value: string) {
    this.#key = unbase64url(value);
    if (this.#key.byteLength !== 32) throw new Error('PRIVACY_LOCAL_KEK must contain 32 base64url bytes');
  }
  async createRootKey(reference: { ownerScope: string; epoch: number }) {
    const plaintextKey = crypto.getRandomValues(new Uint8Array(32));
    return {
      plaintextKey,
      wrappedKey: new TextEncoder().encode(
        JSON.stringify(await wrapKey(this.#key, plaintextKey, purpose(reference))),
      ),
    };
  }
  async unwrapRootKey(reference: { ownerScope: string; epoch: number }, value: Uint8Array) {
    return unwrapKey(
      this.#key,
      JSON.parse(new TextDecoder().decode(value)),
      purpose(reference),
    );
  }
  async rewrapRootKey(reference: { ownerScope: string; epoch: number }, value: Uint8Array) {
    const plaintextKey = await this.unwrapRootKey(reference, value);
    try {
      return new TextEncoder().encode(
        JSON.stringify(await wrapKey(this.#key, plaintextKey, purpose(reference))),
      );
    } finally {
      plaintextKey.fill(0);
    }
  }
}

function purpose(reference: { ownerScope: string; epoch: number }): string {
  return `ronitnath/local-root/${reference.ownerScope}/${reference.epoch}`;
}

const globalForPrivacy = globalThis as unknown as {
  rnEventKeyProvider?: ManagedKeyProvider;
};

function provider(): ManagedKeyProvider {
  if (globalForPrivacy.rnEventKeyProvider) return globalForPrivacy.rnEventKeyProvider;
  const keyId = process.env.PRIVACY_KMS_KEY_ID;
  if (keyId) {
    globalForPrivacy.rnEventKeyProvider = new AwsKmsManagedKeyProvider({
      keyId,
      applicationId: EVENT_PRIVACY_APPLICATION,
      clientConfig: { region: process.env.AWS_REGION ?? 'us-west-1' },
    });
    return globalForPrivacy.rnEventKeyProvider;
  }
  if (
    process.env.NODE_ENV === 'production' &&
    process.env.PRIVACY_ALLOW_LOCAL_KEK !== '1'
  ) {
    throw new Error('PRIVACY_KMS_KEY_ID is required in production');
  }
  const local = process.env.PRIVACY_LOCAL_KEK;
  if (!local) throw new Error('PRIVACY_LOCAL_KEK is required for protected event writes');
  globalForPrivacy.rnEventKeyProvider = new LocalManagedKeyProvider(local);
  return globalForPrivacy.rnEventKeyProvider;
}

export function ownerScope(personId: number): string {
  return encodeId('person', personId);
}

export function eventPrivacyWritesEnabled(): boolean {
  return process.env.EVENT_PRIVACY_WRITE === 'managed';
}

function objectId(eventId: number): string {
  return encodeId('event', eventId);
}

function context(row: EventRouting, epoch: number): ProtectionContext {
  return {
    application: EVENT_PRIVACY_APPLICATION,
    ownerScope: ownerScope(row.hostPersonId),
    objectId: objectId(row.id),
    schema: EVENT_CONTENT_SCHEMA,
    revision: String(row.privacyRevision),
    keyEpoch: epoch,
  };
}

async function selectScope(tx: Transaction, owner: string): Promise<ScopeRow | null> {
  await tx.execute(sql`select set_config('grid.authenticated_user_id', ${owner}, true)`);
  const result = await tx.execute<ScopeRow>(sql`
    select key_epoch, wrapped_root_key
    from personal_privacy_scope
    where application_id = ${EVENT_PRIVACY_APPLICATION} and owner_scope = ${owner}
    for update`);
  return result.rows[0] ?? null;
}

async function rootForWrite(tx: Transaction, owner: string): Promise<{ key: Uint8Array; epoch: number }> {
  const keys = provider();
  const existing = await selectScope(tx, owner);
  if (existing) {
    if (!existing.wrapped_root_key) throw new Error('managed scope has no wrapped root key');
    return {
      key: await keys.unwrapRootKey(
        { ownerScope: owner, epoch: existing.key_epoch },
        new Uint8Array(existing.wrapped_root_key),
      ),
      epoch: existing.key_epoch,
    };
  }

  const reference = { ownerScope: owner, epoch: 1 };
  const created = await keys.createRootKey(reference);
  try {
    await tx.execute(sql`
      insert into personal_privacy_scope
        (application_id, owner_scope, level, lifecycle, key_epoch, wrapped_root_key)
      values
        (${EVENT_PRIVACY_APPLICATION}, ${owner}, 'managed', 'protected', 1,
         ${Buffer.from(created.wrappedKey)})
      on conflict (application_id, owner_scope) do nothing`);
    const stored = await selectScope(tx, owner);
    if (!stored?.wrapped_root_key) throw new Error('managed scope was not created');
    if (Buffer.from(created.wrappedKey).equals(stored.wrapped_root_key)) {
      return { key: created.plaintextKey, epoch: 1 };
    }
    created.plaintextKey.fill(0);
    return {
      key: await keys.unwrapRootKey(
        { ownerScope: owner, epoch: stored.key_epoch },
        new Uint8Array(stored.wrapped_root_key),
      ),
      epoch: stored.key_epoch,
    };
  } catch (error) {
    created.plaintextKey.fill(0);
    throw error;
  }
}

export async function writeEventContent(
  tx: Transaction,
  row: EventRouting,
  value: EventContent,
  expectedStorageVersion: number,
): Promise<void> {
  if (row.privacyRevision < 1) throw new Error('protected event revisions start at one');
  const owner = ownerScope(row.hostPersonId);
  const root = await rootForWrite(tx, owner);
  try {
    const envelope = await encryptProtectedJson({
      rootKey: root.key,
      value,
      context: context(row, root.epoch),
      level: 'managed',
    });
    const encoded = JSON.stringify(envelope);
    const result = await tx.execute<{ storage_version: string | number }>(sql`
      insert into personal_protected_object
        (application_id, owner_scope, object_id, schema_id, object_revision,
         key_epoch, storage_version, envelope, equality_indexes, byte_length)
      values
        (${EVENT_PRIVACY_APPLICATION}, ${owner}, ${objectId(row.id)},
         ${EVENT_CONTENT_SCHEMA}, ${String(row.privacyRevision)}, ${root.epoch},
         1, ${encoded}::jsonb, '{}'::jsonb, ${Buffer.byteLength(encoded)})
      on conflict (application_id, owner_scope, object_id) do update set
        schema_id = excluded.schema_id,
        object_revision = excluded.object_revision,
        key_epoch = excluded.key_epoch,
        storage_version = personal_protected_object.storage_version + 1,
        envelope = excluded.envelope,
        equality_indexes = excluded.equality_indexes,
        byte_length = excluded.byte_length,
        updated_at = now()
      where personal_protected_object.storage_version = ${expectedStorageVersion}
      returning storage_version`);
    if (!result.rows[0]) throw new Error('stale protected event write');
  } finally {
    root.key.fill(0);
  }
}

export async function readEventContent(
  tx: Transaction,
  row: EventRouting,
  cache?: RequestKeyCache,
): Promise<EventContent | null> {
  if (row.privacyRevision === 0) return null;
  const owner = ownerScope(row.hostPersonId);
  const scope = await selectScope(tx, owner);
  if (!scope?.wrapped_root_key) throw new Error('protected event root key is unavailable');
  const result = await tx.execute<ObjectRow>(sql`
    select envelope, storage_version
    from personal_protected_object
    where application_id = ${EVENT_PRIVACY_APPLICATION}
      and owner_scope = ${owner}
      and object_id = ${objectId(row.id)}`);
  const stored = result.rows[0];
  if (!stored) throw new Error('protected event content is missing');
  const expected = context(row, scope.key_epoch);
  const reference = { ownerScope: owner, epoch: scope.key_epoch };
  const root = cache
    ? await cache.get(reference, () =>
        provider().unwrapRootKey(reference, new Uint8Array(scope.wrapped_root_key!)),
      )
    : await provider().unwrapRootKey(reference, new Uint8Array(scope.wrapped_root_key));
  try {
    return await decryptProtectedJson<EventContent>({
      rootKey: root,
      envelope: stored.envelope,
      expected,
      level: 'managed',
    });
  } finally {
    root.fill(0);
  }
}

export function eventContentFromLegacy(row: {
  title: string | null;
  summary: string | null;
  startsAt: Date | null;
  endsAt: Date | null;
  location: string | null;
  address: string | null;
  timezone: string | null;
  body: string | null;
  capacity: number | null;
  colour: string | null;
  posterUrl: string | null;
  revealGuests: boolean | null;
}): EventContent {
  if (!row.title || !row.startsAt || !row.timezone || row.body === null || row.revealGuests === null) {
    throw new Error('legacy event content is incomplete');
  }
  return {
    title: row.title,
    summary: row.summary,
    startsAt: row.startsAt.toISOString(),
    endsAt: row.endsAt?.toISOString() ?? null,
    location: row.location,
    address: row.address,
    timezone: row.timezone,
    body: row.body,
    capacity: row.capacity,
    colour: row.colour,
    posterUrl: row.posterUrl,
    revealGuests: row.revealGuests,
  };
}

export function materializeEvent(row: EventRouting, content: EventContent) {
  return {
    ...row,
    title: content.title,
    summary: content.summary,
    startsAt: new Date(content.startsAt),
    endsAt: content.endsAt ? new Date(content.endsAt) : null,
    location: content.location,
    address: content.address,
    timezone: content.timezone,
    body: content.body,
    capacity: content.capacity,
    colour: content.colour,
    posterUrl: content.posterUrl,
    revealGuests: content.revealGuests,
  };
}

export function localKekForTests(): string {
  return base64url(new Uint8Array(32).fill(7));
}
