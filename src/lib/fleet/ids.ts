/* The fleet's id seam. Every app in the fleet vendors `lib/fleet/ids.ts` with
 * the same exports so that shared code can encode and decode public ids
 * without knowing which app it is running in; what differs between apps is the
 * codec underneath (this one is an AES-tagged block over an integer, dentconnex
 * hands back a raw cuid, rinity base64url uuid bytes).
 *
 * Here the codec already exists and is used by half the app under its own
 * name, so this module is a re-export and nothing else. Two implementations of
 * one id format is how a site starts handing out ids it cannot read back:
 * do not copy the codec down here, and keep importing `@/lib/ids` wherever the
 * app is already talking to itself rather than to fleet code.
 *
 * Rotation lives in the codec: `ID_KEY` makes ids, `ID_KEY` and `ID_KEY_PREV`
 * both read them (contracts.md §ids). */

export {
  ID_TYPES,
  PublicIdError,
  decodeId,
  encodeId,
  tryDecodeId,
  type IdType,
} from '@/lib/ids';
