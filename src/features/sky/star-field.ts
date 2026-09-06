/** The star field, painted on a 2D canvas.
 *
 * The projection, the magnitude response and the atmospheric extinction are
 * the ones the pre-rebuild WebGL starscape shipped (`render.rs` /
 * `shaders.rs`), evaluated on the CPU: the sky is 12,191 points at ≤30 fps,
 * which a 2D context draws comfortably, and keeping WebGL for the globe alone
 * is what the brief asks for.
 *
 * Behind the stars the Milky Way is painted from `sky/milkyway.webp` — Gaia
 * star counts binned onto an equal-area grid, not procedural noise. That map
 * is fixed on the celestial sphere; what changes with the observer is which
 * part of it clears the horizon and how much atmosphere it shines through.
 * Warping an equirectangular map through the view is a per-pixel operation, so
 * it is done into a small offscreen buffer and scaled up: the band is diffuse,
 * and a soft one is what it should look like.
 */

import type { StarCatalog } from './catalog';
import { applyView, FOCAL, type Mat3 } from './sidereal';
import { TUNING, TWILIGHT_MAG_LIMIT } from './tuning';

/** Widest the offscreen Milky Way buffer gets. The band has no detail finer
 * than this at the scale it is drawn. */
const BAND_MAX_WIDTH = 192;

/** Where the halo gradient is sampled, and the flat alpha that stands in for
 * it on a star only two pixels across. */
const HALO_STOPS = [0, 0.25, 0.5, 0.75, 1] as const;
const HALO_MEAN = 0.62;

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
  /** `r,g,b` ready for a colour string. */
  rgb: string[];
  /** How many of the (brightest-first) catalog this look covers. */
  count: number;
  light: boolean;
  dpr: number;
}

export function buildLook(catalog: StarCatalog, light: boolean, dpr: number): StarLook {
  // Twilight needs a different size response than night: a star on a bright
  // sky wins on area rather than on contrast.
  const sizeBase = TUNING.sizeBase * (light ? 4 : 1);
  const sizeExp = TUNING.sizeExp * (light ? 2 : 1);
  const scale = Math.min(dpr, 1.5);

  let count = catalog.count;
  if (light) {
    count = 0;
    while (count < catalog.count && catalog.magnitude[count]! <= TWILIGHT_MAG_LIMIT) count += 1;
  }

  const radius = new Float32Array(count);
  const alpha = new Float32Array(count);
  const rgb: string[] = new Array<string>(count);
  for (let index = 0; index < count; index += 1) {
    const brightness = Math.pow(10, -0.4 * catalog.magnitude[index]!);
    const size = Math.min(
      TUNING.sizeMax,
      Math.max(1, sizeBase + TUNING.sizeScale * Math.pow(brightness, sizeExp)),
    );
    radius[index] = (size / 2) * scale;
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
    rgb[index] =
      `${Math.round(red * 255)},${Math.round(green * 255)},${Math.round(blue * 255)}`;
  }
  return { radius, alpha, rgb, count, light, dpr };
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

/** The Milky Way, sampled through the view into a small RGBA buffer. */
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
      const rx = (ndcX * aspect) / FOCAL;
      const ry = ndcY / FOCAL;
      const norm = Math.hypot(rx, ry, 1);
      const rayZ = 1 / norm;

      const airmass = 1 / Math.max(rayZ, 0.05);
      let scale = Math.pow(10, -0.4 * TUNING.extinctionK * (airmass - 1)) * gain;
      if (light) scale *= 1.15 * smoothstep(0.1, 0.75, rayZ);
      if (scale <= 0) continue;

      // The view basis is orthonormal, so its transpose is its inverse: the
      // map can be sampled in the catalog's own equatorial frame.
      const view = [rx / norm, ry / norm, rayZ] as const;
      const jx = matrix[0]! * view[0] + matrix[1]! * view[1] + matrix[2]! * view[2];
      const jy = matrix[3]! * view[0] + matrix[4]! * view[1] + matrix[5]! * view[2];
      const jz = matrix[6]! * view[0] + matrix[7]! * view[1] + matrix[8]! * view[2];

      const u = Math.atan2(jy, jx) / (2 * Math.PI) + 0.5;
      const v = 0.5 - Math.asin(Math.min(1, Math.max(-1, jz))) / Math.PI;
      const sx = Math.min(source.width - 1, Math.max(0, Math.floor(u * source.width)));
      const sy = Math.min(source.height - 1, Math.max(0, Math.floor(v * source.height)));
      const at = (sy * source.width + sx) * 4;

      const target4 = (py * width + px) * 4;
      let peak = 0;
      for (let channel = 0; channel < 3; channel += 1) {
        const value = Math.pow(source.data[at + channel]! / 255, shape) * scale;
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
  const w = Math.max(32, Math.min(BAND_MAX_WIDTH, Math.round(width / 6)));
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

    const rgb = look.rgb[index]!;
    const radius = look.radius[index]!;

    if (radius <= 0.9) {
      // Under a pixel across there is no halo to draw and no circle to see:
      // a square of the right area reads identically and costs a fill.
      ctx.fillStyle = `rgba(${rgb},${alpha.toFixed(3)})`;
      ctx.fillRect(x - radius, y - radius, radius * 2, radius * 2);
      drawn += 1;
      continue;
    }

    // exp(-glow · d²) is the shader's halo, expressed as gradient stops: the
    // core saturates and the star's colour stays in the wings, which is what a
    // genuinely bright star looks like. Only the stars wide enough to show one
    // pay for it.
    ctx.beginPath();
    ctx.arc(x, y, radius, 0, Math.PI * 2);
    if (radius <= 2.2) {
      // Two pixels of halo is one flat disc to the eye, and a gradient object
      // per star is what a 12,191-star loop cannot afford.
      ctx.fillStyle = `rgba(${rgb},${(alpha * HALO_MEAN).toFixed(3)})`;
    } else {
      const glow = ctx.createRadialGradient(x, y, 0, x, y, radius);
      for (const stop of HALO_STOPS) {
        glow.addColorStop(
          stop,
          `rgba(${rgb},${(alpha * Math.exp(-TUNING.glow * stop * stop)).toFixed(3)})`,
        );
      }
      ctx.fillStyle = glow;
    }
    ctx.fill();
    drawn += 1;
  }

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
