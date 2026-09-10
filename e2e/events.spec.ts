import { expect, test, type Page } from '@playwright/test';

/* R4's golden flows: a host makes a page, hands out links, and strangers
 * answer without an account. Everything here runs against the standalone
 * server and the dev database. */

const PASSWORD = 'a-long-enough-password';

function address(tag: string): string {
  return `${tag}-${Date.now()}-${Math.floor(Math.random() * 1e6)}@example.com`;
}

/** Register, confirm nothing, sign in: an event host only needs an account,
 *  and the verification path is R2's flow, tested there. */
async function member(page: Page, name: string): Promise<string> {
  const email = address('host');
  await page.goto('/auth/sign-in');
  await page.getByLabel('Name').fill(name);
  await page.locator('#register-email').fill(email);
  await page.locator('#register-password').fill(PASSWORD);
  await page.getByRole('button', { name: 'Register' }).click();
  await expect(page.getByText('Check your inbox')).toBeVisible();
  return email;
}

async function signedInHost(page: Page, name: string): Promise<void> {
  const email = await member(page, name);
  /* An unconfirmed address cannot sign in, so the host confirms through the
   * link the transport wrote to disk — the same helper R3 uses, inlined
   * because this spec needs only the one address. */
  const { readdir, readFile } = await import('node:fs/promises');
  const { join } = await import('node:path');
  const dir = join(process.cwd(), '.mail');
  const deadline = Date.now() + 15_000;
  let path: string | null = null;
  while (Date.now() < deadline && path === null) {
    for (const file of (await readdir(dir).catch(() => [])).sort().reverse()) {
      const body = await readFile(join(dir, file), 'utf8');
      if (!body.includes(`To: ${email}`)) continue;
      const found = /https?:\/\/[^\s]*\/api\/auth\/verify-email[^\s]*/.exec(body);
      if (found) {
        path = found[0];
        break;
      }
    }
    if (path === null) await new Promise((resolve) => setTimeout(resolve, 200));
  }
  if (path === null) throw new Error('no verification link');
  /* The click is the confirmation: better-auth verifies at its endpoint. */
  await page.goto(path);
  await page.goto('/auth/sign-in');
  await page.locator('#sign-in-email').fill(email);
  await page.locator('#sign-in-password').fill(PASSWORD);
  await page.getByRole('button', { name: 'Sign in' }).click();
  await expect(page).toHaveURL('/app');
}

interface Draft {
  title: string;
  capacity?: string;
  address?: string;
}

/** Create an event and land on its page. */
async function createEvent(page: Page, draft: Draft): Promise<string> {
  await page.goto('/app/events/new');
  await page.getByLabel('Title').fill(draft.title);
  await page.getByLabel('Starts').fill('2026-08-30T14:00');
  await page.getByLabel('Ends').fill('2026-08-30T19:00');
  await page.getByLabel('Place').fill('Ronit’s apartment');
  await page.getByLabel('Address, after yes').fill(draft.address ?? '1 Sansome St, San Francisco');
  if (draft.capacity) await page.getByLabel('Capacity').fill(draft.capacity);
  await page.getByLabel('Body').fill('Board games and hanging out.\n\n- snacks\n- a game');
  await page.getByRole('button', { name: 'Create' }).click();
  await expect(page).toHaveURL(/\/app\/events\/e_/);
  return page.url();
}

async function invite(page: Page, names: string[]): Promise<void> {
  await page.getByLabel('One name, address or number per line').fill(names.join('\n'));
  await page.getByRole('button', { name: 'Invite' }).click();
  await expect(page.getByText(`${names.length} invited`)).toBeVisible();
}

/** Publish, and take the URLs off the page that shows them once. */
async function publish(page: Page): Promise<Map<string, string>> {
  await page.getByRole('button', { name: 'Publish' }).click();
  await expect(page.locator('.publish .note[data-state="done"]')).toBeVisible();
  const out = new Map<string, string>();
  for (const row of await page.locator('.minted-list li').all()) {
    const name = (await row.locator('.who').innerText()).trim();
    const url = (await row.locator('.url').innerText()).trim();
    /* The page shows an absolute URL built from PUBLIC_ORIGIN, which is not
     * where the test server listens; what a guest follows is the path. */
    out.set(name, `${new URL(url).pathname}${new URL(url).search}`);
  }
  return out;
}

