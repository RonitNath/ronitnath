/** The Milky Way, drawn by a fragment shader.
 *
 * `sky/milkyway-<hash>.webp` is Gaia star counts binned onto an equal-area
 * grid — HEALPix level 9, 0.115 degrees, 3.07 million pixels — and baked in
 * equatorial coordinates. Getting it on screen is a per-pixel
 * operation — the inverse of the star pass's projection, then an
 * equirectangular lookup — and the CPU warp this replaces could only afford it
 * into a 192 px buffer, which scaled up to blobs rather than to the galaxy.
 *
 * So the band gets the shader it had before the rebuild (`shaders.rs`'s
 * `SKY_FRAG`), ported whole: inverse-view sampling, `pow(rgb, shape) · gain`
 * tone mapping, secant-airmass extinction, the twilight gate, an alpha that
 * covers the colour it carries, and the reveal that fades it in when the map
 * lands.
 *
 * It is the first pass on the sky canvas and the stars (`stars-gl.ts`) are the
 * second, sharing one context: the caller sizes the canvas, clears it, and
 * runs the two in that order.
 */

import { fullscreenTriangle, link, uniforms } from './gl-util';
import type { Mat3 } from './sidereal';
import { FOCAL } from './sidereal';
import { TUNING } from './tuning';

/** How long the band takes to arrive once its map has decoded. A texture that
 * appears between one frame and the next reads as a flash. */
export const REVEAL_MS = 900;

const VERT = `#version 300 es
layout(location = 0) in vec2 a_xy;
out vec2 v_ndc;
void main() {
    v_ndc = a_xy;
    gl_Position = vec4(a_xy, 0.0, 1.0);
}
`;

const FRAG = `#version 300 es
precision highp float;
in vec2 v_ndc;
uniform mat3 u_view;
uniform float u_f;
uniform float u_aspect;
uniform float u_light;
uniform float u_reveal;
uniform sampler2D u_map;
uniform float u_extinction_k;
uniform float u_band_gain;
uniform float u_band_shape;
out vec4 frag;

const float PI = 3.14159265359;

void main() {
    // Undo the star pass's projection to recover this pixel's view-space ray.
    // The camera looks at the zenith, so ray.z is the cosine of the zenith
    // angle: 1.0 straight up, about 0.38 in the corners.
    vec3 ray = normalize(vec3(v_ndc.x * u_aspect / u_f, v_ndc.y / u_f, 1.0));

    float airmass = 1.0 / max(ray.z, 0.05);
    float extinction = pow(10.0, -0.4 * u_extinction_k * (airmass - 1.0));

    // The star pass maps J2000 -> view; texturing the sky needs the inverse,
    // and the view basis is orthonormal, so its transpose is that inverse.
    vec3 j2000 = transpose(u_view) * ray;
    vec2 uv = vec2(
        atan(j2000.y, j2000.x) / (2.0 * PI) + 0.5,
        0.5 - asin(clamp(j2000.z, -1.0, 1.0)) / PI
    );

    // The exponent crushes the diffuse floor (which otherwise reads as
    // overcast cloud) and keeps the bright core. Twilight needs the harder
    // curve and less gain: it paints onto a lit sky, where the amplitude that
    // reads as the galaxy on black reads as smog.
    float shape = mix(u_band_shape, 2.7, u_light);
    float gain = mix(u_band_gain, 0.46, u_light);
    // The map wraps in u, and the hardware picks a mipmap level from the
    // screen-space derivatives of the coordinate it is handed. Across the
    // seam at RA 180 those derivatives jump by a whole turn for the one 2x2
    // quad that straddles it, which selects the coarsest level there and draws
    // a dashed dark curve across the band. Wrapping the derivatives back into
    // [-0.5, 0.5] and sampling with them explicitly is the fix; nothing else
    // about the sample changes.
    vec2 ddx = dFdx(uv);
    vec2 ddy = dFdy(uv);
    ddx.x -= round(ddx.x);
    ddy.x -= round(ddy.x);
    vec3 band = pow(textureGrad(u_map, uv, ddx, ddy).rgb, vec3(shape)) * gain * extinction;

    // Twilight keeps the band only high in the sky; near the bottom of the
    // frame the page is nearly white and any band there is a smudge.
    band *= mix(1.0, 1.15 * smoothstep(0.10, 0.75, ray.z), u_light);
    band *= u_reveal;

    // Alpha covers the colour it carries: this canvas composites over the
    // nebula gradient, and colour with zero alpha is out-of-gamut
    // premultiplied output that some GPUs pass through and some clamp away.
    frag = vec4(band, max(max(band.r, band.g), band.b));
}
`;

const UNIFORMS = [
  'u_view',
  'u_f',
  'u_aspect',
  'u_light',
  'u_reveal',
  'u_map',
  'u_extinction_k',
  'u_band_gain',
  'u_band_shape',
] as const;

