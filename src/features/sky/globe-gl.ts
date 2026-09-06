/** The lit mini-globe: an orthographic textured sphere in WebGL2.
 *
 * Day colour, a normal map for relief and a specular map for water, lit from
 * the *simulated* instant's subsolar point — so the terminator on the globe is
 * the real one for the sky overhead, and dawn crosses it as the clock runs.
 * The ground track is the observer's great circle, drawn on the sphere so the
 * far half is hidden by it rather than by a guess.
 *
 * WebGL is allowed here and nowhere else on the page: the star field is 2D
 * canvas by ruling.
 */

import { orientation, RADIUS, SEGMENTS, sphere } from './globe-math';
import { subsolarPoint, unitVector } from './sidereal';
import { observerAt, TRACK_PERIOD_MS } from './track';
import { SIM_EPOCH_MS } from './clock';

const VERT = `#version 300 es
layout(location = 0) in vec3 a_pos;
layout(location = 1) in vec2 a_uv;
uniform mat3 u_orient;
uniform float u_radius;
uniform float u_point;
uniform float u_bias;
out vec2 v_uv;
out vec3 v_view;
out vec3 v_world;
void main() {
    vec3 p = u_orient * a_pos;
    v_uv = a_uv;
    v_view = p;
    v_world = a_pos;
    // Orthographic: the sphere is small enough on screen that perspective
    // would only add a distortion nobody asked for. z carries depth only.
    gl_Position = vec4(p.x * u_radius, p.y * u_radius, -p.z * 0.5 + u_bias, 1.0);
    gl_PointSize = u_point;
}
`;

