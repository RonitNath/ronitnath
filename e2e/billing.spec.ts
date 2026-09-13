import { readdir, readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { expect, test, type Page } from '@playwright/test';

test.describe.configure({ mode: 'serial' });
test.setTimeout(600_000);
const PASSWORD = 'a-long-enough-password';
const OPERATOR = process.env.BILLING_TEST_OPERATOR!;
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
  console.log(`billing-e2e: sign in ${email}`);
  await page.goto('/auth/sign-in');
  if (!page.url().includes('/auth/sign-in')) {
    await page.getByRole('button', { name: 'Sign out' }).click();
    await page.waitForURL(/\/$/);
    await page.goto('/auth/sign-in');
  }
  const response = await page.request.post('/api/auth/sign-in/email', {
    data: { email, password: PASSWORD },
    headers: { origin: new URL(page.url()).origin },
  });
  expect(response.ok(), await response.text()).toBe(true);
  console.log('billing-e2e: authenticated');
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
  if (!/\/u\/p_[A-Za-z0-9_-]+$/.test(new URL(page.url()).pathname)) await page.goto('/auth/sign-in');
  await expect(page).toHaveURL(/\/u\/p_[A-Za-z0-9_-]+$/, { timeout: 90_000 });
  return email;
}

test('publishes immutable offers, invoices a customer, and manually reconciles payment', async ({ page, browser }, testInfo) => {
  await signIn(page, OPERATOR);
  console.log('billing-e2e: load operator billing');
  await page.goto('/o/isoastra/billing');
  console.log('billing-e2e: operator billing loaded');
  const ronit = page.locator('section').filter({ has: page.getByRole('heading', { name: 'Ronit Nath' }) });
  await ronit.getByLabel('Offer key').fill('e2e-support');
  await ronit.getByLabel('Title', { exact: true }).fill('E2E Support');
  await ronit.getByLabel('Description', { exact: true }).fill('Formal billing browser acceptance.');
  await ronit.getByLabel('Price (USD cents)').fill('2500');
  await ronit.locator('select[name="fulfillment"]').selectOption('operator_confirmed');
  const savePilot = ronit.getByRole('button', { name: 'Save pilot offer draft' });
  await expect(savePilot).toBeEnabled();
  console.log('billing-e2e: submit draft');
  await savePilot.click();
  console.log('billing-e2e: draft submitted');
  await expect(ronit.locator('.notice, .error')).toBeVisible({ timeout: 90_000 });
  console.log(`billing-e2e: draft result ${await ronit.locator('.notice, .error').innerText()}`);
  await expect(ronit.getByText(/Pilot offer draft \d+ saved/)).toBeVisible();
  await page.reload();
  const refreshed = page.locator('section').filter({ has: page.getByRole('heading', { name: 'Ronit Nath' }) });
  await refreshed.getByText('Advanced catalog JSON').click();
  await refreshed.getByRole('button', { name: 'Publish immutable version' }).click();
  await expect(refreshed.locator('details .notice')).toHaveText(/Catalog version \d+ published/);
  await page.reload();
  const activation = page.locator('section').filter({ has: page.getByRole('heading', { name: 'Ronit Nath' }) });
  await activation.getByText('Advanced catalog JSON').click();
  await activation.getByRole('button', { name: 'Enable seller' }).click();
  await expect(activation.locator('details .notice')).toHaveText('ronit enabled.');
  await page.reload();
  await expect(page.locator('section').filter({ has: page.getByRole('heading', { name: 'Ronit Nath' }) }).getByText(/Book ronitnath-usd · enabled/)).toBeVisible();

  const customerContext = await browser.newContext();
  const customer = await customerContext.newPage();
  await register(customer, 'Billing Customer');
  const home = new URL(customer.url()).pathname;
  await customer.getByRole('link', { name: 'Billing' }).click();
  await expect(customer).toHaveURL(/\/billing$/);
  const billingPath = new URL(customer.url()).pathname;
  await expect(customer.getByText('No pilot offers are available for this account.')).toBeVisible();
  const publicPersonId = home.split('/').at(-1)!;
  await page.goto('/o/isoastra/billing');
  await page.getByLabel('Public person ID').fill(publicPersonId);
  await page.getByRole('button', { name: 'Enable pilot account' }).click();
  await expect(page.getByText('Pilot account enabled.')).toBeVisible();
  await customer.goto(billingPath);
  const offer = customer.locator('article').filter({ has: customer.getByRole('heading', { name: 'E2E Support' }) });
  await offer.getByRole('button', { name: 'Request invoice' }).click();
  await expect(customer.getByText('Invoice requested.')).toBeVisible();
  await customer.getByLabel('Amount (cents)').fill('3000');
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
  await claim.getByLabel('Receipt ID').fill('E2E-RECEIPT-001');
  await claim.getByRole('button', { name: 'Confirm receipt' }).click();
  await expect(page.getByRole('row', { name: /Billing Customer.*E2E-001/ })).toHaveCount(0);
  await customer.goto(billingPath);
  await expect(customer.getByText(/paid · total \$25\.00 · paid \$25\.00/)).toBeVisible();
  await expect(customer.getByText(/applied \$25\.00 · unapplied credit \$5\.00/)).toBeVisible();
  await customer.screenshot({ path: testInfo.outputPath('customer-paid.png'), fullPage: true });

  await offer.getByRole('button', { name: 'Request invoice' }).click();
  await expect(customer.getByText('Invoice requested.')).toBeVisible();
  await page.goto('/o/isoastra/billing');
  const orders = page.getByRole('row', { name: /Billing Customer.*e2e-support/ });
  const unpaidOrder = orders.first();
  await unpaidOrder.getByLabel('Amount (cents)').first().fill('500');
  await unpaidOrder.getByLabel('Reason').fill('pilot adjustment');
  await unpaidOrder.getByLabel('Service effect').selectOption('revoke_unfulfilled_service');
  await unpaidOrder.getByRole('button', { name: 'Issue credit' }).click();
  await expect(page.getByText('Credit note recorded.')).toBeVisible();
  const revokedOrder = page.getByRole('row', { name: /Billing Customer.*e2e-support.*service_revoked/ }).first();
  await expect(revokedOrder.getByRole('button', { name: 'Record fulfillment' })).toHaveCount(0);
  const paidOrder = page.getByRole('row', { name: /Billing Customer.*e2e-support.*paid/ }).first();
  await paidOrder.getByRole('button', { name: 'Record fulfillment' }).click();
  const fulfilledOrder = page.getByRole('row', { name: /Billing Customer.*e2e-support.*fulfilled/ }).first();
  await expect(fulfilledOrder).toBeVisible();
  await fulfilledOrder.getByLabel('Amount (cents)').last().fill('1000');
  await fulfilledOrder.getByLabel('Reason').last().fill('externally returned');
  await fulfilledOrder.getByLabel('External refund ID').fill('E2E-REFUND-001');
  await fulfilledOrder.getByRole('button', { name: 'Record external refund' }).click();
  await expect(page.getByText('Externally completed refund recorded.')).toBeVisible();
  const pilot = page.locator('article').filter({ has: page.getByRole('heading', { name: 'Billing Customer' }) }).first();
  await pilot.getByRole('button', { name: 'Disable new purchases' }).click();
  await expect(page.getByText('New purchases disabled for this account.')).toBeVisible();
  await customer.goto(billingPath);
  await expect(customer.getByText('No pilot offers are available for this account.')).toBeVisible();
  await expect(customer.getByText(/credited \$5\.00/)).toBeVisible();
  await expect(customer.getByText(/refunded \$10\.00/)).toBeVisible();
  await customer.screenshot({ path: testInfo.outputPath('pilot-history-after-disable.png'), fullPage: true });
  await customer.goto(home);
  await customerContext.close();
});
