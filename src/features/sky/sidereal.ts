/** Sidereal time and the observer's horizon frame.
 *
 * The sky on the landing page is not a decoration that happens to turn: it is
 * the real celestial sphere seen from the visitor's own latitude and
 * longitude, turning at the rate the Earth actually turns. Everything the
 * canvas needs is here and is pure — given a unix timestamp and a place, the
 * frame is fully determined, which is what makes it testable without a DOM.
 *
 * Accuracy: the GMST series below is the low-precision one (arcminute over a
 * century). A background sky is drawn at ~1px per 6 arcminutes at these
 * scales, so precession and nutation are deliberately not modelled — the
 * catalog is a seeded one anyway, and adding them would buy nothing visible.
 */

/** Unix ms at J2000.0 — 2000-01-01T12:00:00Z, the epoch of the GMST series. */
export const J2000_UNIX_MS = 946_728_000_000;

/** Radians of GMST per day; the rotation rate of the series below. */
const GMST_RAD_PER_DAY = 6.300_388_098_984_89;

const TAU = Math.PI * 2;

/** One sidereal day in ms, derived from that rate rather than quoted, so the
 * two can never drift apart (it lands on the textbook 23h56m04.09s). */
export const SIDEREAL_DAY_MS = (86_400_000 * TAU) / GMST_RAD_PER_DAY;

const DEG = Math.PI / 180;

/** Greenwich mean sidereal time, radians in [0, 2π). */
export function gmstRad(unixMs: number): number {
  const daysSinceJ2000 = unixMs / 86_400_000 - 10_957.5;
  const gmst = 4.894_961_212_823_058 + GMST_RAD_PER_DAY * daysSinceJ2000;
  return wrapTau(gmst);
}

/** Local mean sidereal time: the right ascension currently on the meridian. */
export function lstRad(unixMs: number, lonDeg: number): number {
  return wrapTau(gmstRad(unixMs) + lonDeg * DEG);
}

function wrapTau(radians: number): number {
  return ((radians % TAU) + TAU) % TAU;
}

/** Where a star is in the sky for an observer: altitude above the horizon and
 * azimuth measured from north, increasing eastward. Both radians. */
export interface AltAz {
  alt: number;
  az: number;
}

/**
 * Equatorial (right ascension, declination) to horizon (altitude, azimuth).
 *
 * Written as a change of basis rather than the usual atan2 identity: the three
 * components are the star's direction along the observer's north, east and
 * zenith axes, so the azimuth quadrant falls out of atan2 with no special
 * cases at the poles or the meridian.
 */
export function altAz(raRad: number, decRad: number, lst: number, latRad: number): AltAz {
  const hourAngle = lst - raRad;
  const cosDec = Math.cos(decRad);
  const sinDec = Math.sin(decRad);
  const cosH = Math.cos(hourAngle);
  const sinH = Math.sin(hourAngle);
  const cosLat = Math.cos(latRad);
  const sinLat = Math.sin(latRad);

  const north = sinDec * cosLat - cosH * cosDec * sinLat;
  const east = -sinH * cosDec;
  const up = cosH * cosDec * cosLat + sinDec * sinLat;

  return { alt: Math.asin(clamp(up, -1, 1)), az: wrapTau(Math.atan2(east, north)) };
}

function clamp(value: number, lo: number, hi: number): number {
  return value < lo ? lo : value > hi ? hi : value;
}
