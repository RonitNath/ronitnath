/** The tile streamer: what to fetch, how fast, and what to forget.
 *
 * It ticks twice a second rather than every frame — the sky turns a quarter of
 * a degree in that time and a tile is seven degrees across, so there is
 * nothing a per-frame decision could know that this one does not — and each
 * tick it re-scores the whole grid (`lod-tiles.ts`), fetches the best missing
 * tiles at most two at a time, and drops the least recently wanted ones once
 * the GPU's slots are full.
 *
 * Two things bound it, and both are budgets rather than limits:
 *
 * - *bytes*. 51 MB of tiles exist; a landing page may not spend them. A token
 *   bucket paces the stream at {@link TILE_RATE_BYTES_PER_S} with a burst, so
 *   the first minute buys the neighbourhood of the zenith and the rest arrives
 *   over the minutes a visitor might actually stay.
 * - *slots*. The GPU buffer holds a fixed number of tiles (`deep-gl.ts`); past
 *   that the least recently needed one is evicted, which is exactly the tile
 *   the sky has turned away from.
 *
 * A view jump — a drag of the globe, or resuming the orbit — aborts everything
 * in flight: those tiles were chosen for a sky that is no longer on screen.
 */

import {
  type DeepStars,
  LOD_HEADER_LEN,
  LOD_RECORD_BYTES,
  type LodManifest,
  parseDeep,
  parseManifest,
} from './lod';
import { SLOT_RECORDS } from './deep-gl';
import { coveredRadiusRad, frameRadiusRad, rankTiles, zenithOf } from './lod-tiles';
import { observerAt } from './track';
import { type Mat3, type Vec3, viewMatrix } from './sidereal';

export const TICK_MS = 500;

/** Sustained tile bandwidth, and the burst the stream opens with.
 *
 * The plan's budget is 6 MB of sky in five minutes. The page's own assets and
 * `g9.bin` are 4.1 of those, so the tiles have about two, and the shape of the
 * spend is the only thing left to choose. It is front-loaded: a 640 KB burst
 * is the ten tiles directly overhead, which is a cone of deep sky wide enough
 * to see, and 5 KB/s after it widens that cone for as long as someone stays.
 * Spending the same two megabytes evenly instead would mean a minute of
 * nothing followed by a minute of nearly nothing.
 */
export const TILE_RATE_BYTES_PER_S = 5_000;
export const TILE_BURST_BYTES = 640_000;

/** `?skyfill=1` lifts the budget entirely: it is how the performance gate
 * fills every slot in under a minute to measure a frame with 600k deep points
 * on it. Debug only, alongside `?skydebug=1`, and never a code path a visitor
 * can be on by accident. */
const UNPACED = typeof location !== 'undefined' && location.search.includes('skyfill=1');

const MAX_IN_FLIGHT = 2;

/** How far ahead along the orbit to ask, in simulated minutes. The sky moves
 * 60x, so this is one to three seconds of wall time and a degree and a half of
 * arc — the orbit-ahead set is usually one tile beyond the frame's, which is
 * what it should be. A longer lead would be fetching for a sky the visitor may
 * never see. */
const AHEAD_MINUTES = [1, 2, 3];

/** Where the deep stars go once they are decoded. `deep-gl.ts` implements it;
 * the streamer knows nothing about GL. */
export interface DeepSink {
  /** How many tiles may be resident at once. */
  readonly capacity: number;
  setG9(stars: DeepStars): void;
  putTile(id: number, stars: DeepStars): void;
  dropTile(id: number): void;
  hasTile(id: number): boolean;
}

export interface StreamView {
  matrix: Mat3;
  aspect: number;
  simMs: number;
}

export interface StreamDebug {
  order: number[];
  resident: number[];
  fetched: number[];
  inFlight: number[];
  bytes: number;
  coveredDeg: number;
  tilesEnabled: boolean;
}

/** Whether this device should stream tiles at all.
 *
 * `saveData` is the visitor saying so outright. A coarse pointer on a small
 * viewport is a phone: fewer pixels to put grain on, a connection that is
 * often someone's data plan, and a battery. Both get `g9.bin` — one file, once
 * — and nothing after it.
 */
export function tilesAllowed(): boolean {
  const connection = (navigator as { connection?: { saveData?: boolean } }).connection;
  if (connection?.saveData) return false;
  const coarse = matchMedia('(pointer: coarse)').matches;
  return !(coarse && Math.min(innerWidth, innerHeight) < 700);
}

export class TileStreamer {
  private manifest: LodManifest | null = null;
  private readonly resident = new Map<number, number>();
  private readonly inFlight = new Map<number, AbortController>();
  private readonly fetched: number[] = [];
  private order: number[] = [];
  private tick = 0;
  private bucket = TILE_BURST_BYTES;
  private bucketAt = 0;
  private bytes = 0;
  private covered = 0;
  private live = true;
  private tiles = false;

  constructor(
    private readonly sink: DeepSink,
    private readonly path = '/stars/lod',
  ) {}

  /** The manifest and `g9.bin`, in that order, after the page has painted.
   * Resolves once g9 is on the GPU; tiles start on the next tick. */
  async start(): Promise<void> {
    const response = await fetch(`${this.path}/manifest.json`, { cache: 'force-cache' });
    if (!response.ok) throw new Error(`manifest: ${response.status}`);
    this.manifest = parseManifest(await response.json());
    this.tiles = tilesAllowed();
    const g9 = await this.load(`${this.path}/${this.manifest.g9.file}`);
    if (!this.live) return;
    this.sink.setG9(g9);
  }

  get enabled(): boolean {
    return this.tiles && this.manifest !== null;
  }

  dispose(): void {
    this.live = false;
    this.abortInFlight();
  }

