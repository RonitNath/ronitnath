import { expect, test, type Browser, type Page } from "@playwright/test";

// The dev server: `cargo run -p rn-site` binds 127.0.0.1:3004 from the
// checked-in config, which is also this suite's baseURL.
const site = process.env.RN_SITE_URL || "http://127.0.0.1:3004";

/** A password long enough for the floor and obviously not a real one. */
const password = "an obviously fake test password";

/** Every registration in a run needs its own address: email is unique
 *  deployment-wide, and the dev database is not reset between runs. */
const unique = () => `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;

/** Register somebody in this page's own browser context, and land signed in. */
async function register(page: Page, display: string, email: string) {
  await page.goto(`${site}/auth`);
  await page.locator("form[action='/auth/register'] input[name='display_name']").fill(display);
  await page.locator("form[action='/auth/register'] input[name='email']").fill(email);
  await page.locator("form[action='/auth/register'] input[name='password']").fill(password);
  await page.locator("form[action='/auth/register'] button[type='submit']").click();
  await page.waitForURL(/\/app/);
}

/**
 * Run a command the way the bundle does, from inside the signed-in page.
 *
 * The organization tier is reached by creating an organization, and there is
 * no page in any bundle that does that — `/app` owns it (U2). Posting it here
 * is the same dispatch the button will use: one route, one idempotency key,
 * one reply.
 */
async function command(page: Page, name: string, args: Record<string, unknown>) {
  return page.evaluate(
    async ([name, args]) => {
      const response = await fetch(`/api/cmd/${name}`, {
        method: "POST",
        credentials: "same-origin",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ key: crypto.randomUUID(), ...(args as object) }),
      });
      if (!response.ok) throw new Error(`${name}: ${response.status}`);
      return (await response.json()).result;
    },
    [name, args] as const,
  );
}

/** A second browser context — a different person, on a different machine. */
async function second(browser: Browser) {
  const context = await browser.newContext();
  return { context, page: await context.newPage() };
}

test("an organization is founded, grown, and handed on", async ({ page, browser }) => {
  test.slow();
  const run = unique();
  const founderEmail = `org-e2e-founder-${run}@example.invalid`;
  const joinerEmail = `org-e2e-joiner-${run}@example.invalid`;

  await register(page, "E2E Founder", founderEmail);

  // The tier does not exist yet: a member who operates nothing is told
  // nothing about /org beyond that it is not theirs.
  expect((await page.request.get(`${site}/org`)).status()).toBe(404);

  const created = await command(page, "create-organization", {
    display_name: `E2E Org ${run}`,
  });
  const org = created.organization as string;

  // /org now serves, and the overview is the organization itself.
  await page.goto(`${site}/org`);
  await expect(page.locator("h1")).toHaveText(`E2E Org ${run}`);
  await expect(page.locator(".sections")).toContainText("Members 1");
  await expect(page.locator(".facts")).toContainText("owner");

  // A group, and a link into it.
  const group = (await command(page, "create-group", {
    display_name: "E2E Founders",
    organization: org,
  })).group as string;
  await page.goto(`${site}/org/groups`);
  await page.getByRole("row", { name: /E2E Founders/ }).click();
  await expect(page.locator(".drill h2")).toHaveText("E2E Founders");

  await page.getByRole("button", { name: "Mint link" }).click();
  const claim = page.locator(".claim code");
  await expect(claim).toContainText("/links/");
  const claimUrl = (await claim.textContent())!.trim();

  // A second person, in their own context, walks through the link.
  const { context, page: joiner } = await second(browser);
  try {
    await register(joiner, "E2E Joiner", joinerEmail);
    await joiner.goto(claimUrl);
    await joiner.getByRole("button", { name: "Accept" }).click();

    // The founder's roster shows them without a reload: the membership landed
    // in the change feed and the socket carried the diff.
    await expect(page.getByRole("cell", { name: "E2E Joiner" })).toBeVisible({ timeout: 15_000 });

    // Promote them to admin on the organization itself.
    await page.goto(`${site}/org/members`);
    await page.getByRole("row", { name: /E2E Joiner/ }).click();
    await page.locator(".panel select").selectOption("admin");
    await expect
      .poll(async () => (await page.getByRole("row", { name: /E2E Joiner/ }).textContent()) ?? "", {
        timeout: 15_000,
      })
      .toContain("admin");

    // Which is the whole of holding the tier: the second context can now
    // reach /org, and could not before.
    await joiner.goto(`${site}/org`);
    await expect(joiner.locator("h1")).toHaveText(`E2E Org ${run}`);

    // The founder hands the organization on.
    const joinerPerson = await joiner.evaluate(async () => {
      const whoami = await (await fetch("/api/whoami", { credentials: "same-origin" })).json();
      return whoami.person.public_id as string;
    });
    await page.goto(`${site}/org`);
    await page.getByText("Transfer ownership").click();
    await page.locator(".aside-body input[type='text']").fill(joinerPerson);
    await page.getByRole("button", { name: "Transfer ownership" }).click();

    // And loses the affordance that did it, because it is no longer theirs.
    await expect
      .poll(async () => await page.getByText("Transfer ownership").count(), { timeout: 15_000 })
      .toBe(0);
    await expect(page.locator(".facts")).toContainText("admin");
    await expect(page.locator(".facts")).toContainText("E2E Joiner");
  } finally {
    await context.close();
  }
});

test("an organization's pages decline to somebody who operates another one", async ({
  page,
  browser,
}) => {
  const run = unique();
  await register(page, "E2E Alpha", `org-e2e-alpha-${run}@example.invalid`);
  const alpha = (await command(page, "create-organization", {
    display_name: `Alpha ${run}`,
  })).organization as string;

  const { context, page: other } = await second(browser);
  try {
    await register(other, "E2E Beta", `org-e2e-beta-${run}@example.invalid`);
    await command(other, "create-organization", { display_name: `Beta ${run}` });

    for (const query of ["org", "org-members", "org-groups", "org-documents", "org-audit"]) {
      const response = await other.request.get(`${site}/api/q/${query}?org=${alpha}`);
      expect(response.status(), `${query} answered another operator`).toBe(403);
    }
  } finally {
    await context.close();
  }
});

test("a document owned by the organization is written, shared and published", async ({ page }) => {
  test.slow();
  const run = unique();
  await register(page, "E2E Author", `org-e2e-author-${run}@example.invalid`);
  const org = (await command(page, "create-organization", {
    display_name: `Docs ${run}`,
  })).organization as string;

  await page.goto(`${site}/org/documents`);
  await page.getByText("New document").click();
  await page.locator(".aside-body input[type='text']").fill("E2E Charter");
  await page.getByRole("button", { name: "Create document" }).click();

  const row = page.getByRole("row", { name: /E2E Charter/ });
  await expect(row).toBeVisible({ timeout: 15_000 });
  await row.click();

  // The body syncs on blur; there is no save button anywhere on the page.
  await page.locator("#doc-body").fill("The first rule.");
  await page.locator("#doc-body").blur();
  await expect
    .poll(async () => (await page.getByRole("row", { name: /E2E Charter/ }).textContent()) ?? "", {
      timeout: 15_000,
    })
    .toContain("2");
  expect(await page.getByRole("button", { name: /^Save/ }).count()).toBe(0);

  // Publishing is the one commit, and it is tagged to the revision it moves.
  await page.getByRole("button", { name: "Publish document" }).click();
  await expect
    .poll(async () => (await page.getByRole("row", { name: /E2E Charter/ }).textContent()) ?? "", {
      timeout: 15_000,
    })
    .toContain("published");

  // Sharing it with the organization's own party is a grant like any other.
  await page.locator(".minting input[type='text']").fill(org);
  await page.getByRole("button", { name: "Share" }).click();
  await expect(page.locator(".grants")).toContainText("viewer", { timeout: 15_000 });
});
