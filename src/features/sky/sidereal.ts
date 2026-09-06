/** Where the real sky is, at an instant, from a place on Earth.
 *
 * The catalog holds J2000/ICRS unit vectors. Getting one onto the screen is
 * two rotations: precession from J2000 to the mean equinox of date, then the
 * horizon transform for the observer's latitude and local sidereal time.
 * Earth's rotation is composed exactly once, here — the observer track
 * deliberately returns Earth-*fixed* coordinates so it cannot double-count it.
 */

const TAU = Math.PI * 2;
const DEG = Math.PI / 180;

/** Unix ms at J2000.0 — 2000-01-01T12:00:00Z, the epoch of the GMST series. */
export const J2000_UNIX_MS = 946_728_000_000;

/** Radians of GMST per day; the rotation rate of the series below. */
const GMST_RAD_PER_DAY = 6.300_388_098_984_89;

/** One sidereal day in ms, derived from that rate rather than quoted, so the
 * clock's resync lands on an exact GMST period. */
export const SIDEREAL_DAY_MS = (86_400_000 * TAU) / GMST_RAD_PER_DAY;

/** Focal length of the zenith-looking projection. */
export const FOCAL = 0.8391;

/** Where the track starts, and where the sky is drawn from when nothing else
 * has been chosen. */
export const SF_LAT_DEG = 37.7749;
export const SF_LON_DEG = -122.4194;

export type Mat3 = readonly number[];
export type Vec3 = readonly [number, number, number];

function wrapTau(radians: number): number {
  return ((radians % TAU) + TAU) % TAU;
}

/** Greenwich mean sidereal time, radians in [0, 2π). Low-precision series:
 * arcminute accuracy is orders of magnitude beyond what a background sky
 * needs. */
export function gmstRad(unixMs: number): number {
  const daysSinceJ2000 = unixMs / 86_400_000 - 10_957.5;
  return wrapTau(4.894_961_212_823_058 + GMST_RAD_PER_DAY * daysSinceJ2000);
}

/** Local mean sidereal time: the right ascension currently on the meridian. */
export function lstRad(unixMs: number, lonDeg: number): number {
  return wrapTau(gmstRad(unixMs) + lonDeg * DEG);
}

export function normalizeLonDeg(lon: number): number {
  return ((((lon + 180) % 360) + 360) % 360) - 180;
}

/** Unit vector for a latitude/longitude, in the frame the track, the globe and
 * the star catalog all share. */
export function unitVector(latDeg: number, lonDeg: number): Vec3 {
  const lat = latDeg * DEG;
  const lon = lonDeg * DEG;
  return [Math.cos(lat) * Math.cos(lon), Math.cos(lat) * Math.sin(lon), Math.sin(lat)];
}

/** The inverse of {@link unitVector}, longitude normalised to [-180, 180). */
export function latLon(v: Vec3): [number, number] {
  const z = v[2] < -1 ? -1 : v[2] > 1 ? 1 : v[2];
  return [Math.asin(z) / DEG, normalizeLonDeg(Math.atan2(v[1], v[0]) / DEG)];
}

function matMul(a: number[][], b: number[][]): number[][] {
  const out: number[][] = [
    [0, 0, 0],
    [0, 0, 0],
    [0, 0, 0],
  ];
  for (let row = 0; row < 3; row += 1) {
    for (let col = 0; col < 3; col += 1) {
      let sum = 0;
      for (let k = 0; k < 3; k += 1) sum += a[row]![k]! * b[k]![col]!;
      out[row]![col] = sum;
    }
  }
  return out;
}

/** IAU 1976/Lieske rotation from J2000 to the mean equator and equinox of
 * date. `t` is Julian centuries TT from J2000; the axis matrices use the
 * passive convention, so this is exactly `Rz(-z) . Ry(theta) . Rz(-zeta)`. */
