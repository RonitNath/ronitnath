/** The sky's visual gate, as screenshots the reviewer actually looks at.
 *
 * `agent-browser screenshot` hangs against this page (a canvas that never
 * stops asking for frames), so the gate drives Playwright's own chromium
 * directly. Every shot is taken at a stated moment after load, because what
 * S2 added is *streaming*: a single screenshot cannot show that a sky filled
 * in, and a sky that fills in badly is exactly the failure to look for.
 *
 *   node scripts/sky-shots.mjs <base-url> <out-dir> [--quick]
 */

import { mkdir, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { chromium } from '@playwright/test';

const [baseUrl = 'http://127.0.0.1:3157', outDir = 'sky-shots'] = process.argv.slice(2);
const quick = process.argv.includes('--quick');

/** When to shoot, in seconds after load: one before the deep sky can be
 * there, one while it is arriving, one when it has. */
const MOMENTS = quick ? [1, 12] : [1, 10, 40];

const VIEWS = [
  { name: 'desktop-1x', width: 1_440, height: 900, dpr: 1 },
  { name: 'desktop-2x', width: 1_440, height: 900, dpr: 2 },
  { name: 'phone', width: 390, height: 844, dpr: 3 },
];

/** A patch of sky at 2x, above the hero and across the top of the streamed
 * cone: the grain the deep catalogue adds is a texture, and a full-frame shot
 * scaled into a review window is exactly where a texture disappears. */
const CROP = { x: 520, y: 60, width: 400, height: 400 };

const wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

async function shoot(browser, view, theme) {
  const context = await browser.newContext({
    viewport: { width: view.width, height: view.height },
    deviceScaleFactor: view.dpr,
    hasTouch: view.name === 'phone',
    isMobile: view.name === 'phone',
    colorScheme: theme === 'light' ? 'light' : 'dark',
  });
  if (theme === 'light') {
    await context.addInitScript(() => localStorage.setItem('rn_theme', 'light'));
  }
  const page = await context.newPage();
  const startedAt = Date.now();
  await page.goto(`${baseUrl}/?skydebug=1`, { waitUntil: 'load' });

  const shots = [];
  for (const at of MOMENTS) {
    await wait(Math.max(0, startedAt + at * 1_000 - Date.now()));
    const name = `${view.name}-${theme}-t${at}s.png`;
    await page.screenshot({ path: join(outDir, name) });
    shots.push(name);
    if (view.name === 'desktop-2x' && theme === 'dark' && (at === 1 || at === MOMENTS.at(-1))) {
      const crop = `${view.name}-${theme}-t${at}s-crop.png`;
      await page.screenshot({ path: join(outDir, crop), clip: CROP });
      shots.push(crop);
    }
  }

  const readout = await page.evaluate(() => globalThis.__sky?.() ?? null);
  const transfer = await page.evaluate(() =>
    performance.getEntriesByType('resource').map((entry) => ({
      url: entry.name.replace(location.origin, ''),
      bytes: entry.transferSize,
      startMs: Math.round(entry.startTime),
    })),
  );
  await context.close();
  return { view: view.name, theme, shots, readout, transfer };
}

const browser = await chromium.launch();
await mkdir(outDir, { recursive: true });
const runs = [];
for (const view of VIEWS) {
  for (const theme of ['dark', 'light']) {
    if (theme === 'light' && view.name === 'desktop-1x') continue;
    runs.push(await shoot(browser, view, theme));
  }
}
await browser.close();

for (const run of runs) {
  const total = run.transfer.reduce((sum, entry) => sum + entry.bytes, 0);
  const tiles = run.transfer.filter((entry) => /\/stars\/lod\/\d+\.bin$/.test(entry.url));
  const frame = run.readout?.frameMs ?? {};
  console.log(
    `${run.view}/${run.theme}: ${(total / 1e6).toFixed(2)} MB total, ` +
      `${tiles.length} tiles (${(tiles.reduce((s, t) => s + t.bytes, 0) / 1e3).toFixed(0)} kB), ` +
      `frame last/median/max ${(frame.last ?? 0).toFixed(1)}/${(frame.median ?? 0).toFixed(1)}/` +
      `${(frame.max ?? 0).toFixed(1)} ms, ${run.readout?.points ?? 0} points, ` +
      `${run.readout?.residentTiles ?? 0} resident, ` +
      `${((run.readout?.gpuBytes ?? 0) / 1e6).toFixed(1)} MB on the GPU, ` +
      `cone ${(run.readout?.stream?.coveredDeg ?? 0).toFixed(1)}°`,
  );
}
await writeFile(join(outDir, 'runs.json'), `${JSON.stringify(runs, null, 2)}\n`);
console.log(`\n${runs.flatMap((run) => run.shots).length} shots in ${outDir}`);
