/** The grounding readout: where on Earth the viewer is looking out from.
 *
 * Position first, place second. The coordinates tick continuously and are what
 * makes the line read as live; the city is what makes them mean something. The
 * copy states only what the lookup actually knows — a name and a distance — and
 * never guesses at terrain.
 */

import type { City } from './cities';
import type { NamedStar } from './catalog';

/** Under this distance the observer counts as directly over the city.
 * GeoNames `cities15000` entries are places of 15,000 people or more, whose
 * built-up extent plus the ~28 km of ground track between samples fits inside
 * 50 km without reaching into the next city over. */
export const OVER_THRESHOLD_KM = 50;

/** Wall-clock milliseconds between re-samples. At 60x, half a second of real
 * time is 30 simulated seconds — about 28 km of track, which lands in the
 * second decimal of a degree: the digits visibly move without becoming a
 * blur. */
export const UPDATE_INTERVAL_MS = 500;

/** Signed degrees are how the code carries a position, not how anyone reads
 * one. Two decimals is about a kilometre. */
export function positionText(latDeg: number, lonDeg: number): string {
  // Round before choosing the hemisphere, so -0.001 prints as "0.00° N"
  // rather than the jarring "0.00° S".
  const lat = Math.round(latDeg * 100) / 100;
  const lon = Math.round(lonDeg * 100) / 100;
  const ns = lat < 0 ? 'S' : 'N';
  const ew = lon < 0 ? 'W' : 'E';
  return `${Math.abs(lat).toFixed(2)}° ${ns}, ${Math.abs(lon).toFixed(2)}° ${ew}`;
}

/** Thousands-separated and rounded: `1403.8821` is not something a human reads
 * at a glance. */
function km(distance: number): string {
  return Math.max(0, Math.round(distance)).toLocaleString('en-US');
}

export function placeText(city: City): string {
  if (city.distanceKm < OVER_THRESHOLD_KM) return `over ${city.name}, ${city.country}`;
  return `${km(city.distanceKm)} km from ${city.name}, ${city.country}`;
}

export function grounding(latDeg: number, lonDeg: number, city: City | null): string {
  const position = positionText(latDeg, lonDeg);
  return city ? `${position} · ${placeText(city)}` : position;
}

/** The second line of a callout: constellation, spectral class, distance. */
export function starDetail(star: NamedStar): string {
  const distance = Math.max(1, Math.round(star.distanceLy));
  return `${star.constellation} · ${star.classification} · ${distance} ly`;
}
