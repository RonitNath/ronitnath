/** The deep sky on the GPU: `g9.bin` in one buffer, the tiles in another.
 *
 * Both are drawn by the star pass's own program (`stars-gl.ts` hands this an
 * interface that sets the uniforms and issues the draw), because the point of
 * S2 is that a G = 8 star is *exactly* as bright as the law says a G = 8 star
 * is. Two things about the passes differ, and both are arguments to the law
 * rather than departures from it:
 *
 * - `g9.bin` — 165,393 stars from the bright catalogue's floor down to G = 9 —
 *   gets the full profile, glare and all. These are stars a dark sky shows.
 * - the tiles — G 9 to 12, three million of them — get the core alone: glare
 *   weight zero and the point clamped to the core's own width. A twelfth
 *   magnitude star is a thousandth of the reference flux; drawn with a wing it
 *   would be a *dot*, and three million dots is not a sky. Drawn as core-only
 *   flux they sum into the grain the eye reads as depth.
 *
 * The tile buffer is a fixed set of slots, allocated once. A tile lands in a
 * slot, is drawn as one range of that buffer, and is overwritten when the sky
 * turns away from it — so the GPU's memory here is a constant, not a function
 * of how long the page has been open.
 */

import { countBrighterThan, type DeepStars, deepMagLimit, TILE_COUNT, RA_BINS } from './lod';
import type { DeepSink } from './lod-stream';
import { MAX_STAR_PX, type StarResponse, TWILIGHT_MAG_LIMIT } from './tuning';
import { COVERAGE_UNIT, NO_MAG_LIMIT, type PointPass, VERTEX_STRIDE } from './stars-gl';

/** Records per slot, and slots. 4,096 x 146 is 598,016 records — the ~600k the
 * plan budgets — at 20 bytes each, so the tile buffer is a flat 12 MB however
 * the sky is streamed. A tile with more records than a slot holds keeps its
 * brightest 4,096, which is the same magnitude cut the renderer applies
 * anyway, taken at upload; 255 of the 768 tiles are over, all of them in the
 * galactic plane where the grain is already dense enough to read. */
export const SLOT_RECORDS = 4_096;
export const SLOT_COUNT = 146;

/** How long a tile takes to reach full strength, and to leave when it is
 * evicted. Long enough that nothing pops, short enough that the streaming is
 * visible as it happens. */
const FADE_MS = 600;

/** The tile pass's point size, in CSS pixels: the core Gaussian's own width
 * (3 sigma, doubled) and not a pixel more. This is what "sub-pixel flux"
 * means in the plan — the quad exists to hold the Gaussian, not to be seen. */
const DEEP_MAX_PX = 4;

interface Slot {
  slot: number;
  count: number;
}

export class DeepLayer implements DeepSink {
  readonly capacity = SLOT_COUNT;

  private readonly g9Vao: WebGLVertexArrayObject;
  private readonly g9Buffer: WebGLBuffer;
  private readonly tileVao: WebGLVertexArrayObject;
  private readonly tileBuffer: WebGLBuffer;

  private g9Count = 0;
  private g9Magnitude: Float32Array = new Float32Array(0);
  private readonly slots = new Map<number, Slot>();
  private readonly free: number[] = [];
  private highWater = 0;

  /** The streaming coverage field, one cell per tile: how much of that tile is
   * on the GPU *and* faded in. The shader samples it bilinearly and erodes it,
   * so the tile pass draws at full strength only where a tile's neighbours
   * have landed too, and fades to nothing across the outermost tile of
   * whatever has. Half a kilobyte, re-uploaded per frame. */
  private readonly coverage = new Float32Array(TILE_COUNT);
  private readonly coverageBytes = new Uint8Array(TILE_COUNT);
  private readonly coverageTexture: WebGLTexture;
  private coverageAt = 0;

