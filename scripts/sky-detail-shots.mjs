/** S3's visual gate: the hover tag and the detail panel, shot where they land.
 *
 * `agent-browser screenshot` hangs against this page (a canvas that never
 * stops asking for frames), so the gate drives Playwright's chromium directly,
 * the way S1 and S2 did (`scripts/sky-shots.mjs`).
 *
 * Which star is overhead is the orbit's business, not this script's: it asks
 * the page (`?skydebug=1`) for the named stars on screen, drops the ones
 * behind the hero or the chrome, and takes the brightest that is left —
 * Alioth where Alioth is up. The faint star is found by sweeping the pointer
 * across empty sky until the tag reports something past magnitude 7, which is
 * fainter than any star in `named.json` and so certainly a Gaia-only row.
 *
 *   node scripts/sky-detail-shots.mjs <base-url> <out-dir>
 */

import { mkdir } from 'node:fs/promises';
import { join } from 'node:path';
import { chromium } from '@playwright/test';

const [baseUrl = 'http://127.0.0.1:3157', outDir = 'sky-detail-shots'] = process.argv.slice(2);

const VIEWS = [
  { name: 'desktop', width: 1_440, height: 900, dpr: 2, touch: false },
  { name: 'phone', width: 390, height: 844, dpr: 3, touch: true },
];

const wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

/** The named stars on screen that a pointer can actually reach. */
const usable = (page) =>
  page.evaluate(() => {
    const stars = globalThis.__sky?.().named ?? [];
    const blocks = ['.home-card', '.topbar', '.sky-chrome']
      .map((selector) => document.querySelector(selector)?.getBoundingClientRect())
      .filter(Boolean);
    return stars.filter(
      (star) =>
        !blocks.some(
          (box) =>
            star.x > box.left - 24 &&
            star.x < box.right + 24 &&
            star.y > box.top - 24 &&
            star.y < box.bottom + 24,
        ),
    );
  });

/** The sky moves about fifteen pixels a second near the top of the frame, so a
 * position read and then waited on is a position the star has left: the
 * pointer has to be driven the instant the answer arrives, and the tag checked
 * to see that it landed on the star it was aimed at. */
async function hoverBrightStar(page) {
  for (let tries = 0; tries < 40; tries += 1) {
    const stars = await usable(page);
    const star = stars.find((candidate) => candidate.name === 'Alioth') ?? stars[0];
    if (!star) {
      await wait(500);
      continue;
    }
    await page.mouse.move(star.x, star.y);
    // The tag is written by the next animation frame, not by the event.
    await wait(120);
    const text = (await page.locator('.star-tag').textContent().catch(() => '')) ?? '';
    if (text.toUpperCase().startsWith(star.name.toUpperCase())) return star;
  }
  throw new Error('no named star could be hovered');
}

/** Sweep for a star fainter than anything named: the panel then has to hold a
 * Gaia-only row, which is the sparse case worth looking at. */
async function faintStar(page, view) {
  const step = 7;
  for (let y = 120; y < view.height - 160; y += step) {
    for (let x = 40; x < view.width - 40; x += step) {
      await page.mouse.move(x, y);
      const text = (await page.locator('.star-tag').textContent().catch(() => '')) ?? '';
      const magnitude = Number(/mag ([\d.]+)/.exec(text)?.[1] ?? NaN);
      if (magnitude > 7) return { x, y };
    }
  }
  return null;
}

async function shoot(browser, view, theme) {
  const context = await browser.newContext({
    viewport: { width: view.width, height: view.height },
    deviceScaleFactor: view.dpr,
    hasTouch: view.touch,
    isMobile: view.touch,
  });
  if (theme === 'light') {
    await context.addInitScript(() => localStorage.setItem('rn_theme', 'light'));
  }
  const page = await context.newPage();
  const shot = (name) => page.screenshot({ path: join(outDir, `${name}-${view.name}-${theme}.png`) });
  await page.goto(`${baseUrl}/?skydebug=1`, { waitUntil: 'load' });
  // Everything has landed before the first shot: the catalogue, g9 and enough
  // tiles that the sky behind the panel is the sky a visitor sees.
  await wait(12_000);

  let star;
  if (view.touch) {
    // A coarse pointer has no hover to aim by, so the tap goes to a freshly
    // read position and the panel's own key says which star it hit.
    const stars = await usable(page);
    star = stars.find((candidate) => candidate.name === 'Alioth') ?? stars[0];
    await page.touchscreen.tap(star.x, star.y);
  } else {
    star = await hoverBrightStar(page);
    await wait(300);
    await shot('hover');
    // Re-aimed before the click: the star has moved a dozen pixels while the
    // shot was taken, and clicking the old pixel opens a panel on whatever
    // faint neighbour is there now.
    star = await hoverBrightStar(page);
    await page.mouse.down();
    await page.mouse.up();
  }
  await page.locator('.star-detail').waitFor({ timeout: 10_000 });
  await wait(600);
  await shot('panel');
  console.log(
    `${view.name}/${theme}: aimed at ${star.name}, panel on ` +
      (await page.locator('.star-detail').getAttribute('data-key')),
  );

  // And on a faint one, which is where the sparse payload shows.
  await page.keyboard.press('Escape');
  await wait(200);
  if (!view.touch) {
    const faint = await faintStar(page, view);
    if (faint) {
      await page.mouse.click(faint.x, faint.y);
      await page.locator('.star-detail').waitFor({ timeout: 10_000 });
      await wait(600);
      await shot('faint');
      console.log(`${view.name}/${theme}: faint panel at ${faint.x},${faint.y}`);
    } else {
      console.log(`${view.name}/${theme}: no faint star found`);
    }
  }
  await context.close();
}

await mkdir(outDir, { recursive: true });
const browser = await chromium.launch();
for (const view of VIEWS) {
  for (const theme of ['dark', 'light']) await shoot(browser, view, theme);
}
await browser.close();
