import { readFileSync } from 'node:fs';

import { expect, test } from '@playwright/test';

import { parseStars, starPosition } from '../src/features/sky/catalog';
import { simTimeMs } from '../src/features/sky/clock';
import { applyView, viewMatrix } from '../src/features/sky/sidereal';
import { observerFrom, project } from './sky-geometry';
import { starProfile } from './sky-readback';

/** What the GL star pass draws, measured off the canvas it drew it on.
 *
 * The rest of the sky's end-to-end run asserts about *placement* — which star
 * is labelled and where the label goes. This one asserts about the picture:
 * that the response from magnitude to pixels is the one `tuning.ts` describes,
 * and that a first-magnitude star and a magnitude-5 star do not arrive on
 * screen looking the same.
 */

test('the brightest star in frame is drawn brighter and wider than a magnitude-5 star', async ({
  page,
}) => {
  const catalog = parseStars(new Uint8Array(readFileSync('public/stars/bright.bin')));
  // The catalog is sorted brightest first, so its opening run is the
  // first-magnitude sky: Sirius, Canopus, Vega, Arcturus and the rest. The
  // observer flies a fixed orbit, so which of them is overhead is not this
  // spec's to choose — it takes the brightest one that is, and Sirius when
  // Sirius is it.
  const BRIGHT = 20;
  expect(catalog.magnitude[BRIGHT - 1]!).toBeLessThan(1.7);
  const faint: number[] = [];
  for (let index = 0; index < catalog.count; index += 1) {
    const mag = catalog.magnitude[index]!;
    if (mag > 4.9 && mag < 5.1) faint.push(index);
  }
  expect(faint.length).toBeGreaterThan(50);

  const viewport = { width: 1440, height: 900 };
  await page.setViewportSize(viewport);

  // The sky runs at 60x, so it is *paused* before anything is measured: an
  // unpaused frame and the matrix this spec projects with would be a second
  // and a quarter of a degree apart.
  await page.goto('/?skyreadback=1');
  await expect(page.locator('.callout-label').first()).toBeVisible({ timeout: 20_000 });
  // The clock freezes somewhere between these two readings, and at 60x the
  // difference between them is degrees of sky, so the midpoint is what the
  // projection uses and `starProfile` searches a few pixels for the rest.
  const beforeMs = Date.now();
  await page.getByRole('button', { name: 'Pause sky' }).click();
  await expect(page.getByRole('button', { name: 'Resume sky' })).toBeVisible();
  const nowMs = (beforeMs + Date.now()) / 2;
  const caption = (await page.locator('.grounding').textContent()) ?? '';
  const matrix = viewMatrix(simTimeMs(nowMs), ...observerFrom(caption));

  // Clear of the frame edge, of the hero, and of the corner the globe and the
  // caption sit in: those are drawn over the sky and would be measured as it.
  const inFrame = ([x, y]: [number, number]): boolean =>
    x > 60 &&
    x < viewport.width - 60 &&
    y > 90 &&
    y < viewport.height - 90 &&
    !(Math.abs(x - 720) < 280 && Math.abs(y - 460) < 150) &&
    !(x < 260 && y > 620);
  const screen = (index: number): { at: [number, number]; zenithCos: number } | null => {
    const position = starPosition(catalog, index)!;
    const at = project(matrix, position, viewport.width, viewport.height);
    if (!at || !inFrame(at)) return null;
    return { at, zenithCos: applyView(matrix, position)[2] };
  };

  let brightIndex = -1;
  let bright: { at: [number, number]; zenithCos: number } | null = null;
  for (let index = 0; index < BRIGHT && !bright; index += 1) {
    bright = screen(index);
    brightIndex = index;
  }
  expect(bright, 'no first-magnitude star was above the horizon and in frame').not.toBeNull();

  // The comparison star is chosen at the same *altitude*, because the
  // atmosphere takes a share that depends on nothing else: two stars a
  // magnitude apart low in the frame are a fairer test of the response than
  // one at the zenith against one on the horizon.
  const candidates: { at: [number, number]; apart: number }[] = [];
  for (const index of faint) {
    const candidate = screen(index);
    if (!candidate) continue;
    // Far enough from the bright one that its glare is not what gets measured.
    if (Math.hypot(candidate.at[0] - bright!.at[0], candidate.at[1] - bright!.at[1]) < 80) {
      continue;
    }
    candidates.push({
      at: candidate.at,
      apart: Math.abs(candidate.zenithCos - bright!.zenithCos),
    });
  }
  candidates.sort((a, b) => a.apart - b.apart);
  expect(candidates[0], 'no magnitude-5 star was in frame to compare against').toBeTruthy();
  expect(candidates[0]!.apart, 'no magnitude-5 star was at a comparable altitude').toBeLessThan(
    0.12,
  );

  const where = `magnitude ${catalog.magnitude[brightIndex]!.toFixed(2)}`;
  const measured = await starProfile(page, ...bright!.at);
  // The comparison has to be of a star against *sky*. S4's re-baked band
  // resolves the galactic core at five times the detail, and its brightest
  // parts now reach white on their own — a magnitude-5 star standing on one of
  // them is at 255 because of what is behind it, which measures the band and
  // not the response under test. So the nearest candidate in altitude that is
  // also on dark sky is the one taken.
  let dim = await starProfile(page, ...candidates[0]!.at);
  for (const candidate of candidates.slice(0, 6)) {
    if (dim.background <= 60) break;
    dim = await starProfile(page, ...candidate.at);
  }
  expect(dim.background, 'every magnitude-5 star in frame stands on the band').toBeLessThan(80);
  expect(measured.peak, `${where} did not saturate`).toBeGreaterThan(240);
  expect(dim.peak, 'a magnitude-5 star should not be at white').toBeLessThan(measured.peak);
  expect(measured.widthPx, `${where} was no wider than a magnitude-5 star`).toBeGreaterThan(
    dim.widthPx,
  );
  // Its core is at white and cannot say any more, so what says "brighter" is
  // the halo: the bright star still has one five pixels out, the faint one
  // has none.
  expect(measured.halo, `${where} carries no halo`).toBeGreaterThan(12);
  expect(measured.halo).toBeGreaterThan(dim.halo * 3 + 6);
});
