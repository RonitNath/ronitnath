/** Which star the pointer is on.
 *
 * The GL pass projects every star on the GPU and never brings a pixel back:
 * `readPixels` is a pipeline stall, and an id encoded into a colour channel is
 * a second, differently-wrong projection to keep in step with the first. So
 * the pick projects on the CPU instead, through the same view matrix and the
 * same focal length the shader is handed, and the two agree because they are
 * the same three lines of arithmetic (`sidereal.ts` `applyView`, `FOCAL`).
 *
 * One pass over the catalogues does both jobs: reject everything behind the
 * viewer or off the frame, and drop what is left into a screen-space bucket
 * grid. A query then reads nine buckets rather than 177,000 stars. The pass is
 * only run when a pointer asks for it and never more than once a frame, so a
 * visitor who does not move the mouse pays nothing at all.
 *
 * "Nearest" is not nearest in pixels. A star is drawn as wide as its glare
 * reaches (`tuning.ts` `starDiameterPx`), and what a pointer is *on* is the
 * thing it is inside: distance is measured to the edge of the star's own
 * profile, so Sirius half an inch away wins over a magnitude-8 star three
 * pixels off, which is what the eye already believes.
 */

import { KIND_HIP, type StarCatalog } from './catalog';
import type { DeepStars } from './lod';
import { FOCAL, type Mat3, type Vec3 } from './sidereal';
import { NIGHT, starDiameterPx, starFlux } from './tuning';

/** How far from a star a pointer still counts as on it, in CSS pixels, before
 * the star's own drawn radius is taken off. */
export const PICK_RADIUS_PX = 14;

/** Bucket size, CSS pixels. Two buckets either side of the query cover the
 * pick radius plus the widest star's half-width (12 px), so a query reads a
 * 5 x 5 neighbourhood and no candidate can hide outside it. */
const CELL_PX = 16;
const CELL_REACH = 2;

/** Below this the star is at or under the horizon and the projection stretches
 * without bound. The same cut the ring uses (`stars-gl.ts`). */
const MIN_Z = 0.08;

/** Which buffer a hit came from. The bright catalogue and `g9.bin` are both
 * pickable; the streamed G 9-12 tiles are not (docs/sky-plan.md S3). */
export type PickSource = 'bright' | 'g9';

export interface PickHit {
  source: PickSource;
  index: number;
  /** `gaia-<source_id>` or `hip-<n>`: what `/api/sky/star/` reads. */
  key: string;
  /** The star's J2000 unit vector, so the ring needs no second lookup. */
  position: Vec3;
  /** CSS pixels from the top left of the viewport. */
  x: number;
  y: number;
  magnitude: number;
  /** The IAU name, where this star has one in `named.json`. */
  name: string | null;
  /** The star's colour as a word, from the ramp its bytes came off. */
  colour: string;
}

/* ------------------------------------------------------------- colour ----- */

const TEFF_MIN = 2_900;
const TEFF_MAX = 17_000;

/** The temperature a ramp level stands for — {@link colourLevel} run
 * backwards, which is the only way back to a temperature from a catalogue that
 * ships colours and not temperatures. */
export function teffFromLevel(level: number): number {
  const t = Math.max(0, Math.min(23, level)) / 23;
  return 1 / (1 / TEFF_MIN + t * (1 / TEFF_MAX - 1 / TEFF_MIN));
}

/** Harvard's own boundaries, said in English. */
export function colourWord(teff: number): string {
  if (teff < 3_700) return 'red';
  if (teff < 5_200) return 'orange';
  if (teff < 6_000) return 'yellow';
  if (teff < 7_500) return 'yellow-white';
  if (teff < 10_000) return 'white';
  return 'blue-white';
}

/** A catalogue colour back to the word it stands for.
 *
 * The nearest ramp entry, over all three channels — an exact hit in practice,
 * since these bytes were written by quantising onto this very ramp. One
 * channel would not do it: blue climbs to 255 by the middle of the ramp and
 * stays there while red falls, so the blue byte alone names two levels at
 * opposite ends of the temperature scale. */
export function colourWordFromRgb(
  ramp: readonly (readonly number[])[],
  r: number,
  g: number,
  b: number,
): string {
  let best = 0;
  let bestGap = Infinity;
  for (let level = 0; level < ramp.length; level += 1) {
    const entry = ramp[level]!;
    const gap = (entry[0]! - r) ** 2 + (entry[1]! - g) ** 2 + (entry[2]! - b) ** 2;
    if (gap < bestGap) {
      bestGap = gap;
      best = level;
    }
  }
  return colourWord(teffFromLevel((best * 23) / Math.max(1, ramp.length - 1)));
}

