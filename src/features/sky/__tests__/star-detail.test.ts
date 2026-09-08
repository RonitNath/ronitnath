import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';

import {
  catalogOnly,
  normaliseStarDetail,
  parseStarKey,
  type StarKey,
} from '../star-detail';

/** Three real rows, taken out of the loaded table and trimmed only in SIMBAD's
 * forty-odd cross-identifiers: a bright star SIMBAD knows well, a faint one
 * that is nothing but a Gaia measurement, and a Hipparcos-keyed row with no
 * Gaia block at all. Between them they cover every branch the normaliser has,
 * and they are the shapes the builder actually writes rather than shapes a
 * test invented. */
function fixture(name: string): unknown {
  return JSON.parse(readFileSync(join(__dirname, 'fixtures', `${name}.json`), 'utf8'));
}

const key = (raw: string): StarKey => {
  const parsed = parseStarKey(raw);
  if (!parsed) throw new Error(`${raw} did not parse`);
  return parsed;
};

describe('parseStarKey', () => {
  it('reads both catalogues and maps them onto the dataset ids', () => {
    expect(parseStarKey('gaia-1576683529448755328')).toEqual({
      kind: 'gaia',
      number: '1576683529448755328',
      key: 'gaia-1576683529448755328',
      rowId: 'g1576683529448755328',
    });
    expect(parseStarKey('hip-62956')?.rowId).toBe('h62956');
  });

  it('refuses everything that is not one canonical catalogue number', () => {
    for (const raw of [
      '',
      'gaia',
      'gaia-',
      'gaia-0',
      'hip-0',
      'gaia-007', // a second spelling of one star is a second cache entry
      'gaia--7',
      'gaia-1e9',
      'gaia- 7',
      'GAIA-7',
      'tyc-7',
      'gaia-12345678901234567890', // past 2^64
      'hip-999999', // past the catalogue's last star
      'gaia-7/../..',
      "gaia-7'; drop table sky_star_detail; --",
    ]) {
      expect(parseStarKey(raw), raw).toBeNull();
    }
    expect(parseStarKey(undefined)).toBeNull();
  });
});

describe('normaliseStarDetail', () => {
  it('carries a bright SIMBAD-rich star whole', () => {
    const detail = normaliseStarDetail(key('gaia-1576683529448755328'), fixture('alioth'));
    expect(detail.title).toBe('Alioth');
    expect(detail.source).toBe('database');
    expect(detail.names).toMatchObject({
      proper: 'Alioth',
      bayerFlamsteed: '77Eps UMa',
      hd: '112185',
      hr: '4905',
      hip: '62956',
      gaia: '1576683529448755328',
      mainId: '* eps UMa',
    });
    expect(detail.constellation).toEqual({ abbreviation: 'UMa', name: 'Ursa Major' });
    // SIMBAD's `a2*` is a code; the panel says what it means.
    expect(detail.objectType).toBe('chemically peculiar star');
    expect(detail.spectralType).toEqual({ value: 'A1III-IVpkB9', from: 'SIMBAD' });
    // No Gaia parallax on a first-magnitude star, so SIMBAD's is the one that
    // resolves — and it puts Alioth at the 25 pc the catalogues agree on.
    expect(detail.distance?.parallaxMas).toBeCloseTo(39.51, 2);
    expect(detail.distance?.parsecs).toBeCloseTo(25.31, 1);
    expect(detail.distance?.lightYears).toBeCloseTo(82.5, 0);
    expect(detail.luminositySuns).toBeCloseTo(110.3, 1);
    expect(detail.magnitudes).toMatchObject({ v: 1.76, g: 1.732 });
    // HYG carries B−V, not B.
    expect(detail.magnitudes?.b).toBeCloseTo(1.738, 3);
    expect(detail.variable).toBe(true);
    expect(detail.nonSingle).toBe(false);
    expect(detail.temperature?.from).toBe('bp_rp');
    expect(detail.links.simbad).toContain(encodeURIComponent('* eps UMa'));
    expect(detail.sources).toContain('SIMBAD (CDS)');
  });

  it('carries a faint Gaia-only star, and nothing it does not have', () => {
    const detail = normaliseStarDetail(key('gaia-4616204397039146496'), fixture('gaia-only'));
    expect(detail.title).toBe('Gaia DR3 4616204397039146496');
    expect(detail.names.proper).toBeUndefined();
    expect(detail.objectType).toBeUndefined();
    expect(detail.spectralType).toBeUndefined();
    expect(detail.luminositySuns).toBeUndefined();
    expect(detail.massSuns).toBeUndefined();
    expect(detail.magnitudes?.v).toBeUndefined();
    expect(detail.constellation).toEqual({ abbreviation: 'Oct', name: 'Octans' });
    expect(detail.distance?.parallaxMas).toBeCloseTo(0.209857, 6);
    expect(detail.distance?.parsecs).toBeCloseTo(4765, 0);
    expect(detail.distance?.parsecsError).toBeGreaterThan(0);
    expect(detail.properMotion?.total).toBeCloseTo(17.12, 1);
    expect(detail.variable).toBe(false);
    expect(detail.sources).toEqual(['Gaia DR3 gaia_source']);
    // The panel's link has to name something SIMBAD can resolve.
    expect(detail.links.simbad).toContain('Gaia%20DR3%204616204397039146496');
  });

  it('carries a Hipparcos row, which has no Gaia block at all', () => {
    const detail = normaliseStarDetail(key('hip-37826'), fixture('pollux'));
    expect(detail.title).toBe('Pollux');
    expect(detail.names.gaia).toBeUndefined();
    expect(detail.names.hip).toBe('37826');
    expect(detail.constellation?.name).toBe('Gemini');
    expect(detail.spectralType).toEqual({ value: 'K0IIIb', from: 'SIMBAD' });
    expect(detail.magnitudes?.v).toBe(1.16);
    expect(detail.distance?.parsecs).toBeGreaterThan(9);
    expect(detail.distance?.parsecs).toBeLessThan(12);
    expect(detail.links.gaiaSourceId).toBeUndefined();
    // No Gaia photometry means no colour, so no temperature is claimed.
    expect(detail.temperature).toBeUndefined();
  });

  it('answers with the key alone when there is no row', () => {
    const detail = catalogOnly(key('gaia-4611686018427387904'));
    expect(detail.source).toBe('catalog');
    expect(detail.title).toBe('Gaia DR3 4611686018427387904');
    expect(detail.sources).toEqual([]);
    expect(detail.distance).toBeUndefined();
    expect(catalogOnly(key('hip-11767')).title).toBe('HIP 11767');
  });

  it('survives a payload that is not an object', () => {
    for (const junk of [null, 42, 'star', undefined]) {
      expect(normaliseStarDetail(key('gaia-7'), junk).source).toBe('catalog');
    }
  });
});
