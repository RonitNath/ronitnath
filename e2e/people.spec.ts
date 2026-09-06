import { readdir, readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { expect, test, type Page } from '@playwright/test';

/* R3's golden flows: a member writes somebody down, hands them a URL, and the
 * two rows turn out to be one person. Everything here runs against the
 * standalone server and the dev database, and reads its mail off disk. */

const MAIL_DIR = join(process.cwd(), '.mail');
const PASSWORD = 'a-long-enough-password';

function address(tag: string): string {
  return `${tag}-${Date.now()}-${Math.floor(Math.random() * 1e6)}@example.com`;
}

async function linkFor(to: string, path: string): Promise<string> {
  const pattern = new RegExp(`${path}/([A-Za-z0-9_-]{43})`);
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
      if (found) return `${path}/${found[1]}`;
    }
    await new Promise((resolve) => setTimeout(resolve, 200));
  }
  throw new Error(`no ${path} link was mailed to ${to}`);
}

async function signIn(page: Page, email: string, password = PASSWORD) {
  await page.goto('/auth');
  await page.locator('#sign-in-email').fill(email);
  await page.locator('#sign-in-password').fill(password);
  await page.getByRole('button', { name: 'Sign in' }).click();
}

async function member(page: Page, name: string): Promise<string> {
  const email = address('member');
  await page.goto('/auth');
  await page.getByLabel('Name').fill(name);
  await page.locator('#register-email').fill(email);
  await page.locator('#register-password').fill(PASSWORD);
  await page.getByRole('button', { name: 'Register' }).click();
  await expect(page.getByText('Check your inbox')).toBeVisible();
  await page.goto(await linkFor(email, '/auth/verify'));
  await page.getByRole('button', { name: 'Confirm' }).click();
  await expect(page.getByText('Address confirmed')).toBeVisible();
  await signIn(page, email);
  await expect(page).toHaveURL('/app');
  return email;
}

/** Hold somebody, mint their invitation, and return its path. The page shows
 *  an absolute URL built from PUBLIC_ORIGIN, which is not where the test
 *  server listens; what the visitor follows is the path. */
async function hold(page: Page, handle: string, name: string): Promise<string> {
  await page.goto('/app/people');
  await page.getByLabel('Email, phone, or name').fill(handle);
  await page.getByLabel('What you call them').fill(name);
  await page.getByRole('button', { name: 'Hold' }).click();
  await expect(page.getByText(`${name} is yours to invite`)).toBeVisible();

  await page.getByRole('row', { name: new RegExp(name) }).getByRole('button', { name: 'Invite' }).click();
  const url = page.locator('.minted .url');
  await expect(url).toBeVisible();
  const minted = (await url.innerText()).trim();
  expect(minted).toMatch(/^https?:\/\/[^/]+\/links\/[A-Za-z0-9_-]{43}$/);
  return new URL(minted).pathname;
}

test('hold, invite, and claim as somebody with no account', async ({ page, browser }) => {
  await member(page, 'The Holder');
  const guest = address('guest');
  const url = await hold(page, guest, 'A Guest');

  const other = await browser.newContext();
  const visitor = await other.newPage();
  await visitor.goto(url);
  await expect(visitor.getByText('The Holder')).toBeVisible();
  await expect(visitor.getByText('A Guest')).toBeVisible();
  /* The address the member typed is already in the form. */
  await expect(visitor.locator('#claim-email')).toHaveValue(guest);

  await visitor.locator('#claim-password').fill(PASSWORD);
  await visitor.getByRole('button', { name: 'Claim' }).click();
  await expect(visitor.getByText('Check your inbox')).toBeVisible();

  /* The claim already happened: the member's list says so, and the row is
   * the same row — the contact edge followed the merge onto a person who now
   * has an account of their own. */
  await page.goto('/app/people');
  const row = page.getByRole('row', { name: /A Guest/ });
  await expect(row).toContainText('joined');
  await expect(row).toContainText('Claimed');
  await expect(row.getByRole('button', { name: 'Invite' })).toHaveCount(0);

  /* And the account is real once the address is confirmed. */
  await visitor.goto(await linkFor(guest, '/auth/verify'));
  await visitor.getByRole('button', { name: 'Confirm' }).click();
  await signIn(visitor, guest);
  await expect(visitor).toHaveURL('/app');
  await expect(visitor.getByRole('heading', { level: 1 })).toHaveText('A Guest');
  /* One address, not two: the handle and the confirmed address were the same
   * address, and the merge did not leave a second row saying so. */
  await expect(visitor.getByRole('cell', { name: guest })).toHaveCount(1);

  /* The link is spent. */
  await visitor.goto(url);
  await expect(visitor.getByText('This link does not work')).toBeVisible();
  await other.close();
});

