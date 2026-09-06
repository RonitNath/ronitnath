import { expect, test } from '@playwright/test';

test('the landing page renders', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('heading', { level: 1 })).toContainText('Ronit Nath');
  await expect(page.getByRole('button', { name: /theme/i })).toBeVisible();
});

test('the theme toggle switches the document theme', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: /theme/i }).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
});

test('healthz reports a live database', async ({ request }) => {
  const res = await request.get('/healthz');
  expect(res.status()).toBe(200);
  expect(res.headers()['cache-control']).toBe('no-store');
  expect(await res.json()).toMatchObject({ ok: true, db: 'ok' });
});
