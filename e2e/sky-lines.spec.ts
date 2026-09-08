import { expect, test } from '@playwright/test';

import { changedPixels, holdFrame } from './sky-readback';

/** The constellation toggle, asserted on the canvas rather than on the button.
 *
 * A pressed button proves a React state changed; what has to be true is that
 * the GPU drew something more. The sky is paused first so the two readings are
 * of the same sky — at 60x an unpaused frame turns a quarter of a degree
 * between them, which moves more pixels than 665 hairlines cover.
 */

const PAUSE = 'button.sky-control:has-text("Pause sky")';
const LINES = '#toggle-lines';

async function settle(page: import('@playwright/test').Page): Promise<void> {
  await expect(page.locator('.callout-label').first()).toBeVisible({ timeout: 20_000 });
  await page.locator(PAUSE).click();
  // The band's reveal is 900 ms and is the only other thing still changing.
  await page.waitForTimeout(1_200);
}

test('the Lines toggle draws lines, says so, and is remembered', async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.goto('/?skyreadback=1');
  await settle(page);

  const toggle = page.locator(LINES);
  await expect(toggle).toHaveAttribute('aria-pressed', 'false');
  await holdFrame(page);

  await toggle.click();
  await expect(toggle).toHaveAttribute('aria-pressed', 'true');
  await page.waitForTimeout(300);
  // The figures above the horizon change some fifteen thousand pixels of a
  // 1440x900 frame; the streamer's own arrivals in the same 300 ms are a
  // handful of sub-pixel stars.
  const drawn = await changedPixels(page);
  expect(drawn).toBeGreaterThan(8_000);

  // Off again returns the sky it was: the same pixels change back.
  await holdFrame(page);
  await toggle.click();
  await expect(toggle).toHaveAttribute('aria-pressed', 'false');
  await page.waitForTimeout(300);
  expect(await changedPixels(page)).toBeGreaterThan(drawn / 2);

  await toggle.click();
  await expect(toggle).toHaveAttribute('aria-pressed', 'true');
  expect(await page.evaluate(() => localStorage.getItem('rn.sky.lines'))).toBe('1');
});

test('a reload comes back with the figures still on', async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.goto('/?skyreadback=1');
  await settle(page);
  await page.locator(LINES).click();
  await expect(page.locator(LINES)).toHaveAttribute('aria-pressed', 'true');

  await page.reload();
  await settle(page);
  await expect(page.locator(LINES)).toHaveAttribute('aria-pressed', 'true');
  // And that it is the *drawing* that came back, not just the button: turning
  // it off from the remembered state takes the same pixels away again.
  await holdFrame(page);
  await page.locator(LINES).click();
  await expect(page.locator(LINES)).toHaveAttribute('aria-pressed', 'false');
  await page.waitForTimeout(300);
  expect(await changedPixels(page)).toBeGreaterThan(8_000);
});