test('a host publishes, two guests answer, and the host sees both', async ({ page, browser }) => {
  await signedInHost(page, 'The Host');
  const url = await createEvent(page, { title: `Board games ${Date.now()}` });
  await invite(page, ['Sam Okafor', 'Jordan Lee']);
  const links = await publish(page);
  expect(links.size).toBe(3);
  expect(links.get('Anyone')).toBeTruthy();

  /* Jordan answers first, so that Sam has somebody to see softened. */
  const first = await browser.newContext();
  const jordan = await first.newPage();
  await jordan.goto(links.get('Jordan Lee')!);
  await expect(jordan.getByRole('heading', { level: 1 })).toContainText('Board games');
  await jordan.getByRole('radio', { name: 'Yes', exact: true }).check();
  await jordan.locator('.answer button[type="submit"]').click();
  await expect(jordan.locator('.who-list')).toHaveAttribute('data-blurred', 'false');
  await first.close();

  /* Sam's link knows Sam's name, shows the list softened and first-name-only,
   * and does not carry the address. */
  const second = await browser.newContext();
  const sam = await second.newPage();
  await sam.goto(links.get('Sam Okafor')!);
  await expect(sam.getByText('Sam Okafor')).toBeVisible();
  await expect(sam.locator('.who-list')).toHaveAttribute('data-blurred', 'true');
  await expect(sam.locator('.names .name')).toHaveText(['Jordan']);
  await expect(sam.getByText('1 Sansome St')).toHaveCount(0);
  await expect(sam.getByRole('link', { name: 'Add to calendar' })).toHaveCount(0);

  await sam.getByRole('radio', { name: 'Yes', exact: true }).check();
  await sam.locator('.answer button[type="submit"]').click();
  /* After yes: the address, a sharp list, and an entry a calendar can hold. */
  await expect(sam.getByText('1 Sansome St, San Francisco')).toBeVisible();
  await expect(sam.locator('.who-list')).toHaveAttribute('data-blurred', 'false');
  const calendar = sam.getByRole('link', { name: 'Add to calendar' });
  await expect(calendar).toBeVisible();
  const ics = await sam.request.get((await calendar.getAttribute('href'))!);
  expect(ics.headers()['content-type']).toContain('text/calendar');
  const body = await ics.text();
  expect(body).toContain('BEGIN:VEVENT');
  expect(body).toContain('SEQUENCE:');
  expect(body).toContain('1 Sansome St');
  await second.close();

  /* A stranger through the open link says who they are first. */
  const third = await browser.newContext();
  const stranger = await third.newPage();
  await stranger.goto(links.get('Anyone')!);
  await stranger.getByLabel('Your name').fill('Casey Stranger');
  await stranger.getByRole('radio', { name: 'Maybe' }).check();
  await stranger.getByLabel('A note for the host').fill('might be late');
  await stranger.locator('.answer button[type="submit"]').click();
  /* From here on they hold a link of their own. */
  await expect(stranger).toHaveURL(/\/e\/[^?]+\?l=/);
  await expect(stranger.getByText('Casey Stranger')).toBeVisible();
  await third.close();

  await page.goto(url);
  await expect(page.getByRole('row', { name: /Sam Okafor/ })).toContainText('Yes');
  await expect(page.getByRole('row', { name: /Jordan Lee/ })).toContainText('Yes');
  const casey = page.getByRole('row', { name: /Casey Stranger/ });
  await expect(casey).toContainText('Maybe');
  await expect(casey).toContainText('might be late');
  await expect(page.getByRole('heading', { name: /2 yes, 1 maybe/ })).toBeVisible();
});

test('a guest changes their mind, and the row does not double', async ({ page, browser }) => {
  await signedInHost(page, 'Second Host');
  const url = await createEvent(page, { title: `Second thoughts ${Date.now()}` });
  await invite(page, ['Priya Rao']);
  const links = await publish(page);

  const other = await browser.newContext();
  const priya = await other.newPage();
  await priya.goto(links.get('Priya Rao')!);
  await priya.getByRole('radio', { name: 'Yes', exact: true }).check();
  await priya.locator('.answer button[type="submit"]').click();
  await expect(priya.locator('.who-list')).toHaveAttribute('data-blurred', 'false');

  await priya.getByRole('radio', { name: 'No' }).check();
  await priya.locator('.answer button[type="submit"]').click();
  await expect(priya.locator('.who-list')).toHaveAttribute('data-blurred', 'true');
  /* The link remembers them: coming back shows the answer they left. */
  await priya.reload();
  await expect(priya.locator('input[name="response"]:checked')).toHaveValue('no');
  await other.close();

  await page.goto(url);
  await expect(page.getByRole('row', { name: /Priya Rao/ })).toHaveCount(1);
  await expect(page.getByRole('row', { name: /Priya Rao/ })).toContainText('No');
  await expect(page.getByRole('heading', { name: /0 yes, 0 maybe, 1 no/ })).toBeVisible();
});

