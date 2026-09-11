import { expect, test } from '@playwright/test';

test('artifact is ready and hides delivery inspection from visitors', async ({ request }) => {
  const ready = await request.get('/readyz');
  expect(ready.ok()).toBe(true);
  const denied = await request.get('/o/isoastra/delivery');
  expect(denied.status()).toBe(404);
});
