'use client';

import { useEffect, useRef } from 'react';

import { generateCatalog } from './catalog';
import { coarseObserver, FALLBACK_OBSERVER, type Observer } from './observer';
import { altAz, lstRad } from './sidereal';

/* The only interactive-feeling thing on the page, and it is not interactive:
 * a full-viewport 2D canvas holding the real sky over the visitor's head,
 * turning at the rate the Earth turns. No WebGL, no zoom, no labels — the
 * brief's whole point is that this is background, not an instrument.
 *
 * Projection: azimuthal equidistant about the zenith, so screen distance from
 * the zenith is angular distance from it and the celestial pole lands at its
 * true altitude — the field visibly turns about a point whose height on screen
 * IS the visitor's latitude. The zenith sits low in the frame so both it and
 * the pole are on screen at once.
 *
 * The horizon circle is not drawn and does not need to be: stars fade out over
 * the last few degrees the way they really do, so the dome has a soft edge
 * rather than a rim, and the CSS starfield underneath carries the corners.
 */

/** Four seconds a frame, which is not a frame rate — it is arithmetic. At this
 * projection scale a degree of sky is about 8px, and the real sidereal rate is
 * 15 degrees an hour: the field crosses one pixel every thirty seconds. A
 * frame every four seconds is an eighth of a pixel, already finer than the
 * antialiasing, and anything faster is main-thread work spent on motion no
 * screen can show. */
const FRAME_INTERVAL_MS = 4_000;

/** Fraction of viewport height the zenith sits down from the top. */
const ZENITH_Y = 0.8;

/** Stars dim to nothing over the last degrees before the horizon. */
const EXTINCTION_DEG = 12;

const DEG = Math.PI / 180;

interface Frame {
  cx: number;
  cy: number;
  radius: number;
  width: number;
  height: number;
  dpr: number;
}

/** Screen geometry for a viewport: where the zenith is and how many pixels an
 * angular degree of separation from it is worth. */
function frameFor(width: number, height: number, dpr: number): Frame {
  return {
    cx: width / 2,
    cy: height * ZENITH_Y,
    // Tied to height so the pole stays in frame on any aspect; the width term
    // keeps a short, wide window from collapsing the sky into a lens.
    radius: Math.max(height * 0.85, width * 0.45),
    width,
    height,
    dpr,
  };
}

/** Blue-white through to warm, standing in for the colour index. */
function starColor(tint: number): [number, number, number] {
  return [170 + 85 * tint, 195 + 25 * tint, 255 - 75 * tint];
}

export function SkyCanvas() {
  const ref = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = ref.current;
    const context = canvas?.getContext('2d', { alpha: true });
    if (!canvas || !context) return;

    const sky = generateCatalog();
    const reduced = window.matchMedia('(prefers-reduced-motion: reduce)');
    let observer: Observer = FALLBACK_OBSERVER;
    let frame = frameFor(0, 0, 1);
    let timer = 0;
    let live = true;

    function measure() {
      const dpr = Math.min(window.devicePixelRatio || 1, 2);
      const width = window.innerWidth;
      const height = window.innerHeight;
      frame = frameFor(width, height, dpr);
      canvas!.width = Math.round(width * dpr);
      canvas!.height = Math.round(height * dpr);
    }

    function draw() {
      const { cx, cy, radius, width, height, dpr } = frame;
      const ctx = context!;
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      ctx.clearRect(0, 0, width, height);
      // Additive: the canvas leaves its own alpha at zero, so in the light
      // theme the dusk gradient underneath shows between the stars instead of
      // being painted over.
      ctx.globalCompositeOperation = 'lighter';

      const light = document.documentElement.dataset.theme === 'light';
      const lst = lstRad(Date.now(), observer.lonDeg);
      const latRad = observer.latDeg * DEG;
      const pixelsPerRadian = radius / (Math.PI / 2);

      for (let i = 0; i < sky.count; i += 1) {
        // Typed arrays are indexed inside their own length here; the
        // assertions are the cost of noUncheckedIndexedAccess, not a doubt.
        const mag = sky.mag[i]!;
        const { alt, az } = altAz(sky.ra[i]!, sky.dec[i]!, lst, latRad);
        if (alt <= 0) continue;

        const r = (Math.PI / 2 - alt) * pixelsPerRadian;
        // Looking up with north at the top puts east on the left — the
        // planetarium orientation. The mirror image is the star globe seen
        // from outside, which is the wrong sky.
        const x = cx - Math.sin(az) * r;
        const y = cy - Math.cos(az) * r;
        if (x < -4 || x > width + 4 || y < -4 || y > height + 4) continue;

        let alpha = Math.min(1, Math.max(0.05, 1.05 - 0.16 * mag));
        alpha *= Math.min(1, alt / (EXTINCTION_DEG * DEG));
        if (light) {
          // Match the CSS layer's mask: stars only in the dark top of the
          // dusk ramp, gone by the time the sky reaches the horizon glow.
          alpha *= 0.9 * Math.min(1, Math.max(0, (0.62 - y / height) / 0.34));
        }
        if (alpha <= 0.01) continue;

        const size = Math.min(3.2, Math.max(0.45, 2.6 - 0.35 * mag));
        const [red, green, blue] = starColor(sky.tint[i]!);
        ctx.fillStyle = `rgba(${red},${green},${blue},${alpha})`;

        if (size < 1.4) {
          ctx.fillRect(x - size / 2, y - size / 2, size, size);
          continue;
        }
        ctx.beginPath();
        ctx.arc(x, y, size / 2, 0, Math.PI * 2);
        ctx.fill();

        if (mag < 1.2) {
          // A halo on the few genuinely bright ones. Any more than this and
          // the sky reads as bokeh rather than as stars.
          const glow = ctx.createRadialGradient(x, y, 0, x, y, size * 2.6);
          glow.addColorStop(0, `rgba(${red},${green},${blue},${alpha * 0.35})`);
          glow.addColorStop(1, 'rgba(0,0,0,0)');
          ctx.fillStyle = glow;
          ctx.beginPath();
          ctx.arc(x, y, size * 2.6, 0, Math.PI * 2);
          ctx.fill();
        }
      }
    }

    function schedule() {
      window.clearTimeout(timer);
      if (!live || reduced.matches) return;
      timer = window.setTimeout(() => {
        draw();
        schedule();
      }, FRAME_INTERVAL_MS);
    }

    function repaint() {
      measure();
      draw();
      schedule();
    }

    // The first paint of the sky waits for the main thread to be free: it is
    // background, and it is not worth a millisecond of anyone's interaction
    // latency. The CSS starfield is already on screen until it lands.
    const idle = window.requestIdleCallback ?? ((fn: () => void) => window.setTimeout(fn, 200));
    idle(() => {
      if (!live) return;
      repaint();
      void coarseObserver().then((where) => {
        if (!live) return;
        observer = where;
        draw();
      });
    });

    // Under reduced motion the sky is a still picture, so a theme flip has to
    // ask for the one frame that the loop would otherwise have supplied.
    const themeWatch = new MutationObserver(draw);
    themeWatch.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ['data-theme'],
    });
    window.addEventListener('resize', repaint);
    reduced.addEventListener('change', repaint);

    return () => {
      live = false;
      window.clearTimeout(timer);
      themeWatch.disconnect();
      window.removeEventListener('resize', repaint);
      reduced.removeEventListener('change', repaint);
    };
  }, []);

  return <canvas ref={ref} className="starscape" aria-hidden="true" />;
}