  /** A view jump: what is in flight was chosen for the old sky. */
  invalidate(): void {
    this.abortInFlight();
  }

  private abortInFlight(): void {
    for (const controller of this.inFlight.values()) controller.abort();
    this.inFlight.clear();
  }

  private async load(url: string, init?: RequestInit): Promise<DeepStars> {
    const response = await fetch(url, { cache: 'force-cache', ...init });
    if (!response.ok) throw new Error(`${url}: ${response.status}`);
    const bytes = new Uint8Array(await response.arrayBuffer());
    this.bytes += bytes.byteLength;
    return parseDeep(bytes);
  }

  /** What a tile actually costs, which is not what it weighs.
   *
   * A tile on the galactic plane holds twenty thousand stars and the GPU keeps
   * four thousand of them; paying 324 KB to draw 64 KB is how the whole budget
   * went on six tiles in an early cut of this. The files are sorted brightest
   * first, so the front of one *is* the magnitude cut the renderer would apply
   * anyway, and the rest is never asked for. */
  private prefixBytes(count: number): number | null {
    if (count <= SLOT_RECORDS || !this.manifest?.sortedByMagnitude) return null;
    return LOD_HEADER_LEN + SLOT_RECORDS * LOD_RECORD_BYTES;
  }

  /** One decision, twice a second. */
  step(view: StreamView, nowMs: number): void {
    const manifest = this.manifest;
    if (!this.live || !manifest || !this.tiles) return;
    this.tick += 1;
    this.refill(nowMs);

    const zenith = zenithOf(view.matrix);
    const frameRadius = frameRadiusRad(view.aspect);
    const needs = rankTiles(zenith, frameRadius, this.aheadZenith(view));
    this.order = needs.map((need) => need.id);
    for (const id of this.order.slice(0, this.sink.capacity)) {
      if (this.resident.has(id)) this.resident.set(id, this.tick);
    }

    // Not a drawing decision — the renderer has the resident set itself and
    // fades between what it has and what it does not (`deep-gl.ts`). This is
    // the streaming's one honest scalar: how wide a cone of sky is complete.
    this.covered = coveredRadiusRad(zenith, new Set(this.resident.keys()), frameRadius);
    void this.fetchNext(needs.map((need) => need.id));
  }

  /** Where the same view maths says the zenith will be, one to three simulated
   * minutes along the orbit. The observer moves *and* the sky turns, and
   * `viewMatrix` composes both, so this asks it rather than adding a rotation
   * of its own. */
  private aheadZenith(view: StreamView): Vec3[] {
    return AHEAD_MINUTES.map((minutes) => {
      const at = view.simMs + minutes * 60_000;
      const [lat, lon] = observerAt(at);
      return zenithOf(viewMatrix(at, lat, lon));
    });
  }

  private refill(nowMs: number): void {
    if (this.bucketAt === 0) {
      this.bucketAt = nowMs;
      return;
    }
    const seconds = Math.max(0, (nowMs - this.bucketAt) / 1_000);
    this.bucketAt = nowMs;
    this.bucket = Math.min(TILE_BURST_BYTES, this.bucket + seconds * TILE_RATE_BYTES_PER_S);
  }

  private async fetchNext(wanted: readonly number[]): Promise<void> {
    const manifest = this.manifest!;
    for (const id of wanted) {
      if (this.inFlight.size >= MAX_IN_FLIGHT) return;
      if (this.resident.has(id) || this.inFlight.has(id)) continue;
      const tile = manifest.tiles[id]!;
      if (tile.count === 0) continue;
      const cost = this.prefixBytes(tile.count) ?? tile.bytes;
      if (!UNPACED && this.bucket < cost) return;
      if (!this.makeRoom(id)) return;
      this.bucket -= cost;
      void this.fetchTile(id, `${this.path}/${tile.file}`, this.prefixBytes(tile.count));
    }
  }

  /** Evict until a slot is free. The victim is the tile wanted longest ago,
   * which after a tick of re-scoring is the one furthest behind the view. A
   * tile that is *currently* wanted more than the candidate is never evicted
   * for it — that would be a fetch loop. */
  private makeRoom(wantedId: number): boolean {
    if (this.resident.size + this.inFlight.size < this.sink.capacity) return true;
    let victim = -1;
    let oldest = Infinity;
    for (const [id, at] of this.resident) {
      if (at < oldest) [victim, oldest] = [id, at];
    }
    if (victim < 0 || oldest >= this.tick) return false;
    const rank = this.order.indexOf(wantedId);
    if (rank < 0 || rank >= this.sink.capacity) return false;
    this.resident.delete(victim);
    this.sink.dropTile(victim);
    return true;
  }

  private async fetchTile(id: number, url: string, prefix: number | null): Promise<void> {
    const controller = new AbortController();
    this.inFlight.set(id, controller);
    try {
      const stars = await this.load(url, {
        signal: controller.signal,
        headers: prefix ? { Range: `bytes=0-${prefix - 1}` } : undefined,
      });
      if (!this.live) return;
      this.sink.putTile(id, stars);
      this.resident.set(id, this.tick);
      this.fetched.push(id);
    } catch {
      // Aborted, refused or corrupt: the sky is missing some grain, which is
      // the failure this whole layer is allowed to have.
    } finally {
      this.inFlight.delete(id);
    }
  }

  debug(): StreamDebug {
    return {
      order: this.order.slice(0, 24),
      resident: [...this.resident.keys()],
      fetched: [...this.fetched],
      inFlight: [...this.inFlight.keys()],
      bytes: this.bytes,
      coveredDeg: (this.covered * 180) / Math.PI,
      tilesEnabled: this.tiles,
    };
  }
}
