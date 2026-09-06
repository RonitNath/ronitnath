/** The star field, painted on a 2D canvas.
 *
 * The projection, the magnitude response and the atmospheric extinction are
 * the ones the pre-rebuild WebGL starscape shipped (`render.rs` /
 * `shaders.rs`), evaluated on the CPU: the sky is 12,191 points at ≤30 fps,
 * which a 2D context draws comfortably, and keeping WebGL for the globe alone
 * is what the brief asks for.
 *
 * Every star is one `drawImage` of a pre-rendered point sprite (`sprites.ts`)
 * under `lighter`, which is `STAR_FRAG` evaluated once per (colour, size,
 * falloff) instead of once per fragment.
 *
 * Behind the stars the Milky Way is painted from `sky/milkyway.webp` — Gaia
 * star counts binned onto an equal-area grid, not procedural noise. That map
 * is fixed on the celestial sphere; what changes with the observer is which
 * part of it clears the horizon and how much atmosphere it shines through.
 * Warping an equirectangular map through the view is a per-pixel operation, so
 * the shipped path hands it to a fragment shader (`band-gl.ts`) and what is
 * here is the fallback for a browser with no WebGL2.
 */

import type { StarCatalog } from './catalog';
import { applyView, FOCAL, type Mat3 } from './sidereal';
import { falloffFor, type SpriteAtlas } from './sprites';
import { TUNING, TWILIGHT_MAG_LIMIT } from './tuning';

/** Widest the offscreen Milky Way buffer gets. This path is the fallback for a
 * browser with no WebGL2 — the band is drawn by a fragment shader at full
 * resolution otherwise (`band-gl.ts`) — and at 192 px it read as blobs rather
 * than as the galaxy, so the fallback samples wide enough and bilinearly
 * enough to show the dark lanes and the bulge too. */
const BAND_MAX_WIDTH = 512;

export interface FrameGeometry {
  width: number;
  height: number;
  dpr: number;
}

/** The magnitude response, evaluated once per star per theme instead of once
 * per star per frame: it depends only on the catalog and on which sky the
 * theme paints, and a 12,191-star loop that recomputes four powers a star was
 * the whole of this page's blocking time. */
export interface StarLook {
  /** Half the point size, in CSS pixels. */
  radius: Float32Array;
  /** Alpha before atmospheric extinction. */
  alpha: Float32Array;
  /** The pre-rendered point sprite each star draws with. Many stars share
   * one: the sprite depends only on colour, diameter and falloff. */
  sprite: HTMLCanvasElement[];
  /** How many of the (brightest-first) catalog this look covers. */
  count: number;
  light: boolean;
  dpr: number;
}

export function buildLook(
  catalog: StarCatalog,
  light: boolean,
  dpr: number,
  atlas: SpriteAtlas,
): StarLook {
  // Twilight needs a different size response than night: a star on a bright
  // sky wins on area rather than on contrast.
  const sizeBase = TUNING.sizeBase * (light ? 4 : 1);
  const sizeExp = TUNING.sizeExp * (light ? 2 : 1);

  let count = catalog.count;
  if (light) {
    count = 0;
    while (count < catalog.count && catalog.magnitude[count]! <= TWILIGHT_MAG_LIMIT) count += 1;
  }

  const radius = new Float32Array(count);
  const alpha = new Float32Array(count);
  const sprite = new Array<HTMLCanvasElement>(count);
  for (let index = 0; index < count; index += 1) {
    const brightness = Math.pow(10, -0.4 * catalog.magnitude[index]!);
    // `gl_PointSize` is device pixels and the GL canvas was device-sized, so
    // the shipped star is `size` *CSS* pixels across at any density. The 2D
    // context is scaled by the ratio, so the same number is the same star.
    const size = Math.min(
      TUNING.sizeMax,
      Math.max(1, sizeBase + TUNING.sizeScale * Math.pow(brightness, sizeExp)),
    );
    radius[index] = size / 2;
    alpha[index] = Math.min(
      TUNING.alphaMax,
      Math.max(0, TUNING.alphaBase + TUNING.alphaScale * Math.pow(brightness, TUNING.alphaExp)),
    );

    let [red, green, blue] = [
      catalog.color[index * 3]!,
      catalog.color[index * 3 + 1]!,
      catalog.color[index * 3 + 2]!,
    ];
    if (light) {
      // Colour does not survive a bright sky; push toward white.
      red += (1 - red) * 0.25;
      green += (1 - green) * 0.25;
      blue += (1 - blue) * 0.25;
    }
    sprite[index] = atlas.get(size, red, green, blue, falloffFor(brightness));
  }
  return { radius, alpha, sprite, count, light, dpr };
}

