import { readdir, readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { expect, test, type Page } from '@playwright/test';

/* R5's golden flows: an organization asks somebody in, a document is handed
 * around a level at a time, and a group decides who a guest sees first.
 * Everything here runs against the standalone server and the dev database. */

const MAIL_DIR = join(process.cwd(), '.mail');
const PASSWORD = 'a-long-enough-password';

function address(tag: string): string {
  return `${tag}-${Date.now()}-${Math.floor(Math.random() * 1e6)}@example.com`;
}

function handle(tag: string): string {
  return `${tag}-${Date.now().toString(36)}-${Math.floor(Math.random() * 1e4)}`;
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

async function signIn(page: Page, email: string) {
  await page.goto('/auth');
  await page.locator('#sign-in-email').fill(email);
  await page.locator('#sign-in-password').fill(PASSWORD);
  await page.getByRole('button', { name: 'Sign in' }).click();
  await expect(page).toHaveURL('/app');
}

/** Register, confirm, and sign in: a member with a door of their own. */
async function member(page: Page, name: string): Promise<string> {
  const email = address('r5');
  await page.goto('/auth');
  await page.getByLabel('Name').fill(name);
  await page.locator('#register-email').fill(email);
  await page.locator('#register-password').fill(PASSWORD);
  await page.getByRole('button', { name: 'Register' }).click();
  await expect(page.getByText('Check your inbox')).toBeVisible();
  await page.goto(await linkFor(email, '/auth/verify'));
  await page.getByRole('button', { name: 'Confirm' }).click();
  await signIn(page, email);
  return email;
}

/** The path of a URL a page shows once. */
function pathOf(url: string): string {
  const parsed = new URL(url);
  return `${parsed.pathname}${parsed.search}`;
}

test('an organization asks somebody in, and the handle is theirs afterwards', async ({
  page,
  browser,
}) => {
  await member(page, 'The Founder');
  const org = handle('org');

  await page.goto('/app');
  await page.getByLabel('Organization').fill('Isoastra Test');
  await page.getByLabel('Handle').fill(org);
  await page.getByRole('button', { name: 'Create' }).click();
  await expect(page).toHaveURL(`/org/${org}`);
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Isoastra Test');
  await expect(page.getByRole('row', { name: /The Founder/ })).toContainText('owner');

  /* The invitation is a claim link, shown once. */
  const guest = address('colleague');
  await page.getByLabel('Email, phone, or name').fill(guest);
  await page.getByLabel('What you call them').fill('A Colleague');
  await page.getByRole('button', { name: 'Invite' }).click();
  const minted = page.locator('.minted .url');
  await expect(minted).toBeVisible();
  const claim = pathOf((await minted.innerText()).trim());

  await page.reload();
  await expect(page.getByRole('row', { name: /A Colleague/ }).first()).toContainText('Not opened');

  /* They claim it with an account of their own, and the role comes across. */
  const theirs = await browser.newContext();
  const colleague = await theirs.newPage();
  await colleague.goto(claim);
  await expect(colleague.getByText('The Founder')).toBeVisible();
  await colleague.locator('#claim-password').fill(PASSWORD);
  await colleague.getByRole('button', { name: 'Claim' }).click();
  await expect(colleague.getByText('Check your inbox')).toBeVisible();
  await colleague.goto(await linkFor(guest, '/auth/verify'));
  await colleague.getByRole('button', { name: 'Confirm' }).click();
  await signIn(colleague, guest);
  await expect(colleague.getByRole('row', { name: /Isoastra Test/ })).toContainText('member');

  /* A member is not an operator: the page they are in is not their page. */
  await colleague.goto(`/org/${org}`);
  await expect(colleague.getByText('could not be found')).toBeVisible();

  /* The founder makes them an admin, and now it is. */
  await page.reload();
  const row = page.getByRole('row', { name: /A Colleague/ }).first();
  await row.getByLabel('Role').selectOption('admin');
  await row.getByRole('button', { name: 'Set' }).click();
  /* The word in the cell, not the option in the select beside it. */
  await expect(
    page
      .getByRole('row', { name: /A Colleague/ })
      .first()
      .getByRole('cell', { name: 'admin', exact: true }),
  ).toBeVisible();

  await colleague.goto(`/org/${org}`);
  await expect(colleague.getByRole('heading', { level: 1 })).toHaveText('Isoastra Test');
  await theirs.close();

  /* And a member of no organization at all gets the same 404 a made-up
   * handle gets. */
  const outside = await browser.newContext();
  const stranger = await outside.newPage();
  await member(stranger, 'A Stranger');
  await stranger.goto(`/org/${org}`);
  await expect(stranger.getByText('could not be found')).toBeVisible();
  await stranger.goto('/org/no-such-organization');
  await expect(stranger.getByText('could not be found')).toBeVisible();
  await outside.close();
});

test('a document is shared a level at a time, and taken back', async ({ page, browser }) => {
  await member(page, 'The Author');

  /* The reader has to be somebody the author holds, so they are held and they
   * claim — R3's path, which is how a person gets into an address book. */
  const readerAddress = address('reader');
  await page.goto('/app/people');
  await page.getByLabel('Email, phone, or name').fill(readerAddress);
  await page.getByLabel('What you call them').fill('The Reader');
  await page.getByRole('button', { name: 'Hold' }).click();
  await page
    .getByRole('row', { name: /The Reader/ })
    .getByRole('button', { name: 'Invite' })
    .click();
  const claim = pathOf((await page.locator('.minted .url').innerText()).trim());

  const theirs = await browser.newContext();
  const reader = await theirs.newPage();
  await reader.goto(claim);
  await reader.locator('#claim-password').fill(PASSWORD);
  await reader.getByRole('button', { name: 'Claim' }).click();
  await reader.goto(await linkFor(readerAddress, '/auth/verify'));
  await reader.getByRole('button', { name: 'Confirm' }).click();
  await signIn(reader, readerAddress);

  /* The author writes something. */
  const title = `A memo ${Date.now()}`;
  await page.goto('/app/documents');
  await page.getByLabel('Title').fill(title);
  await page.getByRole('button', { name: 'Create' }).click();
  await expect(page).toHaveURL(/\/app\/documents\/r_/);
  const url = new URL(page.url()).pathname;
  await page.getByLabel('Body').fill('The first paragraph.\n\n- one\n- two');
  await page.getByRole('button', { name: 'Save' }).click();
  await expect(page.getByText('Saved.')).toBeVisible();

  /* Before a share, the document is not theirs to see. */
  await reader.goto(url);
  await expect(reader.getByText('could not be found')).toBeVisible();

  await page.getByLabel('Share with').selectOption({ label: 'The Reader' });
  await page.getByLabel('As').selectOption('viewer');
  await page.getByRole('button', { name: 'Share' }).click();
  await expect(page.getByRole('row', { name: /The Reader/ })).toContainText('viewer');

  /* A viewer reads it and cannot touch it. */
  await reader.goto(url);
  await expect(reader.getByRole('heading', { level: 1 })).toHaveText(title);
  await expect(reader.getByText('The first paragraph.')).toBeVisible();
  await expect(reader.getByLabel('Body')).toHaveCount(0);
  await expect(reader.getByText('Share with')).toHaveCount(0);

  /* An editor may write. */
  await page.getByLabel('Share with').selectOption({ label: 'The Reader' });
  await page.getByLabel('As').selectOption('editor');
  await page.getByRole('button', { name: 'Share' }).click();
  await expect(page.getByRole('row', { name: /The Reader/ })).toContainText('editor');

  await reader.goto(url);
  await reader.getByLabel('Body').fill('The reader was here.');
  await reader.getByRole('button', { name: 'Save' }).click();
  await expect(reader.getByText('Saved.')).toBeVisible();

  /* Revoked is gone, and gone is 404. */
  await page.reload();
  await page.getByRole('row', { name: /The Reader/ }).getByRole('button', { name: 'Revoke' }).click();
  /* The row goes with the share: what a revoked share leaves is nothing. */
  await expect(page.getByRole('row', { name: /The Reader/ })).toHaveCount(0);
  await reader.goto(url);
  await expect(reader.getByText('could not be found')).toBeVisible();
  await theirs.close();

  /* Published, it is anybody's. */
  await page.goto(url);
  await page.getByRole('button', { name: 'Publish' }).click();
  const public_ = page.getByRole('link', { name: /^\/d\// });
  await expect(public_).toBeVisible();
  const slug = (await public_.getAttribute('href'))!;

  const outside = await browser.newContext();
  const anybody = await outside.newPage();
  await anybody.goto(slug);
  await expect(anybody.getByRole('heading', { level: 1 })).toHaveText(title);
  await expect(anybody.getByText('The reader was here.')).toBeVisible();

  /* Taken back, it declines exactly as a slug nobody ever used does. */
  await page.getByRole('button', { name: 'Unpublish' }).click();
  await expect(page.getByText('Unpublished.')).toBeVisible();
  await anybody.goto(slug);
  await expect(anybody.getByText('could not be found')).toBeVisible();
  await anybody.goto('/d/no-such-document-anywhere');
  await expect(anybody.getByText('could not be found')).toBeVisible();
  await outside.close();
});
