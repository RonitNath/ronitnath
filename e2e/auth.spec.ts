import { readdir, readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { expect, test, type Page } from '@playwright/test';

/* The golden flows, against the standalone server and the dev database.
 *
 * Mail is read off disk: with no `SMTP_URL` the app writes every message to
 * `.mail/<timestamp>.eml`, which is the only way a test can follow a link it
 * was never told. Addresses are unique per run so two runs against the same
 * database do not collide. */

const MAIL_DIR = join(process.cwd(), '.mail');
const PASSWORD = 'a-long-enough-password';

function address(tag: string): string {
  return `${tag}-${Date.now()}-${Math.floor(Math.random() * 1e6)}@example.com`;
}

/* Better-auth's letters carry an absolute URL to one of its own endpoints —
 * `/api/auth/verify-email?token=…` and `/api/auth/reset-password/<token>` —
 * each of which does its work and bounces to the page named in `callbackURL`.
 * A test follows the mailed URL exactly as a reader would, so what is matched
 * here is the whole thing rather than a token this side would have to
 * reassemble. */
async function linkFor(to: string, path: string): Promise<string> {
  const pattern = new RegExp(`https?://[^\\s]*${path}[^\\s]*`);
  const deadline = Date.now() + 15_000;
  while (Date.now() < deadline) {
    let names: string[] = [];
    try {
      names = (await readdir(MAIL_DIR)).sort().reverse();
    } catch {
      /* the directory appears with the first message */
    }
    for (const name of names) {
      const body = await readFile(join(MAIL_DIR, name), 'utf8');
      if (!body.includes(`To: ${to}`)) continue;
      const found = pattern.exec(body);
      if (found) return found[0];
    }
    await new Promise((resolve) => setTimeout(resolve, 200));
  }
  throw new Error(`no ${path} link was mailed to ${to}`);
}

async function registerAndVerify(page: Page, name: string): Promise<string> {
  const email = address('member');
  await page.goto('/auth/sign-in');
  await page.getByLabel('Name').fill(name);
  await page.locator('#register-email').fill(email);
  await page.locator('#register-password').fill(PASSWORD);
  await page.getByRole('button', { name: 'Register' }).click();
  await expect(page.getByText('Check your inbox')).toBeVisible();

  /* The click is the confirmation: better-auth verifies at its endpoint and
   * lands the reader, signed in, on the page named in `callbackURL`. */
  await page.goto(await linkFor(email, '/api/auth/verify-email'));
  await expect(page.getByText('Address confirmed')).toBeVisible();
  return email;
}

async function signIn(page: Page, email: string, password = PASSWORD) {
  await page.goto('/auth/sign-in');
  await page.locator('#sign-in-email').fill(email);
  await page.locator('#sign-in-password').fill(password);
  await page.getByRole('button', { name: 'Sign in' }).click();
}

test('register, verify, sign in, list sessions, revoke another, sign out', async ({
  page,
  browser,
}) => {
  const email = await registerAndVerify(page, 'Test Member');

  await signIn(page, email);
  await expect(page).toHaveURL('/app');
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Test Member');
  await expect(page.getByText(email)).toBeVisible();
  /* A member is not an operator: the nav must not offer the surface. */
  await expect(page.getByRole('link', { name: 'Platform' })).toHaveCount(0);

  /* A second browser gives the sessions list something to revoke. */
  const other = await browser.newContext();
  const otherPage = await other.newPage();
  await signIn(otherPage, email);
  await expect(otherPage).toHaveURL('/app');

  await page.goto('/app/sessions');
  await expect(page.getByRole('row')).toHaveCount(3); // header + two sessions
  await expect(page.getByText('this one')).toHaveCount(1);
  await page.getByRole('button', { name: 'Revoke' }).click();
  await expect(page.getByRole('row')).toHaveCount(2);

  /* The revoked session is gone at the wrist, not just on the page. */
  await otherPage.goto('/app');
  await expect(otherPage).toHaveURL(/\/auth/);
  await other.close();

  await page.getByRole('button', { name: 'Sign out' }).click();
  await expect(page).toHaveURL('/');
  await expect(page.getByRole('link', { name: 'Sign in' })).toBeVisible();
});

test('the reset flow sets a new password and ends every open session', async ({ page }) => {
  const email = await registerAndVerify(page, 'Reset Member');
  await signIn(page, email);
  await expect(page).toHaveURL('/app');

  await page.goto('/auth/reset');
  await page.getByLabel('Email').fill(email);
  await page.getByRole('button', { name: 'Send the link' }).click();
  await expect(page.getByText('Check your inbox')).toBeVisible();

  await page.goto(await linkFor(email, '/api/auth/reset-password'));
  await page.getByLabel('New password').fill('an-entirely-new-password');
  await page.getByRole('button', { name: 'Set the password' }).click();
  await expect(page.getByText('Password set')).toBeVisible();

  /* The session that asked for the reset is gone with the rest. */
  await page.goto('/app');
  await expect(page).toHaveURL(/\/auth/);

  await signIn(page, email, 'an-entirely-new-password');
  await expect(page).toHaveURL('/app');
});

test('an unknown address and a wrong password decline identically', async ({ page }) => {
  const email = await registerAndVerify(page, 'Uniform Member');

  const decline = page.locator('.pane .note[data-state="invalid"]');

  await signIn(page, email, 'not-the-right-password');
  await expect(decline).toHaveText('Authentication failed.');
  await expect(page).toHaveURL(/\/auth/);

  await signIn(page, address('nobody'), PASSWORD);
  await expect(decline).toHaveText('Authentication failed.');
  await expect(page).toHaveURL(/\/auth/);
});

test('a reset for an address nobody registered says what a real one says', async ({ page }) => {
  await page.goto('/auth/reset');
  await page.getByLabel('Email').fill(address('nobody'));
  await page.getByRole('button', { name: 'Send the link' }).click();
  await expect(page.getByText('Check your inbox')).toBeVisible();
});

test('an anonymous visitor is sent from /app to the door, and back afterwards', async ({
  page,
}) => {
  await page.goto('/app/sessions');
  await expect(page).toHaveURL('/auth/sign-in?next=%2Fapp%2Fsessions');
  await expect(page.getByRole('heading', { name: 'Sign in' })).toBeVisible();
});

test('/platform is a 404 for a member, and is not linked at all', async ({ page }) => {
  const email = await registerAndVerify(page, 'Not An Operator');
  await signIn(page, email);
  await expect(page).toHaveURL('/app');

  const response = await page.goto('/platform');
  expect(response?.status()).toBe(404);
});

test('/platform is a 404 for an anonymous visitor too', async ({ page }) => {
  const response = await page.goto('/platform');
  expect(response?.status()).toBe(404);
});

/* The two ZITADEL route tests are gone with the routes. Better-auth owns the
 * authorization URL, the state, the PKCE pair and the callback at
 * `/api/auth/callback/zitadel`; a test asserting the query parameters it
 * builds would be a test of the library, and a real round trip needs the
 * deployed origin and the registered redirect URI. */

test('a confirmation that never went out can be asked for again', async ({ page }) => {
  const email = address('unconfirmed');
  await page.goto('/auth/sign-in');
  await page.getByLabel('Name').fill('Unconfirmed Member');
  await page.locator('#register-email').fill(email);
  await page.locator('#register-password').fill(PASSWORD);
  await page.getByRole('button', { name: 'Register' }).click();
  await expect(page.getByText('Check your inbox')).toBeVisible();

  /* The password is right; the address is not confirmed. The door says so
   * rather than declining, because the password already proved who is asking. */
  await signIn(page, email);
  await expect(page.getByRole('heading', { name: 'Confirm your email' })).toBeVisible();
  await expect(page).toHaveURL(/\/auth/);

  await page.getByRole('button', { name: 'Send it again' }).click();
  await expect(page.getByText('Check your inbox')).toBeVisible();

  await page.goto(await linkFor(email, '/api/auth/verify-email'));
  await expect(page.getByText('Address confirmed')).toBeVisible();

  await signIn(page, email);
  await expect(page).toHaveURL('/app');
});

test('registering when the mail transport fails still leaves an account', async ({ page }) => {
  /* `MAIL_FAIL=mailfail` in the Playwright server makes the transport refuse
   * this one address: the commit has happened, the letter has not, and the
   * visitor must see the page everyone else sees — not a server exception. */
  const email = address('mailfail');
  await page.goto('/auth/sign-in');
  await page.getByLabel('Name').fill('Undelivered Member');
  await page.locator('#register-email').fill(email);
  await page.locator('#register-password').fill(PASSWORD);
  await page.getByRole('button', { name: 'Register' }).click();
  await expect(page.getByText('Check your inbox')).toBeVisible();
  await expect(page.locator('.note[data-state="invalid"]')).toHaveCount(0);

  /* The account exists: the door knows the password and says what is missing. */
  await signIn(page, email);
  await expect(page.getByRole('heading', { name: 'Confirm your email' })).toBeVisible();
});
