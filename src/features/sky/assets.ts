/** Fetching the sky's four assets, each validated before anything draws it.
 *
 * Every one of these lands after first paint and none of them is required for
 * the page to be a page: a refused fetch or a corrupt file leaves the CSS
 * starfield as the picture and the rest of the document intact.
 */

import { type NamedCatalog, parseNamed, parseStars, type StarCatalog } from './catalog';
import { CityCatalog } from './cities';
import { type LinePairs, parseLines } from './lines';

/** The band's two bakes, named by the content hash `tools/starcat/mwcat.py`
 * writes into them.
 *
 * The hash is in the filename on purpose: the map is served with a long
 * immutable cache and sits behind a CDN, so a re-bake at the same path would
 * be invisible to everyone who already has one until the cache expired. A new
 * bake is a new URL, and nothing has to be purged anywhere.
 */
export const ASSETS = {
  bright: '/stars/bright.bin',
  named: '/stars/named.json',
  lines: '/sky/lines.bin',
  milkyway: '/sky/milkyway-81ee6f522371.webp',
  milkyway2k: '/sky/milkyway-2k-81ee6f522371.webp',
  cities: '/cities/cities.bin',
  earthDay: '/textures/earth/day.jpg',
  earthNormal: '/textures/earth/normal.jpg',
  earthSpecular: '/textures/earth/specular.jpg',
} as const;

/** Above this many device pixels across, the frame can show the 4096-wide
 * bake; below it the 2048 one carries every texel the screen has. A 390 px
 * phone at 3x is 1,170 device pixels, and the half-sized map is 145 KB rather
 * than 315 KB for a picture indistinguishable on it. */
const WIDE_DEVICE_PX = 1_600;

/** Which bake this device should fetch. */
export function bandUrl(widthCss: number, dpr: number): string {
  return widthCss * dpr >= WIDE_DEVICE_PX ? ASSETS.milkyway : ASSETS.milkyway2k;
}

async function bytes(url: string): Promise<Uint8Array> {
  const response = await fetch(url);
  if (!response.ok) throw new Error(`${url}: ${response.status}`);
  return new Uint8Array(await response.arrayBuffer());
}

export async function loadStars(): Promise<StarCatalog> {
  return parseStars(await bytes(ASSETS.bright));
}

export async function loadNamed(): Promise<NamedCatalog> {
  const response = await fetch(ASSETS.named);
  if (!response.ok) throw new Error(`${ASSETS.named}: ${response.status}`);
  return parseNamed(await response.text());
}

export async function loadLines(): Promise<LinePairs> {
  return parseLines(await bytes(ASSETS.lines));
}

export async function loadCities(): Promise<CityCatalog> {
  const catalog = CityCatalog.parse(await bytes(ASSETS.cities));
  if (!catalog) throw new Error('invalid city catalog');
  return catalog;
}

/** An image decoded to pixels, which is the only form the 2D warp can sample
 * an equirectangular map in. */
export async function loadImageData(url: string, maxWidth: number): Promise<ImageData> {
  const response = await fetch(url);
  if (!response.ok) throw new Error(`${url}: ${response.status}`);
  const bitmap = await createImageBitmap(await response.blob());
  const width = Math.min(bitmap.width, maxWidth);
  const height = Math.max(1, Math.round((bitmap.height * width) / bitmap.width));
  const canvas = document.createElement('canvas');
  canvas.width = width;
  canvas.height = height;
  const ctx = canvas.getContext('2d', { willReadFrequently: true });
  if (!ctx) throw new Error('no 2d context to decode into');
  ctx.drawImage(bitmap, 0, 0, width, height);
  bitmap.close();
  return ctx.getImageData(0, 0, width, height);
}
