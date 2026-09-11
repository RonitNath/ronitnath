/* Public ids. Internal ids are integers and never leave the server; what a
 * page, a URL or an API response carries is a type prefix and the base64url
 * of one AES-128 block over that integer.
 *
 * The block is a tag derived from the type plus the id itself, so decoding
 * with the wrong prefix, or after a single flipped character, fails on the tag
 * rather than returning some other row's id. Encryption is deterministic (one
 * block, no IV) because a public id has to be stable and comparable. */

import { aesBlockCodec, PublicIdError } from '@isoastra/fleet-ids';

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
  /* A picture on an event page. `p` is the person's, so a photograph takes
   * the other letter in the word. It reaches a URL — the route that serves
   * the bytes and the button that hides one — so it gets a prefix of its
   * own rather than travelling as a row number. */
  photo: 'h',
  /* A private voice memo. */
  memo: 'v',
  /* An audit row. The operator's Split names the merge it is undoing, and the
   * only name a merge has is the row that recorded it (R6). */
  audit: 'a',
} as const;

export type IdType = keyof typeof ID_TYPES;

export { PublicIdError };

/* Rotation. A public id is printed, bookmarked and mailed, so the key that
 * made it has to keep decoding after the key that makes new ones has changed:
 * `ID_KEY_PREV` is read only on the way in, and `encodeId` never looks at it.
 * The list is ordered — current first — so a live key does no extra work and
 * only an id that fails under it pays for the second attempt.
 *
 * A malformed `ID_KEY_PREV` is not an error: an operator who half-set it would
 * otherwise take the whole site down over ids that the current key still
 * decodes. It is simply not a key we can try. */
function codec() {
  const keyPrev = process.env.ID_KEY_PREV;
  return aesBlockCodec({
    key: process.env.ID_KEY ?? '',
    namespace: 'rn',
    ...(keyPrev ? { keyPrev } : {}),
  });
}

export function encodeId(type: IdType, id: number): string {
  return codec().encode(ID_TYPES[type], id);
}

export function decodeId(type: IdType, publicId: string): number {
  return codec().decode(ID_TYPES[type], publicId);
}

/* For request paths, where a bad id is a 404 and not an exception. */
export function tryDecodeId(type: IdType, publicId: string): number | null {
  try {
    return decodeId(type, publicId);
  } catch {
    return null;
  }
}
