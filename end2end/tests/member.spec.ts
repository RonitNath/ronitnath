import { expect, test, type BrowserContext, type Page } from "@playwright/test";

// The dev server: `cargo run -p rn-site` binds 127.0.0.1:3004 from the
// checked-in config. A worker running beside another leg's server overrides it.
const site = process.env.RN_SITE_URL || "http://127.0.0.1:3004";

const PASSWORD = "an obviously fake test password";

/** A fresh address per run, so a re-run is not a duplicate registration. */
function address(who: string): string {
  return `u2-${who}-${Date.now()}-${Math.floor(Math.random() * 1e6)}@example.invalid`;
}

/** Register somebody through the real auth page and land in their shell. */
async function register(page: Page, name: string, email: string): Promise<void> {
  await page.goto(`${site}/auth`);
  const form = page.locator('form[action="/auth/register"]');
  await form.getByLabel("Name").fill(name);
  await form.getByLabel("Email").fill(email);
  await form.getByLabel("Password").fill(PASSWORD);
  await form.getByRole("button", { name: "Register" }).click();
  await page.waitForURL(`${site}/app`);
}

async function signIn(page: Page, email: string): Promise<void> {
  await page.goto(`${site}/auth`);
  const form = page.locator('form[action="/auth/sign-in"]');
  await form.getByLabel("Email").fill(email);
  await form.getByLabel("Password").fill(PASSWORD);
  await form.getByRole("button", { name: "Sign in" }).click();
  await page.waitForURL(`${site}/app`);
}

async function open(context: BrowserContext): Promise<Page> {
  const page = await context.newPage();
  return page;
}

test("a group, an invitation, a shared document and a revoked session", async ({ browser }) => {
  // Five browser contexts' worth of real registrations, commands and live
  // diffs; the default is a single page's budget.
  test.setTimeout(180_000);

  const founder = await browser.newContext();
  const joiner = await browser.newContext();
  const second_device = await browser.newContext();

  const owner = await open(founder);
  const guest = await open(joiner);

  const owner_email = address("owner");
  const guest_email = address("guest");

  await register(owner, "Owner", owner_email);
  await register(guest, "Guest", guest_email);

  // --- a group, and an invitation into it ---------------------------------
  await owner.goto(`${site}/app/groups`);
  await owner.getByLabel("Name").fill("Founders");
  await owner.getByRole("button", { name: "Create group" }).click();
  await expect(owner.getByRole("button", { name: "Founders" })).toBeVisible();
  // The roster is the group's own, and it starts with the person who made it.
  await expect(owner.getByRole("cell", { name: /Owner \(you\)/ })).toBeVisible();

  await owner.getByRole("button", { name: "Mint a link" }).click();
  const claim = owner.locator(".carry code");
  await expect(claim).toContainText("/links/");
  const url = (await claim.textContent())!.trim();

  // --- the second context claims it, and the first sees it happen ---------
  await guest.goto(url);
  await guest.getByRole("button", { name: "Accept" }).click();
  await guest.waitForURL(new RegExp(`^${site}/app`));

  // No reload on the owner's side: the roster is a live subscription, and a
  // membership somebody else wrote is a diff the socket pushes.
  await expect(owner.getByRole("cell", { name: "Guest" })).toBeVisible({ timeout: 15_000 });

  // --- a document, shared with that group --------------------------------
  await owner.goto(`${site}/app/documents`);
  await owner.getByLabel("Title").fill("Kernel report");
  await owner.getByRole("button", { name: "Create document" }).click();
  await owner.waitForURL(new RegExp(`${site}/app/documents/r_`));
  const document_url = owner.url();

  await owner.getByLabel("One of your groups").selectOption({ label: "Founders" });
  await owner.getByLabel("May", { exact: true }).selectOption("editor");
  await owner.getByRole("button", { name: "Share", exact: true }).click();
  await expect(owner.getByRole("cell", { name: "Founders" })).toBeVisible();

  // --- the second context sees it arrive, live ---------------------------
  await guest.goto(`${site}/app/documents`);
  await expect(guest.getByRole("cell", { name: "Kernel report" })).toBeVisible({
    timeout: 15_000,
  });

  // --- it edits, and the first context sees the words change -------------
  await guest.goto(document_url);
  const body = guest.getByLabel("Body");
  await expect(body).toBeVisible();
  await body.fill("Five rules, written by the reader it was shared with.");
  // The field syncs on blur; there is no save button to press.
  await body.blur();

  await expect(owner.getByLabel("Body")).toHaveValue(
    "Five rules, written by the reader it was shared with.",
    { timeout: 20_000 },
  );

  // --- a second device of the owner's, revoked from the first ------------
  const other = await open(second_device);
  await signIn(other, owner_email);
  await other.goto(`${site}/app/documents`);

  await owner.goto(`${site}/app/sessions`);
  const rows = owner.locator("tbody tr");
  await expect(rows).toHaveCount(2, { timeout: 15_000 });
  // "Revoke" is the other device; the one reading this page says "Sign out".
  await owner.getByRole("button", { name: "Revoke" }).click();
  await expect(rows).toHaveCount(1, { timeout: 15_000 });

  // The revoked tab is a tab holding a session that no longer exists. It finds
  // out on its next navigation, which is a redirect to the door.
  await other.reload();
  await other.waitForURL(new RegExp(`${site}/auth`));
  expect(other.url()).toContain("/auth");

  await founder.close();
  await joiner.close();
  await second_device.close();
});
