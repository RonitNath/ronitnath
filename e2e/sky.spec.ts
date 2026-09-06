import { expect, test, type Page } from '@playwright/test';

import { parsePosition } from '../src/features/sky/annotate';
import { simTimeMs } from '../src/features/sky/clock';
import { applyView, viewMatrix } from '../src/features/sky/sidereal';

/** The landing's sky, asserted against the same modules that draw it. */

async function settle(page: Page): Promise<void> {
  await page.goto('/');
  // The assets land after first paint; the callouts appear with the catalog.
  await expect(page.locator('.star-callout').first()).toBeVisible({ timeout: 15_000 });
  await expect(page.locator('.grounding')).not.toBeEmpty({ timeout: 15_000 });
}

/** The observer, read back off the caption the page renders. */
function observerFrom(caption: string): [number, number] {
  const match = /(\d+\.\d+)° ([NS]), (\d+\.\d+)° ([EW])/.exec(caption);
  expect(match, `no position in ${caption}`).not.toBeNull();
  const [, lat, ns, lon, ew] = match!;
  return [Number(lat) * (ns === 'S' ? -1 : 1), Number(lon) * (ew === 'W' ? -1 : 1)];
}

test('every callout names a star that is really above the horizon', async ({ page }) => {
  await settle(page);
  // The clock runs 60x, so read it and the labels as close together as the
  // page allows; a second of skew is a quarter of a degree of sky.
  const nowMs = Date.now();
  const caption = (await page.locator('.grounding').textContent()) ?? '';
  const callouts = await page.locator('.star-callout').evaluateAll((nodes) =>
    nodes.map((node) => ({
      name: (node as HTMLElement).dataset.name ?? '',
      position: (node as HTMLElement).dataset.position ?? '',
    })),
  );

  expect(callouts.length).toBeGreaterThan(0);
  expect(callouts.length).toBeLessThanOrEqual(3);

  const matrix = viewMatrix(simTimeMs(nowMs), ...observerFrom(caption));
  for (const callout of callouts) {
    const position = parsePosition(callout.position);
    expect(position, `${callout.name} carries no direction`).not.toBeNull();
    const [, , zenith] = applyView(matrix, position!);
    expect(zenith, `${callout.name} is below the horizon`).toBeGreaterThan(0);
  }
});

test('the grounding caption is live', async ({ page }) => {
  await settle(page);
  const caption = page.locator('.grounding');
  const first = await caption.textContent();
  await expect(caption).not.toHaveText(first ?? '', { timeout: 2_000 });
});

test('dragging the globe overrides the shared orbit and Resume gives it back', async ({ page }) => {
  await settle(page);
  await expect(page.getByRole('button', { name: 'Resume orbit' })).toHaveCount(0);

  const globe = page.locator('canvas.mini-globe');
  const box = (await globe.boundingBox())!;
  const [cx, cy] = [box.x + box.width / 2, box.y + box.height / 2];
  await page.mouse.move(cx, cy);
  await page.mouse.down();
  for (let step = 1; step <= 6; step += 1) await page.mouse.move(cx + step * 10, cy + step * 3);
  await page.mouse.up();

  const resume = page.getByRole('button', { name: 'Resume orbit' });
  await expect(resume).toBeVisible();
  const overridden = observerFrom((await page.locator('.grounding').textContent()) ?? '');

  await resume.click();
  await expect(resume).toHaveCount(0, { timeout: 4_000 });
  const back = observerFrom((await page.locator('.grounding').textContent()) ?? '');
  expect(back).not.toEqual(overridden);
});

test('pausing stops the clock', async ({ page }) => {
  await settle(page);
  await page.getByRole('button', { name: 'Pause sky' }).click();
  const caption = page.locator('.grounding');
  const paused = await caption.textContent();
  await page.waitForTimeout(2_000);
  expect(await caption.textContent()).toBe(paused);
  await page.getByRole('button', { name: 'Resume sky' }).click();
  await expect(caption).not.toHaveText(paused ?? '', { timeout: 3_000 });
});

test.describe('reduced motion', () => {
  test.use({ reducedMotion: 'reduce' });

  test('draws the sky once and then leaves it alone', async ({ page }) => {
    await page.goto('/');
    const sky = page.locator('canvas.starscape');
    const lit = async () =>
      sky.evaluate((canvas: HTMLCanvasElement) => {
        const pixels = canvas
          .getContext('2d')!
          .getImageData(0, 0, canvas.width, canvas.height).data;
        let sum = 0;
        for (let i = 3; i < pixels.length; i += 4) sum += pixels[i]!;
        return sum;
      });
    await expect.poll(lit, { timeout: 15_000 }).toBeGreaterThan(0);
    const first = await lit();
    await page.waitForTimeout(1_500);
    expect(await lit()).toBe(first);
  });
});
