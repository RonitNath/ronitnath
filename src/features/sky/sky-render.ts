/** The sky's two drawing paths, and the choice between them.
 *
 * `stage.ts` owns the clock, the observer and the frame loop; what it hands
 * this is a view matrix and a theme. What comes back is a picture, drawn one
 * of two ways:
 *
 * - WebGL2, which is what ships: one context on one canvas, the Milky Way's
 *   fragment shader (`band-gl.ts`) and then the catalogue as GL points
 *   (`stars-gl.ts`) over it.
 * - a 2D canvas (`star-field.ts`), for a browser with no WebGL2. Same sky,
 *   sampled and composited on the CPU.
 *
 * Exactly one of the two canvases is ever visible; the other is hidden the
 * moment the context is or is not obtained, because two skies over each other
 * is twice the stars at half the brightness.
 */

import { ASSETS, bandUrl, loadImageData } from './assets';
import { SKY_DEBUG } from './deep-stage';
import { BandScene } from './band-gl';
import type { StarCatalog } from './catalog';
import { DeepLayer } from './deep-gl';
import { skyContext } from './gl-util';
import { LineScene } from './lines-gl';
import { type LinePairs, lineVertices } from './lines';
import type { Mat3 } from './sidereal';
import { SpriteAtlas } from './sprites';
import {
  bandSize,
  buildLook,
  type Highlight,
  paintBand,
  paintStars,
  type StarLook,
} from './star-field';
import { StarScene } from './stars-gl';

/** The Milky Way is diffuse and its warp is per-pixel, so the CPU fallback
 * re-samples it on its own slower cadence and scales it up between times. */
const BAND_INTERVAL_MS = 250;

/** The two things a frame may be asked to add, both off by default and both
 * only available on the WebGL path. */
export interface SkyOptions {
  lines?: boolean;
  twinkle?: boolean;
}

export class SkyRenderer {
  private canvas: HTMLCanvasElement | null = null;
  private flat: HTMLCanvasElement | null = null;
  private gl: WebGL2RenderingContext | null = null;
  private bandScene: BandScene | null = null;
  private starScene: StarScene | null = null;
  private lineScene: LineScene | null = null;
  private deep: DeepLayer | null = null;
  private uploaded = false;

  private atlas: SpriteAtlas | null = null;
  private look: StarLook | null = null;
  private milkyway: ImageData | null = null;
  private band: HTMLCanvasElement | null = null;
  private bandDrawnAt = 0;
  /** Frame times, kept only under `?skydebug=1`: about twenty seconds of them
   * at 30 fps, which is long enough for a median to mean something. */
  private readonly frames: number[] = [];

  /** Attach the two canvases and decide which of them is the sky. */
  attach(canvas: HTMLCanvasElement | null, flat: HTMLCanvasElement | null): void {
    this.canvas = canvas;
    this.flat = flat;
    this.gl = canvas ? skyContext(canvas) : null;
    this.bandScene = this.gl ? BandScene.create(this.gl) : null;
    this.starScene = this.gl ? StarScene.create(this.gl) : null;
    this.lineScene = this.gl ? LineScene.create(this.gl) : null;
    if (canvas) canvas.hidden = this.gl === null;
    if (flat) flat.hidden = this.gl !== null;
  }

  /** The streamed catalogue's buffers, created on demand — nothing allocates
   * 12 MB of vertex memory until the streamer has decided this device is
   * getting tiles at all. `null` where there is no WebGL2 to put them on: the
   * 2D fallback draws the bright catalogue and nothing deeper. */
  deepLayer(): DeepLayer | null {
    if (!this.gl || !this.starScene) return null;
    if (!this.deep) {
      this.deep = new DeepLayer(this.gl);
      this.starScene.attachDeep(this.deep);
    }
    return this.deep;
  }

  /** Points drawn in the last frame, over all three buffers. */
  get points(): number {
    return this.starScene?.lastPoints ?? 0;
  }

  /** Whether the shipped path is the one drawing. */
  get accelerated(): boolean {
    return this.gl !== null;
  }

  /** Forget the per-theme response, so the next frame rebuilds it. Only the
   * CPU path caches anything: the shader takes the theme as a uniform. */
  reset(): void {
    this.look = null;
    this.band = null;
  }

  /** The Milky Way's map, to whichever renderer is drawing it. The shader
   * takes the image straight to a texture; the fallback needs it decoded to
   * pixels it can sample on the CPU. */
  async loadBand(): Promise<void> {
    if (this.bandScene) {
      await this.bandScene.loadMap(bandUrl(innerWidth, Math.min(devicePixelRatio || 1, 2)));
      return;
    }
    // The CPU path samples the map at 1,024 across whatever it is handed, so
    // the wide bake would be three times the bytes for the same pixels.
    this.milkyway = await loadImageData(ASSETS.milkyway2k, 1_024);
  }