test('a yes past the capacity is a yes, and the page says full', async ({ page, browser }) => {
  await signedInHost(page, 'Small Room');
  const url = await createEvent(page, { title: `Small room ${Date.now()}`, capacity: '1' });
  await invite(page, ['One Guest', 'Two Guest']);
  const links = await publish(page);

  const first = await browser.newContext();
  const one = await first.newPage();
  await one.goto(links.get('One Guest')!);
  await one.getByRole('radio', { name: 'Yes', exact: true }).check();
  await one.locator('.answer button[type="submit"]').click();
  await expect(one.getByText('1 Sansome St')).toBeVisible();
  await first.close();

  const second = await browser.newContext();
  const two = await second.newPage();
  await two.goto(links.get('Two Guest')!);
  await expect(two.getByText('full')).toBeVisible();
  await two.getByRole('radio', { name: 'Yes', exact: true }).check();
  await two.locator('.answer button[type="submit"]').click();
  /* Not an error: the answer landed, and the guest is told where they stand. */
  await expect(two.getByText('You are on the list past the line')).toBeVisible();
  await expect(two.getByText('1 Sansome St')).toBeVisible();
  await second.close();

  await page.goto(url);
  await expect(page.getByRole('heading', { name: /2 yes/ })).toBeVisible();
  await expect(page.getByRole('heading', { name: /past the line/ })).toBeVisible();
});

test('an unpublished event declines its links exactly as a made-up one does', async ({
  page,
  browser,
}) => {
  await signedInHost(page, 'Cancelling Host');
  await createEvent(page, { title: `Called off ${Date.now()}` });
  await invite(page, ['Hopeful Guest']);
  const links = await publish(page);
  const link = links.get('Hopeful Guest')!;

  const other = await browser.newContext();
  const guest = await other.newPage();
  await guest.goto(link);
  await expect(guest.locator('.answer button[type="submit"]')).toBeVisible();

  await page.getByRole('button', { name: 'Unpublish' }).click();
  await expect(page.getByText('Unpublished')).toBeVisible();

  const declined = guest.getByText('This link does not work');
  await guest.goto(link);
  await expect(declined).toBeVisible();

  const slug = new URL(link, 'http://x').pathname;
  await guest.goto(`${slug}?l=${'a'.repeat(43)}`);
  await expect(declined).toBeVisible();
  await guest.goto('/e/no-such-event-at-all');
  await expect(declined).toBeVisible();
  /* And the calendar entry goes with it. */
  const ics = await guest.request.get(`${slug}/calendar.ics?l=${'a'.repeat(43)}`);
  expect(ics.status()).toBe(404);
  await other.close();
});

test('a shared circle comes first in the list of who is coming', async ({ page, browser }) => {
  await signedInHost(page, 'Circle Host');
  await createEvent(page, { title: `Circles ${Date.now()}` });
  await invite(page, ['Ada Circle', 'Bo Circle', 'Cy Outside']);
  const links = await publish(page);

  /* R5: a circle is a group. Two of the three guests are in one. */
  await page.goto('/app/groups');
  await page.getByLabel('Name').fill('Inner');
  await page.getByRole('button', { name: 'Create' }).click();
  await expect(page.getByRole('heading', { name: /^Inner/ })).toBeVisible();
  for (const name of ['Ada Circle', 'Bo Circle']) {
    await page.getByLabel('Add').selectOption({ label: name });
    await page.getByRole('button', { name: 'Add', exact: true }).click();
    await expect(page.getByRole('row', { name: new RegExp(name) })).toBeVisible();
  }

  /* Cy answers first, so answered-order alone would put Cy first. */
  const outside = await browser.newContext();
  const cy = await outside.newPage();
  await cy.goto(links.get('Cy Outside')!);
  await cy.getByRole('radio', { name: 'Yes', exact: true }).check();
  await cy.locator('.answer button[type="submit"]').click();
  await outside.close();

  const second = await browser.newContext();
  const bo = await second.newPage();
  await bo.goto(links.get('Bo Circle')!);
  await bo.getByRole('radio', { name: 'Yes', exact: true }).check();
  await bo.locator('.answer button[type="submit"]').click();
  await second.close();

  /* Ada shares a group with Bo and not with Cy, so Bo is read first. */
  const third = await browser.newContext();
  const ada = await third.newPage();
  await ada.goto(links.get('Ada Circle')!);
  await expect(ada.locator('.names .name')).toHaveText(['Bo', 'Cy']);
  await third.close();
});
