/** What a star *is*, normalised once, on the server.
 *
 * The row `sky_star_detail` holds is the builder's merge of five catalogues
 * (`tools/starcat/README.md`): Gaia DR3 with its astrophysical parameters, HYG
 * v3, the IAU name list, the IAU constellation boundaries and — for the twelve
 * thousand stars a visitor can actually pick — SIMBAD. Their field names,
 * units and types disagree with each other: SIMBAD's numbers arrive as
 * strings, HYG calls a distance `distanceParsecs` and Gaia calls it
 * `distance_gspphot`, and a magnitude is `magV`, `magG` or `mag` depending on
 * who measured it. Reconciling that belongs here and nowhere else — the panel
 * renders a shape it can trust, and the tests can hold a fixture of each kind
 * of star against it.
 *
 * Nothing is invented. A key with no value is absent from the result, and the
 * panel omits the row, so a faint Gaia-only star shows five numbers and Alioth
 * shows twenty rather than both showing twenty with fifteen dashes.
 */

import { CONSTELLATIONS } from './constellations';

/** Which catalogue a URL key names, and the row id that catalogue maps to.
 *
 * Two spellings exist and both are load-bearing. The browser says
 * `gaia-<source_id>` / `hip-<n>` (`catalog.ts` `starKey()`), which is what a
 * URL should read like; the dataset says `g<source_id>` / `h<n>`, which is
 * what three million CSV rows carry and what a COPY must not have to rewrite.
 * This function is the only place the two meet. */
export interface StarKey {
  kind: 'gaia' | 'hip';
  /** The catalogue number, canonical: no leading zeros, no sign. */
  number: string;
  /** The URL form, `gaia-<n>` / `hip-<n>`. */
  key: string;
  /** The `sky_star_detail.id` form, `g<n>` / `h<n>`. */
  rowId: string;
}

/** Gaia DR3 source ids are 64-bit; the largest is 19 digits. */
const GAIA_MAX = 18_446_744_073_709_551_615n;
/** The Hipparcos catalogue ends at 120404. */
const HIP_MAX = 120_404;

/** Strict on purpose. The key is a cache key, a database key and part of a
 * URL that is served with a day of `max-age`, so `gaia-007` and `gaia-7` must
 * not be two spellings of one star, and a key that is not a catalogue number
 * must be refused before it reaches Postgres rather than after. */
export function parseStarKey(raw: string | undefined | null): StarKey | null {
  if (typeof raw !== 'string') return null;
  const match = /^(gaia|hip)-(0|[1-9][0-9]{0,18})$/.exec(raw);
  if (!match) return null;
  const [, kind, number] = match as unknown as [string, 'gaia' | 'hip', string];
  if (number === '0') return null;
  if (kind === 'gaia' && BigInt(number) > GAIA_MAX) return null;
  if (kind === 'hip' && (number.length > 6 || Number(number) > HIP_MAX)) return null;
  return { kind, number, key: `${kind}-${number}`, rowId: `${kind[0]}${number}` };
}

/* ------------------------------------------------------------ the shape --- */

export interface StarNames {
  /** The IAU-approved proper name, where the star has one. */
  proper?: string;
  bayerFlamsteed?: string;
  hd?: string;
  hr?: string;
  hip?: string;
  gaia?: string;
  /** SIMBAD's own primary identifier, e.g. `* eps UMa`. */
  mainId?: string;
}

export interface StarDistance {
  parallaxMas?: number;
  parallaxErrorMas?: number;
  /** From the parallax, `1000 / p`. */
  parsecs?: number;
  parsecsError?: number;
  lightYears?: number;
  /** Gaia's own `distance_gspphot`, which is a different estimate and is
   * shown as one rather than averaged into the parallax distance. */
  gspphotParsecs?: number;
}

export interface StarDetail {
  key: string;
  /** Where the answer came from: a row, or the key alone. */
  source: 'database' | 'catalog';
  title: string;
  names: StarNames;
  constellation?: { abbreviation: string; name: string };
  objectType?: string;
  spectralType?: { value: string; from: 'SIMBAD' | 'HYG' };
  temperature?: { kelvin: number; from: 'gspphot' | 'bp_rp' };
  distance?: StarDistance;
  luminositySuns?: number;
  radiusSuns?: number;
  massSuns?: number;
  ageGyr?: number;
  properMotion?: { ra: number; dec: number; total: number };
  radialVelocityKmS?: number;
  ruwe?: number;
  variable?: boolean;
  nonSingle?: boolean;
  magnitudes?: { g?: number; bp?: number; rp?: number; v?: number; b?: number };
  links: { simbad: string; gaiaArchive: string; gaiaSourceId?: string };
  sources: string[];
}

