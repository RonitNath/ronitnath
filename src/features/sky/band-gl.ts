/** The Milky Way, drawn by a fragment shader.
 *
 * `sky/milkyway.webp` is Gaia star counts binned onto an equal-area grid and
 * baked in equatorial coordinates. Getting it on screen is a per-pixel
 * operation — the inverse of the star pass's projection, then an
 * equirectangular lookup — and the CPU warp this replaces could only afford it
 * into a 192 px buffer, which scaled up to blobs rather than to the galaxy.
 *
 * So the band gets the shader it had before the rebuild (`shaders.rs`'s
 * `SKY_FRAG`), ported whole: inverse-view sampling, `pow(rgb, shape) · gain`
 * tone mapping, secant-airmass extinction, the twilight gate, an alpha that
 * covers the colour it carries, and the reveal that fades it in when the map
 * lands. It draws onto its own canvas beneath the star canvas, so the stars
 * stay 2D — twelve thousand `drawImage` calls of a pre-rendered sprite are
 * cheaper than the buffer churn of moving them into this context too.
 */

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
    vec3 band = pow(texture(u_map, uv).rgb, vec3(shape)) * gain * extinction;

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

function compile(gl: WebGL2RenderingContext, type: number, source: string): WebGLShader {
  const shader = gl.createShader(type);
  if (!shader) throw new Error('createShader');
  gl.shaderSource(shader, source);
  gl.compileShader(shader);
  if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
    throw new Error(gl.getShaderInfoLog(shader) ?? 'shader compile failed');
  }
  return shader;
}

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
  private readonly uniform: Record<string, WebGLUniformLocation | null> = {};
  private readonly texture: WebGLTexture;
  private mapLoadedAt = 0;
  private hasMap = false;

  private constructor(
    private readonly canvas: HTMLCanvasElement,
    private readonly gl: WebGL2RenderingContext,
  ) {
    const program = gl.createProgram();
    if (!program) throw new Error('createProgram');
    gl.attachShader(program, compile(gl, gl.VERTEX_SHADER, VERT));
    gl.attachShader(program, compile(gl, gl.FRAGMENT_SHADER, FRAG));
    gl.linkProgram(program);
    if (!gl.getProgramParameter(program, gl.LINK_STATUS)) {
      throw new Error(gl.getProgramInfoLog(program) ?? 'program link failed');
    }
    this.program = program;
    gl.useProgram(program);
    for (const name of UNIFORMS) this.uniform[name] = gl.getUniformLocation(program, name);

    // One triangle large enough to cover the viewport — cheaper than a quad
    // and free of the diagonal seam two triangles produce.
    const buffer = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 3, -1, -1, 3]), gl.STATIC_DRAW);
    gl.enableVertexAttribArray(0);
    gl.vertexAttribPointer(0, 2, gl.FLOAT, false, 0, 0);

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
  }

  /** Best-effort: a browser with no WebGL2 gets the CPU warp instead. */
  static create(canvas: HTMLCanvasElement): BandScene | null {
    const gl = canvas.getContext('webgl2', {
      alpha: true,
      antialias: false,
      premultipliedAlpha: true,
      powerPreference: 'low-power',
    });
    if (!gl) return null;
    try {
      return new BandScene(canvas, gl);
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
    this.hasMap = true;
    this.mapLoadedAt = performance.now();
  }

  get ready(): boolean {
    return this.hasMap;
  }

  /** `instant` skips the reveal: under reduced motion the sky is one still
   * frame, and a fade is an animation. */
  draw(matrix: Mat3, light: boolean, dpr: number, instant = false): void {
    const { gl, canvas } = this;
    const width = Math.round(innerWidth * dpr);
    const height = Math.round(innerHeight * dpr);
    if (canvas.width !== width) canvas.width = width;
    if (canvas.height !== height) canvas.height = height;
    gl.viewport(0, 0, width, height);
    gl.clearColor(0, 0, 0, 0);
    gl.clear(gl.COLOR_BUFFER_BIT);
    if (!this.hasMap) return;

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
    gl.drawArrays(gl.TRIANGLES, 0, 3);
  }

  /** Whether the reveal is still running, and the band therefore still
   * changing with nothing else moving. */
  revealing(): boolean {
    return this.hasMap && performance.now() - this.mapLoadedAt < REVEAL_MS;
  }
}
