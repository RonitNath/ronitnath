import { expect, test } from '@playwright/test';

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
  await page.goto('/');
  const sky = page.locator('canvas.starscape');
  await expect(sky).toBeAttached();
  // Attached is not drawn: ask the canvas whether any star landed on it.
  await expect
    .poll(async () =>
      sky.evaluate((canvas: HTMLCanvasElement) => {
        const pixels = canvas
          .getContext('2d')!
          .getImageData(0, 0, canvas.width, canvas.height).data;
        let lit = 0;
        for (let i = 3; i < pixels.length; i += 4) if (pixels[i]! > 0) lit += 1;
        return lit;
      }),
    )
    .toBeGreaterThan(100);
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
