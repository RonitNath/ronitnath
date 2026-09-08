/** The constellation figures, drawn as hairlines between the band and the
 * stars.
 *
 * Off by default and never labelled: the callouts already say which
 * constellation a named star belongs to, so a second set of words over the sky
 * would be the same fact twice in a smaller type size. What the lines add is
 * the shape — which is the thing a name cannot carry.
 *
 * Quiet is the whole specification. They are drawn at `--fg` and about a fifth
 * of an alpha in the dark theme, at one CSS pixel, *under* the star pass, and
 * through the same secant-airmass extinction the band and the stars use, so a
 * figure setting in the west dims exactly as its own stars do. Below the
 * horizon they are gone.
 *
 * A segment is a quad, not a `LINES` primitive: `lineWidth` is 1 device pixel
 * and nothing else on the drivers that matter, which at 2x DPR is half a CSS
 * pixel of unfeathered diagonal — a dotted line over a star field. The vertex
 * shader projects both endpoints, works out the screen-space direction, and
 * offsets its own corner by the half width; the fragment feathers the last
 * device pixel of it.
 */

import { link, uniforms } from './gl-util';
import { LINE_VERTEX_FLOATS, vertexCount } from './lines';
import type { Mat3 } from './sidereal';
import { FOCAL } from './sidereal';
import { LINE_DARK, LINE_LIGHT, TUNING } from './tuning';

const VERT = `#version 300 es
layout(location = 0) in vec3 a_a;
layout(location = 1) in vec3 a_b;
/** Sign is which side of the line this corner sits on; magnitude minus one is
 * which endpoint it is. One float rather than two attributes for six vertices
 * whose only difference is a corner. */
layout(location = 2) in float a_corner;

uniform mat3 u_view;
uniform float u_f;
uniform float u_aspect;
uniform vec2 u_viewport;
uniform float u_half_px;
uniform float u_extinction_k;

out float v_side;
out float v_fade;

/** Clip space for a J2000 direction, with w floored so a point at or below the
 * horizon projects far off screen instead of dividing by zero. Its fade is
 * already 0 there, so the segment has faded out before it gets there. */
vec4 project(vec3 view) {
    return vec4(view.x * u_f / u_aspect, view.y * u_f, 0.0, max(view.z, 0.02));
}

vec2 screen(vec4 clip) {
    return (clip.xy / clip.w) * 0.5 * u_viewport;
}

float fade(vec3 view) {
    float airmass = 1.0 / max(view.z, 0.05);
    float extinction = pow(10.0, -0.4 * u_extinction_k * (airmass - 1.0));
    // The horizon gate is the same 0.08 the star pass parks a set star at,
    // widened into a taper so a figure crossing it dissolves rather than ends.
    return extinction * smoothstep(0.03, 0.16, view.z);
}

void main() {
    vec3 view_a = u_view * a_a;
    vec3 view_b = u_view * a_b;
    vec4 clip_a = project(view_a);
    vec4 clip_b = project(view_b);

    float end = abs(a_corner) - 1.0;
    v_side = sign(a_corner);
    v_fade = mix(fade(view_a), fade(view_b), end);

    vec2 pixel_a = screen(clip_a);
    vec2 pixel_b = screen(clip_b);
    vec2 along = pixel_b - pixel_a;
    float span = length(along);
    vec2 normal = span > 0.0001 ? vec2(-along.y, along.x) / span : vec2(0.0, 1.0);
    vec2 pixel = mix(pixel_a, pixel_b, end) + normal * v_side * u_half_px;

    gl_Position = vec4(pixel / (0.5 * u_viewport), 0.0, 1.0);
}
`;

const FRAG = `#version 300 es
precision highp float;
in float v_side;
in float v_fade;
uniform vec3 u_color;
uniform float u_alpha;
uniform float u_half_px;
out vec4 frag;

void main() {
    // The quad is a device pixel wider than the line on each side; that margin
    // is where the edge is feathered, which is the only antialiasing this
    // canvas has (the context is created without MSAA).
    float distance_px = abs(v_side) * u_half_px;
    float line_half = u_half_px - 1.0;
    float coverage = clamp(line_half + 0.5 - distance_px, 0.0, 1.0);
    float alpha = u_alpha * v_fade * coverage;
    // Premultiplied, because the canvas composites over the CSS nebula: in the
    // light theme a dark line has to darken the ground it covers, not be added
    // to it.
    frag = vec4(u_color * alpha, alpha);
}
`;

