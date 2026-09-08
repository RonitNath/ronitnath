import { expect, test, type Page } from '@playwright/test';

/** S3: pick a star out of the sky and read what the site knows about it.
 *
 * The sky the observer flies over is a function of the instant, so which star
 * is overhead is not this file's to choose. It asks the page which named stars
 * are on screen (`window.__sky` under `?skydebug=1`, and only there), takes the
 * brightest one that is not behind the page's own chrome, and drives the
 * pointer to it. Alioth is preferred where Alioth is up, because it is the
 * star the plan's gate names.
 */

interface DebugStar {
  name: string;
  key: string;
  x: number;
  y: number;
  magnitude: number;
}

const GATE_STAR = 'Alioth';

/** The names the page has already labelled, in the order they are drawn. */
async function calloutNames(page: Page): Promise<string[]> {
  return page.$$eval('.star-callout', (nodes) =>
    nodes.map((node) => (node as HTMLElement).dataset.name ?? ''),
  );
}

/** The boxes a pointer must stay out of: a pick over the hero, the header or
 * the corner block is a click on the page, not on the sky (`star-pick.tsx`). */
async function onScreen(page: Page): Promise<DebugStar[]> {
  return page.evaluate(() => {
    const stars =
      (globalThis as { __sky?: () => { named?: DebugStar[] } }).__sky?.().named ?? [];
    const blocks = ['.home-card', '.topbar', '.sky-chrome']
      .map((selector) => document.querySelector(selector)?.getBoundingClientRect())
      .filter((box): box is DOMRect => Boolean(box));
    return stars.filter(
      (star) =>
        !blocks.some(
          (box) =>
            star.x > box.left - 20 &&
            star.x < box.right + 20 &&
            star.y > box.top - 20 &&
            star.y < box.bottom + 20,
        ),
    );
  });
}

/** A star with no callout on it: the ones that get a hover tag. A star the
 * page has already named gets none, on purpose (`star-pick.tsx`). */
async function usable(page: Page): Promise<DebugStar[]> {
  const [stars, named] = await Promise.all([onScreen(page), calloutNames(page)]);
  return stars.filter((star) => !named.includes(star.name));
}

async function starOnScreen(page: Page): Promise<DebugStar> {
  await page.goto('/?skydebug=1');
  await expect.poll(async () => (await usable(page)).length, { timeout: 30_000 }).toBeGreaterThan(0);
  // Pause before reading the positions the pointer will be driven to. The sky
  // turns 15 arcminutes a second, and the pick weighs a neighbour's brightness
  // against its distance — so a couple of frames between the reading and the
  // click is enough for a magnitude-5 star beside Arcturus to be the nearer of
  // the two. Paused, the frame the positions came from is the frame the
  // pointer lands in.
  await page.locator('button.sky-control', { hasText: 'Pause sky' }).click();
  await page.waitForTimeout(200);
  const stars = await usable(page);
  return stars.find((star) => star.name === GATE_STAR) ?? stars[0]!;
}

test('the detail route answers for a star, for an unknown key, and refuses junk', async ({
  request,
}) => {
  const alioth = await request.get('/api/sky/star/gaia-1576683529448755328');
  expect(alioth.status()).toBe(200);
  expect(alioth.headers()['cache-control']).toContain('max-age=86400');
  const detail = await alioth.json();
  expect(detail.title).toBe('Alioth');
  expect(detail.names.mainId).toBe('* eps UMa');
  expect(detail.spectralType).toEqual({ value: 'A1III-IVpkB9', from: 'SIMBAD' });
  expect(detail.constellation).toEqual({ abbreviation: 'UMa', name: 'Ursa Major' });
  expect(detail.source).toBe('database');

  // The same star by the name the *sky* calls it: the bright catalogue keys
  // Alioth by its Hipparcos number and the detail build filed it under its
  // Gaia source id, and the route is what closes that gap.
  const byHip = await request.get('/api/sky/star/hip-62956');
  expect(byHip.status()).toBe(200);
  expect(await byHip.json()).toMatchObject({
    source: 'database',
    title: 'Alioth',
    names: { mainId: '* eps UMa', gaia: '1576683529448755328' },
  });

  // A valid key with no row: the key's own truth, at 200. The detail build and
  // the bright catalogue were cut from Gaia differently, so this happens.
  const missing = await request.get('/api/sky/star/gaia-1');
  expect(missing.status()).toBe(200);
  expect(await missing.json()).toMatchObject({
    source: 'catalog',
    title: 'Gaia DR3 1',
    sources: [],
  });

  for (const junk of ['tyc-7', 'gaia-007', 'gaia-0', 'hip-999999', 'gaia-abc']) {
    expect((await request.get(`/api/sky/star/${junk}`)).status(), junk).toBe(400);
  }
});

