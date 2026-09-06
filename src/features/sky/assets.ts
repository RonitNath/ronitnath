/** Fetching the sky's four assets, each validated before anything draws it.
 *
 * Every one of these lands after first paint and none of them is required for
 * the page to be a page: a refused fetch or a corrupt file leaves the CSS
 * starfield as the picture and the rest of the document intact.
 */

import { type NamedCatalog, parseNamed, parseStars, type StarCatalog } from './catalog';
import { CityCatalog } from './cities';

export const ASSETS = {
  bright: '/stars/bright.bin',
  named: '/stars/named.json',
  milkyway: '/sky/milkyway.webp',
  cities: '/cities/cities.bin',
  earthDay: '/textures/earth/day.jpg',
  earthNormal: '/textures/earth/normal.jpg',
  earthSpecular: '/textures/earth/specular.jpg',
} as const;

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
