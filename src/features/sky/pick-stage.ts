/** The pick, as the stage holds it: one picker, one hovered star, one pinned.
 *
 * `pick.ts` is the geometry — projection, buckets, nearest — and knows nothing
 * about a page. This is the state around it: which star the pointer is on,
 * which star the panel is open on, and where either of them is on screen a
 * frame later. It takes the view matrix from whoever owns the clock rather
 * than working one out, so the pick, the ring and the drawn sky cannot end up
 * projecting through three slightly different views.
 */

import type { NamedStar } from './catalog';
import { type PickHit, Picker } from './pick';
import { applyView, FOCAL, type Mat3, type Vec3 } from './sidereal';

/** At or below this the star is on the horizon and the projection stretches
 * without bound — the same cut the ring and the picker use. */
const MIN_Z = 0.08;

/** One named star as the end-to-end run reads it. */
export interface DebugStar {
  name: string;
  key: string;
  x: number;
  y: number;
  magnitude: number;
}

export class StagePicking {
  readonly picker = new Picker();
  private hovered: PickHit | null = null;
  private pinned: PickHit | null = null;

  /** `view` is the matrix being drawn *now*; `changed` asks for a frame, for
   * the still skies (reduced motion, paused) that have no loop to supply one. */
  constructor(
    private readonly view: () => Mat3,
    private readonly changed: () => void,
  ) {}

  /** The star at a point in the viewport. Nothing is read back from the GPU. */
  pickAt(clientX: number, clientY: number): PickHit | null {
    if (!this.picker.ready) return null;
    this.picker.project(this.view(), innerWidth, innerHeight);
    return this.picker.pick(clientX, clientY);
  }

  /** The star a named-star callout points at, as a pick. */
  brightHit(brightIndex: number): PickHit | null {
    return this.picker.brightHit(brightIndex);
  }

  setHover(hit: PickHit | null): void {
    if (this.hovered?.key === hit?.key) return;
    this.hovered = hit;
    this.changed();
  }

  setPinned(hit: PickHit | null): void {
    this.pinned = hit;
    this.changed();
  }

  /** The star to ring. The hover wins while there is one: it is answering the
   * question the pointer is asking right now. */
  get ringed(): PickHit | null {
    return this.hovered ?? this.pinned;
  }

  /** Where a star is on screen right now — the hover tag follows the star per
   * frame rather than sitting where the pointer stopped, because the sky keeps
   * moving under a still pointer. */
  screenPosition(position: Vec3): [number, number] | null {
    const [vx, vy, vz] = applyView(this.view(), position);
    if (vz <= MIN_Z) return null;
    const aspect = innerHeight > 0 ? innerWidth / innerHeight : 1.6;
    return [
      ((((vx / vz) * FOCAL) / aspect + 1) / 2) * innerWidth,
      ((1 - (vy / vz) * FOCAL) / 2) * innerHeight,
    ];
  }

  /** Every named star on screen right now, brightest first, with the key its
   * panel is addressed by and the pixel a pointer has to be on to hover it.
   * For the end-to-end run and for nobody else; the page a visitor gets
   * publishes none of it. */
  debugNamed(named: readonly NamedStar[]): DebugStar[] {
    const out: DebugStar[] = [];
    for (const star of named) {
      const hit = this.brightHit(star.brightIndex);
      if (!hit) continue;
      const at = this.screenPosition(hit.position);
      if (!at || at[0] < 0 || at[0] > innerWidth || at[1] < 0 || at[1] > innerHeight) continue;
      out.push({ name: star.name, key: hit.key, x: at[0], y: at[1], magnitude: hit.magnitude });
    }
    return out.sort((a, b) => a.magnitude - b.magnitude);
  }
}
