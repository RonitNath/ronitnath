import type { Page } from '@playwright/test';

/** Reading the sky back off the GPU.
 *
 * The sky is one WebGL2 canvas, and a Playwright `evaluate` runs *between*
 * frames — by which point the drawing buffer is undefined unless the context
 * was asked to keep it. So the run loads the page as `/?skyreadback=1`, which
 * is the only thing that turns `preserveDrawingBuffer` on (`gl-util.ts`). The
 * alternative — having the page read itself back from inside its own frame —
 * would put test-only code in the render path, and the render path is the
 * thing under test.
 *
 * Everything here measures inside the browser and returns numbers: a 1440x900
 * buffer is five megabytes, and shipping it across the wire once per assertion
 * is slower than the whole suite.
 */

/** GL's origin is the bottom-left of the buffer and CSS's is the top-left. */
const READ_BACK = `
  const canvas = document.querySelector('canvas.starscape');
  const gl = canvas.getContext('webgl2');
  const width = canvas.width;
  const height = canvas.height;
`;

/** How many pixels of the sky canvas carry any light at all. */
export async function litPixels(page: Page): Promise<number> {
  return page.evaluate(`(() => {
    ${READ_BACK}
    const pixels = new Uint8Array(width * height * 4);
    gl.readPixels(0, 0, width, height, gl.RGBA, gl.UNSIGNED_BYTE, pixels);
    let lit = 0;
    for (let i = 3; i < pixels.length; i += 4) if (pixels[i] > 0) lit += 1;
    return lit;
  })()`) as Promise<number>;
}

export interface StarProfile {
  /** The brightest channel found anywhere in the window, 0..255. */
  peak: number;
  /** How wide the star is, in CSS pixels: the span of the row through its
   * centre that is still clearly above the sky around it. */
  widthPx: number;
  /** The halo, five CSS pixels out: the *median* of a ring of samples there,
   * above the sky behind them. This is what "brighter" means once a core has
   * saturated and only the wings can still grow, and the median is what makes
   * it a measurement of one star rather than of whatever else is in the
   * window — a field this dense always has a neighbour in it somewhere. */
  halo: number;
  /** The level of that sky, 0..255 — the window's median. */
  background: number;
}

/** Measure one star, given where the page's own projection says it is.
 *
 * `x`/`y` are CSS pixels from the top-left. The window is deliberately small:
 * a star's whole profile is bounded at 24 px, and a wider window would collect
 * its neighbours instead.
 */
export async function starProfile(
  page: Page,
  x: number,
  y: number,
  radiusPx = 16,
): Promise<StarProfile> {
  return page.evaluate(
    `(() => {
      const [cx, cy, radius] = [${x}, ${y}, ${radiusPx}];
      ${READ_BACK}
      const dpr = width / innerWidth;
      // One read of the window, not one per pixel: readPixels synchronises
      // with the GPU and a thousand of them is slower than the whole suite.
      const box = Math.ceil((radius + 8) * dpr) * 2;
      const originX = Math.max(0, Math.min(width - box, Math.round(cx * dpr) - box / 2));
      const originY = Math.max(0, Math.min(height - box, Math.round(height - cy * dpr) - box / 2));
      const pixels = new Uint8Array(box * box * 4);
      gl.readPixels(originX, originY, box, box, gl.RGBA, gl.UNSIGNED_BYTE, pixels);
      const at = (px, py) => {
        // CSS y counts down the page; the read-back buffer counts up.
        const bx = Math.round(px * dpr) - originX;
        const by = Math.round(height - py * dpr) - originY;
        if (bx < 0 || by < 0 || bx >= box || by >= box) return 0;
        const i = (by * box + bx) * 4;
        return Math.max(pixels[i], pixels[i + 1], pixels[i + 2]);
      };

      // The sky the star is standing on: the median of the window, which a
      // star and its halo cannot move because they are a small part of it.
      const all = [];
      for (let i = 0; i < pixels.length; i += 4) {
        all.push(Math.max(pixels[i], pixels[i + 1], pixels[i + 2]));
      }
      all.sort((a, b) => a - b);
      const background = all[all.length >> 1];

      // Find the real centre first: the projection and the frame it is
      // compared against are a fraction of a second apart, and the sky runs
      // at 60x — so the star is a few pixels from where the maths puts it.
      let peak = -1, bestX = cx, bestY = cy;
      for (let dy = -8; dy <= 8; dy += 1) {
        for (let dx = -8; dx <= 8; dx += 1) {
          const value = at(cx + dx, cy + dy);
          if (value > peak) { peak = value; bestX = cx + dx; bestY = cy + dy; }
        }
      }

      const above = (px, py) => Math.max(0, at(px, py) - background);
      const ring = [];
      for (let step = 0; step < 16; step += 1) {
        const angle = (2 * Math.PI * step) / 16;
        ring.push(above(bestX + 5 * Math.cos(angle), bestY + 5 * Math.sin(angle)));
      }
      ring.sort((a, b) => a - b);
      const halo = (ring[7] + ring[8]) / 2;
      // Width: the run through the centre that stays a quarter of the peak or
      // better, which is the visible extent rather than the quad's.
      const floor = Math.max(8, (peak - background) * 0.25);
      let widthPx = 0;
      for (let dx = -radius; dx <= radius; dx += 1) {
        if (above(bestX + dx, bestY) >= floor) widthPx += 1;
      }
      return { peak, widthPx, halo, background };
    })()`,
  ) as Promise<StarProfile>;
}