/* ------------------------------------------------------------- reading ---- */

type Row = Record<string, unknown>;

function block(payload: Row, name: string): Row {
  const value = payload[name];
  return typeof value === 'object' && value !== null ? (value as Row) : {};
}

/** A number however it was stored. SIMBAD's TAP returns its columns as text,
 * so `plx_value` arrives as `"39.51"` beside Gaia's `39.51`. */
function num(value: unknown): number | undefined {
  if (typeof value === 'number') return Number.isFinite(value) ? value : undefined;
  if (typeof value === 'string' && value.trim() !== '') {
    const parsed = Number(value);
    return Number.isFinite(parsed) ? parsed : undefined;
  }
  return undefined;
}

function str(value: unknown): string | undefined {
  return typeof value === 'string' && value.trim() !== '' ? value.trim() : undefined;
}

/** Round to a sensible number of significant figures. The builder already cut
 * everything to six decimals; carrying `9442.535` K into a panel that has room
 * for `9440 K` is precision nobody asked for and nobody can use. */
function sig(value: number | undefined, figures: number): number | undefined {
  if (value === undefined) return undefined;
  if (value === 0) return 0;
  const factor = 10 ** (figures - 1 - Math.floor(Math.log10(Math.abs(value))));
  return Math.round(value * factor) / factor;
}

const PARSEC_LY = 3.261_563_777;

/** SIMBAD's object-type codes, as words. Only the ones this catalogue actually
 * produces — it is stars down to G = 12, so there are no galaxies in it — and
 * anything unmapped falls through as the code SIMBAD gave, which is still a
 * true statement about the star. */
const OTYPE: Readonly<Record<string, string>> = {
  '*': 'star',
  '**': 'double star',
  'a2*': 'chemically peculiar star',
  'Al*': 'eclipsing binary',
  'AB*': 'asymptotic giant branch star',
  'Ae*': 'Herbig Ae/Be star',
  'bC*': 'Beta Cephei variable',
  'BS*': 'blue straggler',
  'BY*': 'BY Draconis variable',
  'Ce*': 'Cepheid variable',
  'C*': 'carbon star',
  'cC*': 'classical Cepheid',
  'dS*': 'Delta Scuti variable',
  'EB*': 'eclipsing binary',
  'Em*': 'emission-line star',
  'Er*': 'eruptive variable',
  'Fl*': 'flare star',
  'HB*': 'horizontal branch star',
  'HS*': 'hot subdwarf',
  'LP*': 'long-period variable',
  'MS*': 'main-sequence star',
  'Or*': 'Orion variable',
  'PM*': 'high proper-motion star',
  'Pu*': 'pulsating variable',
  'RG*': 'red giant',
  'RR*': 'RR Lyrae variable',
  'RS*': 'RS Canum Venaticorum variable',
  's*b': 'blue supergiant',
  's*r': 'red supergiant',
  's*y': 'yellow supergiant',
  'SB*': 'spectroscopic binary',
  'Sy*': 'symbiotic star',
  'V*': 'variable star',
  'WD*': 'white dwarf',
  'WR*': 'Wolf-Rayet star',
  'Y*O': 'young stellar object',
};

/* ---------------------------------------------------------- normalising --- */

/** The payload for a star with no row: the key, and what the key itself says.
 * A star can be picked out of the sky that the detail build never saw — the
 * bright catalogue and the dataset were built from different cuts — and a
 * panel that says "Gaia DR3 4611…, no further record" is the truth, where a
 * 500 would be a bug report about a working system. */
export function catalogOnly(key: StarKey): StarDetail {
  const names: StarNames =
    key.kind === 'gaia' ? { gaia: key.number } : { hip: key.number };
  return {
    key: key.key,
    source: 'catalog',
    title: key.kind === 'gaia' ? `Gaia DR3 ${key.number}` : `HIP ${key.number}`,
    names,
    links: links(key, names),
    sources: [],
  };
}

