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
  resource: 'r',
  session: 's',
  link: 'l',
  event: 'e',
  /* A match candidate. It is a row in the model like any other and it reaches
   * a form on /app, so it gets a prefix rather than borrowing one. */
  match: 'm',
} as const;

export type IdType = keyof typeof ID_TYPES;

export class PublicIdError extends Error {}

const TAG_BYTES = 8;
const BLOCK_BYTES = 16;

function key(): Buffer {
  const hex = process.env.ID_KEY;
  if (!hex || !/^[0-9a-fA-F]{32}$/.test(hex)) {
    throw new PublicIdError('ID_KEY must be exactly 32 hex characters');
  }
  return Buffer.from(hex, 'hex');
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

  const k = key();
  const decipher = createDecipheriv('aes-128-ecb', k, null);
  decipher.setAutoPadding(false);
  const block = Buffer.concat([decipher.update(bytes), decipher.final()]);

  if (!timingSafeEqual(block.subarray(0, TAG_BYTES), tag(type, k))) {
    throw new PublicIdError('wrong id type');
  }
  const id = block.readBigUInt64BE(TAG_BYTES);
  if (id <= 0n || id > BigInt(Number.MAX_SAFE_INTEGER)) throw new PublicIdError('malformed id');
  return Number(id);
}

/* For request paths, where a bad id is a 404 and not an exception. */
export function tryDecodeId(type: IdType, publicId: string): number | null {
  try {
    return decodeId(type, publicId);
  } catch {
    return null;
  }
}
