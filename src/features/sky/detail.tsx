'use client';

import { useEffect, useRef, useState } from 'react';

import type { PickHit } from './pick';
import type { StarDetail } from './star-detail';

/** The detail panel: a pane of glass over the sky with everything the site
 * knows about one star.
 *
 * It fetches `/api/sky/star/<key>` when it opens and renders what comes back —
 * which is a *sparse* shape on purpose (`star-detail.ts`): a magnitude-12 star
 * in a Gaia-only row has five numbers and a bright one has twenty, and a table
 * of twenty rows where fifteen are dashes says nothing louder than it says
 * anything. Empty rows are omitted, so the panel's own height is how much is
 * known about the star.
 *
 * Nothing here explains itself. There is no caption about what a parallax is,
 * no legend, no hint about clicking elsewhere to close.
 */

const UNITS = {
  parsec: 'pc',
  lightYear: 'ly',
  mas: 'mas',
  masPerYear: 'mas/yr',
  kmPerSecond: 'km/s',
  kelvin: 'K',
  gigayear: 'Gyr',
} as const;

/** Digit grouping, no currency, no rounding of its own — the server already
 * cut every number to the figures it can support. */
function fmt(value: number, decimals?: number): string {
  return value.toLocaleString('en-US', {
    minimumFractionDigits: decimals,
    maximumFractionDigits: decimals ?? 6,
  });
}

interface Row {
  label: string;
  value: string;
}

/** The table, built once from the payload. A row exists only where its number
 * does, and the order is the order a star is usually described in: how bright,
 * how hot, how far, how big, how it moves. */
export function detailRows(detail: StarDetail): Row[] {
  const rows: Row[] = [];
  const push = (label: string, value: string | undefined): void => {
    if (value) rows.push({ label, value });
  };
  const magnitudes = detail.magnitudes;
  if (magnitudes) {
    push('Magnitude (V)', magnitudes.v !== undefined ? fmt(magnitudes.v, 2) : undefined);
    push('Magnitude (B)', magnitudes.b !== undefined ? fmt(magnitudes.b, 2) : undefined);
    push('Magnitude (G)', magnitudes.g !== undefined ? fmt(magnitudes.g, 3) : undefined);
    push(
      'BP / RP',
      magnitudes.bp !== undefined && magnitudes.rp !== undefined
        ? `${fmt(magnitudes.bp, 3)} / ${fmt(magnitudes.rp, 3)}`
        : undefined,
    );
  }
  if (detail.temperature) {
    push(
      detail.temperature.from === 'gspphot' ? 'Temperature' : 'Temperature (BP−RP)',
      `${fmt(detail.temperature.kelvin)} ${UNITS.kelvin}`,
    );
  }
  const distance = detail.distance;
  if (distance) {
    push(
      'Distance',
      distance.parsecs !== undefined
        ? `${fmt(distance.parsecs)} ${UNITS.parsec} · ${fmt(distance.lightYears ?? 0)} ${UNITS.lightYear}`
        : undefined,
    );
    push(
      'Parallax',
      distance.parallaxMas !== undefined
        ? distance.parallaxErrorMas !== undefined
          ? `${fmt(distance.parallaxMas)} ± ${fmt(distance.parallaxErrorMas)} ${UNITS.mas}`
          : `${fmt(distance.parallaxMas)} ${UNITS.mas}`
        : undefined,
    );
    push(
      'Distance (Gaia)',
      distance.gspphotParsecs !== undefined
        ? `${fmt(distance.gspphotParsecs)} ${UNITS.parsec}`
        : undefined,
    );
  }
  push('Luminosity', detail.luminositySuns !== undefined ? `${fmt(detail.luminositySuns)} L⊙` : undefined);
  push('Radius', detail.radiusSuns !== undefined ? `${fmt(detail.radiusSuns)} R⊙` : undefined);
  push('Mass', detail.massSuns !== undefined ? `${fmt(detail.massSuns)} M⊙` : undefined);
  push('Age', detail.ageGyr !== undefined ? `${fmt(detail.ageGyr)} ${UNITS.gigayear}` : undefined);
  // The total, not the two components: a pair of grouped decimals separated by
  // a comma reads as one number with a thousands separator in it, and how fast
  // a star is moving across the sky is the number the row is for.
  if (detail.properMotion) {
    push('Proper motion', `${fmt(detail.properMotion.total, 2)} ${UNITS.masPerYear}`);
  }
  push(
    'Radial velocity',
    detail.radialVelocityKmS !== undefined
      ? `${fmt(detail.radialVelocityKmS, 2)} ${UNITS.kmPerSecond}`
      : undefined,
  );
  push('RUWE', detail.ruwe !== undefined ? fmt(detail.ruwe, 2) : undefined);
  // Flags are rows only when they are true: "not variable" is not a fact about
  // a star, it is the absence of one.
  if (detail.variable) push('Variability', 'variable');
  if (detail.nonSingle) push('Multiplicity', 'non-single');
  return rows;
}