/* --------------------------------------------------------------- search --- */

/** One catalogue's screen positions, and the grid over them. */
class Layer {
  x = new Float32Array(0);
  y = new Float32Array(0);
  /** Star indices per bucket, reused between frames. */
  cells: number[][] = [];
  columns = 0;
  rows = 0;

  reset(count: number, width: number, height: number): void {
    if (this.x.length < count) {
      this.x = new Float32Array(count);
      this.y = new Float32Array(count);
    }
    const columns = Math.max(1, Math.ceil(width / CELL_PX));
    const rows = Math.max(1, Math.ceil(height / CELL_PX));
    if (columns !== this.columns || rows !== this.rows || this.cells.length === 0) {
      this.columns = columns;
      this.rows = rows;
      this.cells = Array.from({ length: columns * rows }, () => [] as number[]);
    } else {
      for (const cell of this.cells) cell.length = 0;
    }
  }

  add(index: number, x: number, y: number): void {
    this.x[index] = x;
    this.y[index] = y;
    const column = Math.min(this.columns - 1, Math.max(0, Math.floor(x / CELL_PX)));
    const row = Math.min(this.rows - 1, Math.max(0, Math.floor(y / CELL_PX)));
    this.cells[row * this.columns + column]!.push(index);
  }

  /** Every candidate within reach of a point, as bucket contents. */
  near(x: number, y: number, visit: (index: number) => void): void {
    const column = Math.floor(x / CELL_PX);
    const row = Math.floor(y / CELL_PX);
    for (let r = row - CELL_REACH; r <= row + CELL_REACH; r += 1) {
      if (r < 0 || r >= this.rows) continue;
      for (let c = column - CELL_REACH; c <= column + CELL_REACH; c += 1) {
        if (c < 0 || c >= this.columns) continue;
        for (const index of this.cells[r * this.columns + c]!) visit(index);
      }
    }
  }
}

interface Frame {
  matrix: Mat3;
  width: number;
  height: number;
}

export class Picker {
  private catalog: StarCatalog | null = null;
  private g9: DeepStars | null = null;
  private names = new Map<number, string>();
  private ramp: readonly (readonly number[])[] = [];

  private readonly bright = new Layer();
  private readonly deep = new Layer();
  private frame: Frame | null = null;
  private brightCount = 0;
  private deepCount = 0;

  setCatalog(catalog: StarCatalog, ramp: readonly (readonly number[])[]): void {
    this.catalog = catalog;
    this.ramp = ramp;
    this.frame = null;
  }

  /** `named.json`, by index into the bright catalogue: the hover tag says the
   * star's name where it has one and its catalogue number where it does not. */
  setNames(names: Map<number, string>): void {
    this.names = names;
  }

  setG9(stars: DeepStars | null): void {
    // Without ids a record cannot be named, and an unnameable hit would open a
    // panel about no star in particular.
    this.g9 = stars?.sourceId ? stars : null;
    this.frame = null;
  }

  get ready(): boolean {
    return this.catalog !== null;
  }

  /** Project both catalogues for this view, unless that has already been done
   * for exactly this one. The stage hands the same matrix object to the
   * renderer and to this, so identity is the cheapest correct test. */
  project(matrix: Mat3, width: number, height: number): void {
    if (
      this.frame &&
      this.frame.matrix === matrix &&
      this.frame.width === width &&
      this.frame.height === height
    ) {
      return;
    }
    this.frame = { matrix, width, height };
    const aspect = height > 0 ? width / height : 1.6;
    this.brightCount = this.catalog
      ? fill(this.bright, this.catalog.position, this.catalog.count, matrix, aspect, width, height)
      : 0;
    this.deepCount = this.g9
      ? fill(this.deep, this.g9.position, this.g9.count, matrix, aspect, width, height)
      : 0;
  }

