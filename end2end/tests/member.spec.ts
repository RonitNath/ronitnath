import { expect, test, type BrowserContext, type Page } from "@playwright/test";

// The dev server: `cargo run -p rn-site` binds 127.0.0.1:3004 from the
// checked-in config. A worker running beside another leg's server overrides it.
const site = process.env.RN_SITE_URL || "http://127.0.0.1:3004";

const PASSWORD = "an obviously fake test password";

/** The operator `tools/seed.sh` registers. The merge cases need one: a
 *  candidate about two registrations is raised by a signal or by an operator,
 *  and no signal on this deployment can raise this pair. */
const OPERATOR_EMAIL = process.env.RN_SEED_EMAIL || "operator@example.invalid";
const OPERATOR_PASSWORD = process.env.RN_SEED_PASSWORD || "an obviously fake dev password";

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

async function signIn(page: Page, email: string, password: string = PASSWORD): Promise<void> {
  await page.goto(`${site}/auth`);
  const form = page.locator('form[action="/auth/sign-in"]');
  await form.getByLabel("Email").fill(email);
  await form.getByLabel("Password").fill(password);
  await form.getByRole("button", { name: "Sign in", exact: true }).click();
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

/**
 * Two registrations of one human, joined by the proof a browser can give.
 *
 * A signal proposes and proof disposes. The signal here is an operator's
 * `propose-match`, because no scan can raise this pair on this deployment —
 * `verified_email` cannot fire when `factor(value)` is unique deployment-wide,
 * and `oidc` is schema-only in this cut. What is being proven is the other
 * half: that the person themselves can confirm it, with the one proof a
 * browser can offer, which is the other registration's own credentials.
 *
 * Needs `tools/seed.sh` first, for the operator that raises the candidate.
 */
test("a person confirms their own two registrations with the other's credentials", async ({
  browser,
}) => {
  test.setTimeout(120_000);

  const operator_context = await browser.newContext();
  const first_context = await browser.newContext();
  const second_context = await browser.newContext();

  const operator = await open(operator_context);
  const first = await open(first_context);
  const second = await open(second_context);

  const first_email = address("twin-a");
  const second_email = address("twin-b");

  try {
    // Two self-registrations, sharing a name and nothing else.
    await register(first, "Twin", first_email);
    await register(second, "Twin", second_email);

    const ids = async (page: Page) =>
      await page.evaluate(async () => await (await fetch("/api/whoami")).json());
    const a = (await ids(first)).identity.public_id;
    const b = (await ids(second)).identity.public_id;
    expect(a).not.toEqual(b);

    // The operator raises the question. It decides nothing.
    await signIn(operator, OPERATOR_EMAIL, OPERATOR_PASSWORD);
    const proposed = await operator.evaluate(
      async ([a, b]) => {
        const response = await fetch("/api/cmd/propose-match", {
          method: "POST",
          credentials: "same-origin",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({
            key: crypto.randomUUID(),
            identity_a: a,
            identity_b: b,
            signal: "name_and_group",
          }),
        });
        return { status: response.status, body: await response.json() };
      },
      [a, b] as const,
    );
    expect(proposed.status).toBe(200);

    // It arrives on the first person's own page, live.
    await first.goto(`${site}/app/merge`);
    await expect(first.locator("section.wide")).toContainText("name and group", {
      timeout: 15_000,
    });
    // Before the merge, this person is made of one registration.
    await expect(first.locator("section", { hasText: "Made of" }).locator(".mono")).toHaveCount(1);

    // The proof: the *other* registration's address and password. Nothing is
    // minted — proving you could sign in is not signing in — so the page does
    // not have to put itself back together, and this tab keeps its cookie.
    await first.getByLabel("The other registration's address").fill(second_email);
    await first.getByLabel("Its password").fill(PASSWORD);
    await first.getByRole("button", { name: "Confirm" }).click();

    // One person, made of two registrations. No reload: a merge names two
    // persons and no identity, and this list re-reads itself on one anyway.
    await expect(first.locator("section", { hasText: "Made of" }).locator(".mono")).toHaveCount(2, {
      timeout: 20_000,
    });
    // And the same tab is still signed in as the same session it started with.
    expect((await ids(first)).identity.public_id).toEqual(a);
  } finally {
    await operator_context.close();
    await first_context.close();
    await second_context.close();
  }
});

/** A wrong password spends a verification and says the same thing a wrong
 *  address does: the page cannot be used to learn which half was wrong. */
test("a merge offered the wrong credentials is refused without saying which half", async ({
  browser,
}) => {
  test.setTimeout(120_000);

  const operator_context = await browser.newContext();
  const first_context = await browser.newContext();
  const second_context = await browser.newContext();
  const operator = await open(operator_context);
  const first = await open(first_context);
  const second = await open(second_context);

  try {
    const first_email = address("wrong-a");
    const second_email = address("wrong-b");
    await register(first, "Wrong", first_email);
    await register(second, "Wrong", second_email);

    const whoami = async (page: Page) =>
      await page.evaluate(async () => await (await fetch("/api/whoami")).json());
    const a = (await whoami(first)).identity.public_id;
    const b = (await whoami(second)).identity.public_id;

    await signIn(operator, OPERATOR_EMAIL, OPERATOR_PASSWORD);
    await operator.evaluate(
      async ([a, b]) => {
        await fetch("/api/cmd/propose-match", {
          method: "POST",
          credentials: "same-origin",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({
            key: crypto.randomUUID(),
            identity_a: a,
            identity_b: b,
            signal: "name_and_group",
          }),
        });
      },
      [a, b] as const,
    );

    await first.goto(`${site}/app/merge`);
    await expect(first.locator("section.wide")).toContainText("name and group", {
      timeout: 15_000,
    });

    await first.getByLabel("The other registration's address").fill(second_email);
    await first.getByLabel("Its password").fill("not the password");
    await first.getByRole("button", { name: "Confirm" }).click();

    // Refused, and still one registration.
    await expect(first.locator("section", { hasText: "Made of" }).locator(".mono")).toHaveCount(1);
    // An address nobody holds spends the same verification and answers the
    // same: neither attempt tells the caller which half it got wrong.
    await first.getByLabel("The other registration's address").fill(address("nobody"));
    await first.getByLabel("Its password").fill(PASSWORD);
    await first.getByRole("button", { name: "Confirm" }).click();
    await expect(first.locator("section", { hasText: "Made of" }).locator(".mono")).toHaveCount(1);
  } finally {
    await operator_context.close();
    await first_context.close();
    await second_context.close();
  }
});

/**
 * A refusal about a field is drawn beside that field.
 *
 * The server answers a validation failure with the field it is about, which
 * is only worth carrying if a form places it: "frontend messages should be in
 * the appropriate location". The factor form is the sharp case — one control
 * standing for either an address or a secret — so it is the one walked here.
 */
test("a refusal names a field, and the note is under that field", async ({ browser }) => {
  const context = await browser.newContext();
  const page = await open(context);
  try {
    await register(page, "Placed", address("placed"));
    await page.goto(`${site}/app/identities`);

    const value = page.getByLabel("Address or password");
    await value.fill("not an address");
    await page.getByRole("button", { name: "Add factor" }).click();

    // Beside the control that produced it, not at the bottom of the page.
    const beside = page.locator("#factor-value").locator("xpath=../span[@class='note']");
    await expect(beside).toHaveText(/address/i, { timeout: 10_000 });
    await expect(beside).toHaveAttribute("data-state", "invalid");

    // The same control, the other kind, and the server names the other field.
    await page.getByLabel("Kind").selectOption("password");
    await value.fill("short");
    await page.getByRole("button", { name: "Add factor" }).click();
    await expect(beside).toHaveText(/character/i, { timeout: 10_000 });
  } finally {
    await context.close();
  }
});