/** The identifiers, in the order a catalogue would list them. */
export function identifierRows(detail: StarDetail): Row[] {
  const { names } = detail;
  const rows: Row[] = [];
  const push = (label: string, value: string | undefined): void => {
    if (value) rows.push({ label, value });
  };
  push('Name', names.proper);
  push('Bayer / Flamsteed', names.bayerFlamsteed);
  push('SIMBAD', names.mainId);
  push('HD', names.hd && `HD ${names.hd}`);
  push('HR', names.hr && `HR ${names.hr}`);
  push('HIP', names.hip && `HIP ${names.hip}`);
  push('Gaia DR3', names.gaia);
  return rows;
}

/** The line under the title: where the star is and what it is. */
export function subtitle(detail: StarDetail): string {
  return [detail.constellation?.name, detail.objectType, detail.spectralType?.value]
    .filter((part): part is string => Boolean(part))
    .join(' · ');
}

type Load = { state: 'loading' } | { state: 'ready'; detail: StarDetail } | { state: 'failed' };

export function StarDetailPanel({ hit, onClose }: { hit: PickHit; onClose: () => void }) {
  const [load, setLoad] = useState<Load>({ state: 'loading' });
  const panel = useRef<HTMLDivElement>(null);
  const returnTo = useRef<HTMLElement | null>(null);

  useEffect(() => {
    let live = true;
    setLoad({ state: 'loading' });
    fetch(`/api/sky/star/${encodeURIComponent(hit.key)}`)
      .then((response) => (response.ok ? response.json() : Promise.reject(response.status)))
      .then((detail: StarDetail) => {
        if (live) setLoad({ state: 'ready', detail });
      })
      .catch(() => {
        if (live) setLoad({ state: 'failed' });
      });
    return () => {
      live = false;
    };
  }, [hit.key]);

  /* A dialog takes the focus and gives it back. The trap is the whole of what
   * makes this reachable without a pointer: the panel is opened from a callout
   * button, and without it the next Tab would land behind the panel on the
   * page it covers. */
  useEffect(() => {
    returnTo.current = document.activeElement as HTMLElement | null;
    const node = panel.current;
    node?.querySelector<HTMLElement>('.star-detail-close')?.focus();
    const onKey = (event: KeyboardEvent): void => {
      if (event.key === 'Escape') {
        event.stopPropagation();
        onClose();
        return;
      }
      if (event.key !== 'Tab' || !node) return;
      const focusable = node.querySelectorAll<HTMLElement>('button, a[href]');
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (!first || !last) return;
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    };
    document.addEventListener('keydown', onKey, true);
    return () => {
      document.removeEventListener('keydown', onKey, true);
      returnTo.current?.focus?.();
    };
  }, [onClose]);

  const detail = load.state === 'ready' ? load.detail : null;
  const rows = detail ? detailRows(detail) : [];
  const identifiers = detail ? identifierRows(detail) : [];
  const line = detail ? subtitle(detail) : '';

  return (
    <div
      className="star-detail"
      role="dialog"
      aria-modal="false"
      aria-label={detail?.title ?? hit.key}
      data-key={hit.key}
      ref={panel}
    >
      <div className="star-detail-head">
        <h2>{detail?.title ?? hit.name ?? hit.key}</h2>
        <button type="button" className="star-detail-close" onClick={onClose} aria-label="Close">
          ×
        </button>
      </div>
      {line ? <p className="star-detail-line">{line}</p> : null}
      {load.state === 'loading' ? <p className="star-detail-state">Loading</p> : null}
      {load.state === 'failed' ? <p className="star-detail-state">Unavailable</p> : null}
      {rows.length ? (
        <table className="star-detail-table">
          <tbody>
            {rows.map((row) => (
              <tr key={row.label}>
                <th scope="row">{row.label}</th>
                <td>{row.value}</td>
              </tr>
            ))}
          </tbody>
        </table>
      ) : null}
      {identifiers.length ? (
        <dl className="star-detail-ids">
          {identifiers.map((row) => (
            <div key={row.label}>
              <dt>{row.label}</dt>
              <dd>{row.value}</dd>
            </div>
          ))}
        </dl>
      ) : null}
      {detail ? (
        <p className="star-detail-links">
          <a href={detail.links.simbad} rel="noopener noreferrer" target="_blank">
            SIMBAD
          </a>
          <a href={detail.links.gaiaArchive} rel="noopener noreferrer" target="_blank">
            Gaia archive
          </a>
        </p>
      ) : null}
    </div>
  );
}