function links(key: StarKey, names: StarNames): StarDetail['links'] {
  const ident = names.mainId ?? (names.gaia ? `Gaia DR3 ${names.gaia}` : `HIP ${names.hip}`);
  return {
    simbad: `https://simbad.cds.unistra.fr/simbad/sim-id?Ident=${encodeURIComponent(ident)}`,
    gaiaArchive: 'https://gea.esac.esa.int/archive/',
    gaiaSourceId: names.gaia,
  };
}

/** One row, as the panel reads it. */
export function normaliseStarDetail(key: StarKey, payload: unknown): StarDetail {
  if (typeof payload !== 'object' || payload === null) return catalogOnly(key);
  const row = payload as Row;
  const gaia = block(row, 'gaia');
  const astro = block(row, 'astrophysical');
  const hyg = block(row, 'hyg');
  const iau = block(row, 'iau');
  const simbad = block(row, 'simbad');

  const names: StarNames = {
    // HYG's proper name first: the IAU list is parsed out of fixed columns and
    // a two-word name lands split across two fields — HIP 71683 arrives as
    // `iauName` "Rigil" and `iauNameDiacritics` "Kentaurus", and neither half
    // is the star's name.
    proper: str(hyg.proper) ?? str(row.name) ?? str(iau.iauName),
    bayerFlamsteed: str(hyg.bayerFlamsteed),
    hd: str(hyg.hd) ?? str(iau.hd),
    hr: str(hyg.hr),
    hip: str(row.hip) ?? str(hyg.hip) ?? str(iau.hip) ?? (key.kind === 'hip' ? key.number : undefined),
    gaia: str(row.sourceId) ?? (key.kind === 'gaia' ? key.number : undefined),
    mainId: str(simbad.main_id),
  };

  const detail: StarDetail = {
    key: key.key,
    source: 'database',
    title:
      names.proper ??
      names.mainId ??
      names.bayerFlamsteed ??
      (names.hip ? `HIP ${names.hip}` : `Gaia DR3 ${names.gaia ?? key.number}`),
    names,
    links: links(key, names),
    sources: Array.isArray(row.sources) ? row.sources.filter((s): s is string => typeof s === 'string') : [],
  };

  // HYG's per-star constellation where it has a usable one, and the boundary
  // walk the build did otherwise — see `constellations.ts` for why that way
  // round.
  const hygConstellation = str(hyg.constellation);
  const abbreviation =
    hygConstellation && CONSTELLATIONS[hygConstellation]
      ? hygConstellation
      : str(row.constellation);
  if (abbreviation) {
    detail.constellation = {
      abbreviation,
      name: CONSTELLATIONS[abbreviation] ?? str(row.constellationName) ?? abbreviation,
    };
  }

  const otype = str(simbad.otype_txt);
  if (otype) detail.objectType = OTYPE[otype] ?? otype;
  const simbadSpectral = str(simbad.sp_type);
  const hygSpectral = str(hyg.spectralType);
  if (simbadSpectral) detail.spectralType = { value: simbadSpectral, from: 'SIMBAD' };
  else if (hygSpectral) detail.spectralType = { value: hygSpectral, from: 'HYG' };

  const teff = num(astro.teff_gspphot);
  if (teff !== undefined) detail.temperature = { kelvin: Math.round(teff / 10) * 10, from: 'gspphot' };

  const distance = distanceOf(gaia, astro, hyg, simbad);
  if (distance) detail.distance = distance;

  detail.luminositySuns = sig(num(astro.lum_flame) ?? num(hyg.luminosity), 4);
  detail.radiusSuns = sig(num(astro.radius_flame), 3);
  detail.massSuns = sig(num(astro.mass_flame), 3);
  detail.ageGyr = sig(num(astro.age_flame), 3);

  const pmra = num(gaia.pmra) ?? num(simbad.pmra);
  const pmdec = num(gaia.pmdec) ?? num(simbad.pmdec);
  if (pmra !== undefined && pmdec !== undefined) {
    detail.properMotion = {
      ra: sig(pmra, 5)!,
      dec: sig(pmdec, 5)!,
      total: sig(Math.hypot(pmra, pmdec), 5)!,
    };
  }
  detail.radialVelocityKmS = sig(num(gaia.radialVelocity) ?? num(simbad.rvz_radvel), 4);
  detail.ruwe = sig(num(gaia.ruwe), 3);

  const variableFlag = str(gaia.variableFlag);
  if (variableFlag === 'VARIABLE' || variableFlag === 'NOT_AVAILABLE') {
    detail.variable = variableFlag === 'VARIABLE';
  }
  const nonSingle = num(gaia.nonSingleStar);
  if (nonSingle !== undefined) detail.nonSingle = nonSingle > 0;

  const magnitudes = magnitudesOf(gaia, hyg, iau);
  if (magnitudes) detail.magnitudes = magnitudes;

  // A colour temperature is not a measurement of the same thing `teff_gspphot`
  // is, so it is only offered where Gaia has no astrophysical row, and it
  // carries where it came from so the panel can label it as what it is.
  if (!detail.temperature) {
    const bpRp = num(gaia.bpRp);
    if (bpRp !== undefined && bpRp > -0.5 && bpRp < 5) {
      detail.temperature = { kelvin: Math.round(teffFromColour(bpRp) / 10) * 10, from: 'bp_rp' };
    }
  }
  return strip(detail);
}

