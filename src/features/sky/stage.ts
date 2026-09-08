/** One owner for the clock, the observer and the canvases.
 *
 * Everything the island draws is a function of (server time, the observer).
 * Holding both here is what keeps the sky, the globe's marker and the
 * grounding caption from disagreeing.
 *
 * Nothing here is required for the page to be a page. Every attachment is
 * best-effort: no canvas, no WebGL, a failed fetch or reduced motion each
 * leave the CSS starfield as the picture and the document intact.
 */

import { keepOutFor, place, placementAvoid, type Placement } from './annotate';
import { ASSETS, loadCities, loadImageData, loadNamed, loadStars } from './assets';
import { simTimeMs, syncedSimTimeMs } from './clock';
import { DeepStreaming } from './deep-stage';
import { type NamedStar, namedVectors, type StarCatalog } from './catalog';
import type { CityCatalog } from './cities';
import { GlobeScene } from './globe-gl';
import { paintFlatGlobe } from './globe-flat';
import { dragTo, RADIUS, unproject } from './globe-math';
import { grounding, UPDATE_INTERVAL_MS } from './label';
import { Observer, TRANSITION_MS, type Point } from './observer';
import type { StreamView } from './lod-stream';
import { type Vec3, viewMatrix } from './sidereal';
import { SkyRenderer } from './sky-render';
import type { Highlight } from './star-field';

/** ≤30 fps. At 60× the sky moves 15 arcminutes a second, which is a pixel and
 * a half per frame at this scale — smooth, and half the main-thread cost of a
 * 60 fps loop that would show the same motion. */
const FRAME_INTERVAL_MS = 33;

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

  private readonly sky = new SkyRenderer();
  private readonly deep: DeepStreaming;
  private globeCanvas: HTMLCanvasElement | null = null;
  private globeScene: GlobeScene | null = null;

  private stars: StarCatalog | null = null;
  private named: NamedStar[] = [];
  private vectors: Vec3[] = [];
  private cities: CityCatalog | null = null;
  private earth: ImageData | null = null;

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
    const view = (): StreamView => this.streamView();
    this.deep = new DeepStreaming(this.sky, view, () => this.draw());
    this.reduced = matchMedia('(prefers-reduced-motion: reduce)').matches;
    this.deep.setStill(this.reduced);
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
      placementAvoid(innerWidth, innerHeight),
    );
  }

  /** The catalog colour of a named star: the callout's ring is drawn in it, so
   * the ring says which star as well as where. */
  namedColor(index: number): string | null {
    const star = this.named[index];
    if (!star || !this.stars) return null;
    const at = star.brightIndex * 3;
    const byte = (offset: number): number =>
      Math.round((this.stars!.color[at + offset] ?? 1) * 255);
    return `rgb(${byte(0)},${byte(1)},${byte(2)})`;
  }

  /** What the tile queue scores against (`lod-tiles.ts`). */
  private streamView(): StreamView {
    const [lat, lon] = this.observerNow();
    const simMs = this.simMs();
    return { matrix: viewMatrix(simMs, lat, lon), aspect: this.aspect(), simMs };
  }

  private aspect(): number {
    return innerHeight > 0 ? innerWidth / innerHeight : 1.6;
  }

  /** The sky's canvases: the WebGL2 one that ships and the 2D one that stands
   * in for it (`sky-render.ts` picks between them). */
  attachSky(canvas: HTMLCanvasElement | null, flat: HTMLCanvasElement | null): void {
    this.sky.attach(canvas, flat);
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
    this.deep.dispose();
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
    this.deep.setStill(reduced);
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
    this.sky.reset();
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
    const drawn = this.sky.draw(
      this.stars,
      viewMatrix(simMs, lat, lon),
      document.documentElement.dataset.theme === 'light',
      Math.min(devicePixelRatio || 1, 2),
      // Under reduced motion the sky is one still frame, and the band's
      // reveal is an animation.
      this.reduced,
      this.currentHighlight(),
    );
    // The CSS starfield was the picture until the first real frame. Now that
    // the catalog is on screen the two would be one sky over another, so the
    // designed one is faded out and stays out (`atmosphere.css`).
    if (drawn && !this.catalogDrawn) {
      this.catalogDrawn = true;
      document.documentElement.dataset.sky = 'live';
    }
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
    this.deep.invalidate(); // in flight for a sky that just left the screen
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
        this.sky.loadBand().then(() => {
          if (this.live) this.draw();
        }),
      ]);
      if (!this.live) return;
      await this.loadGlobeTextures();
      if (!this.live) return;
      // Last and largest, competing with nothing (`deep-stage.ts`).
      await this.deep.start();
    })();
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
