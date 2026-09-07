/** The star catalogue, drawn as points of light rather than as sprites.
 *
 * One `GL_POINTS` pass over all 12,191 stars, accumulated *additively into a
 * float buffer*, and only then tone mapped to the screen. That order is the
 * whole design. A star's brightness spans five decades — Sirius is some seven
 * hundred times a magnitude-6 star — and an 8-bit framebuffer holds two. Any
 * renderer that maps magnitude straight to a pixel value has to invent a curve
 * to squeeze that in, and the curve is what makes stars read as tiers of
 * discs. Accumulating linear flux and taking `1 − exp(−x·exposure)` at the end
 * instead means the bright stars saturate their cores to white while their
 * wings, still on the linear part of the curve, keep the colour the catalogue
 * gave them — which is what a star actually looks like.
 *
 * The fragment profile is two terms (`tuning.ts` carries the numbers):
 *
 * - an energy-conserving Gaussian core, `exp(−d²/2σ²)/2πσ²` in *device*
 *   pixels, so a star between two pixels lands half in each and a star sliding
 *   across the sky fades from one to the next instead of blinking.
 * - a glare wing, `√b · k/(d+ε)²` in *CSS* pixels, which is the scattering in
 *   the eye rather than anything in the sky — it is why a bright star looks
 *   large. Its radius is bounded (Celestia's roughly-one-degree rule, 24 px
 *   here) and the profile is faded out before the point's own quad edge, so
 *   there is no rim and no `discard`.
 *
 * This shares the band's context and canvas: the band draws the Milky Way
 * first, the stars land over it. `star-field.ts` is the same picture painted
 * on a 2D canvas, kept only for a browser with no WebGL2.
 */

import { type StarCatalog } from './catalog';
import { floatTarget, fullscreenTriangle, link, uniforms } from './gl-util';
import { FOCAL, type Mat3 } from './sidereal';
import { applyView } from './sidereal';
import {
  GLARE_EPS_PX,
  MAX_STAR_PX,
  NIGHT,
  type StarResponse,
  TUNING,
  TWILIGHT,
  TWILIGHT_MAG_LIMIT,
  GLARE_FLOOR,
} from './tuning';
import type { Highlight } from './star-field';
import {
  POINT_FRAG,
  POINT_UNIFORMS,
  POINT_VERT,
  RING_FRAG,
  RING_VERT,
  TONE_FRAG,
  TONE_VERT,
} from './star-shaders';

/** Bytes per star in the GPU buffer: three f32 of direction, one of magnitude,
 * and four bytes of colour. The id and kind the catalogue carries are for
 * looking a star up, not for drawing it, and stay on the CPU. */
const VERTEX_STRIDE = 20;

export class StarScene {
  private readonly pointProgram: WebGLProgram;
  private readonly toneProgram: WebGLProgram;
  private readonly ringProgram: WebGLProgram;
  private readonly pointUniform: Record<string, WebGLUniformLocation | null>;
  private readonly toneUniform: Record<string, WebGLUniformLocation | null>;
  private readonly ringUniform: Record<string, WebGLUniformLocation | null>;
  private readonly quad: WebGLVertexArrayObject;
  private readonly ringVao: WebGLVertexArrayObject;
  private readonly starVao: WebGLVertexArrayObject;
  private readonly starBuffer: WebGLBuffer;

  /** `null` where neither float-target extension exists; the point pass then
   * tone maps in its own fragment and adds straight to the screen. */
  readonly float: 'half' | 'full' | null;
  private target: WebGLFramebuffer | null = null;
  private targetTexture: WebGLTexture | null = null;
  private targetSize: [number, number] = [0, 0];

  private count = 0;
  private twilightCount = 0;

  private constructor(private readonly gl: WebGL2RenderingContext) {
    this.pointProgram = link(gl, POINT_VERT, POINT_FRAG);
    this.pointUniform = uniforms(gl, this.pointProgram, POINT_UNIFORMS);
    this.toneProgram = link(gl, TONE_VERT, TONE_FRAG);
    this.toneUniform = uniforms(gl, this.toneProgram, ['u_hdr', 'u_exposure']);
    this.ringProgram = link(gl, RING_VERT, RING_FRAG);
    this.ringUniform = uniforms(gl, this.ringProgram, [
      'u_centre_px',
      'u_viewport',
      'u_radius_px',
      'u_ink',
    ]);
    this.quad = fullscreenTriangle(gl);
    this.ringVao = gl.createVertexArray()!;
    this.starVao = gl.createVertexArray()!;
    this.starBuffer = gl.createBuffer()!;
    this.float = floatTarget(gl);
  }

