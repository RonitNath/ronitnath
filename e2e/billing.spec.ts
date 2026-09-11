import { execFile } from 'node:child_process';
import { readdir, readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { promisify } from 'node:util';
import { expect, test, type Page } from '@playwright/test';

test.describe.configure({ mode: 'serial' });
test.setTimeout(180_000);
const run = promisify(execFile);
const PASSWORD = 'a-long-enough-password';
const OPERATOR = `billing-operator-${Date.now()}@example.com`;
const MAIL_DIR = join(process.cwd(), '.mail');

async function linkFor(to: string) {
  const deadline = Date.now() + 15_000;
  while (Date.now() < deadline) {
    for (const name of await readdir(MAIL_DIR).catch(() => [])) {
      const body = await readFile(join(MAIL_DIR, name), 'utf8');
      if (body.includes(`To: ${to}`)) {
        const found = /https?:\/\/[^\s]*\/api\/auth\/verify-email[^\s]*/.exec(body);
        if (found) return found[0];
      }
    }
    await new Promise((resolve) => setTimeout(resolve, 200));
  }
  throw new Error('confirmation email missing');
}

async function signIn(page: Page, email: string) {
  await page.goto('/auth/sign-in');
  if (!page.url().includes('/auth/sign-in')) {
    await page.getByRole('button', { name: 'Sign out' }).click();
    await page.waitForURL(/\/$/);
    await page.goto('/auth/sign-in');
  }
  await page.locator('#sign-in-email').fill(email);
  await page.locator('#sign-in-password').fill(PASSWORD);
  const submit = page.getByRole('button', { name: 'Sign in', exact: true });
  await expect(submit).toBeEnabled({ timeout: 20_000 });
  await submit.click();
  await expect(page).toHaveURL(/\/u\/p_[A-Za-z0-9_-]+$/, { timeout: 30_000 });
}

async function register(page: Page, name: string) {
  const email = `billing-${Date.now()}-${Math.floor(Math.random() * 1e6)}@example.com`;
  await page.goto('/auth/sign-in');
  await page.getByLabel('Name').fill(name);
  await page.locator('#register-email').fill(email);
  await page.locator('#register-password').fill(PASSWORD);
  const submit = page.getByRole('button', { name: 'Register' });
  await expect(submit).toBeEnabled({ timeout: 20_000 });
  await submit.click();
  await page.goto(await linkFor(email));
  await signIn(page, email);
  return email;
}

test.beforeAll(async () => {
  await run('pnpm', ['seed:operator', '--email', OPERATOR, '--password', PASSWORD], { cwd: process.cwd() });
});

test('publishes immutable offers, invoices a customer, and manually reconciles payment', async ({ page, browser }, testInfo) => {
  await signIn(page, OPERATOR);
  await page.goto('/o/isoastra/billing');
  const ronit = page.locator('section').filter({ has: page.getByRole('heading', { name: 'Ronit Nath' }) });
  const catalog = {
    offers: [
      { id: 'e2e-support', title: 'E2E Support', description: 'Formal billing browser acceptance.', kind: 'one_time', amountAtoms: '2500', interval: null, accessPolicy: 'payment_first', trialDays: 0, paymentDueDays: 7, graceDays: 0, fulfillment: 'operator_confirmed', benefits: {}, available: true },
      { id: 'e2e-member', title: 'E2E Member', description: 'Private membership.', kind: 'subscription', amountAtoms: '1000', interval: 'month', accessPolicy: 'payment_first', trialDays: 0, paymentDueDays: 7, graceDays: 0, fulfillment: 'operator_confirmed', benefits: { 'social.private': true }, available: true },
    ],
  };
  await ronit.getByLabel('Catalog JSON').fill(JSON.stringify(catalog));
  await ronit.getByRole('button', { name: 'Save draft' }).click();
  await expect(ronit.getByText(/Draft \d+ saved/)).toBeVisible();
  await page.reload();
  const refreshed = page.locator('section').filter({ has: page.getByRole('heading', { name: 'Ronit Nath' }) });
  await refreshed.getByRole('button', { name: 'Publish immutable version' }).click();
  await expect(refreshed.getByText(/Catalog version \d+ published/)).toBeVisible();
  if (await refreshed.getByRole('button', { name: 'Enable seller' }).count()) {
    await refreshed.getByRole('button', { name: 'Enable seller' }).click();
    await expect(refreshed.getByText('ronit enabled.')).toBeVisible();
  }

  const customerContext = await browser.newContext();
  const customer = await customerContext.newPage();
  await register(customer, 'Billing Customer');
  const home = new URL(customer.url()).pathname;
  await customer.getByRole('link', { name: 'Billing' }).click();
  await expect(customer).toHaveURL(/\/billing$/);
  const billingPath = new URL(customer.url()).pathname;
  const offer = customer.locator('article').filter({ has: customer.getByRole('heading', { name: 'E2E Support' }) });
  await offer.getByRole('button', { name: 'Request invoice' }).click();
  await expect(customer.getByText('Invoice requested.')).toBeVisible();
  await customer.getByLabel('Evidence or reference').fill('bank transfer E2E-001');
  await customer.getByRole('button', { name: 'Submit payment evidence' }).click();
  await expect(customer.getByText('Payment evidence submitted')).toBeVisible();

  const strangerContext = await browser.newContext();
  const stranger = await strangerContext.newPage();
  await register(stranger, 'Other Customer');
  expect((await stranger.goto(billingPath))?.status()).toBe(404);
  await stranger.screenshot({ path: testInfo.outputPath('cross-customer-denied.png'), fullPage: true });
  await strangerContext.close();

  await page.goto('/o/isoastra/billing');
  const claim = page.getByRole('row', { name: /Billing Customer.*E2E-001/ });
  await claim.getByRole('button', { name: 'Confirm receipt' }).click();
  await expect(page.getByRole('row', { name: /Billing Customer.*E2E-001/ })).toHaveCount(0);
  await customer.goto(billingPath);
  await expect(customer.getByText(/paid · total \$25\.00 · paid \$25\.00/)).toBeVisible();
  await customer.screenshot({ path: testInfo.outputPath('customer-paid.png'), fullPage: true });

  const membershipOffer = customer.locator('article').filter({ has: customer.getByRole('heading', { name: 'E2E Member' }) });
  await membershipOffer.getByRole('button', { name: 'Request invoice' }).click();
  await expect(customer.getByText('Invoice requested.')).toBeVisible();
  const membershipInvoice = customer.locator('article').filter({ has: customer.getByRole('heading', { name: /e2e-member/ }) });
  await membershipInvoice.getByLabel('Amount (cents)').fill('400');
  await membershipInvoice.getByLabel('Evidence or reference').fill('bank transfer E2E-SUB-1');
  await membershipInvoice.getByRole('button', { name: 'Submit payment evidence' }).click();
  await expect(customer.getByText('Payment evidence submitted')).toBeVisible();
  await page.goto('/o/isoastra/billing');
  await page.getByRole('row', { name: /Billing Customer.*E2E-SUB-1/ }).getByRole('button', { name: 'Confirm receipt' }).click();
  await customer.goto(billingPath);
  await expect(customer.getByRole('heading', { name: 'expired' })).toBeVisible();
  const remainder = customer.locator('article').filter({ has: customer.getByRole('heading', { name: /e2e-member/ }) });
  await remainder.getByLabel('Amount (cents)').fill('600');
  await remainder.getByLabel('Evidence or reference').fill('bank transfer E2E-SUB-2');
  await remainder.getByRole('button', { name: 'Submit payment evidence' }).click();
  await expect(customer.getByText(/E2E-SUB-2.*pending/)).toBeVisible();
  await page.goto('/o/isoastra/billing');
  const finalClaim = page.getByRole('row', { name: /Billing Customer.*E2E-SUB-2/ });
  await finalClaim.getByRole('button', { name: 'Confirm receipt' }).click();
  await expect(finalClaim).toHaveCount(0);
  await customer.goto(billingPath);
  await expect(customer.getByRole('heading', { name: 'active' })).toBeVisible();
  await customer.getByRole('button', { name: 'Cancel renewal' }).click();
  await expect(customer.getByText('Renewal cancelled. Existing balances remain due.')).toBeVisible();
  await customer.screenshot({ path: testInfo.outputPath('membership-active-cancelled.png'), fullPage: true });
  await customer.goto(home);
  await customerContext.close();
});
