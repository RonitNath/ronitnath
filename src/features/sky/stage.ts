/** One owner for the clock, the observer and the two canvases.
 *
 * Everything the island draws is a function of (server time, the observer).
 * Holding both here is what keeps the sky, the globe's marker and the
 * grounding caption from disagreeing.
 *
 * Nothing here is required for the page to be a page. Every attachment is
 * best-effort: no canvas, no WebGL, a failed fetch or reduced motion each
 * leave the CSS starfield as the picture and the document intact.
 */

import { keepOutFor, place, type Placement } from './annotate';
import { BandScene } from './band-gl';
import { ASSETS, loadCities, loadImageData, loadNamed, loadStars } from './assets';
import { simTimeMs, syncedSimTimeMs } from './clock';
import { type NamedStar, namedVectors, type StarCatalog } from './catalog';
import type { CityCatalog } from './cities';
import { GlobeScene } from './globe-gl';
import { paintFlatGlobe } from './globe-flat';
import { dragTo, RADIUS, unproject } from './globe-math';
import { grounding, UPDATE_INTERVAL_MS } from './label';
import { Observer, TRANSITION_MS, type Point } from './observer';
import { type Mat3, type Vec3, viewMatrix } from './sidereal';
import { SpriteAtlas } from './sprites';
import {
  bandSize,
  buildLook,
  type Highlight,
  paintBand,
  paintStars,
  type StarLook,
} from './star-field';

/** ≤30 fps. At 60× the sky moves 15 arcminutes a second, which is a pixel and
 * a half per frame at this scale — smooth, and half the main-thread cost of a
 * 60 fps loop that would show the same motion. */
const FRAME_INTERVAL_MS = 33;

/** The Milky Way is diffuse and its warp is per-pixel, so it is re-sampled on
 * its own slower cadence and scaled up between times. */
const BAND_INTERVAL_MS = 250;

const HIGHLIGHT_MS = 1_800;

/** What the HTML parts of the island render from. Callout *positions* are not
 * in here: they move every frame, and pushing them through React state is what
 * made the labels step across the sky instead of gliding. React is told the
 * named stars; the positions go to the DOM (see `callouts.tsx`). */
export interface Readout {
  grounding: string;
  named: NamedStar[];
  manual: boolean;
  paused: boolean;
}

export class Stage {
  private readonly mountMs = performance.timeOrigin + performance.now();
  private readonly observer = new Observer();
  private readonly listeners = new Set<(readout: Readout) => void>();

  private skyCanvas: HTMLCanvasElement | null = null;
  private bandCanvas: HTMLCanvasElement | null = null;
  private bandScene: BandScene | null = null;
  private globeCanvas: HTMLCanvasElement | null = null;
  private globeScene: GlobeScene | null = null;
  private atlas: SpriteAtlas | null = null;

  private stars: StarCatalog | null = null;
  private look: StarLook | null = null;
  private named: NamedStar[] = [];
  private vectors: Vec3[] = [];
  private cities: CityCatalog | null = null;
  private milkyway: ImageData | null = null;
  private earth: ImageData | null = null;

  private band: HTMLCanvasElement | null = null;
  private bandDrawnAt = 0;
  private lastFrameAt = 0;
  private highlight: (Highlight & { until: number }) | null = null;

  private frame = 0;
  private timer = 0;
  private catalogDrawn = false;
  private live = true;
  private reduced = false;
  private paused = false;
  private frozenSimMs: number | null = null;

  constructor(private readonly serverEpochMs: number) {
    this.reduced = matchMedia('(prefers-reduced-motion: reduce)').matches;
    if (this.reduced) this.frozenSimMs = simTimeMs(serverEpochMs);
  }

  /** The simulation instant on screen right now. */
  simMs(): number {
    if (this.frozenSimMs !== null) return this.frozenSimMs;
    return syncedSimTimeMs(this.serverEpochMs, this.mountMs, Date.now());
  }

