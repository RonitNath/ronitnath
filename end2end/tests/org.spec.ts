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
    // in the change feed and the socket carried the diff. Scoped to the roster
    // — the drill's Invitations table names the claimant too, in its "Claimed
    // by" column, so an unscoped cell locator matches two cells the moment the
    // second diff arrives and which one lands first is a race.
    const roster = page.locator(".drill .tbl").first();
    await expect(roster.getByRole("cell", { name: "E2E Joiner" })).toBeVisible({ timeout: 15_000 });

    // Belonging to a group of an organization is not belonging to the
    // organization: the tier comes from a membership on the organization's own
    // party, so that is a second invitation, minted where the roster is.
    await page.goto(`${site}/org/members`);
    await page.locator("summary", { hasText: "Invite to organization" }).click();
    await page.getByRole("button", { name: "Mint link" }).click();
    const orgClaim = page.locator(".claim code");
    await expect(orgClaim).toContainText("/links/");
    await joiner.goto((await orgClaim.textContent())!.trim());
    await joiner.getByRole("button", { name: "Accept" }).click();

    // Promote them to admin on the organization itself.
    await page.reload();
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
    await page.locator("summary", { hasText: "Transfer ownership" }).click();
    await page.locator(".aside-body input[type='text']").fill(joinerPerson);
    await page.getByRole("button", { name: "Transfer ownership" }).click();

    // And loses the affordance that did it, because it is no longer theirs.
    await expect
      .poll(async () => await page.locator("summary", { hasText: "Transfer ownership" }).count(), {
        timeout: 15_000,
      })
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
  await page.locator("summary", { hasText: "New document" }).click();
  await page.locator(".aside-body input[type='text']").fill("E2E Charter");
  await page.getByRole("button", { name: "Create document" }).click();

  const row = page.getByRole("row", { name: /E2E Charter/ });
  await expect(row).toBeVisible({ timeout: 15_000 });
  await row.click();

  // The body syncs on blur; there is no save button anywhere on the page.
  await page.getByLabel("Body").fill("The first rule.");
  await page.getByLabel("Body").blur();
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

test("an invitation is withdrawn, a member is removed, and the organization speaks for itself", async ({
  page,
  browser,
}) => {
  test.slow();
  const run = unique();
  await register(page, "E2E Chair", `org-e2e-chair-${run}@example.invalid`);
  const org = (await command(page, "create-organization", {
    display_name: `Authority ${run}`,
  })).organization as string;

  // --- a link, and taking it back ----------------------------------------
  await page.goto(`${site}/org/members`);
  await page.locator("summary", { hasText: "Invite to organization" }).click();
  await page.getByRole("button", { name: "Mint link" }).click();
  await expect(page.locator(".claim code")).toContainText("/links/");
  const spare = (await page.locator(".claim code").textContent())!.trim();

  await page.goto(`${site}/org/invitations`);
  const link = page.getByRole("row", { name: /unclaimed/ });
  await expect(link).toBeVisible({ timeout: 15_000 });
  // The verb is in the row, on the rows it can act on.
  await link.getByRole("button", { name: "Withdraw" }).click();
  await expect(page.getByRole("row", { name: /unclaimed/ })).toHaveCount(0, { timeout: 15_000 });

  // And the token it minted opens nothing: the grant went with the row.
  const { context: stale, page: latecomer } = await second(browser);
  try {
    await register(latecomer, "E2E Latecomer", `org-e2e-late-${run}@example.invalid`);
    await latecomer.goto(spare);
    await expect(latecomer.getByRole("button", { name: "Accept" })).toHaveCount(0);
  } finally {
    await stale.close();
  }

  // --- somebody joins, and is removed by somebody else --------------------
  await page.goto(`${site}/org/members`);
  await page.locator("summary", { hasText: "Invite to organization" }).click();
  await page.getByRole("button", { name: "Mint link" }).click();
  const claim = (await page.locator(".claim code").textContent())!.trim();

  const { context, page: joiner } = await second(browser);
  try {
    await register(joiner, "E2E Guest", `org-e2e-guest-${run}@example.invalid`);
    await joiner.goto(claim);
    await joiner.getByRole("button", { name: "Accept" }).click();

    const guest = page.getByRole("row", { name: /E2E Guest/ });
    await expect(guest).toBeVisible({ timeout: 15_000 });
    // The owner may remove them; the owner's own row offers nothing, because
    // leaving is a different question and carries the last-owner rule.
    await expect(page.getByRole("row", { name: /E2E Chair/ })).not.toContainText("Remove");
    await guest.getByRole("button", { name: "Remove" }).click();
    await expect(page.getByRole("row", { name: /E2E Guest/ })).toHaveCount(0, { timeout: 15_000 });

    // Which is the whole of holding the tier: /org is not theirs any more.
    await expect
      .poll(async () => (await joiner.request.get(`${site}/org`)).status(), { timeout: 15_000 })
      .toBe(404);
  } finally {
    await context.close();
  }

  // --- acting as the organization ----------------------------------------
  //
  // Attribution, not authority. What moves is the party the audit row names,
  // and the organization's own tail is the place that shows it: a command that
  // names no container of this organization is in it because the principal was
  // speaking as the organization when they ran it.
  await page.goto(`${site}/org/audit`);
  await expect(page.getByRole("row", { name: /create-organization/ })).toBeVisible({
    timeout: 15_000,
  });
  expect(await page.getByRole("row", { name: /create-document/ }).count()).toBe(0);

  // A personal document, made while speaking as themselves. It names nothing
  // about the organization, so it is not the organization's business.
  await command(page, "create-document", { title: `Mine ${run}`, body: "" });
  await page.waitForTimeout(1000);
  expect(await page.getByRole("row", { name: /create-document/ }).count()).toBe(0);

  // Now speak as the organization. The rail says who that is.
  await page.getByLabel("Acting as").selectOption({ label: `Authority ${run}` });
  await expect(page.locator(".rail-identity .acting")).toHaveText(`Authority ${run}`, {
    timeout: 15_000,
  });

  // The same command, run by the same person, is now the organization's.
  await command(page, "create-document", { title: `Theirs ${run}`, body: "" });
  await expect(page.getByRole("row", { name: /create-document/ })).toBeVisible({
    timeout: 15_000,
  });

  // And back to themselves, which is the half that proves the switch was a
  // switch rather than a promotion.
  await page.getByLabel("Acting as").selectOption({ label: "E2E Chair" });
  await expect(page.locator(".rail-identity .acting")).toHaveText("", { timeout: 15_000 });
});