  /** The constellation figures, expanded against the catalogue that is already
   * on the GPU. Both have to have landed, and either may land first. */
  setLines(catalog: StarCatalog | null, pairs: LinePairs | null): void {
    if (!this.lineScene || !catalog || !pairs) return;
    const vertices = lineVertices(catalog, pairs);
    if (vertices) this.lineScene.upload(vertices);
  }

  /** One frame. Returns whether the star catalogue actually landed on it,
   * which is what tells the page the designed CSS sky can go. */
  draw(
    stars: StarCatalog | null,
    matrix: Mat3,
    light: boolean,
    dpr: number,
    instant: boolean,
    highlight: Highlight | null,
    options: SkyOptions = {},
  ): boolean {
    if (!SKY_DEBUG) {
      return this.gl
        ? this.drawGl(stars, matrix, light, dpr, instant, highlight, options)
        : this.drawFlat(stars, matrix, light, dpr, highlight);
    }
    // The measured path blocks on the GPU. That is the only honest way to
    // time a frame from here, and exactly what production must not do.
    const startedAt = performance.now();
    const drawn = this.gl
      ? this.drawGl(stars, matrix, light, dpr, instant, highlight, options)
      : this.drawFlat(stars, matrix, light, dpr, highlight);
    this.gl?.finish();
    this.frames.push(performance.now() - startedAt);
    if (this.frames.length > 600) this.frames.shift();
    return drawn;
  }

  /** Milliseconds a frame took, GPU included: last, median, worst. */
  frameStats(): { last: number; median: number; max: number } {
    const sorted = [...this.frames].sort((a, b) => a - b);
    return {
      last: this.frames.at(-1) ?? 0,
      median: sorted[sorted.length >> 1] ?? 0,
      max: sorted.at(-1) ?? 0,
    };
  }

  private drawGl(
    stars: StarCatalog | null,
    matrix: Mat3,
    light: boolean,
    dpr: number,
    instant: boolean,
    highlight: Highlight | null,
    options: SkyOptions,
  ): boolean {
    const gl = this.gl!;
    const canvas = this.canvas!;
    const width = Math.round(innerWidth * dpr);
    const height = Math.round(innerHeight * dpr);
    if (canvas.width !== width) canvas.width = width;
    if (canvas.height !== height) canvas.height = height;

    gl.viewport(0, 0, width, height);
    gl.disable(gl.BLEND);
    gl.clearColor(0, 0, 0, 0);
    gl.clear(gl.COLOR_BUFFER_BIT);
    this.bandScene?.draw(matrix, light, width, height, instant);
    // Between the band and the stars: a figure is behind the sky it names.
    if (options.lines) this.lineScene?.draw(matrix, light, dpr, width, height);

    if (!stars || !this.starScene) return false;
    if (!this.uploaded) {
      this.starScene.upload(stars);
      this.uploaded = true;
    }
    this.starScene.draw(matrix, light, dpr, width, height, highlight, options.twinkle);
    return true;
  }

  private drawFlat(
    stars: StarCatalog | null,
    matrix: Mat3,
    light: boolean,
    dpr: number,
    highlight: Highlight | null,
  ): boolean {
    const canvas = this.flat;
    const ctx = canvas?.getContext('2d', { alpha: true });
    if (!canvas || !ctx) return false;

    const [width, height] = [innerWidth, innerHeight];
    if (canvas.width !== Math.round(width * dpr)) canvas.width = Math.round(width * dpr);
    if (canvas.height !== Math.round(height * dpr)) canvas.height = Math.round(height * dpr);

    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, width, height);
    // Additive: the canvas leaves its own alpha at zero, so in the light theme
    // the dusk gradient underneath shows between the stars.
    ctx.globalCompositeOperation = 'lighter';

    if (this.milkyway) this.paintFallbackBand(ctx, matrix, width, height, light);
    if (!stars) return false;
    if (!this.look || this.look.light !== light || this.look.dpr !== dpr) {
      this.atlas ??= new SpriteAtlas();
      this.look = buildLook(stars, light, dpr, this.atlas);
    }
    paintStars(ctx, stars, this.look, matrix, { width, height, dpr }, highlight);
    return true;
  }

  private paintFallbackBand(
    ctx: CanvasRenderingContext2D,
    matrix: Mat3,
    width: number,
    height: number,
    light: boolean,
  ): void {
    const [bw, bh] = bandSize(width, height);
    if (!this.band || this.band.width !== bw || this.band.height !== bh) {
      this.band = document.createElement('canvas');
      this.band.width = bw;
      this.band.height = bh;
      this.bandDrawnAt = 0;
    }
    const now = performance.now();
    if (now - this.bandDrawnAt >= BAND_INTERVAL_MS) {
      this.bandDrawnAt = now;
      const bandCtx = this.band.getContext('2d');
      if (bandCtx && this.milkyway) paintBand(bandCtx, this.milkyway, matrix, light);
    }
    ctx.imageSmoothingEnabled = true;
    ctx.imageSmoothingQuality = 'high';
    ctx.drawImage(this.band, 0, 0, width, height);
  }
}