/** Atmospheric extinction against the zenith cosine, sampled once per frame
 * rather than raised to a power once per star. */
const EXTINCTION_STEPS = 96;

function extinctionTable(): Float32Array {
  const table = new Float32Array(EXTINCTION_STEPS + 1);
  for (let step = 0; step <= EXTINCTION_STEPS; step += 1) {
    const zenithCos = step / EXTINCTION_STEPS;
    const airmass = 1 / Math.max(zenithCos, 0.05);
    table[step] = Math.pow(10, -0.4 * TUNING.extinctionK * (airmass - 1));
  }
  return table;
}

const EXTINCTION = extinctionTable();

export interface Highlight {
  /** J2000 unit vector of the star to ring. */
  position: readonly number[];
  /** 0..1, faded out by the caller as the highlight expires. */
  strength: number;
}

function smoothstep(edge0: number, edge1: number, x: number): number {
  const t = Math.min(1, Math.max(0, (x - edge0) / (edge1 - edge0)));
  return t * t * (3 - 2 * t);
}

/** Where a frame pixel lands on the equirectangular map.
 *
 * The star pass maps J2000 → view; texturing the sky needs the inverse, and
 * the view basis is orthonormal, so its transpose is that inverse — which is
 * why the map is sampled in the catalog's own equatorial frame and no galactic
 * transform appears anywhere. Returns `[u, v, rayZ]`: `rayZ` is the cosine of
 * the zenith angle, which the extinction and the twilight gate both need. */
export function bandUv(
  matrix: Mat3,
  ndcX: number,
  ndcY: number,
  aspect: number,
): [number, number, number] {
  const rx = (ndcX * aspect) / FOCAL;
  const ry = ndcY / FOCAL;
  const norm = Math.hypot(rx, ry, 1);
  const view = [rx / norm, ry / norm, 1 / norm] as const;
  const jx = matrix[0]! * view[0] + matrix[1]! * view[1] + matrix[2]! * view[2];
  const jy = matrix[3]! * view[0] + matrix[4]! * view[1] + matrix[5]! * view[2];
  const jz = matrix[6]! * view[0] + matrix[7]! * view[1] + matrix[8]! * view[2];
  return [
    Math.atan2(jy, jx) / (2 * Math.PI) + 0.5,
    0.5 - Math.asin(Math.min(1, Math.max(-1, jz))) / Math.PI,
    view[2],
  ];
}

/** One channel of the map, sampled bilinearly and wrapping in u. Nearest
 * sampling is what made the fallback band read as blobs: the map is 1024 px
 * around the whole sky and one frame pixel spans a fraction of a texel. */
function sampleBilinear(source: ImageData, u: number, v: number, channel: number): number {
  const x = u * source.width - 0.5;
  const y = Math.min(source.height - 1, Math.max(0, v * source.height - 0.5));
  const x0 = Math.floor(x);
  const y0 = Math.floor(y);
  const fx = x - x0;
  const fy = y - y0;
  const wrap = (column: number): number =>
    ((column % source.width) + source.width) % source.width;
  const columns = [wrap(x0), wrap(x0 + 1)];
  const rows = [Math.max(0, y0), Math.min(source.height - 1, y0 + 1)];
  const at = (row: number, column: number): number =>
    source.data[(row * source.width + column) * 4 + channel]!;
  const top = at(rows[0]!, columns[0]!) * (1 - fx) + at(rows[0]!, columns[1]!) * fx;
  const bottom = at(rows[1]!, columns[0]!) * (1 - fx) + at(rows[1]!, columns[1]!) * fx;
  return top * (1 - fy) + bottom * fy;
}

/** The Milky Way, sampled through the view into an RGBA buffer. The fallback
 * for a browser with no WebGL2; `band-gl.ts` is what ships. */
