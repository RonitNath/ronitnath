/** The bits of WebGL2 boilerplate the band and the stars both need.
 *
 * Both passes draw into one context on one canvas — the band first, then the
 * stars over it — so the context itself is created here rather than by either
 * of them, and neither owns the other.
 */

/** The one canvas the sky is drawn on. Alpha is on and premultiplied because
 * the page composites this over the CSS nebula: in the light theme the dusk
 * gradient has to show between the stars rather than be painted over. */
export function skyContext(canvas: HTMLCanvasElement): WebGL2RenderingContext | null {
  return canvas.getContext('webgl2', {
    alpha: true,
    antialias: false,
    premultipliedAlpha: true,
    powerPreference: 'low-power',
    // Reading the sky back is how the end-to-end run asserts that it was
    // drawn at all; without this the buffer is undefined after the frame.
    preserveDrawingBuffer: PRESERVE_DRAWING_BUFFER,
  });
}

/** Whether the drawing buffer survives past its frame.
 *
 * Off in production: keeping it costs a copy per frame on some drivers. The
 * end-to-end run turns it on with `?skyreadback=1`, which is the only way a
 * Playwright `evaluate` — which runs *between* frames — can call `readPixels`
 * and see anything but zeros. The alternative, reading back inside the frame
 * from the page's own loop, would mean shipping test-only code in the render
 * path, which is the worse of the two.
 */
const PRESERVE_DRAWING_BUFFER =
  typeof location !== 'undefined' && location.search.includes('skyreadback=1');

export function compile(
  gl: WebGL2RenderingContext,
  type: number,
  source: string,
): WebGLShader {
  const shader = gl.createShader(type);
  if (!shader) throw new Error('createShader');
  gl.shaderSource(shader, source);
  gl.compileShader(shader);
  if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
    throw new Error(gl.getShaderInfoLog(shader) ?? 'shader compile failed');
  }
  return shader;
}

export function link(
  gl: WebGL2RenderingContext,
  vertex: string,
  fragment: string,
): WebGLProgram {
  const program = gl.createProgram();
  if (!program) throw new Error('createProgram');
  gl.attachShader(program, compile(gl, gl.VERTEX_SHADER, vertex));
  gl.attachShader(program, compile(gl, gl.FRAGMENT_SHADER, fragment));
  gl.linkProgram(program);
  if (!gl.getProgramParameter(program, gl.LINK_STATUS)) {
    throw new Error(gl.getProgramInfoLog(program) ?? 'program link failed');
  }
  return program;
}

/** Every uniform a program declares, by name — cheaper to look up once than
 * to ask the driver for a location on every frame. */
export function uniforms(
  gl: WebGL2RenderingContext,
  program: WebGLProgram,
  names: readonly string[],
): Record<string, WebGLUniformLocation | null> {
  const found: Record<string, WebGLUniformLocation | null> = {};
  for (const name of names) found[name] = gl.getUniformLocation(program, name);
  return found;
}

/** One triangle large enough to cover the viewport — cheaper than a quad and
 * free of the diagonal seam two triangles produce. */
export function fullscreenTriangle(gl: WebGL2RenderingContext): WebGLVertexArrayObject {
  const vao = gl.createVertexArray()!;
  gl.bindVertexArray(vao);
  const buffer = gl.createBuffer();
  gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
  gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 3, -1, -1, 3]), gl.STATIC_DRAW);
  gl.enableVertexAttribArray(0);
  gl.vertexAttribPointer(0, 2, gl.FLOAT, false, 0, 0);
  gl.bindVertexArray(null);
  return vao;
}

/** A float colour target the star pass can accumulate into, and the extension
 * that made it possible — or `null` where neither float extension exists and
 * the pass has to fold its tone curve into the fragment instead.
 *
 * Half float first: it is the more widely supported of the two, and 11 bits of
 * mantissa is far more headroom than a sky whose brightest star is a few
 * hundred times its reference needs.
 */
export function floatTarget(gl: WebGL2RenderingContext): 'half' | 'full' | null {
  if (gl.getExtension('EXT_color_buffer_half_float')) return 'half';
  if (gl.getExtension('EXT_color_buffer_float')) return 'full';
  return null;
}
