/** The star point-sprite, pre-rendered.
 *
 * The pre-rebuild starscape drew every star as a GL point running
 * `STAR_FRAG`: inside the unit disc the fragment is the star's colour at
 * `exp(-falloff·d²)`, additively blended, so the core saturates toward white
 * and the colour survives only in the wings. A 2D canvas cannot run that per
 * fragment twelve thousand times a frame, but it does not have to: the
 * function depends on nothing but the colour, the diameter and the falloff, so
 * one sprite per (colour bucket × size tier × falloff bucket) reproduces it
 * exactly and the frame becomes a `drawImage` loop.
 *
 * The sprite carries the halo in its *alpha* and the flat star colour in its
 * RGB. Drawn with `globalAlpha = starAlpha` under `globalCompositeOperation:
 * 'lighter'`, the contribution is `colour · exp(-falloff·d²) · starAlpha`,
 * which is `STAR_FRAG` term for term.
 */

import { TUNING } from './tuning';

/** The shader's halo: `glow = exp(-falloff · d²)`, discarded outside the
 * disc. `d` here is the squared distance from the centre in sprite radii,
 * which is what `dot(c, c)` is in the shader. */
export function haloAlpha(distanceSquared: number, falloff: number): number {
  if (distanceSquared > 1) return 0;
  return Math.exp(-falloff * distanceSquared);
}

/** Halo width as a function of brightness rather than of alpha: once the core
 * saturates this is the only channel left that still says "brighter". The
 * shipped `halo` is 0, which reproduces the fixed falloff exactly — the
 * expression is carried over whole so the two constants stay one knob. */
export function falloffFor(brightness: number): number {
  return TUNING.glow / (1 + TUNING.halo * Math.pow(brightness, 0.25));
}

/** Sub-samples per axis inside a sprite pixel. The shader's `discard` is a
 * hard edge at d = 1 and the sprites are a handful of pixels across, so
 * without this the rim of a bright star is a visible staircase. */
const SUPERSAMPLE = 2;

/** The two resolutions a sprite is rendered at, in device pixels. `drawImage`
 * resamples, so the sprite does not need a tier per pixel of diameter — one
 * small sprite for the points that are barely more than a dot and one large
 * one for the stars wide enough to show a halo is the whole ladder. A tier per
 * quarter-pixel meant several hundred offscreen canvases and half a second of
 * blocking time on the landing, for a difference nothing can see. */
const SPRITE_SIZES = [8, 32] as const;

/** Above this diameter in CSS pixels a star is drawn from the large sprite. */
const LARGE_ABOVE_PX = 3;

/** Colour finer than this is below what the eye separates on a point two
 * pixels wide, and star colours lie along the blackbody ramp anyway. */
const COLOR_LEVELS = 6;
const FALLOFF_STEP = 0.05;

/** The halo, evaluated over a `side × side` sprite. Alpha only: the colour is
 * flat across the sprite and multiplied in by the draw. */
export function spriteAlphaField(side: number, falloff: number): Float32Array {
  const field = new Float32Array(side * side);
  const samples = SUPERSAMPLE * SUPERSAMPLE;
  for (let row = 0; row < side; row += 1) {
    for (let column = 0; column < side; column += 1) {
      let sum = 0;
      for (let sy = 0; sy < SUPERSAMPLE; sy += 1) {
        // Sprite coordinates run -1..1 across the point, exactly as
        // `gl_PointCoord * 2.0 - 1.0` does.
        const cy = (2 * (row + (sy + 0.5) / SUPERSAMPLE)) / side - 1;
        for (let sx = 0; sx < SUPERSAMPLE; sx += 1) {
          const cx = (2 * (column + (sx + 0.5) / SUPERSAMPLE)) / side - 1;
          sum += haloAlpha(cx * cx + cy * cy, falloff);
        }
      }
      field[row * side + column] = sum / samples;
    }
  }
  return field;
}

/** Which of the two sprite resolutions a diameter in CSS pixels draws from. */
export function sizeTier(diameterPx: number): number {
  return diameterPx > LARGE_ABOVE_PX ? 1 : 0;
}

/** The sprite side, in device pixels, a tier renders at. */
export function tierSide(tier: number): number {
  return SPRITE_SIZES[tier] ?? SPRITE_SIZES[1];
}

/** Which colour bucket a 0..1 RGB triple falls in. Star colours lie along the
 * blackbody ramp, so the cube is sparse and the buckets actually used are a
 * handful. */
export function colorBucket(red: number, green: number, blue: number): number {
  const level = (value: number): number =>
    Math.min(COLOR_LEVELS - 1, Math.max(0, Math.round(value * (COLOR_LEVELS - 1))));
  return (level(red) * COLOR_LEVELS + level(green)) * COLOR_LEVELS + level(blue);
}

/** The RGB a bucket stands for, 0..255. */
export function bucketRgb(bucket: number): [number, number, number] {
  const blue = bucket % COLOR_LEVELS;
  const green = Math.floor(bucket / COLOR_LEVELS) % COLOR_LEVELS;
  const red = Math.floor(bucket / (COLOR_LEVELS * COLOR_LEVELS));
  const byte = (level: number): number => Math.round((level / (COLOR_LEVELS - 1)) * 255);
  return [byte(red), byte(green), byte(blue)];
}

/** One pre-rendered point sprite, at device resolution. */
export function renderSprite(
  sideDevicePx: number,
  falloff: number,
  rgb: readonly [number, number, number],
): HTMLCanvasElement {
  const side = Math.max(2, Math.round(sideDevicePx));
  const canvas = document.createElement('canvas');
  canvas.width = side;
  canvas.height = side;
  const ctx = canvas.getContext('2d');
  if (!ctx) return canvas;
  const field = spriteAlphaField(side, falloff);
  const image = ctx.createImageData(side, side);
  for (let index = 0; index < field.length; index += 1) {
    const at = index * 4;
    image.data[at] = rgb[0];
    image.data[at + 1] = rgb[1];
    image.data[at + 2] = rgb[2];
    image.data[at + 3] = Math.round(Math.min(1, field[index]!) * 255);
  }
  ctx.putImageData(image, 0, 0);
  return canvas;
}

/** The sprites a frame draws with, built on first use and kept for the life of
 * the look that asked for them. */
export class SpriteAtlas {
  private readonly cache = new Map<number, HTMLCanvasElement>();

  /** The sprite for a star, keyed by the three quantised axes so twelve
   * thousand stars resolve to a few dozen images. */
  get(
    diameterPx: number,
    red: number,
    green: number,
    blue: number,
    falloff: number,
  ): HTMLCanvasElement {
    const tier = sizeTier(diameterPx);
    const bucket = colorBucket(red, green, blue);
    const falloffBucket = Math.max(0, Math.round(falloff / FALLOFF_STEP));
    const key = (tier * 1_000 + bucket) * 1_000 + falloffBucket;
    let sprite = this.cache.get(key);
    if (!sprite) {
      sprite = renderSprite(tierSide(tier), falloffBucket * FALLOFF_STEP, bucketRgb(bucket));
      this.cache.set(key, sprite);
    }
    return sprite;
  }

  get size(): number {
    return this.cache.size;
  }
}