test('hold, invite, and claim while already signed in', async ({ page, browser }) => {
  await member(page, 'Another Holder');
  const url = await hold(page, '+1 (415) 555-0188', 'Phone Friend');

  const other = await browser.newContext();
  const claimant = await other.newPage();
  const email = await member(claimant, 'Already A Member');

  await claimant.goto(url);
  await expect(claimant.getByText('Another Holder')).toBeVisible();
  await expect(claimant.getByText('Signed in as')).toBeVisible();
  await claimant.getByRole('button', { name: 'This is me' }).click();
  await expect(claimant).toHaveURL('/app');

  /* What the contact card carried is now theirs: the phone handle moved,
   * because nothing they already held spelled it. */
  await expect(claimant.getByRole('cell', { name: '+14155550188' })).toBeVisible();
  await expect(claimant.getByRole('cell', { name: email })).toBeVisible();

  /* The member's list keeps the row and loses the placeholder name: it is
   * the person themselves now, under the name they chose. */
  await page.goto('/app/people');
  await expect(page.getByRole('row', { name: /Phone Friend/ })).toHaveCount(0);
  await expect(page.getByRole('row', { name: /Already A Member/ })).toContainText('joined');
  await other.close();
});

test('a revoked link and a link that never existed decline identically', async ({
  page,
  browser,
}) => {
  await member(page, 'Careful Holder');
  const url = await hold(page, address('revoked'), 'Second Thoughts');

  await page.goto('/app/people');
  const revoked = page.getByRole('row', { name: /Second Thoughts/ });
  await revoked.getByRole('button', { name: 'Revoke' }).click();
  await expect(revoked).toContainText('Revoked');

  const other = await browser.newContext();
  const visitor = await other.newPage();
  const declined = visitor.getByText('This link does not work');

  await visitor.goto(url);
  await expect(declined).toBeVisible();

  const invented = `/links/${'a'.repeat(43)}`;
  await visitor.goto(invented);
  await expect(declined).toBeVisible();

  await visitor.goto('/links/not-a-token');
  await expect(declined).toBeVisible();
  await other.close();
});

test('an invitation is watched: not opened, then opened', async ({ page, browser }) => {
  await member(page, 'Watching Holder');
  const url = await hold(page, address('watched'), 'Watched Guest');

  await page.goto('/app/people');
  const row = page.getByRole('row', { name: /Watched Guest/ });
  await expect(row).toContainText('Not opened');

  const other = await browser.newContext();
  const visitor = await other.newPage();
  await visitor.goto(url);
  await expect(visitor.getByRole('button', { name: 'Claim' })).toBeVisible();
  await other.close();

  await page.reload();
  await expect(page.getByRole('row', { name: /Watched Guest/ })).toContainText('Opened');
});

test('a confirmed address that somebody else holds becomes a question, not a merge', async ({
  page,
  browser,
}) => {
  await member(page, 'Proposing Holder');
  const shared = address('proposed');
  await page.goto('/app/people');
  await page.getByLabel('Email, phone, or name').fill(shared);
  await page.getByLabel('What you call them').fill('Maybe You');
  await page.getByRole('button', { name: 'Hold' }).click();
  await expect(page.getByText('Maybe You is yours to invite')).toBeVisible();

  /* Somebody registers that address on their own, with no link at all. */
  const other = await browser.newContext();
  const claimant = await other.newPage();
  await claimant.goto('/auth');
  await claimant.getByLabel('Name').fill('The Real One');
  await claimant.locator('#register-email').fill(shared);
  await claimant.locator('#register-password').fill(PASSWORD);
  await claimant.getByRole('button', { name: 'Register' }).click();
  await claimant.goto(await linkFor(shared, '/auth/verify'));
  await claimant.getByRole('button', { name: 'Confirm' }).click();
  await signIn(claimant, shared);

  /* Nothing merged on its own. The member is asked. */
  await expect(claimant.getByRole('heading', { name: 'Is this you?' })).toBeVisible();
  await expect(claimant.getByText('Proposing Holder')).toBeVisible();
  await claimant.getByRole('button', { name: 'That is me' }).click();
  /* The question is answered and gone; what is left is one person. */
  await expect(claimant.getByRole('heading', { name: 'Is this you?' })).toHaveCount(0);

  await page.goto('/app/people');
  await expect(page.getByRole('row', { name: /The Real One/ })).toContainText('joined');
  await other.close();
});

test('a member may add a second address and remove one, but never the last', async ({ page }) => {
  const first = await member(page, 'Two Doors');

  await page.goto('/app');
  await expect(page.getByRole('button', { name: 'Remove' })).toHaveCount(0);

  const second = address('second');
  await page.getByLabel('Add an address').fill(second);
  await page.getByRole('button', { name: 'Send the link' }).click();
  await expect(page.getByText('Check your inbox')).toBeVisible();

  await page.goto(await linkFor(second, '/auth/verify'));
  await page.getByRole('button', { name: 'Confirm' }).click();
  await expect(page.getByText('Address confirmed')).toBeVisible();

  await page.goto('/app');
  await expect(page.getByRole('cell', { name: second })).toBeVisible();
  /* Two doors, so either may go. */
  await page.getByRole('row', { name: new RegExp(second) }).getByRole('button', { name: 'Remove' }).click();
  await expect(page.getByRole('cell', { name: second })).toHaveCount(0);

  /* One door, so it may not. */
  await expect(page.getByRole('cell', { name: first })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Remove' })).toHaveCount(0);
});

test('a member can change what they are called', async ({ page }) => {
  await member(page, 'Before');
  await page.goto('/app');
  await page.getByLabel('Name').fill('After');
  await page.getByRole('button', { name: 'Save' }).click();
  await expect(page.getByText('Name changed')).toBeVisible();
  await page.reload();
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('After');
});
