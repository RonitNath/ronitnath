import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { expect, test, type Page } from '@playwright/test';

const run = promisify(execFile);
const PASSWORD = 'a-long-enough-password';
const OPERATOR = `realtime-operator-${Date.now()}@example.com`;

async function signIn(page: Page) {
  await page.goto('/auth/sign-in');
  await page.locator('#sign-in-email').fill(OPERATOR);
  await page.locator('#sign-in-password').fill(PASSWORD);
  await page.getByRole('button', { name: 'Sign in', exact: true }).click();
  await expect(page).toHaveURL(/\/u\/p_[A-Za-z0-9_-]+$/);
}

test.beforeAll(async () => {
  await run('pnpm', ['seed:operator', '--email', OPERATOR, '--password', PASSWORD], {
    cwd: process.cwd(),
  });
});

test('an operator can inspect and drive an anonymous browser view', async ({ browser }) => {
  const privateContext = await browser.newContext();
  const privatePage = await privateContext.newPage();
  await privatePage.goto('/');
  await expect(privatePage.getByRole('heading', { level: 1 })).toBeVisible();
  const visitorId = await privatePage.evaluate(() =>
    localStorage.getItem('rn_realtime_visitor'),
  );
  expect(visitorId).toMatch(/^[0-9a-f-]{36}$/);
  const visitorLabel = `visitor ${visitorId!.slice(0, 8)}`;

  const adminContext = await browser.newContext();
  const adminPage = await adminContext.newPage();
  await signIn(adminPage);
  await adminPage.goto('/o/isoastra/realtime');
  const visitor = adminPage.getByRole('link', { name: visitorLabel });
  await expect(visitor).toBeVisible({ timeout: 15_000 });
  await visitor.click();
  await expect(
    adminPage.getByRole('heading', { name: 'Authorized subscriptions' }),
  ).toBeVisible();
  await expect(adminPage.locator('pre.payload')).toContainText('homepage');
  await expect(adminPage.locator('pre.payload')).toContainText('announcement');

  const forged = await privatePage.evaluate(async () => {
    const id = () => crypto.randomUUID();
    const response = await fetch('/api/realtime', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({
        action: 'register',
        view: {
          visitorId: id(),
          tabId: id(),
          viewId: id(),
          path: '/d/not-published',
          title: 'forged',
          subscriptions: [
            { type: 'resource', resourceKind: 'document', resourceId: 'r_forged' },
          ],
          viewport: { visible: true, sectionIds: [], rowIds: [] },
          interactedAt: null,
        },
      }),
    });
    return response.json();
  });
  expect(forged.subscriptions).toEqual([]);

  const announcement = `Realtime proof ${Date.now()}`;
  await adminPage.goto('/o/isoastra/configuration');
  await adminPage.getByLabel('Text').fill(announcement);
  await adminPage.getByLabel('Enabled').check();
  await adminPage.getByRole('button', { name: 'Save live' }).click();
  await expect(adminPage.getByText(/Live revision \d+\./)).toBeVisible();
  await expect(privatePage.getByText(announcement)).toBeVisible({ timeout: 15_000 });

  await adminPage.goto('/o/isoastra/realtime');
  await adminPage.getByRole('link', { name: visitorLabel }).click();
  const delivered = adminPage
    .getByRole('row')
    .filter({ hasText: 'live configuration changed' })
    .first();
  await expect(delivered.getByRole('cell').nth(3)).toHaveText('yes', { timeout: 15_000 });
  await expect(delivered.getByRole('cell').nth(4)).toHaveText('yes');
  await expect(delivered.getByRole('cell').nth(5)).toHaveText('yes');

  await privatePage.reload();
  await expect(privatePage.getByText(announcement)).toBeVisible();
  await adminPage.goto('/o/isoastra/realtime');
  await expect(adminPage.getByRole('link', { name: visitorLabel })).toBeVisible();

  await privateContext.close();
  await adminContext.close();
});