export class BandScene {
  private readonly program: WebGLProgram;
  private readonly uniform: Record<string, WebGLUniformLocation | null>;
  private readonly quad: WebGLVertexArrayObject;
  private readonly texture: WebGLTexture;
  private readonly anisotropy: EXT_texture_filter_anisotropic | null = null;
  private mapLoadedAt = 0;
  private hasMap = false;

  private constructor(private readonly gl: WebGL2RenderingContext) {
    this.program = link(gl, VERT, FRAG);
    this.uniform = uniforms(gl, this.program, UNIFORMS);
    this.quad = fullscreenTriangle(gl);

    this.texture = gl.createTexture()!;
    gl.bindTexture(gl.TEXTURE_2D, this.texture);
    gl.texImage2D(
      gl.TEXTURE_2D,
      0,
      gl.RGBA,
      1,
      1,
      0,
      gl.RGBA,
      gl.UNSIGNED_BYTE,
      new Uint8Array([0, 0, 0, 0]),
    );
    // The map wraps around the whole sky in u and stops at the poles in v.
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.REPEAT);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
    this.anisotropy = gl.getExtension('EXT_texture_filter_anisotropic');
  }

  /** Best-effort: a browser with no WebGL2 gets the CPU warp instead. */
  static create(gl: WebGL2RenderingContext): BandScene | null {
    try {
      return new BandScene(gl);
    } catch {
      return null;
    }
  }

  /** Hand the shader its map. Until this lands the band draws nothing, which
   * is the sky the CSS starfield and the nebula already are. */
  async loadMap(url: string): Promise<void> {
    const response = await fetch(url);
    if (!response.ok) throw new Error(`${url}: ${response.status}`);
    const bitmap = await createImageBitmap(await response.blob());
    const { gl } = this;
    gl.bindTexture(gl.TEXTURE_2D, this.texture);
    gl.pixelStorei(gl.UNPACK_FLIP_Y_WEBGL, 0);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, gl.RGBA, gl.UNSIGNED_BYTE, bitmap);
    bitmap.close();
    // At 4096 across, most of the frame samples the map minified: a texel is
    // 0.088 degrees and a screen pixel near the frame edge covers several of
    // them. Without mipmaps that is point sampling a texel out of every group
    // the pixel covers, which crawls as the sky turns. Trilinear plus whatever
    // anisotropy the driver has keeps the band still and the lanes sharp
    // toward the horizon, where the sample footprint is most stretched.
    gl.generateMipmap(gl.TEXTURE_2D);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR_MIPMAP_LINEAR);
    if (this.anisotropy) {
      const max = gl.getParameter(this.anisotropy.MAX_TEXTURE_MAX_ANISOTROPY_EXT);
      gl.texParameterf(
        gl.TEXTURE_2D,
        this.anisotropy.TEXTURE_MAX_ANISOTROPY_EXT,
        Math.min(8, typeof max === 'number' ? max : 1),
      );
    }
    this.hasMap = true;
    this.mapLoadedAt = performance.now();
  }

  get ready(): boolean {
    return this.hasMap;
  }

  /** `instant` skips the reveal: under reduced motion the sky is one still
   * frame, and a fade is an animation. The canvas is sized and cleared by the
   * caller, which owns both passes. */
  draw(matrix: Mat3, light: boolean, width: number, height: number, instant = false): void {
    const { gl } = this;
    if (!this.hasMap) return;
    gl.viewport(0, 0, width, height);

    const reveal = instant
      ? 1
      : Math.min(1, (performance.now() - this.mapLoadedAt) / REVEAL_MS);
    gl.useProgram(this.program);
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, this.texture);
    gl.uniform1i(this.uniform.u_map!, 0);
    // `viewMatrix` is already column-major, which is what GL wants.
    gl.uniformMatrix3fv(this.uniform.u_view!, false, new Float32Array(matrix));
    gl.uniform1f(this.uniform.u_f!, FOCAL);
    gl.uniform1f(this.uniform.u_aspect!, width / Math.max(1, height));
    gl.uniform1f(this.uniform.u_light!, light ? 1 : 0);
    gl.uniform1f(this.uniform.u_reveal!, reveal);
    gl.uniform1f(this.uniform.u_extinction_k!, TUNING.extinctionK);
    gl.uniform1f(this.uniform.u_band_gain!, TUNING.bandGain);
    gl.uniform1f(this.uniform.u_band_shape!, TUNING.bandShape);
    gl.bindVertexArray(this.quad);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
    gl.bindVertexArray(null);
  }

  /** Whether the reveal is still running, and the band therefore still
   * changing with nothing else moving. */
  revealing(): boolean {
    return this.hasMap && performance.now() - this.mapLoadedAt < REVEAL_MS;
  }
}
