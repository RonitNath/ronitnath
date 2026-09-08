import { readFileSync } from 'node:fs';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { type DeepStars, LOD_HEADER_LEN, LOD_RECORD_BYTES } from '../lod';
import { frameRadiusRad, rankTiles, zenithOf } from '../lod-tiles';
import {
  type DeepSink,
  type StreamView,
  TILE_BURST_BYTES,
  TILE_RATE_BYTES_PER_S,
  TileStreamer,
} from '../lod-stream';
import { viewMatrix } from '../sidereal';

/** A `GDR3LOD1` file of `count` identical faint stars — the streamer cares
 * about lengths and order, not about where the stars are. */
function blob(count: number): Uint8Array {
  const bytes = new Uint8Array(LOD_HEADER_LEN + count * LOD_RECORD_BYTES);
  bytes.set([...'GDR3LOD1'].map((character) => character.charCodeAt(0)));
  new DataView(bytes.buffer).setUint32(8, count, true);
  return bytes;
}

const manifest = JSON.parse(readFileSync('public/stars/lod/manifest.json', 'utf8'));

class Sink implements DeepSink {
  capacity = 4;
  g9: DeepStars | null = null;
  readonly tiles = new Set<number>();
  readonly dropped: number[] = [];

  setG9(stars: DeepStars): void {
    this.g9 = stars;
  }
  putTile(id: number): void {
    this.tiles.add(id);
  }
  dropTile(id: number): void {
    this.tiles.delete(id);
    this.dropped.push(id);
  }
  hasTile(id: number): boolean {
    return this.tiles.has(id);
  }
}

/** Every fetch the streamer makes, answered from the real manifest so the
 * byte budget is spent on the real file sizes. `hold` keeps a tile's response
 * pending, which is how the abort case is set up. */
function server(options: { hold?: boolean } = {}): {
  calls: string[];
  ranges: (string | null)[];
  signals: AbortSignal[];
  settle: () => void;
} {
  const calls: string[] = [];
  const ranges: (string | null)[] = [];
  const signals: AbortSignal[] = [];
  const pending: (() => void)[] = [];
  vi.stubGlobal('fetch', (url: string, init?: RequestInit) => {
    calls.push(url);
    const range = (init?.headers as Record<string, string> | undefined)?.Range ?? null;
    if (!url.endsWith('manifest.json')) ranges.push(range);
    if (init?.signal) signals.push(init.signal);
    const body = url.endsWith('manifest.json')
      ? { ok: true, json: async () => manifest }
      : {
          ok: true,
          arrayBuffer: async () => {
            const name = url.split('/').at(-1)!;
            const id = Number(name.replace('.bin', ''));
            const count = Number.isNaN(id) ? manifest.g9.count : manifest.tiles[id].count;
            const bytes = blob(count);
            // A ranged request gets the front of the file, header and all,
            // exactly as a static server answers one.
            const end = /bytes=0-(\d+)/.exec(range ?? '');
            return (end ? bytes.slice(0, Number(end[1]) + 1) : bytes).buffer;
          },
        };
    if (!options.hold || url.endsWith('manifest.json') || url.endsWith('g9.bin')) {
      return Promise.resolve(body);
    }
    return new Promise((resolve, reject) => {
      init?.signal?.addEventListener('abort', () => reject(new Error('aborted')));
      pending.push(() => resolve(body));
    });
  });
  return { calls, ranges, signals, settle: () => pending.forEach((resolve) => resolve()) };
}

function viewAt(simMs: number): StreamView {
  return { matrix: viewMatrix(simMs, 37.7749, -122.4194), aspect: 16 / 9, simMs };
}

/** Let every already-resolved promise run. */
const flush = async (): Promise<void> => {
  for (let round = 0; round < 32; round += 1) await Promise.resolve();
};

