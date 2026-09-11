import { expect, test } from '@playwright/test';

test('artifact is ready and hides delivery inspection from visitors', async ({ page, request }) => {
  const ready = await request.get('/readyz');
  expect(ready.ok()).toBe(true);
  await page.goto('/o/isoastra/delivery');
  await expect(page.getByRole('heading', { name: 'Nothing here.' })).toBeVisible();
});