  static create(gl: WebGL2RenderingContext): StarScene | null {
    try {
      return new StarScene(gl);
    } catch {
      return null;
    }
  }

  /** Hand the GPU the catalogue, once. */
  upload(catalog: StarCatalog): void {
    const { gl } = this;
    const bytes = new ArrayBuffer(catalog.count * VERTEX_STRIDE);
    const view = new DataView(bytes);
    for (let index = 0; index < catalog.count; index += 1) {
      const at = index * VERTEX_STRIDE;
      for (let axis = 0; axis < 3; axis += 1) {
        view.setFloat32(at + axis * 4, catalog.position[index * 3 + axis]!, true);
      }
      view.setFloat32(at + 12, catalog.magnitude[index]!, true);
      for (let channel = 0; channel < 3; channel += 1) {
        view.setUint8(at + 16 + channel, Math.round(catalog.color[index * 3 + channel]! * 255));
      }
      view.setUint8(at + 19, 255);
    }

    gl.bindVertexArray(this.starVao);
    gl.bindBuffer(gl.ARRAY_BUFFER, this.starBuffer);
    gl.bufferData(gl.ARRAY_BUFFER, bytes, gl.STATIC_DRAW);
    gl.enableVertexAttribArray(0);
    gl.vertexAttribPointer(0, 3, gl.FLOAT, false, VERTEX_STRIDE, 0);
    gl.enableVertexAttribArray(1);
    gl.vertexAttribPointer(1, 1, gl.FLOAT, false, VERTEX_STRIDE, 12);
    gl.enableVertexAttribArray(2);
    gl.vertexAttribPointer(2, 3, gl.UNSIGNED_BYTE, true, VERTEX_STRIDE, 16);
    gl.bindVertexArray(null);

    this.count = catalog.count;
    // The catalogue is sorted brightest first, so the twilight cut is a prefix.
    let cut = 0;
    while (cut < catalog.count && catalog.magnitude[cut]! <= TWILIGHT_MAG_LIMIT) cut += 1;
    this.twilightCount = cut;
  }

  get ready(): boolean {
    return this.count > 0;
  }

  /** The float target the pass accumulates into, sized to the frame. */
  private resize(width: number, height: number): boolean {
    if (!this.float) return false;
    const { gl } = this;
    if (this.target && this.targetSize[0] === width && this.targetSize[1] === height) {
      return true;
    }
    if (this.target) gl.deleteFramebuffer(this.target);
    if (this.targetTexture) gl.deleteTexture(this.targetTexture);
    this.targetTexture = gl.createTexture();
    gl.bindTexture(gl.TEXTURE_2D, this.targetTexture);
    const internal = this.float === 'half' ? gl.RGBA16F : gl.RGBA32F;
    const type = this.float === 'half' ? gl.HALF_FLOAT : gl.FLOAT;
    gl.texImage2D(gl.TEXTURE_2D, 0, internal, width, height, 0, gl.RGBA, type, null);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    this.target = gl.createFramebuffer();
    gl.bindFramebuffer(gl.FRAMEBUFFER, this.target);
    gl.framebufferTexture2D(
      gl.FRAMEBUFFER,
      gl.COLOR_ATTACHMENT0,
      gl.TEXTURE_2D,
      this.targetTexture,
      0,
    );
    const complete =
      gl.checkFramebufferStatus(gl.FRAMEBUFFER) === gl.FRAMEBUFFER_COMPLETE;
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
    if (!complete) {
      gl.deleteFramebuffer(this.target);
      this.target = null;
      return false;
    }
    this.targetSize = [width, height];
    return true;
  }

