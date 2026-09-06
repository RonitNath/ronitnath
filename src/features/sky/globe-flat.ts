/** The globe without WebGL: a static orthographic disc of the day texture.
 *
 * No lighting, no track, no marker halo — a browser that cannot run WebGL2 is
 * shown where the observer is and nothing it cannot honestly draw. It is
 * sampled per pixel at the disc's own size (a few hundred by a few hundred,
 * once per observer change), which no GPU is needed for.
 */

import { RADIUS } from './globe-math';
import { orientation } from './globe-math';

export function paintFlatGlobe(
  ctx: CanvasRenderingContext2D,
  source: ImageData,
  latDeg: number,
  lonDeg: number,
): void {
  const { width, height } = ctx.canvas;
  const out = ctx.createImageData(width, height);
  const orient = orientation(latDeg, lonDeg);

  for (let py = 0; py < height; py += 1) {
    const y = (1 - (2 * (py + 0.5)) / height) / RADIUS;
    for (let px = 0; px < width; px += 1) {
      const x = ((2 * (px + 0.5)) / width - 1) / RADIUS;
      const squared = x * x + y * y;
      if (squared > 1) continue;
      const view = [x, y, Math.sqrt(1 - squared)] as const;
      // The basis is orthonormal, so its transpose is its inverse.
      const wx = orient[0]! * view[0] + orient[1]! * view[1] + orient[2]! * view[2];
      const wy = orient[3]! * view[0] + orient[4]! * view[1] + orient[5]! * view[2];
      const wz = orient[6]! * view[0] + orient[7]! * view[1] + orient[8]! * view[2];

      const u = Math.atan2(wy, wx) / (2 * Math.PI) + 0.5;
      const v = 0.5 - Math.asin(Math.min(1, Math.max(-1, wz))) / Math.PI;
      const sx = Math.min(source.width - 1, Math.max(0, Math.floor(u * source.width)));
      const sy = Math.min(source.height - 1, Math.max(0, Math.floor(v * source.height)));
      const at = (sy * source.width + sx) * 4;
      const to = (py * width + px) * 4;
      out.data[to] = source.data[at]!;
      out.data[to + 1] = source.data[at + 1]!;
      out.data[to + 2] = source.data[at + 2]!;
      // Feather the limb by one pixel so the disc does not alias.
      out.data[to + 3] = Math.round(255 * Math.min(1, (1 - squared) * 24));
    }
  }
  ctx.putImageData(out, 0, 0);
}