/** Mucciarelli & Bellazzini 2020 (RNAAS 4, 52), the BP-RP dwarf fit at solar
 * metallicity: `5040/Teff = 0.4988 + 0.4925 C - 0.0287 C^2`. The same relation
 * the catalogue's own colours are quantised through (`lod.ts` `teffFromBpRp`),
 * repeated rather than imported so that a route does not pull the renderer's
 * module graph onto the server. */
function teffFromColour(bpRp: number): number {
  const c = Math.max(-0.4, Math.min(3, bpRp));
  return 5_040 / (0.4988 + 0.4925 * c - 0.0287 * c * c);
}

function distanceOf(gaia: Row, astro: Row, hyg: Row, simbad: Row): StarDistance | undefined {
  const distance: StarDistance = {};
  const parallax = num(gaia.parallax) ?? num(simbad.plx_value);
  const error = num(gaia.parallaxError);
  if (parallax !== undefined && parallax > 0) {
    distance.parallaxMas = sig(parallax, 6);
    const parsecs = 1_000 / parallax;
    distance.parsecs = sig(parsecs, 4);
    distance.lightYears = sig(parsecs * PARSEC_LY, 4);
    if (error !== undefined && error > 0) {
      distance.parallaxErrorMas = sig(error, 3);
      distance.parsecsError = sig((1_000 * error) / (parallax * parallax), 2);
    }
  } else {
    const hygParsecs = num(hyg.distanceParsecs);
    // HYG marks "no parallax" with a placeholder distance of 100000 pc.
    if (hygParsecs !== undefined && hygParsecs > 0 && hygParsecs < 100_000) {
      distance.parsecs = sig(hygParsecs, 4);
      distance.lightYears = sig(hygParsecs * PARSEC_LY, 4);
    }
  }
  const gspphot = num(astro.distance_gspphot);
  if (gspphot !== undefined && gspphot > 0) distance.gspphotParsecs = sig(gspphot, 4);
  return Object.keys(distance).length ? distance : undefined;
}

function magnitudesOf(gaia: Row, hyg: Row, iau: Row): StarDetail['magnitudes'] | undefined {
  const v = num(hyg.magV) ?? num(iau.magV);
  const colourIndex = num(hyg.colorIndex);
  const magnitudes = {
    g: sig(num(gaia.magG), 4),
    bp: sig(num(gaia.magBp), 4),
    rp: sig(num(gaia.magRp), 4),
    v: sig(v, 4),
    // HYG carries B−V rather than B; the sum is the same measurement said
    // differently, not a new one.
    b: v !== undefined && colourIndex !== undefined ? sig(v + colourIndex, 4) : undefined,
  };
  return Object.values(magnitudes).some((value) => value !== undefined) ? magnitudes : undefined;
}

/** Drop every key with no value, at both levels. What survives is what is
 * known, which is what the panel draws a row for. */
function strip(detail: StarDetail): StarDetail {
  const out = detail as unknown as Row;
  for (const [key, value] of Object.entries(out)) {
    if (value === undefined) delete out[key];
    else if (value !== null && typeof value === 'object' && !Array.isArray(value)) {
      const inner = value as Row;
      for (const [innerKey, innerValue] of Object.entries(inner)) {
        if (innerValue === undefined) delete inner[innerKey];
      }
      if (Object.keys(inner).length === 0) delete out[key];
    }
  }
  return detail;
}
