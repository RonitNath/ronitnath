import { readdir, readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { expect, test, type Page } from '@playwright/test';
import { Pool } from 'pg';

const mailDirectory = join(process.cwd(), '.mail');
const password = 'calendar-test-password';

function address(label: string): string {
  return `calendar-${label}-${Date.now()}-${Math.floor(Math.random() * 1e6)}@example.com`;
}

async function mailedLink(to: string): Promise<string> {
  const deadline = Date.now() + 20_000;
  while (Date.now() < deadline) {
    let files: string[] = [];
    try { files = (await readdir(mailDirectory)).sort().reverse(); } catch { /* first mail creates it */ }
    for (const file of files) {
      const body = await readFile(join(mailDirectory, file), 'utf8');
      if (!body.includes(`To: ${to}`)) continue;
      const link = /https?:\/\/[^\s]*\/api\/auth\/verify-email[^\s]*/.exec(body)?.[0];
      if (link) return link;
    }
    await new Promise((resolve) => setTimeout(resolve, 200));
  }
  throw new Error(`No verification link for ${to}`);
}

async function register(page: Page, name: string): Promise<{ email: string; userPath: string }> {
  const email = address(name.toLowerCase().replaceAll(' ', '-'));
  await page.goto('/auth/sign-in');
  await page.getByLabel('Name').fill(name);
  await page.locator('#register-email').fill(email);
  await page.locator('#register-password').fill(password);
  await page.getByRole('button', { name: 'Register' }).click();
  await page.goto(await mailedLink(email));
  await page.goto('/auth/sign-in');
  if (!page.url().includes('/auth/sign-in')) {
    await page.getByRole('button', { name: 'Sign out' }).click();
    await page.goto('/auth/sign-in');
  }
  await page.locator('#sign-in-email').fill(email);
  await page.locator('#sign-in-password').fill(password);
  await page.getByRole('button', { name: 'Sign in', exact: true }).click();
  await expect(page).toHaveURL(/\/u\/p_[A-Za-z0-9_-]+$/, { timeout: 30_000 });
  return { email, userPath: new URL(page.url()).pathname };
}

async function signIn(page: Page, email: string): Promise<void> {
  await page.goto('/auth/sign-in');
  await page.locator('#sign-in-email').fill(email);
  await page.locator('#sign-in-password').fill(password);
  await page.getByRole('button', { name: 'Sign in', exact: true }).click();
  await expect(page).toHaveURL(/\/u\/p_[A-Za-z0-9_-]+$/, { timeout: 30_000 });
}

test('personal calendar works through HTTP and keeps account boundaries', async ({ browser }) => {
  test.setTimeout(600_000);
  const tokyo = await browser.newContext({ timezoneId: 'Asia/Tokyo', viewport: { width: 1440, height: 1000 } });
  const ownerPage = await tokyo.newPage();
  const owner = await register(ownerPage, 'Calendar Owner');
  const calendarPath = `${owner.userPath}/calendar`;
  await ownerPage.goto(calendarPath);
  await expect(ownerPage.getByRole('heading', { name: 'Calendar', exact: true })).toBeVisible();
  await expect(ownerPage.getByText('Asia/Tokyo', { exact: true })).toBeVisible({ timeout: 20_000 });

  const tomorrow = new Date(Date.now() + 86_400_000).toISOString().slice(0, 10);
  await ownerPage.getByRole('button', { name: 'New event' }).click();
  const eventDialog = ownerPage.getByRole('dialog', { name: 'New event' });
  await eventDialog.getByLabel('Title').fill('Private appointment');
  await eventDialog.getByLabel('Starts').fill(`${tomorrow}T09:00`);
  await eventDialog.getByLabel('Minutes').fill('45');
  await eventDialog.getByRole('button', { name: 'Save' }).click();
  await expect(ownerPage.getByText('Event saved.')).toBeVisible();
  await expect(ownerPage.getByText('Private appointment')).toBeVisible();

  await ownerPage.getByRole('button', { name: 'New task' }).click();
  const taskDialog = ownerPage.getByRole('dialog', { name: 'New task' });
  await taskDialog.getByLabel('Title').fill('Write itinerary');
  await taskDialog.getByLabel('Due').fill(`${tomorrow}T11:00`);
  await taskDialog.getByLabel('Estimate in minutes').fill('30');
  await taskDialog.getByRole('button', { name: 'Save' }).click();
  await expect(ownerPage.getByText('Task saved.')).toBeVisible();
  const task = ownerPage.getByText('Write itinerary').first();
  await expect(task).toBeVisible();
  await ownerPage.getByRole('button', { name: 'Schedule' }).click();
  const blockDialog = ownerPage.getByRole('dialog', { name: 'Schedule task' });
  await blockDialog.getByLabel('Starts').fill(`${tomorrow}T13:00`);
  await blockDialog.getByRole('button', { name: 'Save' }).click();
  await expect(ownerPage.getByText('Work block scheduled.')).toBeVisible();
  await ownerPage.getByLabel('Tasks').getByRole('checkbox').click();
  await expect(ownerPage.getByText('Task completed.')).toBeVisible();

  const stalePool = new Pool({ connectionString: process.env.DATABASE_URL });
  try {
    await stalePool.query("UPDATE calendar_item SET version=version+100 WHERE title='Private appointment' AND scope_id=$1", [owner.userPath.split('/').at(-1)]);
  } finally {
    await stalePool.end();
  }
  await ownerPage.getByText('Private appointment').click();
  let editDialog = ownerPage.getByRole('dialog', { name: 'Edit occurrence' });
  await editDialog.getByLabel('Starts').fill(`${tomorrow}T10:00`);
  await editDialog.getByLabel('Ends').fill(`${tomorrow}T10:45`);
  await editDialog.getByRole('button', { name: 'Save' }).click();
  await expect(ownerPage.getByText(/Expected version \d+, found \d+/)).toBeVisible();
  await expect(editDialog).toBeVisible();
  await ownerPage.reload();
  await ownerPage.getByText('Private appointment').click();
  editDialog = ownerPage.getByRole('dialog', { name: 'Edit occurrence' });
  await editDialog.getByLabel('Starts').fill(`${tomorrow}T10:00`);
  await editDialog.getByLabel('Ends').fill(`${tomorrow}T10:45`);
  await editDialog.getByRole('button', { name: 'Save' }).click();
  await expect(ownerPage.getByText('Occurrence moved.')).toBeVisible({ timeout: 30_000 });

  const downloadPromise = ownerPage.waitForEvent('download');
  await ownerPage.getByRole('link', { name: 'Download .ics' }).click();
  const download = await downloadPromise;
  const exported = await readFile(await download.path(), 'utf8');
  expect(exported).toContain('Private appointment');
  expect(exported).toContain('Write itinerary');

  const imported = `BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:imported@example.test\r\nDTSTART:${tomorrow.replaceAll('-', '')}T150000Z\r\nDTEND:${tomorrow.replaceAll('-', '')}T153000Z\r\nSUMMARY:Imported appointment\r\nEND:VEVENT\r\nEND:VCALENDAR`;
  await ownerPage.getByText('Import .ics', { exact: true }).click();
  await ownerPage.getByLabel('Calendar file').setInputFiles({ name: 'import.ics', mimeType: 'text/calendar', buffer: Buffer.from(imported) });
  await expect(ownerPage.getByText('Imported appointment')).toBeVisible();
  await ownerPage.getByRole('button', { name: 'Apply import' }).click();
  await expect(ownerPage.getByText('1 calendar items imported.')).toBeVisible();

  const losAngeles = await browser.newContext({ timezoneId: 'America/Los_Angeles' });
  const secondDevice = await losAngeles.newPage();
  await signIn(secondDevice, owner.email);
  await secondDevice.goto(calendarPath);
  await expect(secondDevice.getByText('America/Los_Angeles', { exact: true })).toBeVisible({ timeout: 20_000 });
  await secondDevice.evaluate(() => window.blur());
  await ownerPage.bringToFront();
  await ownerPage.evaluate(() => window.dispatchEvent(new Event('focus')));
  await expect(ownerPage.getByText('Asia/Tokyo', { exact: true })).toBeVisible({ timeout: 20_000 });

  const outsiderContext = await browser.newContext();
  const outsiderPage = await outsiderContext.newPage();
  await register(outsiderPage, 'Calendar Outsider');
  expect((await outsiderPage.goto(calendarPath))?.status()).toBe(404);
  expect((await outsiderPage.goto(`${calendarPath}/export`))?.status()).toBe(404);

  const operatorContext = await browser.newContext();
  const operatorPage = await operatorContext.newPage();
  const operator = await register(operatorPage, 'Calendar Operator');
  const pool = new Pool({ connectionString: process.env.DATABASE_URL });
  try {
    await pool.query(`INSERT INTO relation(subject_kind,subject_id,verb,resource_kind,resource_id)
      SELECT 'person',p.id,'operator','platform',0 FROM person p JOIN auth."user" u ON u.id=p.user_id WHERE u.email=$1 ON CONFLICT DO NOTHING`, [operator.email]);
  } finally {
    await pool.end();
  }
  await operatorPage.goto(calendarPath);
  await expect(operatorPage.getByText('Viewing this account as an operator.')).toBeVisible();
  await expect(operatorPage.getByText('Device zone updates are disabled.')).toBeVisible();

  await ownerPage.bringToFront();
  await ownerPage.reload();
  await ownerPage.screenshot({ path: 'test-results/calendar-desktop.png', fullPage: true });
  await ownerPage.setViewportSize({ width: 390, height: 844 });
  await expect(ownerPage.getByRole('tab', { name: 'Day view' })).toHaveAttribute('aria-selected', 'true');
  await ownerPage.screenshot({ path: 'test-results/calendar-mobile.png', fullPage: true });

  await operatorContext.close();
  await outsiderContext.close();
  await losAngeles.close();
  await tokyo.close();
});
