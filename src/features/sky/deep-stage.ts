/** The deep sky's place in the running page: when it starts, what wakes it,
 * and what it will tell you if you ask.
 *
 * `stage.ts` owns the clock, the observer and the frame; this owns the one
 * thing S2 adds to them — a half-second tick that decides which tiles the view
 * wants next — and keeps that out of the frame loop entirely. The stage's only
 * duties are to hand over a view, to say when the view jumped, and to let this
 * time a frame when someone asks for numbers.
 */

import { DeepLayer } from './deep-gl';
import { type StreamDebug, type StreamView, TICK_MS, TileStreamer } from './lod-stream';
import type { SkyRenderer } from './sky-render';

export interface DeepReadout {
  /** Milliseconds per frame, GPU included: last, median, worst. */
  frameMs: { last: number; median: number; max: number };
  points: number;
  residentTiles: number;
  gpuBytes: number;
  stream: StreamDebug | null;
}

/** The debug readout is opt-in and off by default. It calls `gl.finish()`,
 * which stalls the pipeline on purpose to get an honest frame time — exactly
 * what production must not do. */
export const SKY_DEBUG =
  typeof location !== 'undefined' && location.search.includes('skydebug=1');

export class DeepStreaming {
  private layer: DeepLayer | null = null;
  private streamer: TileStreamer | null = null;
  private timer = 0;
  private live = true;
  private still = false;
  private starting = false;

  constructor(
    private readonly sky: SkyRenderer,
    private readonly view: () => StreamView,
    private readonly redraw: () => void,
  ) {}

  /** `g9.bin` first and whole, then the tile tick. Everything here is
   * best-effort: a refused manifest or a corrupt file leaves the bright
   * catalogue as the sky, which is what it was a moment ago. */
  async start(): Promise<void> {
    const layer = this.sky.deepLayer();
    if (!layer || !this.live || this.still || this.starting) return;
    this.starting = true;
    const streamer = new TileStreamer(layer);
    try {
      await streamer.start();
    } catch {
      return;
    }
    if (!this.live) {
      streamer.dispose();
      return;
    }
    this.layer = layer;
    this.streamer = streamer;
    this.redraw();
    // The readout is a function rather than a snapshot: whoever asks gets the
    // frame times and the queue as they are when they ask.
    if (SKY_DEBUG) {
      (window as unknown as { __sky: () => DeepReadout }).__sky = () => this.readout();
    }
    if (!streamer.enabled) return;
    this.timer = window.setInterval(() => this.step(), TICK_MS);
  }

  /** A still sky — reduced motion — has no deep sky at all: not the tiles,
   * which arrive over minutes, and not `g9.bin`, whose landing is itself a
   * repaint. "Draws the sky once and then leaves it alone" is the whole of
   * what that visitor asked for. Turning motion back on starts the stream. */
  setStill(still: boolean): void {
    this.still = still;
    if (!still && !this.streamer) void this.start();
  }

  private step(): void {
    if (!this.live || this.still || !this.streamer) return;
    this.streamer.step(this.view(), performance.now());
    // A still sky — reduced motion, or paused — has no loop to carry a tile
    // onto the screen, so the tick that landed it asks for the frame.
    this.redraw();
  }

  /** The view jumped: what is in flight was chosen for the old sky. */
  invalidate(): void {
    this.streamer?.invalidate();
  }

  dispose(): void {
    this.live = false;
    clearInterval(this.timer);
    this.streamer?.dispose();
  }

  readout(): DeepReadout {
    return {
      frameMs: this.sky.frameStats(),
      points: this.sky.points,
      residentTiles: this.layer?.residentTiles ?? 0,
      gpuBytes: this.layer?.bytes ?? 0,
      stream: this.streamer?.debug() ?? null,
    };
  }
}