const UNIFORMS = [
  'u_view',
  'u_f',
  'u_aspect',
  'u_viewport',
  'u_half_px',
  'u_extinction_k',
  'u_color',
  'u_alpha',
] as const;

/** One CSS pixel, and the device-pixel margin the fragment feathers in. */
const WIDTH_CSS = 1;
const FEATHER_PX = 1;

export class LineScene {
  private readonly program: WebGLProgram;
  private readonly uniform: Record<string, WebGLUniformLocation | null>;
  private readonly vao: WebGLVertexArrayObject;
  private readonly buffer: WebGLBuffer;
  private vertices = 0;

  private constructor(private readonly gl: WebGL2RenderingContext) {
    this.program = link(gl, VERT, FRAG);
    this.uniform = uniforms(gl, this.program, UNIFORMS);
    this.vao = gl.createVertexArray()!;
    this.buffer = gl.createBuffer()!;
    gl.bindVertexArray(this.vao);
    gl.bindBuffer(gl.ARRAY_BUFFER, this.buffer);
    const stride = LINE_VERTEX_FLOATS * 4;
    gl.enableVertexAttribArray(0);
    gl.vertexAttribPointer(0, 3, gl.FLOAT, false, stride, 0);
    gl.enableVertexAttribArray(1);
    gl.vertexAttribPointer(1, 3, gl.FLOAT, false, stride, 12);
    gl.enableVertexAttribArray(2);
    gl.vertexAttribPointer(2, 1, gl.FLOAT, false, stride, 24);
    gl.bindVertexArray(null);
  }

  /** Best-effort, like every other pass: a context that will not link this
   * leaves the sky exactly as it was. */
  static create(gl: WebGL2RenderingContext): LineScene | null {
    try {
      return new LineScene(gl);
    } catch {
      return null;
    }
  }

  /** Hand the pass its geometry, expanded by `lines.ts`. */
  upload(vertices: Float32Array): void {
    const { gl } = this;
    gl.bindBuffer(gl.ARRAY_BUFFER, this.buffer);
    gl.bufferData(gl.ARRAY_BUFFER, vertices, gl.STATIC_DRAW);
    this.vertices = vertexCount(vertices);
  }

  get ready(): boolean {
    return this.vertices > 0;
  }

  /** Draw over whatever the band left, before the stars. */
  draw(matrix: Mat3, light: boolean, dpr: number, width: number, height: number): void {
    const { gl } = this;
    if (!this.vertices) return;
    const look = light ? LINE_LIGHT : LINE_DARK;

    gl.useProgram(this.program);
    gl.enable(gl.BLEND);
    gl.blendFunc(gl.ONE, gl.ONE_MINUS_SRC_ALPHA);
    gl.uniformMatrix3fv(this.uniform.u_view!, false, new Float32Array(matrix));
    gl.uniform1f(this.uniform.u_f!, FOCAL);
    gl.uniform1f(this.uniform.u_aspect!, width / Math.max(1, height));
    gl.uniform2f(this.uniform.u_viewport!, width, height);
    gl.uniform1f(this.uniform.u_half_px!, (WIDTH_CSS * dpr) / 2 + FEATHER_PX);
    gl.uniform1f(this.uniform.u_extinction_k!, TUNING.extinctionK);
    gl.uniform3f(this.uniform.u_color!, look.color[0], look.color[1], look.color[2]);
    gl.uniform1f(this.uniform.u_alpha!, look.alpha);
    gl.bindVertexArray(this.vao);
    gl.drawArrays(gl.TRIANGLES, 0, this.vertices);
    gl.bindVertexArray(null);
    gl.disable(gl.BLEND);
  }
}