  /** Where the view is from right now, resolving any move in flight. */
  observerNow(): Point {
    return this.observer.resolve(this.simMs(), performance.now());
  }

  subscribe(listener: (readout: Readout) => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  private publish(): void {
    const [lat, lon] = this.observerNow();
    // Position first, place second (label.ts): the coordinates need no fetch
    // and are what makes the line read as live, so they are on screen from the
    // first tick and the city joins them when its catalog lands.
    const city = this.cities?.nearest(lat, lon) ?? null;
    const readout: Readout = {
      grounding: grounding(lat, lon, city),
      named: this.named,
      manual: this.observer.isManual(),
      paused: this.paused,
    };
    for (const listener of this.listeners) listener(readout);
  }

  /** Where the named stars are *now*, through the same view the canvas last
   * drew with. The callout loop asks for this every frame; nothing about it
   * goes through React, and nothing else recomputes it. */
  placements(): Placement[] {
    if (!this.vectors.length) return [];
    const [lat, lon] = this.observerNow();
    return place(
      this.vectors,
      viewMatrix(this.simMs(), lat, lon),
      this.aspect(),
      undefined,
      keepOutFor(innerWidth, innerHeight),
    );
  }

  /** The catalog colour of a named star: what the callout's ring is drawn in,
   * so the ring says which star as well as where. */
  namedColor(index: number): string | null {
    const star = this.named[index];
    if (!star || !this.stars) return null;
    const at = star.brightIndex * 3;
    const byte = (offset: number): number =>
      Math.round((this.stars!.color[at + offset] ?? 1) * 255);
    return `rgb(${byte(0)},${byte(1)},${byte(2)})`;
  }

  private aspect(): number {
    return innerHeight > 0 ? innerWidth / innerHeight : 1.6;
  }

  attachSky(canvas: HTMLCanvasElement | null): void {
    this.skyCanvas = canvas;
  }

  /** The Milky Way's own canvas, beneath the stars. WebGL2 draws it at full
   * resolution; without WebGL2 the CPU warp paints into the star canvas as
   * before. */
  attachBand(canvas: HTMLCanvasElement | null): void {
    this.bandCanvas = canvas;
    this.bandScene = canvas ? BandScene.create(canvas) : null;
  }

  attachGlobe(canvas: HTMLCanvasElement | null): void {
    this.globeCanvas = canvas;
    this.globeScene = canvas ? GlobeScene.create(canvas) : null;
  }

  /** Whether the globe is drawing through WebGL rather than the flat disc. */
  get hasWebgl(): boolean {
    return this.globeScene !== null;
  }

  start(): void {
    // The sky is background: nothing is fetched, decoded or painted until the
    // main thread is free, and the CSS starfield holds the frame until then.
    const idle = window.requestIdleCallback ?? ((fn: () => void) => setTimeout(fn, 200));
    idle(() => {
      if (!this.live) return;
      this.draw();
      this.publish();
      this.resume();
      this.loadAssets();
    });
    this.timer = window.setInterval(() => {
      if (this.live) this.publish();
    }, UPDATE_INTERVAL_MS);
  }

  dispose(): void {
    this.live = false;
    delete document.documentElement.dataset.sky;
    cancelAnimationFrame(this.frame);
    clearInterval(this.timer);
    this.listeners.clear();
  }

  private resume(): void {
    cancelAnimationFrame(this.frame);
    if (!this.live || this.reduced || this.paused) return;
    const tick = (): void => {
      if (!this.live || this.reduced || this.paused) return;
      const now = performance.now();
      if (now - this.lastFrameAt >= FRAME_INTERVAL_MS) {
        this.lastFrameAt = now;
        this.draw();
      }
      this.frame = requestAnimationFrame(tick);
    };
    this.frame = requestAnimationFrame(tick);
  }

  setReducedMotion(reduced: boolean): void {
    this.reduced = reduced;
    this.frozenSimMs = reduced ? this.simMs() : null;
    if (reduced) {
      cancelAnimationFrame(this.frame);
      this.draw();
      this.publish();
    } else {
      this.resume();
    }
  }

  setPaused(paused: boolean): void {
    this.paused = paused;
    this.frozenSimMs = paused ? this.simMs() : null;
    if (paused) cancelAnimationFrame(this.frame);
    else this.resume();
    this.draw();
    this.publish();
  }

  /** One frame, drawn on demand — a theme flip under reduced motion has to ask
   * for the frame the loop would otherwise have supplied. */
  redraw(): void {
    this.band = null;
    this.look = null;
    this.draw();
    this.publish();
  }

  private draw(): void {
    const simMs = this.simMs();
    const [lat, lon] = this.observerNow();
    this.drawSky(simMs, lat, lon);
    this.drawGlobe(simMs, lat, lon);
  }

  private drawSky(simMs: number, lat: number, lon: number): void {
    const dpr = Math.min(devicePixelRatio || 1, 2);
    const light = document.documentElement.dataset.theme === 'light';
    const matrix = viewMatrix(simMs, lat, lon);
    // The band is its own canvas underneath, so the shader owns the whole
    // frame and the stars keep a 2D context they can draw sprites into.
    this.bandScene?.draw(matrix, light, dpr, this.reduced);

    const canvas = this.skyCanvas;
    const ctx = canvas?.getContext('2d', { alpha: true });
    if (!canvas || !ctx) return;

    const [width, height] = [innerWidth, innerHeight];
    if (canvas.width !== Math.round(width * dpr)) canvas.width = Math.round(width * dpr);
    if (canvas.height !== Math.round(height * dpr)) canvas.height = Math.round(height * dpr);

    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, width, height);
    // Additive: the canvas leaves its own alpha at zero, so in the light theme
    // the dusk gradient underneath shows between the stars.
    ctx.globalCompositeOperation = 'lighter';

    // Only when there is no shader to draw it with.
    if (!this.bandScene && this.milkyway) this.drawBand(ctx, matrix, width, height, light);
    if (this.stars) {
      if (!this.look || this.look.light !== light || this.look.dpr !== dpr) {
        this.atlas ??= new SpriteAtlas();
        this.look = buildLook(this.stars, light, dpr, this.atlas);
      }
      paintStars(
        ctx,
        this.stars,
        this.look,
        matrix,
        { width, height, dpr },
        this.currentHighlight(),
      );
      // The CSS starfield was the picture until this moment. Now that the real
      // catalog is on screen the two would be one sky over another, so the
      // designed one is faded out and stays out (`atmosphere.css`).
      if (!this.catalogDrawn) {
        this.catalogDrawn = true;
        document.documentElement.dataset.sky = 'live';
      }
    }
  }

