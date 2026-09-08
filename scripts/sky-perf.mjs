/** The sky's performance gate: fill every tile slot, then measure.
 *
 * `?skyfill=1` lifts the streamer's byte budget so the 146 slots fill in under
 * a minute instead of a quarter of an hour, and `?skydebug=1` publishes the
 * readout. Run it `--headed`: headless chromium is SwiftShader, which renders
 * the same picture on the CPU at a hundredth of the speed and would measure
 * nothing a visitor will ever experience.
 *
 *   node scripts/sky-perf.mjs http://127.0.0.1:3157 /tmp/gpu --headed
 */

import { chromium } from '@playwright/test';
const [url = 'http://127.0.0.1:3157', out = '/tmp/fill'] = process.argv.slice(2);
const b = await chromium.launch({ headless: !process.argv.includes('--headed') });
const p = await b.newPage({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: 2 });
await p.goto(`${url}/?skydebug=1&skyfill=1`, { waitUntil: 'load' });
let s;
for (let i = 0; i < 24; i += 1) {
  await new Promise((r) => setTimeout(r, 5000));
  s = await p.evaluate(() => globalThis.__sky?.() ?? null);
  console.log(
    `t+${(i + 1) * 5}s resident=${s?.residentTiles} points=${s?.points} cone=${s?.stream?.coveredDeg?.toFixed(1)} draw last/med/max=${s?.frameMs.last.toFixed(2)}/${s?.frameMs.median.toFixed(2)}/${s?.frameMs.max.toFixed(2)}ms gpu=${(s?.gpuBytes / 1e6).toFixed(1)}MB fetched=${(s?.stream?.bytes / 1e6).toFixed(1)}MB`,
  );
  if (s?.residentTiles >= 146) break;
}
// What the page actually delivers: the interval between painted frames, and
// how long the main thread is busy in each animation frame.
const cadence = await p.evaluate(
  () =>
    new Promise((resolve) => {
      const deltas = [];
      const work = [];
      let last = performance.now();
      const tick = () => {
        const now = performance.now();
        deltas.push(now - last);
        last = now;
        const after = performance.now();
        work.push(after - now);
        if (deltas.length < 180) requestAnimationFrame(tick);
        else resolve({ deltas, work });
      };
      requestAnimationFrame(tick);
    }),
);
const q = (a, f) => a.slice().sort((x, y) => x - y)[Math.floor(a.length * f)];
console.log(
  `frame interval: median ${q(cadence.deltas, 0.5).toFixed(1)}ms p95 ${q(cadence.deltas, 0.95).toFixed(1)}ms max ${Math.max(...cadence.deltas).toFixed(1)}ms over ${cadence.deltas.length} frames`,
);
await p.screenshot({ path: `${out}-full.png` });
await p.screenshot({
  path: `${out}-crop.png`,
  clip: { x: 520, y: 60, width: 400, height: 400 },
});
await b.close();