export function paintBand(
  target: CanvasRenderingContext2D,
  source: ImageData,
  matrix: Mat3,
  light: boolean,
): void {
  const out = target.createImageData(target.canvas.width, target.canvas.height);
  const { width, height } = target.canvas;
  const aspect = width / height;
  const shape = light ? 2.7 : TUNING.bandShape;
  const gain = light ? 0.46 : TUNING.bandGain;

  for (let py = 0; py < height; py += 1) {
    const ndcY = 1 - (2 * (py + 0.5)) / height;
    for (let px = 0; px < width; px += 1) {
      const ndcX = (2 * (px + 0.5)) / width - 1;
      // Undo the star pass's projection to recover this pixel's view ray. The
      // camera looks at the zenith, so ray.z is the cosine of the zenith angle.
      const [u, v, rayZ] = bandUv(matrix, ndcX, ndcY, aspect);

      const airmass = 1 / Math.max(rayZ, 0.05);
      let scale = Math.pow(10, -0.4 * TUNING.extinctionK * (airmass - 1)) * gain;
      if (light) scale *= 1.15 * smoothstep(0.1, 0.75, rayZ);
      if (scale <= 0) continue;

      const target4 = (py * width + px) * 4;
      let peak = 0;
      for (let channel = 0; channel < 3; channel += 1) {
        const value = Math.pow(sampleBilinear(source, u, v, channel) / 255, shape) * scale;
        const byte = Math.min(255, Math.round(value * 255));
        out.data[target4 + channel] = byte;
        if (byte > peak) peak = byte;
      }
      // Alpha covers the colour it carries, so the dusk gradient underneath
      // composites into one picture rather than being painted over.
      out.data[target4 + 3] = peak;
    }
  }
  target.putImageData(out, 0, 0);
}

/** The offscreen size the band is sampled at for a given viewport. */
export function bandSize(width: number, height: number): [number, number] {
  const w = Math.max(32, Math.min(BAND_MAX_WIDTH, Math.round(width / 3)));
  const h = Math.max(16, Math.round((w * height) / Math.max(width, 1)));
  return [w, h];
}

/** Draw one frame of stars. The context is expected to be cleared and in
 * `lighter` composite mode: starlight is additive. */
export function paintStars(
  ctx: CanvasRenderingContext2D,
  catalog: StarCatalog,
  look: StarLook,
  matrix: Mat3,
  frame: FrameGeometry,
  highlight: Highlight | null,
): number {
  const { width, height } = frame;
  const aspect = width / height;
  const light = look.light;
  const position = catalog.position;
  let drawn = 0;

  for (let index = 0; index < look.count; index += 1) {
    const at = index * 3;
    const px = position[at]!;
    const py = position[at + 1]!;
    const pz = position[at + 2]!;
    const vz = matrix[2]! * px + matrix[5]! * py + matrix[8]! * pz;
    if (vz <= 0.08) continue;
    const vx = matrix[0]! * px + matrix[3]! * py + matrix[6]! * pz;
    const vy = matrix[1]! * px + matrix[4]! * py + matrix[7]! * pz;

    const x = (((vx / vz) * FOCAL) / aspect + 1) * 0.5 * width;
    if (x < -12 || x > width + 12) continue;
    const y = (1 - (vy / vz) * FOCAL) * 0.5 * height;
    if (y < -12 || y > height + 12) continue;

    let alpha = look.alpha[index]! * EXTINCTION[(vz * EXTINCTION_STEPS) | 0]!;
    if (light) alpha *= smoothstep(0.2, 0.7, vz);
    if (alpha <= 0.012) continue;

    // The whole of the star: colour and halo are baked into the sprite, and
    // the per-star alpha is the shader's `v_alpha` scalar, so the drawn
    // contribution is `colour · exp(-falloff·d²) · alpha` — `STAR_FRAG` term
    // for term, with `lighter` standing in for additive blending.
    const radius = look.radius[index]!;
    ctx.globalAlpha = alpha;
    ctx.drawImage(look.sprite[index]!, x - radius, y - radius, radius * 2, radius * 2);
    drawn += 1;
  }
  ctx.globalAlpha = 1;

  if (highlight && highlight.strength > 0) {
    const [hx, hy, hz] = applyView(matrix, highlight.position);
    if (hz > 0.08) {
      const x = (((hx / hz) * FOCAL) / aspect + 1) * 0.5 * width;
      const y = (1 - (hy / hz) * FOCAL) * 0.5 * height;
      ctx.strokeStyle = `rgba(190,225,255,${(0.85 * highlight.strength).toFixed(3)})`;
      ctx.lineWidth = 1.5;
      ctx.beginPath();
      ctx.arc(x, y, 14 + 6 * (1 - highlight.strength), 0, Math.PI * 2);
      ctx.stroke();
    }
  }

  return drawn;
}
