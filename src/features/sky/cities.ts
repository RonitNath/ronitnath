/** Nearest-city lookup over the compact GeoNames catalog.
 *
 * Source data: the GeoNames `cities15000` export (CC BY 4.0), filtered into
 * `public/cities/cities.bin`. The browser fetches it separately rather than
 * having 100 KB inline in the document, so the grounding caption is honestly
 * empty until the fetch lands rather than naming a place the observer is not
 * over.
 *
 * Layout: a `CTY1` header (magic + u32 count), then one 16-byte record per
 * city — i32 latitude and longitude in microdegrees, u32 offset and u16 length
 * into the name block that follows the records, and two bytes of ISO-3166
 * alpha-2 country code.
 *
 * Distance is a spherical great-circle distance, so the antimeridian and the
 * poles are ordinary inputs rather than special cases.
 */

const MAGIC = 0x31_59_54_43; // "CTY1", little-endian.
const HEADER_LEN = 8;
const RECORD_LEN = 16;
const MAX_CITIES = 100_000;
export const EARTH_MEAN_RADIUS_KM = 6_371.008_8;

export interface City {
  name: string;
  /** ISO-3166 alpha-2 country code, as GeoNames records it. */
  country: string;
  distanceKm: number;
}

interface CityRecord {
  name: string;
  country: string;
  latDeg: number;
  lonDeg: number;
}

/** A structurally validated catalog. Every string range is checked once, on
 * construction, so a scan cannot fail halfway through and leave the caption
 * naming a city from a corrupt record. */
export class CityCatalog {
  private constructor(private readonly records: CityRecord[]) {}

  static parse(bytes: Uint8Array): CityCatalog | null {
    if (bytes.byteLength < HEADER_LEN) return null;
    const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    if (view.getUint32(0, true) !== MAGIC) return null;
    const count = view.getUint32(4, true);
    if (count === 0 || count > MAX_CITIES) return null;

    const namesStart = HEADER_LEN + count * RECORD_LEN;
    if (namesStart > bytes.byteLength) return null;

    const text = new TextDecoder('utf-8', { fatal: true });
    const records: CityRecord[] = [];
    for (let index = 0; index < count; index += 1) {
      const at = HEADER_LEN + index * RECORD_LEN;
      const latDeg = view.getInt32(at, true) / 1_000_000;
      const lonDeg = view.getInt32(at + 4, true) / 1_000_000;
      const nameStart = namesStart + view.getUint32(at + 8, true);
      const nameEnd = nameStart + view.getUint16(at + 12, true);
      if (nameEnd > bytes.byteLength || nameStart > nameEnd) return null;
      if (latDeg < -90 || latDeg > 90) return null;
      try {
        records.push({
          name: text.decode(bytes.subarray(nameStart, nameEnd)),
          country: text.decode(bytes.subarray(at + 14, at + 16)),
          latDeg,
          lonDeg,
        });
      } catch {
        return null;
      }
    }
    return new CityCatalog(records);
  }

  get length(): number {
    return this.records.length;
  }

  /** The nearest catalog city and its great-circle distance, or `null` for a
   * coordinate that is not a point on Earth. */
  nearest(latDeg: number, lonDeg: number): City | null {
    if (!Number.isFinite(latDeg) || !Number.isFinite(lonDeg)) return null;
    if (latDeg < -90 || latDeg > 90) return null;

    let best: City | null = null;
    for (const record of this.records) {
      const distanceKm = distanceBetweenKm(latDeg, lonDeg, record.latDeg, record.lonDeg);
      if (!best || distanceKm < best.distanceKm) {
        best = { name: record.name, country: record.country, distanceKm };
      }
    }
    return best;
  }
}

export function distanceBetweenKm(
  lat1Deg: number,
  lon1Deg: number,
  lat2Deg: number,
  lon2Deg: number,
): number {
  const DEG = Math.PI / 180;
  const lat1 = lat1Deg * DEG;
  const lat2 = lat2Deg * DEG;
  const dLat = lat2 - lat1;
  const raw = (lon2Deg - lon1Deg) * DEG + Math.PI;
  const dLon = (((raw % (2 * Math.PI)) + 2 * Math.PI) % (2 * Math.PI)) - Math.PI;
  const h = Math.sin(dLat / 2) ** 2 + Math.cos(lat1) * Math.cos(lat2) * Math.sin(dLon / 2) ** 2;
  return 2 * EARTH_MEAN_RADIUS_KM * Math.asin(Math.min(1, Math.sqrt(h)));
}
