import { expect, test, type Page } from '@playwright/test';

import { keepOutFor, parsePosition } from '../src/features/sky/annotate';
import { simTimeMs } from '../src/features/sky/clock';
import { applyView, FOCAL, viewMatrix } from '../src/features/sky/sidereal';

/** The landing's sky, asserted against the same modules that draw it. */

async function settle(page: Page): Promise<void> {
  await page.goto('/');
  // The assets land after first paint; the callouts appear with the catalog.
  await expect(page.locator('.callout-label').first()).toBeVisible({ timeout: 15_000 });
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

test('dragging the globe overrides the shared orbit and Resume gives it back', async ({
  page,
}) => {
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

test('the designed starfield gives way to the real one', async ({ page }) => {
  await page.goto('/');
  // Before the catalog lands the CSS starfield is the picture...
  await expect(page.locator('.starfield')).toBeVisible();
  // ...and once a frame of the real sky is drawn it is gone, rather than
  // sitting behind it as a second, static sky.
  await expect(page.locator('.starfield')).toBeHidden({ timeout: 15_000 });
  await expect(page.locator('canvas.starscape')).toHaveCount(1);
  await expect(page.locator('.nebula')).toBeAttached();
});

test('a callout glides without React re-rendering it', async ({ page }) => {
  await settle(page);
  const read = () =>
    page.evaluate(() => {
      const nodes = [...document.querySelectorAll('.star-callout')];
      return {
        count: document.querySelectorAll('.star-annotations *').length,
        names: nodes.map((node) => (node as HTMLElement).dataset.name ?? ''),
        transforms: nodes.map((node) => (node as HTMLElement).style.transform),
      };
    });
  const first = await read();
  await page.waitForTimeout(100);
  const second = await read();

  // Same nodes, same stars — nothing was rendered again...
  expect(second.count).toBe(first.count);
  expect(second.names).toEqual(first.names);
  // ...and yet every callout has moved, because the frame loop writes the
  // transform straight to the DOM.
  expect(second.transforms).not.toEqual(first.transforms);
  for (const transform of second.transforms) expect(transform).toContain('translate3d');
});

test('every callout carries a leader from a ring on its star', async ({ page }) => {
  await settle(page);
  const nowMs = Date.now();
  const caption = (await page.locator('.grounding').textContent()) ?? '';
  const leaders = await page.evaluate(() =>
    [...document.querySelectorAll('.star-callout')].map((node) => {
      const centre = (element: Element) => {
        const box = element.getBoundingClientRect();
        return [box.x + box.width / 2, box.y + box.height / 2];
      };
      const line = node.querySelector('.callout-line')!;
      const label = node.querySelector('.callout-label')!.getBoundingClientRect();
      return {
        name: (node as HTMLElement).dataset.name ?? '',
        position: (node as HTMLElement).dataset.position ?? '',
        ring: centre(node.querySelector('.callout-ring')!),
        ends: [
          Number(line.getAttribute('x1')),
          Number(line.getAttribute('y1')),
          Number(line.getAttribute('x2')),
          Number(line.getAttribute('y2')),
        ],
        label: [label.x, label.y, label.width, label.height],
        viewport: [innerWidth, innerHeight],
      };
    }),
  );
  expect(leaders.length).toBeGreaterThan(0);

  const [lat, lon] = observerFrom(caption);
  const matrix = viewMatrix(simTimeMs(nowMs), lat, lon);
  for (const leader of leaders) {
    const [width, height] = leader.viewport as [number, number];
    const [vx, vy, vz] = applyView(matrix, parsePosition(leader.position)!);
    const aspect = width / height;
    const expected = [
      (((vx / vz) * FOCAL) / aspect + 1) * 0.5 * width,
      (1 - (vy / vz) * FOCAL) * 0.5 * height,
    ];

    // The ring is on the star, unless the placement was clamped to the frame.
    const clamped = expected.some(
      (value, axis) => value < 0 || value > (axis === 0 ? width : height),
    );
    if (!clamped) {
      // 60x means a second of skew between reading the clock and reading the
      // DOM is a few pixels of sky, which is what this tolerance is.
      expect(
        Math.hypot(leader.ring[0]! - expected[0]!, leader.ring[1]! - expected[1]!),
      ).toBeLessThan(60);
    }

    // The line starts at the ring and ends short of the label it points to.
    const [x1, y1, x2, y2] = leader.ends as [number, number, number, number];
    expect(Math.hypot(x1, y1)).toBeLessThan(8);
    expect(Math.hypot(x2, y2)).toBeGreaterThan(Math.hypot(x1, y1));
    const end = [leader.ring[0]! + x2, leader.ring[1]! + y2];
    const [lx, ly, lw, lh] = leader.label as [number, number, number, number];
    expect(end[0]!).toBeGreaterThan(lx - 40);
    expect(end[0]!).toBeLessThan(lx + lw + 40);
    expect(end[1]!).toBeGreaterThan(ly - 40);
    expect(end[1]!).toBeLessThan(ly + lh + 40);
  }
});

test.describe('on a phone', () => {
  test.use({ viewport: { width: 390, height: 844 } });

  test('no callout is laid across the hero', async ({ page }) => {
    await settle(page);
    const hero = (await page.locator('.home-card').boundingBox())!;
    const [keepOutX, keepOutY] = keepOutFor(390, 844);
    // Across, a phone's band is the whole placeable frame, so the rule that
    // can actually be broken is the vertical one.
    expect(keepOutX).toBeGreaterThan(0.8);
    const band = {
      top: 422 - (keepOutY * 844) / 2,
      bottom: 422 + (keepOutY * 844) / 2,
    };
    for (const box of await page
      .locator('.callout-label')
      .evaluateAll((nodes) => nodes.map((node) => node.getBoundingClientRect().toJSON()))) {
      // On screen...
      expect(box.x).toBeGreaterThanOrEqual(0);
      expect(box.x + box.width).toBeLessThanOrEqual(390);
      // ...clear of the hero's own box, which is what the owner saw a callout
      // sitting on...
      expect(box.y > hero.y + hero.height || box.y + box.height < hero.y).toBe(true);
      // ...and anchored outside the keep-out band. The band is the card grown
      // by a whole label, so a leader may tip the text a few pixels into it and
      // still be nowhere near the name; what has to hold is that the callout
      // is placed outside it rather than in it.
      const middle = box.y + box.height / 2;
      expect(middle > band.bottom || middle < band.top).toBe(true);
    }
  });
});
