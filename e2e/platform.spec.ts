import { execFile } from 'node:child_process';
import { readdir, readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { promisify } from 'node:util';
import { expect, test, type Browser, type Page } from '@playwright/test';

/* The operator console is `/o/isoastra` — a static segment under the org
 * grammar, not an audience route — and a member's own surfaces are
 * `/u/<their public id>`, which no constant can name. */
const CONSOLE = '/o/isoastra';
const HOME = /\/u\/p_[A-Za-z0-9_-]+$/;


/* R6's golden flows: the operator surface, against the standalone server the
 * image ships and the dev database.
 *
 * The operator here is a *local* account, seeded by `pnpm seed:operator
 * --password`. Ronit's own operator signs in through ZITADEL and has no
 * password on this side at all, which is exactly why the seed script grew the
 * flag: a browser cannot be sent to an identity provider in a test.
 *
 * Serial, because every test in the file shares one operator account and the
 * commands under test change what the others' pages list. */

test.describe.configure({ mode: 'serial' });

const run = promisify(execFile);
const MAIL_DIR = join(process.cwd(), '.mail');
const PASSWORD = 'a-long-enough-password';
const OPERATOR = `r6-operator-${Date.now()}@example.com`;

function address(tag: string): string {
  return `${tag}-${Date.now()}-${Math.floor(Math.random() * 1e6)}@example.com`;
}

function named(tag: string): string {
  return `R6 ${tag} ${Date.now().toString(36)}${Math.floor(Math.random() * 1e4)}`;
}

/* Better-auth's letters carry an absolute URL to one of its own endpoints,
 * which does its work and bounces the reader to the page named in
 * `callbackURL`; a test follows the mailed URL exactly as a reader would. */
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

async function signIn(page: Page, email: string) {
  await page.goto('/auth/sign-in');
  /* Confirming an address signs the reader in, and the door does not stand
     open to somebody who is already through it: it sends them to their own
     surfaces. A test that means to prove a password has to leave first, and it
     signs out and waits for the landing: the session row has to go, because
     the sessions list counts it, and a sign-out still in flight would arrive
     after the next sign-in and end that one instead. */
  if (!page.url().includes('/auth/sign-in')) {
    await page.getByRole('button', { name: 'Sign out' }).click();
    await page.waitForURL(/\/$/);
    await page.goto('/auth/sign-in');
  }
  await page.locator('#sign-in-email').fill(email);
  await page.locator('#sign-in-password').fill(PASSWORD);
  await page.getByRole('button', { name: 'Sign in', exact: true }).click();
}

/** Register, confirm and sign in: an ordinary member with a door of their own. */
async function member(page: Page, name: string): Promise<string> {
  const email = address('r6');
  await page.goto('/auth/sign-in');
  await page.getByLabel('Name').fill(name);
  await page.locator('#register-email').fill(email);
  await page.locator('#register-password').fill(PASSWORD);
  await page.getByRole('button', { name: 'Register' }).click();
  await expect(page.getByText('Check your inbox')).toBeVisible();
  await page.goto(await linkFor(email, '/api/auth/verify-email'));
  await signIn(page, email);
  await expect(page).toHaveURL(HOME);
  return email;
}

/** A member in a browser of their own, left signed in. */
async function elsewhere(browser: Browser, name: string) {
  const context = await browser.newContext();
  const page = await context.newPage();
  const email = await member(page, name);
  return { context, page, email };
}

async function asOperator(page: Page) {
  await signIn(page, OPERATOR);
  await expect(page).toHaveURL(HOME);
}

/** The window the sharp commands ask for. */
async function confirmIsMe(page: Page) {
  await page.goto(`${CONSOLE}/reauth?next=${encodeURIComponent(CONSOLE)}`);
  await page.getByLabel('Password').fill(PASSWORD);
  await page.getByRole('button', { name: 'Confirm', exact: true }).click();
  await expect(page.getByText('Confirmed. The window is open')).toBeVisible();
}

test.beforeAll(async () => {
  await run('pnpm', ['seed:operator', '--email', OPERATOR, '--password', PASSWORD], {
    cwd: process.cwd(),
  });
});

const PAGES = [
  CONSOLE,
  `${CONSOLE}/parties`,
  `${CONSOLE}/matches`,
  `${CONSOLE}/sessions`,
  `${CONSOLE}/audit`,
  `${CONSOLE}/operators`,
  `${CONSOLE}/deployment`,
];

test('every platform page is the operator’s, and nobody else’s', async ({ page, browser }) => {
  /* Anonymous: a 404, never a redirect to the door — a visitor must not be
   * able to tell an internal page from a missing one. */
  for (const path of PAGES) {
    const response = await page.goto(path);
    expect(response?.status(), path).toBe(404);
  }

  /* A signed-in member: the same 404, and no link to it in the chrome. */
  const them = await elsewhere(browser, named('Member'));
  await expect(them.page.getByRole('link', { name: 'Platform' })).toHaveCount(0);
  for (const path of PAGES) {
    const response = await them.page.goto(path);
    expect(response?.status(), path).toBe(404);
  }
  await them.context.close();

  /* The operator: every one of them, and the way in from the shell. */
  await asOperator(page);
  await expect(page.getByRole('link', { name: 'Platform' })).toBeVisible();
  for (const path of PAGES) {
    const response = await page.goto(path);
    expect(response?.status(), path).toBe(200);
  }
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Deployment');
  await expect(page.getByText('Database round trip')).toBeVisible();
});

test('an operator proposes a pair, rules on it, and splits it again', async ({ page, browser }) => {
  const one = await elsewhere(browser, named('Kept'));
  const two = await elsewhere(browser, named('Absorbed'));
  const keptName = await one.page.getByRole('heading', { level: 1 }).innerText();
  const goneName = await two.page.getByRole('heading', { level: 1 }).innerText();
  await one.context.close();
  await two.context.close();

  await asOperator(page);
  await confirmIsMe(page);

  await page.goto(`${CONSOLE}/matches`);
  await page.getByLabel('One party').selectOption({ label: keptName });
  await page.getByLabel('The other party').selectOption({ label: goneName });
  await page.getByRole('button', { name: 'Propose' }).click();
  await expect(page.getByText('Nothing has merged')).toBeVisible();

  /* Ruling on it is a merge with a written reason. */
  await page.reload();
  const open = page.locator('#open').getByRole('row', { name: new RegExp(keptName) }).first();
  await open.getByLabel('Merge').selectOption({ label: `${keptName} survives` });
  await open.getByLabel('Why').fill('one person, two registrations');
  await open.getByRole('button', { name: 'Merge' }).click();
  /* The question is answered, so it leaves the queue — and with it the
   * control that asked it, which is why the effect is what this waits on. */
  await expect(page.locator('#open').getByRole('row', { name: new RegExp(goneName) })).toHaveCount(
    0,
  );

  /* The absorbed person is retired. */
  await page.goto(`${CONSOLE}/parties?q=${encodeURIComponent(goneName)}`);
  await expect(page.getByRole('row', { name: new RegExp(goneName) })).toContainText('merged');

  /* And a split puts them both back. */
  await page.goto(`${CONSOLE}/matches`);
  const merged = page.locator('#merged').getByRole('row', { name: new RegExp(goneName) }).first();
  await merged.getByLabel('Why').fill('two people after all');
  await merged.getByRole('button', { name: 'Split' }).click();
  await expect(
    page.locator('#merged').getByRole('row', { name: new RegExp(goneName) }),
  ).toHaveCount(0);

  await page.goto(`${CONSOLE}/parties?q=${encodeURIComponent(goneName)}`);
  await expect(page.getByRole('row', { name: new RegExp(goneName) })).toContainText('active');
});

test('disabling a person ends their session and refuses their sign-in', async ({
  page,
  browser,
}) => {
  const name = named('Switched Off');
  const them = await elsewhere(browser, name);

  await asOperator(page);
  await confirmIsMe(page);
  await page.goto(`${CONSOLE}/parties?q=${encodeURIComponent(name)}`);
  await page.getByRole('link', { name }).click();
  await page.getByLabel('Why').first().fill('asked to be closed');
  await page.getByRole('button', { name: 'Disable' }).click();
  await expect(page.getByRole('button', { name: 'Enable' })).toBeVisible();
  await expect(page.locator('.party-state')).toHaveText('disabled');

  /* Their session is gone: the door again. */
  await them.page.goto('/app');
  await expect(them.page).toHaveURL(/\/auth/);

  /* And the door answers the way it answers a password nobody has. */
  await signIn(them.page, them.email);
  await expect(them.page.getByText('Authentication failed.')).toBeVisible();
  await them.context.close();

  /* Enabling puts it back. */
  await page.reload();
  await page.getByRole('button', { name: 'Enable' }).click();
  await expect(page.getByRole('button', { name: 'Disable' })).toBeVisible();
});

test('signing in as somebody shows the bar, names both, and ends', async ({ page, browser }) => {
  const name = named('Impersonated');
  const them = await elsewhere(browser, name);
  await them.context.close();

  await asOperator(page);
  await confirmIsMe(page);
  await page.goto(`${CONSOLE}/parties?q=${encodeURIComponent(name)}`);
  await page.getByRole('link', { name }).click();
  /* The reason is not optional: the control cannot even be submitted without
   * one, and the command refuses one that is too short. */
  const impersonate = page.locator('form').filter({
    has: page.getByRole('button', { name: 'Sign in as' }),
  });
  await expect(impersonate.getByLabel('Why')).toHaveAttribute('required', '');
  await impersonate.getByLabel('Why').fill('a support question they asked about');
  await impersonate.getByRole('button', { name: 'Sign in as' }).click();
  await expect(page).toHaveURL(HOME);

  /* The bar is on every page, and it says whose name is being worn. */
  const bar = page.getByRole('status').filter({ hasText: 'Acting as' });
  await expect(bar).toContainText(name);
  await page.goto('/');
  await expect(bar).toBeVisible();

  /* An action taken while wearing it names both. Wearing a name lands on that
     person's own surfaces, so the path is already theirs. */
  await page.goto('/app');
  await page.getByLabel('Name').fill(`${name} renamed`);
  await page.getByRole('button', { name: 'Save' }).click();
  await expect(page.getByText('Name changed')).toBeVisible();

  await bar.getByRole('button', { name: 'End' }).click();
  await expect(page).toHaveURL(CONSOLE);
  await expect(page.getByText('Acting as')).toHaveCount(0);

  await page.goto(`${CONSOLE}/parties?q=${encodeURIComponent(name)}`);
  await page.getByRole('link', { name: `${name} renamed` }).click();
  const renamed = page.getByRole('row', { name: /set-display-name/ }).first();
  await expect(renamed).toBeVisible();
  /* Both names on the one row: the person whose account it was, and the
   * operator who was wearing it. */
  await expect(renamed).toContainText('acting_operator');
});

test('an operator grants the tier to somebody else, who then reaches it', async ({
  page,
  browser,
}) => {
  const name = named('Promoted');
  const them = await elsewhere(browser, name);
  expect((await them.page.goto(CONSOLE))?.status()).toBe(404);

  await asOperator(page);
  await confirmIsMe(page);
  await page.goto(`${CONSOLE}/operators`);
  const grant = page.locator('form').filter({ has: page.getByRole('button', { name: 'Grant' }) });
  await grant.getByLabel('Grant').selectOption({ label: name });
  await grant.getByLabel('Why').fill('second pair of hands');
  await grant.getByRole('button', { name: 'Grant' }).click();
  await expect(page.getByRole('row', { name: new RegExp(name) })).toBeVisible();

  await them.page.goto(CONSOLE);
  await expect(them.page.getByRole('heading', { level: 1 })).toHaveText('Platform');
  await expect(them.page.getByRole('link', { name: 'Platform' })).toBeVisible();

  /* And they can be taken back off it. */
  await page.goto(`${CONSOLE}/operators`);
  const row = page.getByRole('row', { name: new RegExp(name) });
  await row.getByLabel('Why').fill('done');
  await row.getByRole('button', { name: 'Revoke' }).click();
  await expect(page.getByRole('row', { name: new RegExp(name) })).toHaveCount(0);
  expect((await them.page.goto(CONSOLE))?.status()).toBe(404);
  await them.context.close();
});

test('a sharp command asks for the password again once the window has closed', async ({
  page,
  browser,
}) => {
  const name = named('Untouched');
  const them = await elsewhere(browser, name);
  await them.context.close();

  /* A fresh operator session has never been confirmed, so the first sharp
   * command refuses — and says so, because the caller is already inside. */
  await asOperator(page);
  await page.goto(`${CONSOLE}/parties?q=${encodeURIComponent(name)}`);
  await page.getByRole('link', { name }).click();
  await page.getByLabel('Why').first().fill('no reason at all');
  await page.getByRole('button', { name: 'Disable' }).click();
  await expect(page.getByText('Confirm it is you')).toBeVisible();
  await page.getByRole('link', { name: 'Confirm' }).click();
  await expect(page).toHaveURL(new RegExp(`${CONSOLE}/reauth`));

  /* The party is untouched: a refused command wrote nothing. */
  await page.goto(`${CONSOLE}/parties?q=${encodeURIComponent(name)}`);
  await expect(page.getByRole('row', { name: new RegExp(name) })).toContainText('active');
});