export function precessionMatrix(t: number): number[][] {
  const arcsec = Math.PI / (180 * 3_600);
  const zeta = (2_306.218_1 * t + 0.301_88 * t ** 2 + 0.017_998 * t ** 3) * arcsec;
  const z = (2_306.218_1 * t + 1.094_68 * t ** 2 + 0.018_203 * t ** 3) * arcsec;
  const theta = (2_004.310_9 * t - 0.426_65 * t ** 2 - 0.041_833 * t ** 3) * arcsec;

  const rz = (angle: number): number[][] => {
    const [s, c] = [Math.sin(angle), Math.cos(angle)];
    return [
      [c, s, 0],
      [-s, c, 0],
      [0, 0, 1],
    ];
  };
  const ry = (angle: number): number[][] => {
    const [s, c] = [Math.sin(angle), Math.cos(angle)];
    return [
      [c, 0, -s],
      [0, 1, 0],
      [s, 0, c],
    ];
  };
  return matMul(matMul(rz(-z), ry(theta)), rz(-zeta));
}

function precessionAt(unixMs: number): number[][] {
  // TT-UTC is 69.184 s today. Future leap seconds are unknowable, and even a
  // minute of error moves these century-scale angles by under a milliarcsec.
  const TT_MINUS_UTC_MS = 69_184;
  const JULIAN_CENTURY_MS = 36_525 * 86_400_000;
  return precessionMatrix((unixMs + TT_MINUS_UTC_MS - J2000_UNIX_MS) / JULIAN_CENTURY_MS);
}

/** Column-major mat3 taking J2000/ICRS unit vectors into the observer's
 * mean-of-date view frame: x = screen-right, y = north (screen-up),
 * z = zenith (forward).
 *
 * Looking *up* with north at the top puts east on the LEFT, because
 * screen-right is forward × up = -east. That is the planetarium orientation;
 * a +east basis would render the mirrored "globe seen from outside" sky. */
export function viewMatrix(unixMs: number, latDeg: number, lonDeg: number): number[] {
  const lst = gmstRad(unixMs) + lonDeg * DEG;
  const [sinL, cosL] = [Math.sin(lst), Math.cos(lst)];
  const [sinP, cosP] = [Math.sin(latDeg * DEG), Math.cos(latDeg * DEG)];

  const horizon = [
    [sinL, -cosL, 0],
    [-sinP * cosL, -sinP * sinL, cosP],
    [cosP * cosL, cosP * sinL, sinP],
  ];
  const view = matMul(horizon, precessionAt(unixMs));

  const out = new Array<number>(9);
  for (let row = 0; row < 3; row += 1) {
    for (let col = 0; col < 3; col += 1) out[col * 3 + row] = view[row]![col]!;
  }
  return out;
}

/** A vector through a column-major view matrix. */
export function applyView(matrix: Mat3, v: Vec3 | ArrayLike<number>): Vec3 {
  const at = (row: number): number =>
    matrix[row]! * v[0]! + matrix[3 + row]! * v[1]! + matrix[6 + row]! * v[2]!;
  return [at(0), at(1), at(2)];
}

/** Where the sun is directly overhead at an instant: the subsolar point.
 *
 * Low-precision solar coordinates (NOAA/Meeus, ~0.01°), which is far finer
 * than a terminator drawn across 150 px of globe can show. */
export function subsolarPoint(unixMs: number): [number, number] {
  const days = (unixMs - J2000_UNIX_MS) / 86_400_000;
  const meanLon = (280.46 + 0.985_647_36 * days) * DEG;
  const meanAnomaly = (357.528 + 0.985_600_28 * days) * DEG;
  const eclipticLon =
    meanLon + (1.915 * Math.sin(meanAnomaly) + 0.02 * Math.sin(2 * meanAnomaly)) * DEG;
  const obliquity = (23.439 - 0.000_000_36 * days) * DEG;

  const declination = Math.asin(Math.sin(obliquity) * Math.sin(eclipticLon));
  const rightAscension = Math.atan2(
    Math.cos(obliquity) * Math.sin(eclipticLon),
    Math.cos(eclipticLon),
  );
  return [declination / DEG, normalizeLonDeg((rightAscension - gmstRad(unixMs)) / DEG)];
}
