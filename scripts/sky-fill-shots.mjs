/** The catalogue fill, framed on the figures it closed.
 *
 * Five second-magnitude stars were drawn nowhere until `build_bright.py`
 * filled the band between Gaia's saturated end and the old Hipparcos cut, and
 * eleven constellation segments were dropped for want of them. Whether that
 * looks right — a second-magnitude star reading brighter than the fourth
 * magnitudes around it, in a plausible colour, with its figure closed — is
 * something only a screenshot can say.
 *
 * There is no camera: the observer flies a fixed orbit and the sky turns at
 * 60x, so which constellation is up is the clock's business, not the script's.
 * So this watches, the way `sky-hover-shots.mjs` surveys what is actually on
 * screen: it polls `__sky().named` (which is `?skydebug=1` only) and, the
 * moment a target star is in the middle of the frame, pauses the sky and
 * shoots. One pass is a sidereal day of sky, about 24 minutes of wall time,
 * and all three viewports watch through it at once.
 *
 *   node scripts/sky-fill-shots.mjs <base-url> <out-dir> [minutes]
 */

import { mkdir } from 'node:fs/promises';
import { join } from 'node:path';
import { chromium } from '@playwright/test';

const [baseUrl = 'http://127.0.0.1:3141', outDir = 'sky-fill-shots', minutes = '26'] =
  process.argv.slice(2);

/** The star to frame on, and the figure it belongs to. */
const TARGETS = [
  { star: 'Enif', figure: 'pegasus' },
  { star: 'Menkar', figure: 'cetus' },
  { star: 'Gienah', figure: 'corvus' },
  { star: 'Mahasim', figure: 'auriga' },
  { star: 'Sheratan', figure: 'aries' },
];

const VIEWS = [
  { name: 'desktop-dark', width: 1_440, height: 900, dpr: 2, theme: 'dark' },
  { name: 'desktop-light', width: 1_440, height: 900, dpr: 2, theme: 'light' },
  { name: 'phone-dark', width: 390, height: 844, dpr: 3, theme: 'dark' },
];

const wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

/** How near the middle a star has to be before the frame is worth keeping.
 * Half the width and the middle 60% of the height: high enough that the hero
 * card and the chrome are not over it, low enough that it is not the horizon. */
function centred(star, view) {
  return (
    Math.abs(star.x - view.width / 2) < view.width * 0.25 &&
    star.y > view.height * 0.18 &&
    star.y < view.height * 0.7
  );
}

async function open(browser, view) {
  const context = await browser.newContext({
    viewport: { width: view.width, height: view.height },
    deviceScaleFactor: view.dpr,
  });
  // Lines on from the first frame: the figures are what the fill closed, and
  // the toggle remembers itself in localStorage anyway.
  await context.addInitScript(
    ([theme]) => {
      localStorage.setItem('rn.sky.lines', '1');
      if (theme === 'light') localStorage.setItem('rn_theme', 'light');
    },
    [view.theme],
  );
  const page = await context.newPage();
  await page.goto(`${baseUrl}/?skydebug=1&skyfill=1`, { waitUntil: 'load' });
  await page.waitForFunction(() => (globalThis.__sky?.().named ?? []).length > 0, null, {
    timeout: 60_000,
  });
  return { context, page };
}

/** Pause, shoot, resume — the sky has to stand still for the exposure or the
 * figure smears a quarter of a degree across it. */
async function capture(page, view, target, seen) {
  const pause = page.locator('button.sky-control', { hasText: 'Pause sky' });
  await pause.click();
  await wait(700);
  await page.screenshot({ path: join(outDir, `${target.figure}-${view.name}.png`) });
  const magnitudes = await page.evaluate(
    (name) => {
      const stars = globalThis.__sky?.().named ?? [];
      const star = stars.find((candidate) => candidate.name === name);
      return { star, neighbours: stars.slice(0, 12).map((s) => `${s.name} ${s.magnitude}`) };
    },
    target.star,
  );
  console.log(
    `${view.name}: ${target.figure} on ${target.star} ` +
      `mag ${magnitudes.star?.magnitude?.toFixed(2)} at ` +
      `${Math.round(magnitudes.star?.x)},${Math.round(magnitudes.star?.y)}`,
  );
  seen.add(target.figure);
  await page.locator('button.sky-control', { hasText: 'Resume sky' }).click();
  await wait(400);
}

await mkdir(outDir, { recursive: true });
/* A headless host with no GPU has no WebGL2, and the sky is a WebGL2 canvas:
 * without ANGLE's software rasteriser every shot here is an empty frame. Same
 * switch `playwright.config.ts` takes for the end-to-end run. */
const browser = await chromium.launch({
  args: process.env.SOFTWARE_GL
    ? ['--use-gl=angle', '--use-angle=swiftshader', '--enable-unsafe-swiftshader']
    : [],
});
const sessions = await Promise.all(VIEWS.map(async (view) => ({ view, ...(await open(browser, view)) })));
const seen = sessions.map(() => new Set());

const deadline = Date.now() + Number(minutes) * 60_000;
while (Date.now() < deadline && seen.some((set) => set.size < TARGETS.length)) {
  for (const [index, { view, page }] of sessions.entries()) {
    if (seen[index].size === TARGETS.length) continue;
    const stars = await page.evaluate(() => globalThis.__sky?.().named ?? []);
    for (const target of TARGETS) {
      if (seen[index].has(target.figure)) continue;
      const star = stars.find((candidate) => candidate.name === target.star);
      if (star && centred(star, view)) await capture(page, view, target, seen[index]);
    }
  }
  await wait(1_000);
}

for (const [index, { view }] of sessions.entries()) {
  const missing = TARGETS.filter((target) => !seen[index].has(target.figure)).map((t) => t.figure);
  console.log(`${view.name}: ${seen[index].size}/${TARGETS.length} captured${missing.length ? `, missed ${missing.join(', ')}` : ''}`);
}
await browser.close();