  /** One frame of stars, over whatever the band already put on the canvas.
   * `width`/`height` are device pixels; the canvas is sized by the caller. */
  draw(
    matrix: Mat3,
    light: boolean,
    dpr: number,
    width: number,
    height: number,
    highlight: Highlight | null,
  ): void {
    if (!this.count) return;
    const { gl } = this;
    const response = light ? TWILIGHT : NIGHT;
    const hdr = this.resize(width, height);

    gl.enable(gl.BLEND);
    gl.blendFunc(gl.ONE, gl.ONE);
    if (hdr) {
      gl.bindFramebuffer(gl.FRAMEBUFFER, this.target);
      gl.clearColor(0, 0, 0, 0);
      gl.clear(gl.COLOR_BUFFER_BIT);
    }

    this.drawPoints(matrix, light, dpr, width, height, response, hdr);

    if (hdr) {
      gl.bindFramebuffer(gl.FRAMEBUFFER, null);
      gl.viewport(0, 0, width, height);
      gl.useProgram(this.toneProgram);
      gl.activeTexture(gl.TEXTURE0);
      gl.bindTexture(gl.TEXTURE_2D, this.targetTexture);
      gl.uniform1i(this.toneUniform.u_hdr!, 0);
      gl.uniform1f(this.toneUniform.u_exposure!, response.exposure);
      gl.bindVertexArray(this.quad);
      gl.drawArrays(gl.TRIANGLES, 0, 3);
    }

    if (highlight) this.drawRing(matrix, dpr, width, height, highlight);
    gl.bindVertexArray(null);
    gl.disable(gl.BLEND);
  }

  private drawPoints(
    matrix: Mat3,
    light: boolean,
    dpr: number,
    width: number,
    height: number,
    response: StarResponse,
    hdr: boolean,
  ): void {
    const { gl } = this;
    gl.viewport(0, 0, width, height);
    gl.useProgram(this.pointProgram);
    const set = (name: string, value: number): void =>
      gl.uniform1f(this.pointUniform[name]!, value);
    gl.uniformMatrix3fv(this.pointUniform.u_view!, false, new Float32Array(matrix));
    set('u_f', FOCAL);
    set('u_aspect', width / Math.max(1, height));
    set('u_dpr', dpr);
    set('u_mag_ref', response.magRef);
    set('u_extinction_k', TUNING.extinctionK);
    set('u_glare_k', response.glareK);
    set('u_glare_exp', response.glareExp);
    set('u_sigma', response.coreSigmaPx);
    set('u_floor', GLARE_FLOOR);
    set('u_max_px', MAX_STAR_PX);
    set('u_whiten', response.whiten);
    set('u_twilight', light ? 1 : 0);
    set('u_eps', GLARE_EPS_PX);
    // With no float target the tone curve has nowhere to run but here, so
    // each star arrives already curved and the additive blend piles up
    // slightly-too-bright overlaps. It is the fallback, not the picture.
    set('u_fold', hdr ? 0 : 1);
    set('u_exposure', response.exposure);
    gl.bindVertexArray(this.starVao);
    gl.drawArrays(gl.POINTS, 0, light ? this.twilightCount : this.count);
  }

  private drawRing(
    matrix: Mat3,
    dpr: number,
    width: number,
    height: number,
    highlight: Highlight,
  ): void {
    const [vx, vy, vz] = applyView(matrix, highlight.position);
    if (vz <= 0.08) return;
    const { gl } = this;
    const aspect = width / Math.max(1, height);
    const x = (((vx / vz) * FOCAL) / aspect + 1) * 0.5 * width;
    const y = (1 + (vy / vz) * FOCAL) * 0.5 * height;
    gl.useProgram(this.ringProgram);
    gl.uniform2f(this.ringUniform.u_centre_px!, x, y);
    gl.uniform2f(this.ringUniform.u_viewport!, width, height);
    gl.uniform1f(
      this.ringUniform.u_radius_px!,
      (14 + 6 * (1 - highlight.strength)) * dpr,
    );
    const ink = 0.85 * highlight.strength;
    gl.uniform4f(this.ringUniform.u_ink!, 0.745 * ink, 0.882 * ink, ink, ink);
    gl.bindVertexArray(this.ringVao);
    gl.drawArrays(gl.LINE_LOOP, 0, 64);
  }
}
