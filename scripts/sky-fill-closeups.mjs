/** The filled stars up close, against the stars they stand among.
 *
 * The wide frames (`sky-fill-shots.mjs`) say the figures closed. This says
 * whether the star itself looks right — a second-magnitude star reading
 * clearly brighter than the fourth-magnitude ones around it, in a colour its
 * spectral type would give it. A full frame scaled into a review window is
 * exactly where that disappears, so each shot is a 420 px box around the star.
 *
 * The condition is only "on screen and clear of the chrome", not "centred":
 * five stars spread over eleven hours of right ascension are rarely centred
 * together, and a crop does not care where in the frame it was taken.
 *
 *   node scripts/sky-fill-closeups.mjs <base-url> <out-dir> [minutes]
 */

import { mkdir } from 'node:fs/promises';
import { join } from 'node:path';
import { chromium } from '@playwright/test';

const [baseUrl = 'http://127.0.0.1:3157', outDir = 'sky-fill-closeups', minutes = '26'] =
  process.argv.slice(2);

const TARGETS = ['Enif', 'Menkar', 'Gienah', 'Mahasim', 'Sheratan'];
const BOX = 420;
const VIEW = { width: 1_440, height: 900, dpr: 2 };

const wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

/** On screen, far enough from the edge for a whole box, and off the hero card
 * and the corner the globe and caption sit in. */
function usable(star) {
  const half = BOX / 2;
  return (
    star.x > half &&
    star.x < VIEW.width - half &&
    star.y > half &&
    star.y < VIEW.height - half &&
    !(Math.abs(star.x - 720) < 420 && Math.abs(star.y - 460) < 290) &&
    !(star.x < 400 && star.y > 560)
  );
}

await mkdir(outDir, { recursive: true });
const browser = await chromium.launch();
const context = await browser.newContext({
  viewport: { width: VIEW.width, height: VIEW.height },
  deviceScaleFactor: VIEW.dpr,
});
await context.addInitScript(() => localStorage.setItem('rn.sky.lines', '1'));
const page = await context.newPage();
await page.goto(`${baseUrl}/?skydebug=1&skyfill=1`, { waitUntil: 'load' });
await page.waitForFunction(() => (globalThis.__sky?.().named ?? []).length > 0, null, {
  timeout: 60_000,
});

const seen = new Set();
const deadline = Date.now() + Number(minutes) * 60_000;
while (Date.now() < deadline && seen.size < TARGETS.length) {
  const stars = await page.evaluate(() => globalThis.__sky?.().named ?? []);
  for (const name of TARGETS) {
    if (seen.has(name)) continue;
    const star = stars.find((candidate) => candidate.name === name);
    if (!star || !usable(star)) continue;
    await page.locator('button.sky-control', { hasText: 'Pause sky' }).click();
    await wait(700);
    // Re-read after the pause: the frame the positions came from has to be the
    // frame the crop is taken from, and at 60x a second is a quarter degree.
    const at = await page.evaluate(
      (which) => (globalThis.__sky?.().named ?? []).find((s) => s.name === which),
      name,
    );
    await page.screenshot({
      path: join(outDir, `closeup-${name.toLowerCase()}.png`),
      clip: { x: at.x - BOX / 2, y: at.y - BOX / 2, width: BOX, height: BOX },
      timeout: 60_000,
    });
    const around = await page.evaluate(
      ([which, box]) => {
        const stars = globalThis.__sky?.().named ?? [];
        const it = stars.find((s) => s.name === which);
        return stars
          .filter((s) => s !== it && Math.hypot(s.x - it.x, s.y - it.y) < box)
          .map((s) => `${s.name} ${s.magnitude.toFixed(2)}`);
      },
      [name, BOX],
    );
    console.log(`${name} mag ${at.magnitude.toFixed(2)} at ${Math.round(at.x)},${Math.round(at.y)}` +
      (around.length ? ` — named neighbours in frame: ${around.join(', ')}` : ''));
    seen.add(name);
    await page.locator('button.sky-control', { hasText: 'Resume sky' }).click();
    await wait(400);
  }
  await wait(1_000);
}
console.log(`captured ${seen.size}/${TARGETS.length}: ${[...seen].join(', ')}`);
await browser.close();