const FRAG = `#version 300 es
precision highp float;
in vec2 v_uv;
in vec3 v_view;
in vec3 v_world;
uniform sampler2D u_day;
uniform sampler2D u_normal;
uniform sampler2D u_spec;
uniform vec3 u_sun;
uniform vec3 u_eye;
uniform vec3 u_ink;
uniform float u_mode;
out vec4 frag;

void main() {
    if (u_mode > 1.5) {
        // The ground track: one flat ink line on the lit face.
        frag = vec4(u_ink, 0.75);
        return;
    }
    if (u_mode > 0.5) {
        // A round marker, feathered so it does not alias at the limb.
        vec2 c = gl_PointCoord * 2.0 - 1.0;
        float d = dot(c, c);
        if (d > 1.0) discard;
        frag = vec4(u_ink, 1.0 - smoothstep(0.35, 1.0, d));
        return;
    }

    // Tangent frame from the position itself: an equirectangular map's u runs
    // east and its v runs south, so east and north are the tangent axes.
    vec3 up = normalize(v_world);
    vec3 east = normalize(cross(vec3(0.0, 0.0, 1.0), up) + vec3(1e-6, 0.0, 0.0));
    vec3 north = cross(up, east);
    vec3 tangentNormal = texture(u_normal, v_uv).xyz * 2.0 - 1.0;
    vec3 n = normalize(east * tangentNormal.x + north * tangentNormal.y + up * tangentNormal.z);

    float lambert = dot(n, u_sun);
    // A real terminator: a narrow band, not a hard circle, and a night side
    // that keeps just enough light to read as Earth rather than as a hole.
    float lit = smoothstep(-0.10, 0.22, lambert);
    vec3 albedo = texture(u_day, v_uv).rgb;

    // Water is what the specular map marks, and water is what glints.
    float water = texture(u_spec, v_uv).r;
    vec3 half_vector = normalize(u_sun + u_eye);
    float gloss = pow(max(dot(n, half_vector), 0.0), 90.0) * water * lit * 0.30;

    vec3 color = albedo * (0.05 + 0.95 * lit) + vec3(gloss);
    float limb = smoothstep(0.0, 0.18, v_view.z);
    frag = vec4(color, limb);
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

function link(gl: WebGL2RenderingContext): WebGLProgram {
  const program = gl.createProgram();
  if (!program) throw new Error('createProgram');
  gl.attachShader(program, compile(gl, gl.VERTEX_SHADER, VERT));
  gl.attachShader(program, compile(gl, gl.FRAGMENT_SHADER, FRAG));
  gl.linkProgram(program);
  if (!gl.getProgramParameter(program, gl.LINK_STATUS)) {
    throw new Error(gl.getProgramInfoLog(program) ?? 'program link failed');
  }
  return program;
}

/** The observer's whole lap, as unit vectors on the sphere. */
export function groundTrack(samples = 256): Float32Array {
  const points = new Float32Array(samples * 3);
  for (let i = 0; i < samples; i += 1) {
    const [lat, lon] = observerAt(SIM_EPOCH_MS + (TRACK_PERIOD_MS * i) / samples);
    const v = unitVector(lat, lon);
    points[i * 3] = v[0] * 1.004;
    points[i * 3 + 1] = v[1] * 1.004;
    points[i * 3 + 2] = v[2] * 1.004;
  }
  return points;
}

export interface GlobeTextures {
  day: string;
  normal: string;
  specular: string;
}

export class GlobeScene {
  private readonly program: WebGLProgram;
  private readonly vertices: WebGLBuffer;
  private readonly indices: WebGLBuffer;
  private readonly indexCount: number;
  private readonly scratch: WebGLBuffer;
  private readonly maps: WebGLTexture[];
  private readonly track: Float32Array;
  private readonly uniform: Record<string, WebGLUniformLocation | null>;

  private constructor(
    private readonly canvas: HTMLCanvasElement,
    private readonly gl: WebGL2RenderingContext,
  ) {
    gl.enable(gl.DEPTH_TEST);
    gl.enable(gl.BLEND);
    gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);
    this.program = link(gl);

    const mesh = sphere(SEGMENTS[0], SEGMENTS[1]);
    this.vertices = gl.createBuffer()!;
    gl.bindBuffer(gl.ARRAY_BUFFER, this.vertices);
    gl.bufferData(gl.ARRAY_BUFFER, mesh.vertices, gl.STATIC_DRAW);
    this.indices = gl.createBuffer()!;
    gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER, this.indices);
    gl.bufferData(gl.ELEMENT_ARRAY_BUFFER, mesh.indices, gl.STATIC_DRAW);
    this.indexCount = mesh.indices.length;
    this.scratch = gl.createBuffer()!;
    this.track = groundTrack();

    // A neutral blue until the textures land: the globe reads as Earth from
    // the first frame rather than as a black hole in the corner.
    this.maps = [
      [24, 42, 78, 255],
      [128, 128, 255, 255],
      [0, 0, 0, 255],
    ].map((rgba) => {
      const texture = gl.createTexture()!;
      gl.bindTexture(gl.TEXTURE_2D, texture);
      gl.texImage2D(
        gl.TEXTURE_2D,
        0,
        gl.RGBA,
        1,
        1,
        0,
        gl.RGBA,
        gl.UNSIGNED_BYTE,
        new Uint8Array(rgba),
      );
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.REPEAT);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
      return texture;
    });

    const at = (name: string): WebGLUniformLocation | null =>
      gl.getUniformLocation(this.program, name);
    this.uniform = {
      orient: at('u_orient'),
      radius: at('u_radius'),
      point: at('u_point'),
      bias: at('u_bias'),
      day: at('u_day'),
      normal: at('u_normal'),
      spec: at('u_spec'),
      sun: at('u_sun'),
      eye: at('u_eye'),
      ink: at('u_ink'),
      mode: at('u_mode'),
    };
  }

  /** `null` when the browser or the context cannot run WebGL2, which is what
   * leaves the 2D fallback disc as the picture. */
  static create(canvas: HTMLCanvasElement): GlobeScene | null {
    try {
      const gl = canvas.getContext('webgl2', { alpha: true, antialias: true });
      return gl ? new GlobeScene(canvas, gl) : null;
    } catch {
      return null;
    }
  }

  async loadTextures(urls: GlobeTextures): Promise<void> {
    const sources = [urls.day, urls.normal, urls.specular];
    await Promise.all(
      sources.map(async (url, slot) => {
        const response = await fetch(url);
        if (!response.ok) throw new Error(`${url}: ${response.status}`);
        const bitmap = await createImageBitmap(await response.blob());
        const gl = this.gl;
        gl.bindTexture(gl.TEXTURE_2D, this.maps[slot]!);
        gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, gl.RGBA, gl.UNSIGNED_BYTE, bitmap);
        gl.generateMipmap(gl.TEXTURE_2D);
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR_MIPMAP_LINEAR);
        bitmap.close();
      }),
    );
  }

  draw(latDeg: number, lonDeg: number, simMs: number, ink: readonly number[]): void {
    const gl = this.gl;
    const dpr = Math.min(typeof devicePixelRatio === 'number' ? devicePixelRatio : 1, 2);
    const size = Math.round(Math.max(1, this.canvas.clientWidth) * dpr);
    if (this.canvas.width !== size || this.canvas.height !== size) {
      this.canvas.width = size;
      this.canvas.height = size;
    }

    gl.viewport(0, 0, size, size);
    gl.clearColor(0, 0, 0, 0);
    gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT);
    gl.useProgram(this.program);

    gl.uniformMatrix3fv(this.uniform.orient!, false, orientation(latDeg, lonDeg));
    gl.uniform1f(this.uniform.radius!, RADIUS);
    gl.uniform1f(this.uniform.point!, 0);
    gl.uniform1f(this.uniform.bias!, 0);
    gl.uniform1f(this.uniform.mode!, 0);
    gl.uniform3fv(this.uniform.sun!, new Float32Array(unitVector(...subsolarPoint(simMs))));
    gl.uniform3fv(this.uniform.eye!, new Float32Array(unitVector(latDeg, lonDeg)));
    gl.uniform3fv(this.uniform.ink!, new Float32Array(ink));
    for (const [slot, name] of [
      [0, 'day'],
      [1, 'normal'],
      [2, 'spec'],
    ] as const) {
      gl.activeTexture(gl.TEXTURE0 + slot);
      gl.bindTexture(gl.TEXTURE_2D, this.maps[slot]!);
      gl.uniform1i(this.uniform[name]!, slot);
    }

    gl.bindBuffer(gl.ARRAY_BUFFER, this.vertices);
    gl.vertexAttribPointer(0, 3, gl.FLOAT, false, 20, 0);
    gl.enableVertexAttribArray(0);
    gl.vertexAttribPointer(1, 2, gl.FLOAT, false, 20, 12);
    gl.enableVertexAttribArray(1);
    gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER, this.indices);
    gl.drawElements(gl.TRIANGLES, this.indexCount, gl.UNSIGNED_SHORT, 0);

    gl.disableVertexAttribArray(1);
    gl.bindBuffer(gl.ARRAY_BUFFER, this.scratch);

    // The lap, on the sphere: the depth test hides the half behind the Earth,
    // and the bias keeps it off the surface it lies on.
    gl.bufferData(gl.ARRAY_BUFFER, this.track, gl.DYNAMIC_DRAW);
    gl.vertexAttribPointer(0, 3, gl.FLOAT, false, 0, 0);
    gl.enableVertexAttribArray(0);
    gl.uniform1f(this.uniform.bias!, -0.006);
    gl.uniform1f(this.uniform.mode!, 2);
    gl.drawArrays(gl.LINE_LOOP, 0, this.track.length / 3);

    // The marker is the observer's own position, so it is always the point
    // facing the camera — drawn last, in front of the sphere it sits on.
    gl.disable(gl.DEPTH_TEST);
    gl.bufferData(
      gl.ARRAY_BUFFER,
      new Float32Array(unitVector(latDeg, lonDeg)),
      gl.DYNAMIC_DRAW,
    );
    gl.vertexAttribPointer(0, 3, gl.FLOAT, false, 0, 0);
    gl.uniform1f(this.uniform.point!, 7 * dpr);
    gl.uniform1f(this.uniform.mode!, 1);
    gl.drawArrays(gl.POINTS, 0, 1);
    gl.enable(gl.DEPTH_TEST);
  }
}
