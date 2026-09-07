import { expect, test } from '@playwright/test';

import { litPixels } from './sky-readback';

test('the landing page renders', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('heading', { level: 1 })).toContainText('Ronit Nath');
  await expect(page.getByText('Founder of')).toBeVisible();
  await expect(page.getByRole('link', { name: 'Isoastra' })).toHaveAttribute(
    'href',
    'https://isoastra.com',
  );
  for (const name of ['GitHub', 'Instagram', 'LinkedIn', 'Email']) {
    await expect(page.getByRole('link', { name, exact: true })).toBeVisible();
  }
  await expect(page.getByRole('button', { name: /theme/i })).toBeVisible();
});

test('the sky is drawn on the canvas behind the hero', async ({ page }) => {
  // `?skyreadback=1` is what turns `preserveDrawingBuffer` on (gl-util.ts).
  // Without it the drawing buffer is undefined by the time a Playwright
  // `evaluate` runs, because that runs between frames; the alternative —
  // reading back from the page's own loop — would mean shipping test-only
  // code inside the render path, which is the worse of the two.
  await page.goto('/?skyreadback=1');
  const sky = page.locator('canvas.starscape');
  await expect(sky).toBeAttached();
  // Attached is not drawn: ask the canvas whether any starlight landed on it.
  await expect.poll(async () => litPixels(page), { timeout: 20_000 }).toBeGreaterThan(100);
  // ...and the 2D fallback is not also on screen: two skies is twice the
  // stars at half the brightness.
  await expect(page.locator('canvas.starscape-flat')).toBeHidden();
});

test('the theme toggle switches the document theme both ways', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: /theme/i }).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  await page.getByRole('button', { name: /theme/i }).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
});

test('healthz reports a live database', async ({ request }) => {
  const res = await request.get('/healthz');
  expect(res.status()).toBe(200);
  expect(res.headers()['cache-control']).toBe('no-store');
  expect(await res.json()).toMatchObject({ ok: true, db: 'ok' });
});

test('the about page explains the sky and links back to it', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('link', { name: 'About the sky' }).click();
  await expect(page).toHaveURL(/\/about$/);
  await expect(page.getByRole('heading', { level: 1 })).toContainText('About the sky');
  for (const name of ['The stars', 'The Milky Way', 'From catalogue to screen']) {
    await expect(page.getByRole('heading', { level: 2, name })).toBeVisible();
  }
  await expect(page.getByRole('link', { name: 'ESA Gaia Archive' })).toHaveAttribute(
    'href',
    'https://gea.esac.esa.int/archive/',
  );
  await expect(page.locator('canvas.starscape')).toHaveCount(0);
  await page.getByRole('link', { name: 'Back to the sky' }).click();
  await expect(page).toHaveURL(/\/$/);
});