beforeEach(() => {
  vi.stubGlobal('matchMedia', () => ({ matches: false }));
  vi.stubGlobal('innerWidth', 1_440);
  vi.stubGlobal('innerHeight', 900);
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('the streamer', () => {
  it('takes g9 whole, then the tiles the view is under', async () => {
    const calls = server().calls;
    const sink = new Sink();
    const streamer = new TileStreamer(sink);
    await streamer.start();
    expect(sink.g9?.count).toBe(manifest.g9.count);
    expect(calls).toEqual(['/stars/lod/manifest.json', '/stars/lod/g9.bin']);

    const view = viewAt(Date.UTC(2026, 2, 3, 4, 5));
    streamer.step(view, 1_000);
    await flush();

    const wanted = rankTiles(zenithOf(view.matrix), frameRadiusRad(16 / 9)).map((n) => n.id);
    const first = calls
      .slice(2)
      .map((url) => Number(url.split('/').at(-1)!.replace('.bin', '')));
    // Two in flight at a time, and both of them from the head of the queue.
    expect(first).toHaveLength(2);
    expect(wanted.slice(0, 4)).toContain(first[0]);
    expect(sink.tiles.size).toBe(2);
  });

  it('drops the tile wanted longest ago when the slots are full', async () => {
    server();
    const sink = new Sink();
    const streamer = new TileStreamer(sink);
    await streamer.start();

    // Half a simulated day later the sky is a different sky, so the tiles the
    // first ticks fetched are the ones nothing wants any more.
    const start = Date.UTC(2026, 2, 3, 4, 5);
    const early: number[] = [];
    // Ten seconds apart, because the byte budget is real: at the half-second
    // tick the burst runs out after two or three of these tiles.
    for (let tick = 0; tick < 6; tick += 1) {
      streamer.step(viewAt(start), 1_000 + tick * 10_000);
      await flush();
      if (tick === 1) early.push(...sink.tiles);
    }
    expect(sink.tiles.size).toBe(sink.capacity);

    for (let tick = 0; tick < 8; tick += 1) {
      streamer.step(viewAt(start + 6 * 3_600_000), 120_000 + tick * 10_000);
      await flush();
    }
    expect(sink.tiles.size).toBeLessThanOrEqual(sink.capacity);
    expect(sink.dropped.length).toBeGreaterThan(0);
    // What was evicted is what the first ticks fetched.
    expect(early.some((id) => sink.dropped.includes(id))).toBe(true);
  });

  it('asks a dense tile only for the stars it will draw', async () => {
    const { calls, ranges } = server();
    const sink = new Sink();
    sink.capacity = 60;
    const streamer = new TileStreamer(sink);
    await streamer.start();
    for (let tick = 0; tick < 12; tick += 1) {
      streamer.step(viewAt(Date.UTC(2026, 6, 20, 8, 0)), 1_000 + tick * 5_000);
      await flush();
    }
    const asked = calls.slice(2).map((url, index) => ({
      count: manifest.tiles[Number(url.split('/').at(-1)!.replace('.bin', ''))].count,
      range: ranges[index + 1] ?? null,
    }));
    expect(asked.length).toBeGreaterThan(4);
    for (const { count, range } of asked) {
      // Under a slot's worth of stars the whole file is the cheapest thing to
      // ask for; over it, only the brightest 4,096 are.
      expect(range).toBe(count > 4_096 ? `bytes=0-${12 + 4_096 * 16 - 1}` : null);
    }
    // Every resident tile is still a whole tile as far as the renderer knows.
    expect(sink.tiles.size).toBeGreaterThan(4);
  });

  it('pays for tiles out of a budget', async () => {
    const calls = server().calls;
    const sink = new Sink();
    sink.capacity = 200;
    const streamer = new TileStreamer(sink);
    await streamer.start();
    for (let tick = 0; tick < 40; tick += 1) {
      streamer.step(viewAt(Date.UTC(2026, 2, 3, 4, 5)), 1_000 + tick * 500);
      await flush();
    }
    const spent = calls
      .slice(2)
      .map((url) => manifest.tiles[Number(url.split('/').at(-1)!.replace('.bin', ''))].bytes)
      .map((bytes: number) => Math.min(bytes, 12 + 4_096 * 16))
      .reduce((total: number, bytes: number) => total + bytes, 0);
    // The burst plus twenty seconds of trickle, and not the 51 MB on disk.
    expect(spent).toBeLessThan(TILE_BURST_BYTES + 30 * TILE_RATE_BYTES_PER_S);
    expect(spent).toBeGreaterThan(100_000);
  });

  it('abandons what is in flight when the view jumps', async () => {
    const { signals } = server({ hold: true });
    const sink = new Sink();
    const streamer = new TileStreamer(sink);
    await streamer.start();
    streamer.step(viewAt(Date.UTC(2026, 2, 3, 4, 5)), 1_000);
    await flush();
    expect(signals).toHaveLength(2);
    expect(signals.every((signal) => signal.aborted)).toBe(false);

    streamer.invalidate();
    await flush();
    expect(signals.every((signal) => signal.aborted)).toBe(true);
    expect(sink.tiles.size).toBe(0);
  });

  it('leaves a phone on a data plan with g9 alone', async () => {
    vi.stubGlobal('matchMedia', () => ({ matches: true }));
    vi.stubGlobal('innerWidth', 390);
    vi.stubGlobal('innerHeight', 844);
    const calls = server().calls;
    const streamer = new TileStreamer(new Sink());
    await streamer.start();
    streamer.step(viewAt(Date.UTC(2026, 2, 3, 4, 5)), 1_000);
    await flush();
    expect(streamer.enabled).toBe(false);
    expect(calls).toEqual(['/stars/lod/manifest.json', '/stars/lod/g9.bin']);
  });
});
