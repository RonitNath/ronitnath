/** The hover tag against the callouts: the two cases the fix is about.
 *
 * A star that already has a callout gets no tag — the callout lights up
 * instead — and a star *beside* a callout gets a tag placed clear of it. Both
 * are geometry a screenshot is the only honest check of, so this drives
 * Playwright's chromium the way `sky-detail-shots.mjs` does (agent-browser
 * hangs on a page whose canvas never stops asking for frames).
 *
 * The sky is paused first: which star is where is the orbit's business, and a
 * pointer driven at a position read two frames ago lands on a neighbour.
 *
 *   node scripts/sky-hover-shots.mjs <base-url> <out-dir>
 */

import { mkdir } from 'node:fs/promises';
import { join } from 'node:path';
import { chromium } from '@playwright/test';

const [baseUrl = 'http://127.0.0.1:3157', outDir = 'sky-hover-shots'] = process.argv.slice(2);

const VIEW = { width: 1_440, height: 900, dpr: 2 };

const wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

/** Every named star on screen, and which of them the page has labelled. */
const survey = (page) =>
  page.evaluate(() => {
    const stars = globalThis.__sky?.().named ?? [];
    const callouts = [...document.querySelectorAll('.star-callout')].map((node) => ({
      name: node.dataset.name,
      box: node.querySelector('.callout-label')?.getBoundingClientRect().toJSON(),
    }));
    const blocks = ['.home-card', '.topbar', '.sky-chrome']
      .map((selector) => document.querySelector(selector)?.getBoundingClientRect())
      .filter(Boolean);
    const clear = (star) =>
      !blocks.some(
        (box) =>
          star.x > box.left - 24 &&
          star.x < box.right + 24 &&
          star.y > box.top - 24 &&
          star.y < box.bottom + 24,
      );
    return { stars: stars.filter(clear), callouts };
  });

const tagText = (page) =>
  page
    .locator('.star-tag')
    .textContent()
    .catch(() => '');

/** A star with no name on screen, as near a callout's label as one can be
 * found: the case where the tag has to be placed around the label. */
async function neighbourOf(page, callout) {
  const { left, top, width, height } = callout.box;
  const centre = [left + width / 2, top + height / 2];
  for (let radius = 60; radius <= 220; radius += 20) {
    for (let angle = 0; angle < 360; angle += 12) {
      const x = Math.round(centre[0] + radius * Math.cos((angle * Math.PI) / 180));
      const y = Math.round(centre[1] + radius * Math.sin((angle * Math.PI) / 180));
      if (x < 20 || y < 80 || x > VIEW.width - 20 || y > VIEW.height - 20) continue;
      await page.mouse.move(x, y);
      await wait(60);
      if (await page.locator('.star-tag').isVisible()) return { x, y, text: await tagText(page) };
    }
  }
  return null;
}

async function shoot(browser, theme) {
  const context = await browser.newContext({
    viewport: { width: VIEW.width, height: VIEW.height },
    deviceScaleFactor: VIEW.dpr,
  });
  if (theme === 'light') {
    await context.addInitScript(() => localStorage.setItem('rn_theme', 'light'));
  }
  const page = await context.newPage();
  const shot = (name) => page.screenshot({ path: join(outDir, `${name}-${theme}.png`) });
  await page.goto(`${baseUrl}/?skydebug=1`, { waitUntil: 'load' });
  await wait(12_000);
  await page.locator('button.sky-control', { hasText: 'Pause sky' }).click();
  await wait(400);

  const { stars, callouts } = await survey(page);
  const named = callouts.map((callout) => callout.name);
  const star = stars.find((candidate) => named.includes(candidate.name));
  if (!star) throw new Error('no callout star is clear of the page chrome');

  await page.mouse.move(star.x, star.y);
  await wait(400);
  const state = await page
    .locator(`.star-callout[data-name="${star.name}"]`)
    .getAttribute('data-hover');
  console.log(
    `${theme}: hovered ${star.name} at ${Math.round(star.x)},${Math.round(star.y)} — ` +
      `callout hover=${state}, tag visible=${await page.locator('.star-tag').isVisible()}`,
  );
  await shot('callout-hover');

  const callout = callouts.find((candidate) => candidate.box);
  const neighbour = await neighbourOf(page, callout);
  if (neighbour) {
    await wait(300);
    console.log(
      `${theme}: neighbour tag "${neighbour.text?.trim()}" at ${neighbour.x},${neighbour.y}, ` +
        `side=${await page.locator('.star-tag').getAttribute('data-side')}`,
    );
    await shot('neighbour-tag');
  } else {
    console.log(`${theme}: no star found beside ${callout.name}`);
  }
  await context.close();
}

await mkdir(outDir, { recursive: true });
const browser = await chromium.launch();
for (const theme of ['dark', 'light']) await shoot(browser, theme);
await browser.close();