  constructor(private readonly gl: WebGL2RenderingContext) {
    this.g9Vao = gl.createVertexArray()!;
    this.g9Buffer = gl.createBuffer()!;
    this.tileVao = gl.createVertexArray()!;
    this.tileBuffer = gl.createBuffer()!;
    this.bind(this.tileVao, this.tileBuffer);
    gl.bufferData(gl.ARRAY_BUFFER, SLOT_COUNT * SLOT_RECORDS * VERTEX_STRIDE, gl.DYNAMIC_DRAW);
    gl.bindVertexArray(null);
    for (let slot = SLOT_COUNT - 1; slot >= 0; slot -= 1) this.free.push(slot);

    this.coverageTexture = gl.createTexture()!;
    gl.activeTexture(gl.TEXTURE0 + COVERAGE_UNIT);
    gl.bindTexture(gl.TEXTURE_2D, this.coverageTexture);
    gl.texImage2D(
      gl.TEXTURE_2D,
      0,
      gl.R8,
      RA_BINS,
      TILE_COUNT / RA_BINS,
      0,
      gl.RED,
      gl.UNSIGNED_BYTE,
      this.coverageBytes,
    );
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
    // Right ascension wraps and declination does not.
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.REPEAT);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    gl.activeTexture(gl.TEXTURE0);
  }

  /** Bytes of GPU vertex memory this layer holds — reported rather than
   * guessed at, because "bounded" is a claim with a number behind it. */
  get bytes(): number {
    return (SLOT_COUNT * SLOT_RECORDS + this.g9Count) * VERTEX_STRIDE;
  }

  get residentTiles(): number {
    return this.slots.size;
  }

  private bind(vao: WebGLVertexArrayObject, buffer: WebGLBuffer): void {
    const { gl } = this;
    gl.bindVertexArray(vao);
    gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
    gl.enableVertexAttribArray(0);
    gl.vertexAttribPointer(0, 3, gl.FLOAT, false, VERTEX_STRIDE, 0);
    gl.enableVertexAttribArray(1);
    gl.vertexAttribPointer(1, 1, gl.FLOAT, false, VERTEX_STRIDE, 12);
    gl.enableVertexAttribArray(2);
    gl.vertexAttribPointer(2, 3, gl.UNSIGNED_BYTE, true, VERTEX_STRIDE, 16);
  }

  /** The same 20-byte vertex the bright catalogue uses, so one program draws
   * all three buffers. */
  private static pack(stars: DeepStars, from: number, count: number): ArrayBuffer {
    const bytes = new ArrayBuffer(count * VERTEX_STRIDE);
    const view = new DataView(bytes);
    for (let index = 0; index < count; index += 1) {
      const at = index * VERTEX_STRIDE;
      const source = from + index;
      for (let axis = 0; axis < 3; axis += 1) {
        view.setFloat32(at + axis * 4, stars.position[source * 3 + axis]!, true);
      }
      view.setFloat32(at + 12, stars.magnitude[source]!, true);
      for (let channel = 0; channel < 3; channel += 1) {
        view.setUint8(at + 16 + channel, stars.color[source * 3 + channel]!);
      }
      view.setUint8(at + 19, 255);
    }
    return bytes;
  }

  setG9(stars: DeepStars): void {
    const { gl } = this;
    this.bind(this.g9Vao, this.g9Buffer);
    gl.bufferData(gl.ARRAY_BUFFER, DeepLayer.pack(stars, 0, stars.count), gl.STATIC_DRAW);
    gl.bindVertexArray(null);
    this.g9Count = stars.count;
    this.g9Magnitude = stars.magnitude;
  }

  hasTile(id: number): boolean {
    return this.slots.has(id);
  }

  putTile(id: number, stars: DeepStars): void {
    const existing = this.slots.get(id);
    const slot = existing?.slot ?? this.free.pop();
    if (slot === undefined) return;
    const count = Math.min(stars.count, SLOT_RECORDS);
    const { gl } = this;
    gl.bindBuffer(gl.ARRAY_BUFFER, this.tileBuffer);
    gl.bufferSubData(
      gl.ARRAY_BUFFER,
      slot * SLOT_RECORDS * VERTEX_STRIDE,
      new Uint8Array(DeepLayer.pack(stars, 0, count)),
    );
    gl.bindBuffer(gl.ARRAY_BUFFER, null);
    this.slots.set(id, { slot, count });
    this.highWater = Math.max(this.highWater, slot + 1);
  }

  dropTile(id: number): void {
    const slot = this.slots.get(id);
    if (!slot) return;
    this.slots.delete(id);
    this.free.push(slot.slot);
  }

  /** Ease every cell toward what is resident and hand the field to the GPU.
   * One pass over 768 floats and a 768-byte upload, which is why the fade can
   * be per frame rather than per tick. */
  private uploadCoverage(nowMs: number): void {
    const elapsed = this.coverageAt === 0 ? 0 : nowMs - this.coverageAt;
    this.coverageAt = nowMs;
    const step = Math.min(1, elapsed / FADE_MS);
    for (let id = 0; id < TILE_COUNT; id += 1) {
      const target = this.slots.has(id) ? 1 : 0;
      const value = this.coverage[id]! + (target - this.coverage[id]!) * step;
      this.coverage[id] = Math.abs(value - target) < 0.002 ? target : value;
      this.coverageBytes[id] = Math.round(this.coverage[id]! * 255);
    }
    const { gl } = this;
    gl.activeTexture(gl.TEXTURE0 + COVERAGE_UNIT);
    gl.bindTexture(gl.TEXTURE_2D, this.coverageTexture);
    gl.texSubImage2D(
      gl.TEXTURE_2D,
      0,
      0,
      0,
      RA_BINS,
      TILE_COUNT / RA_BINS,
      gl.RED,
      gl.UNSIGNED_BYTE,
      this.coverageBytes,
    );
    gl.activeTexture(gl.TEXTURE0);
  }

  /** Both deep passes, into whatever framebuffer the star pass has bound.
   * Returns how many points were drawn, which is what the debug readout and
   * the performance gate report. */
  draw(
    pass: PointPass,
    response: StarResponse,
    light: boolean,
    dpr: number,
    nowMs: number,
  ): number {
    let drawn = 0;
    if (this.g9Count) {
      // In the light theme the sky is already two thirds of white and the
      // twilight cut is the whole catalogue's rule, so g9 keeps it and
      // effectively disappears — which is the light theme staying as it was.
      const count = light
        ? countBrighterThan(this.g9Magnitude, TWILIGHT_MAG_LIMIT)
        : this.g9Count;
      if (count) {
        pass.points(this.g9Vao, 0, count, {
          glareK: response.glareK,
          maxPx: MAX_STAR_PX,
          magLimit: NO_MAG_LIMIT,
        });
        drawn += count;
      }
    }
    if (light || !this.highWater) return drawn;

    // One draw for every slot ever filled. The coverage field decides what is
    // visible — including an evicted tile's records, which fade out where they
    // stand rather than vanishing — so there is nothing per tile to set.
    this.uploadCoverage(nowMs);
    const points = this.highWater * SLOT_RECORDS;
    pass.points(this.tileVao, 0, points, {
      glareK: 0,
      maxPx: DEEP_MAX_PX,
      magLimit: deepMagLimit(dpr),
      deep: true,
    });
    return drawn + points;
  }
}