  /** The star under a point, or nothing. The bright catalogue is searched
   * first and its hit wins ties: a star that is in both files is the same
   * star, and the bright record is the one that carries a name. */
  pick(x: number, y: number): PickHit | null {
    if (!this.frame) return null;
    let best: PickHit | null = null;
    let bestScore = Infinity;

    const consider = (source: PickSource, layer: Layer, index: number): void => {
      const magnitude =
        source === 'bright' ? this.catalog!.magnitude[index]! : this.g9!.magnitude[index]!;
      const distance = Math.hypot(layer.x[index]! - x, layer.y[index]! - y);
      // Fourteen pixels is the reach, for every star: brightness decides which
      // of the candidates wins, never whether there is one.
      if (distance > PICK_RADIUS_PX) return;
      // The star's own half-width comes off the distance, so a pointer inside
      // a bright star's disc scores below zero and no faint neighbour beats it.
      const score = distance - halfWidth(magnitude);
      if (score < bestScore) {
        bestScore = score;
        best = this.hit(source, index, layer.x[index]!, layer.y[index]!, magnitude);
      }
    };

    if (this.brightCount) {
      this.bright.near(x, y, (index) => consider('bright', this.bright, index));
    }
    if (this.deepCount) {
      this.deep.near(x, y, (index) => consider('g9', this.deep, index));
    }
    return best;
  }

  /** The hit a *named* star would be, without a pointer: a callout is a
   * button onto the same panel, and it names a star by its index into the
   * bright catalogue rather than by where the pointer is. The screen position
   * is left at the origin — nothing on the panel reads it, and the ring is
   * drawn from the sky vector. */
  brightHit(index: number): PickHit | null {
    const catalog = this.catalog;
    if (!catalog || index < 0 || index >= catalog.count) return null;
    return this.hit('bright', index, 0, 0, catalog.magnitude[index]!);
  }

  private hit(
    source: PickSource,
    index: number,
    x: number,
    y: number,
    magnitude: number,
  ): PickHit {
    if (source === 'bright') {
      const catalog = this.catalog!;
      const prefix = catalog.kind[index] === KIND_HIP ? 'hip' : 'gaia';
      return {
        source,
        index,
        key: `${prefix}-${catalog.id[index]!}`,
        position: [
          catalog.position[index * 3]!,
          catalog.position[index * 3 + 1]!,
          catalog.position[index * 3 + 2]!,
        ],
        x,
        y,
        magnitude,
        name: this.names.get(index) ?? null,
        colour: colourWordFromRgb(
          this.ramp,
          Math.round(catalog.color[index * 3]! * 255),
          Math.round(catalog.color[index * 3 + 1]! * 255),
          Math.round(catalog.color[index * 3 + 2]! * 255),
        ),
      };
    }
    const stars = this.g9!;
    return {
      source,
      index,
      key: `gaia-${stars.sourceId![index]!}`,
      position: [
        stars.position[index * 3]!,
        stars.position[index * 3 + 1]!,
        stars.position[index * 3 + 2]!,
      ],
      x,
      y,
      magnitude,
      name: null,
      colour: colourWordFromRgb(
        this.ramp,
        stars.color[index * 3]!,
        stars.color[index * 3 + 1]!,
        stars.color[index * 3 + 2]!,
      ),
    };
  }
}

/** Half the width the renderer draws this star at, at the zenith. Taking the
 * extinction at the zenith rather than at the star's own altitude keeps the
 * pick's idea of a star's size from shrinking as it sets, which would make a
 * low star harder to hit exactly where it is already hardest to see. */
function halfWidth(magnitude: number): number {
  return starDiameterPx(starFlux(magnitude, NIGHT.magRef, 1), NIGHT) / 2;
}

/** One pass: project, reject, bucket. Returns how many stars landed. */
function fill(
  layer: Layer,
  position: Float32Array,
  count: number,
  matrix: Mat3,
  aspect: number,
  width: number,
  height: number,
): number {
  layer.reset(count, width, height);
  const margin = PICK_RADIUS_PX;
  // `applyView` unrolled: it returns a fresh three-element array, and a
  // hundred and seventy thousand of those per pass is garbage this loop can
  // simply not make. The arithmetic is the same, column-major, and the test
  // holds one against the other.
  const [m0, m1, m2, m3, m4, m5, m6, m7, m8] = matrix as number[];
  let kept = 0;
  for (let index = 0; index < count; index += 1) {
    const at = index * 3;
    const px = position[at]!;
    const py = position[at + 1]!;
    const pz = position[at + 2]!;
    const vz = m2! * px + m5! * py + m8! * pz;
    if (vz <= MIN_Z) continue;
    const vx = m0! * px + m3! * py + m6! * pz;
    const vy = m1! * px + m4! * py + m7! * pz;
    const x = ((((vx / vz) * FOCAL) / aspect + 1) / 2) * width;
    if (x < -margin || x > width + margin) continue;
    const y = ((1 - (vy / vz) * FOCAL) / 2) * height;
    if (y < -margin || y > height + margin) continue;
    layer.add(index, x, y);
    kept += 1;
  }
  return kept;
}
