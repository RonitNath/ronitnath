/** Reading the assets the site actually ships, by the names it ships them under.
 *
 * Every one is content-addressed, so no test may name a file: the generated
 * `asset-names.ts` is the only thing that knows what `bright.bin` is called
 * this build, and a test that hard-coded the name would break on every
 * rebuild for no reason worth reading.
 */
import { readFileSync } from 'node:fs';

import { SKY_ASSETS } from '../../asset-names';

/** The bytes behind one of the site's own asset URLs. */
export function shipped(url: string): Buffer {
  return readFileSync(`public${url}`);
}

export function shippedText(url: string): string {
  return readFileSync(`public${url}`, 'utf8');
}

/** A file beside the LOD manifest — a tile, or `g9`. */
export function shippedLod(file: string): Buffer {
  return readFileSync(`public${SKY_ASSETS.lodManifest.replace(/[^/]+$/, '')}${file}`);
}
