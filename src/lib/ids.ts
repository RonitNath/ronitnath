/* Public ids. Internal ids are integers and never leave the server; what a
 * page, a URL or an API response carries is a type prefix and the base64url
 * of one AES-128 block over that integer.
 *
 * The block is a tag derived from the type plus the id itself, so decoding
 * with the wrong prefix, or after a single flipped character, fails on the tag
 * rather than returning some other row's id. Encryption is deterministic (one
 * block, no IV) because a public id has to be stable and comparable. */

import { createCipheriv, createDecipheriv, createHash, timingSafeEqual } from 'node:crypto';

export const ID_TYPES = {
  person: 'p',
  identity: 'i',
  organization: 'o',
  group: 'g',
  /* `r` is the document's, not the resource registry's: the registry row is
   * bookkeeping that never reaches a URL, and a document is the only resource
   * a member names by hand. */
  document: 'r',
  session: 's',
  link: 'l',
  event: 'e',
  /* A match candidate. It is a row in the model like any other and it reaches
   * a form on /app, so it gets a prefix rather than borrowing one. */
  match: 'm',
  /* An audit row. The operator's Split names the merge it is undoing, and the
   * only name a merge has is the row that recorded it (R6). */
  audit: 'a',
} as const;

export type IdType = keyof typeof ID_TYPES;

export class PublicIdError extends Error {}

const TAG_BYTES = 8;
const BLOCK_BYTES = 16;

function keyFrom(hex: string | undefined, name: string): Buffer {
  if (!hex || !/^[0-9a-fA-F]{32}$/.test(hex)) {
    throw new PublicIdError(`${name} must be exactly 32 hex characters`);
  }
  return Buffer.from(hex, 'hex');
}

function key(): Buffer {
  return keyFrom(process.env.ID_KEY, 'ID_KEY');
}

/* Rotation. A public id is printed, bookmarked and mailed, so the key that
 * made it has to keep decoding after the key that makes new ones has changed:
 * `ID_KEY_PREV` is read only on the way in, and `encodeId` never looks at it.
 * The list is ordered — current first — so a live key does no extra work and
 * only an id that fails under it pays for the second attempt.
 *
 * A malformed `ID_KEY_PREV` is not an error: an operator who half-set it would
 * otherwise take the whole site down over ids that the current key still
 * decodes. It is simply not a key we can try. */
function decodeKeys(): Buffer[] {
  const keys = [key()];
  const prev = process.env.ID_KEY_PREV;
  if (prev && /^[0-9a-fA-F]{32}$/.test(prev)) keys.push(Buffer.from(prev, 'hex'));
  return keys;
}

function tag(type: IdType, k: Buffer): Buffer {
  return createHash('sha256').update(k).update(`rn:${type}`).digest().subarray(0, TAG_BYTES);
}

export function encodeId(type: IdType, id: number): string {
  if (!Number.isSafeInteger(id) || id <= 0) {
    throw new PublicIdError(`not an internal id: ${id}`);
  }
  const k = key();
  const block = Buffer.alloc(BLOCK_BYTES);
  tag(type, k).copy(block, 0);
  block.writeBigUInt64BE(BigInt(id), TAG_BYTES);

  const cipher = createCipheriv('aes-128-ecb', k, null);
  cipher.setAutoPadding(false);
  const out = Buffer.concat([cipher.update(block), cipher.final()]);
  return `${ID_TYPES[type]}_${out.toString('base64url')}`;
}

export function decodeId(type: IdType, publicId: string): number {
  const prefix = `${ID_TYPES[type]}_`;
  if (!publicId.startsWith(prefix)) throw new PublicIdError('wrong id type');

  const body = publicId.slice(prefix.length);
  if (!/^[A-Za-z0-9_-]{22}$/.test(body)) throw new PublicIdError('malformed id');
  const bytes = Buffer.from(body, 'base64url');
  if (bytes.length !== BLOCK_BYTES) throw new PublicIdError('malformed id');

  for (const k of decodeKeys()) {
    const decipher = createDecipheriv('aes-128-ecb', k, null);
    decipher.setAutoPadding(false);
    const block = Buffer.concat([decipher.update(bytes), decipher.final()]);

    if (!timingSafeEqual(block.subarray(0, TAG_BYTES), tag(type, k))) continue;
    const id = block.readBigUInt64BE(TAG_BYTES);
    if (id <= 0n || id > BigInt(Number.MAX_SAFE_INTEGER)) throw new PublicIdError('malformed id');
    return Number(id);
  }
  throw new PublicIdError('wrong id type');
}

/* For request paths, where a bad id is a 404 and not an exception. */
export function tryDecodeId(type: IdType, publicId: string): number | null {
  try {
    return decodeId(type, publicId);
  } catch {
    return null;
  }
}