test('hovering a star tags it, and clicking it opens the panel', async ({ page, request }) => {
  const star = await starOnScreen(page);

  await page.mouse.move(star.x, star.y);
  const tag = page.locator('.star-tag');
  await expect(tag).toBeVisible();
  await expect(tag.locator('strong')).toHaveText(star.name);
  // Beside the star, never on it.
  const box = (await tag.boundingBox())!;
  expect(Math.abs(box.x + box.width / 2 - star.x)).toBeGreaterThan(10);

  await page.mouse.click(star.x, star.y);
  const panel = page.locator('.star-detail');
  await expect(panel).toBeVisible();
  await expect(panel).toHaveAttribute('data-key', star.key);
  await expect(panel.locator('.star-detail-state')).toHaveCount(0);

  // What the panel says is what the route said, for this star.
  const payload = await (await request.get(`/api/sky/star/${star.key}`)).json();
  expect(payload.names.mainId).toBeTruthy();
  await expect(panel.locator('h2')).toHaveText(payload.title);
  await expect(panel.locator('.star-detail-table tr')).not.toHaveCount(0);
  await expect(panel.locator('.star-detail-links a').first()).toHaveAttribute(
    'href',
    /simbad\.cds\.unistra\.fr/,
  );
  if (star.name === GATE_STAR) expect(payload.names.mainId).toBe('* eps UMa');

  await page.keyboard.press('Escape');
  await expect(panel).toHaveCount(0);
});

test('a named callout opens the same panel', async ({ page }) => {
  await page.goto('/?skydebug=1');
  const callout = page.locator('.callout-label').first();
  await expect(callout).toBeVisible({ timeout: 30_000 });
  // Dispatched rather than pointed at: a callout glides across the sky every
  // frame, so Playwright's stability check never settles on one.
  await callout.dispatchEvent('click');
  await expect(page.locator('.star-detail')).toBeVisible();
  await expect(page.locator('.star-detail h2')).not.toBeEmpty();
});

test('a star the page has already named gets no tag, and its callout lights up', async ({
  page,
}) => {
  await page.goto('/?skydebug=1');
  // The group itself has no size — the label inside it is what is drawn.
  await expect(page.locator('.callout-label').first()).toBeVisible({ timeout: 30_000 });
  await page.locator('button.sky-control', { hasText: 'Pause sky' }).click();
  await page.waitForTimeout(200);

  // A callout whose *star* a pointer can reach: a forced one is clamped to the
  // edge and the star it names may be under the card, and a bright neighbour
  // can take the pick at the pixel the star is on. Each candidate is tried
  // until one of them is actually hovered.
  const named = await calloutNames(page);
  const reachable = (await onScreen(page)).filter((star) => named.includes(star.name));
  expect(reachable.length, 'a callout star clear of the page chrome').toBeGreaterThan(0);

  let hovered: string | null = null;
  for (const candidate of reachable) {
    await page.mouse.move(candidate.x, candidate.y);
    await page.waitForTimeout(150);
    const state = await page
      .locator(`.star-callout[data-name="${candidate.name}"]`)
      .getAttribute('data-hover');
    if (state === 'true') {
      hovered = candidate.name;
      break;
    }
  }
  expect(hovered, 'a callout star the pick agrees is under the pointer').toBeTruthy();

  const callout = page.locator(`.star-callout[data-name="${hovered}"]`);
  await expect(callout).toHaveAttribute('data-hover', 'true');
  // The callout already says the name; the tag would say it again, on top.
  await expect(page.locator('.star-tag')).toBeHidden();

  // And it is given back when the pointer moves off the star.
  await page.mouse.move(20, 500);
  await expect(callout).not.toHaveAttribute('data-hover', 'true');
});

test('a tap picks on a coarse pointer', async ({ browser }) => {
  const context = await browser.newContext({
    viewport: { width: 390, height: 844 },
    hasTouch: true,
    isMobile: true,
  });
  const page = await context.newPage();
  const star = await starOnScreen(page);
  await page.touchscreen.tap(star.x, star.y);
  await expect(page.locator('.star-detail')).toBeVisible();
  // No hover tag is left behind under the finger.
  await expect(page.locator('.star-tag')).toBeHidden();
  await context.close();
});