  private drawBand(
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

  private currentHighlight(): Highlight | null {
    const highlight = this.highlight;
    if (!highlight) return null;
    const left = highlight.until - performance.now();
    if (left <= 0) {
      this.highlight = null;
      return null;
    }
    return { position: highlight.position, strength: left / HIGHLIGHT_MS };
  }

  private drawGlobe(simMs: number, lat: number, lon: number): void {
    const canvas = this.globeCanvas;
    if (!canvas) return;
    const light = document.documentElement.dataset.theme === 'light';
    const ink = light ? [1, 0.93, 0.72] : [1, 0.42, 0.33];
    if (this.globeScene) {
      this.globeScene.draw(lat, lon, simMs, ink);
      return;
    }
    const ctx = canvas.getContext('2d');
    if (!ctx || !this.earth) return;
    const size = Math.round(
      Math.max(1, canvas.clientWidth) * Math.min(devicePixelRatio || 1, 2),
    );
    if (canvas.width !== size) {
      canvas.width = size;
      canvas.height = size;
    }
    ctx.clearRect(0, 0, size, size);
    paintFlatGlobe(ctx, this.earth, lat, lon);
    ctx.fillStyle = light ? 'rgb(255,237,184)' : 'rgb(255,107,84)';
    ctx.beginPath();
    ctx.arc(size / 2, size / 2, Math.max(2.5, (size * RADIUS) / 44), 0, Math.PI * 2);
    ctx.fill();
  }

  // --- the controls, all of which write the one observer -------------------

  setObserver(lat: number, lon: number, travel: boolean): void {
    const duration = travel && !this.reduced ? TRANSITION_MS : 0;
    this.observer.set(lat, lon, this.simMs(), performance.now(), duration);
    this.redrawSoon();
  }

  resumeOrbit(): void {
    this.observer.resume(this.simMs(), performance.now(), this.reduced ? 0 : TRANSITION_MS);
    this.redrawSoon();
  }

  drag(dx: number, dy: number): void {
    const [lat, lon] = this.observerNow();
    const [nextLat, nextLon] = dragTo(lat, lon, dx, dy);
    this.setObserver(nextLat, nextLon, false);
  }

  /** Which point on Earth a pointer event landed on, if it hit the globe. */
  pointAt(clientX: number, clientY: number): Point | null {
    const canvas = this.globeCanvas;
    if (!canvas) return null;
    const bounds = canvas.getBoundingClientRect();
    if (bounds.width <= 0 || bounds.height <= 0) return null;
    const [lat, lon] = this.observerNow();
    const x = (2 * ((clientX - bounds.left) / bounds.width) - 1) / RADIUS;
    const y = (1 - 2 * ((clientY - bounds.top) / bounds.height)) / RADIUS;
    return unproject(x, y, lat, lon);
  }

  /** Ring a star in the canvas for a moment: the callout points at something,
   * and this is what it points at. */
  ringStar(position: Vec3): void {
    this.highlight = { position, strength: 1, until: performance.now() + HIGHLIGHT_MS };
    this.redrawSoon();
  }

  private redrawSoon(): void {
    if (this.reduced || this.paused) this.draw();
    this.publish();
  }

  // --- assets --------------------------------------------------------------

  /** The catalog first and alone: the sky is the picture, and on a slow link
   * every other byte in flight is a byte the stars are waiting behind. The
   * globe's three textures are the largest and the last, because the globe
   * draws as a blue sphere in the meantime and the corner of the frame is not
   * what a visitor is waiting for. */
  private loadAssets(): void {
    void (async () => {
      try {
        const stars = await loadStars();
        if (!this.live) return;
        this.stars = stars;
        this.draw();
        const named = await loadNamed();
        if (!this.live) return;
        this.vectors = namedVectors(stars, named);
        this.named = named.stars;
        this.publish();
      } catch {
        // No catalog is no stars; the CSS starfield is still the picture.
      }
      await Promise.allSettled([
        loadCities().then((cities) => {
          if (!this.live) return;
          this.cities = cities;
          this.publish();
        }),
        this.loadBand(),
      ]);
      if (!this.live) return;
      await this.loadGlobeTextures();
    })();
  }

  /** The Milky Way map, to whichever renderer is drawing it. The shader takes
   * the image straight to a texture; the fallback needs it decoded to pixels
   * it can sample on the CPU. */
  private async loadBand(): Promise<void> {
    if (this.bandScene) {
      await this.bandScene.loadMap(ASSETS.milkyway);
      if (this.live) this.draw();
      return;
    }
    const band = await loadImageData(ASSETS.milkyway, 1_024);
    if (!this.live) return;
    this.milkyway = band;
    this.draw();
  }

  private async loadGlobeTextures(): Promise<void> {
    try {
      if (this.globeScene) {
        await this.globeScene.loadTextures({
          day: ASSETS.earthDay,
          normal: ASSETS.earthNormal,
          specular: ASSETS.earthSpecular,
        });
      } else {
        this.earth = await loadImageData(ASSETS.earthDay, 1_024);
      }
      if (this.live) this.draw();
    } catch {
      // The globe keeps the neutral blue it starts on.
    }
  }
}
