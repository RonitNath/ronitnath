/** The mini-globe as one thing the stage can hold.
 *
 * It is the same observer and the same instant as the sky — that is the whole
 * point of the corner of the frame — but none of the code is shared: a
 * textured sphere lit from the subsolar point (`globe-gl.ts`), or a flat day
 * disc where there is no WebGL2 (`globe-flat.ts`). Keeping it here rather than
 * in `stage.ts` is what leaves the stage about the clock, the observer and the
 * frame loop.
 */

import { ASSETS, loadImageData } from './assets';
import { paintFlatGlobe } from './globe-flat';
import { GlobeScene } from './globe-gl';
import { RADIUS, unproject } from './globe-math';
import type { Point } from './observer';

export class GlobeView {
  private canvas: HTMLCanvasElement | null = null;
  private scene: GlobeScene | null = null;
  private earth: ImageData | null = null;

  attach(canvas: HTMLCanvasElement | null): void {
    this.canvas = canvas;
    this.scene = canvas ? GlobeScene.create(canvas) : null;
  }

  /** Whether the globe is drawing through WebGL rather than the flat disc. */
  get accelerated(): boolean {
    return this.scene !== null;
  }

  /** Best-effort: a failed texture leaves the globe the neutral blue it
   * starts on, which is a globe. */
  async loadTextures(): Promise<void> {
    try {
      if (this.scene) {
        await this.scene.loadTextures({
          day: ASSETS.earthDay,
          normal: ASSETS.earthNormal,
          specular: ASSETS.earthSpecular,
        });
      } else {
        this.earth = await loadImageData(ASSETS.earthDay, 1_024);
      }
    } catch {
      // Nothing to do: the globe draws without them.
    }
  }

  draw(lat: number, lon: number, simMs: number, light: boolean): void {
    const canvas = this.canvas;
    if (!canvas) return;
    const ink = light ? [1, 0.93, 0.72] : [1, 0.42, 0.33];
    if (this.scene) {
      this.scene.draw(lat, lon, simMs, ink);
      return;
    }
    const ctx = canvas.getContext('2d');
    if (!ctx || !this.earth) return;
    const size = Math.round(
      Math.max(1, canvas.clientWidth) * Math.min(devicePixelRatio || 1, 2),
    );
    if (canvas.width !== size) {
      canvas.width = size;
      canvas.height = size;
    }
    ctx.clearRect(0, 0, size, size);
    paintFlatGlobe(ctx, this.earth, lat, lon);
    ctx.fillStyle = light ? 'rgb(255,237,184)' : 'rgb(255,107,84)';
    ctx.beginPath();
    ctx.arc(size / 2, size / 2, Math.max(2.5, (size * RADIUS) / 44), 0, Math.PI * 2);
    ctx.fill();
  }

  /** Which point on Earth a pointer event landed on, if it hit the globe. */
  pointAt(clientX: number, clientY: number, lat: number, lon: number): Point | null {
    const canvas = this.canvas;
    if (!canvas) return null;
    const bounds = canvas.getBoundingClientRect();
    if (bounds.width <= 0 || bounds.height <= 0) return null;
    const x = (2 * ((clientX - bounds.left) / bounds.width) - 1) / RADIUS;
    const y = (1 - 2 * ((clientY - bounds.top) / bounds.height)) / RADIUS;
    return unproject(x, y, lat, lon);
  }
}
