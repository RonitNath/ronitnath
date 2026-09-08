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

import { simTimeMs, syncedSimTimeMs } from './clock';
import { type DeepReadout, DeepStreaming, SKY_DEBUG } from './deep-stage';
import type { NamedStar, StarCatalog } from './catalog';
import type { CityCatalog } from './cities';
import { GlobeView } from './globe-stage';
import { dragTo } from './globe-math';
import { grounding, UPDATE_INTERVAL_MS } from './label';
import { COLOUR_RAMP } from './lod';
import { Observer, TRANSITION_MS, type Point } from './observer';
import type { StreamView } from './lod-stream';
import type { PickHit } from './pick';
import { type DebugStar, StagePicking } from './pick-stage';
import { type Vec3, viewMatrix } from './sidereal';
import { SkyRenderer } from './sky-render';
import { loadSkyAssets } from './stage-assets';
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
  private readonly globe = new GlobeView();
  private readonly picking = new StagePicking(
    () => viewMatrix(this.simMs(), ...this.observerNow()),
    () => {
      if (this.reduced || this.paused) this.draw();
    },
  );

  private stars: StarCatalog | null = null;
  private named: NamedStar[] = [];
  private vectors: Vec3[] = [];
  private cities: CityCatalog | null = null;

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
    this.globe.attach(canvas);
  }

  /** Whether the globe is drawing through WebGL rather than the flat disc. */
  get hasWebgl(): boolean {
    return this.globe.accelerated;
  }

  start(): void {
    // The readout is a function rather than a snapshot: whoever asks gets the
    // frame times, the queue and what is on screen as they are when they ask.
    // Opt-in, and unreachable without typing `?skydebug=1`.
    if (SKY_DEBUG) {
      (window as unknown as { __sky: () => DeepReadout & { named: DebugStar[] } }).__sky =
        () => ({ ...this.deep.readout(), named: this.debugNamed() });
    }
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
    const ringed = this.picking.ringed;
    if (ringed) return { position: ringed.position, strength: 1 };
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
    this.globe.draw(lat, lon, simMs, document.documentElement.dataset.theme === 'light');
  }

  // --- picking -------------------------------------------------------------

  /** The pick's own state is in `pick-stage.ts`; the stage supplies it the one
   * view matrix everything else is drawn through, so the pick, the ring and
   * the drawn sky cannot disagree about where a star is. */
  pickAt(clientX: number, clientY: number): PickHit | null {
    return this.picking.pickAt(clientX, clientY);
  }

  /** The star a named-star callout points at, as a pick. */
  brightHit(brightIndex: number): PickHit | null {
    return this.picking.brightHit(brightIndex);
  }

  setHover(hit: PickHit | null): void {
    this.picking.setHover(hit);
  }

  setPinned(hit: PickHit | null): void {
    this.picking.setPinned(hit);
  }

  screenPosition(position: Vec3): [number, number] | null {
    return this.picking.screenPosition(position);
  }

  /** Every named star on screen, for `?skydebug=1` and nothing else. */
  debugNamed(): DebugStar[] {
    return this.picking.debugNamed(this.named);
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
    const [lat, lon] = this.observerNow();
    return this.globe.pointAt(clientX, clientY, lat, lon);
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

  /** The fetch order lives in `stage-assets.ts`; what each arrival *means* is
   * here, because it is the stage's own state that changes. */
  private loadAssets(): void {
    void loadSkyAssets({
      live: () => this.live,
      stars: (catalog) => {
        this.stars = catalog;
        this.picking.picker.setCatalog(catalog, COLOUR_RAMP);
        this.draw();
      },
      named: (named, vectors) => {
        this.vectors = vectors;
        this.named = named.stars;
        this.picking.picker.setNames(
          new Map(named.stars.map((star) => [star.brightIndex, star.name] as const)),
        );
        this.publish();
      },
      cities: (cities) => {
        this.cities = cities;
        this.publish();
      },
      band: () =>
        this.sky.loadBand().then(() => {
          if (this.live) this.draw();
        }),
      globe: async () => {
        await this.globe.loadTextures();
        if (this.live) this.draw();
      },
      deep: async () => {
        await this.deep.start();
        // g9 is the second half of what a pointer can pick, and it has only
        // just landed.
        this.picking.picker.setG9(this.sky.deepLayer()?.g9 ?? null);
      },
    });
  }
}
