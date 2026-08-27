import { expect, test, type BrowserContext, type Page } from "@playwright/test";

// The dev server. `tools/seed.sh` has already registered the operator and run
// `rn-site bootstrap-operator`, so the relation this whole surface is guarded
// by exists before the first page loads.
//
//   tools/seed.sh && rn-site &
//   npx playwright test platform.spec.ts --workers=1
//
// One worker, deliberately. There is exactly one platform operator on a
// deployment — that is the product contract, not a limitation of the fixture —
// so three browser projects running this journey at once are three windows of
// the same person disabling and re-enabling each other's subjects. The suite
// is serial within a project and has to be serialised across them too.
const site = process.env.RN_SITE_URL || "http://127.0.0.1:3004";

const operatorEmail = process.env.RN_SEED_EMAIL || "operator@example.invalid";
const password = process.env.RN_SEED_PASSWORD || "an obviously fake dev password";

/**
 * The two registrations this journey merges share a display name and nothing
 * else. That is deliberate: `verified_email` cannot fire on this deployment —
 * migration 1 puts a UNIQUE index on `factor(value) WHERE kind = 'email'`, so
 * two identities cannot hold the same address — and `oidc` is schema-only in
 * this cut. So the candidate is raised by `propose-match`, run by the operator,
 * which is the command that exists precisely for the pair a scan will not
 * propose. The scan itself is proven in `cargo test -p rn-kernel`.
 */
const NAME = "Twice Over";

async function register(page: Page, name: string, email: string) {
  await page.goto(`${site}/auth`);
  await page.getByRole("textbox", { name: "Name" }).fill(name);
  await page.locator("form").filter({ hasText: "Register" }).getByLabel("Email").fill(email);
  await page.locator("form").filter({ hasText: "Register" }).getByLabel("Password").fill(password);
  await page.getByRole("button", { name: "Register" }).click();
  await page.waitForURL(`${site}/app`);
}

async function signIn(page: Page, email: string) {
  await page.goto(`${site}/auth`);
  await page.locator("form").filter({ hasText: "Sign in" }).getByLabel("Email").fill(email);
  await page.locator("form").filter({ hasText: "Sign in" }).getByLabel("Password").fill(password);
  await page.getByRole("button", { name: "Sign in", exact: true }).click();
  await page.waitForURL(`${site}/app`);
}

/** The public ids the chrome is allowed to know. */
async function whoami(page: Page) {
  return await page.evaluate(async () => await (await fetch("/api/whoami")).json());
}

/** Post a command the way the bundle posts one, from a context's own cookie. */
async function command(page: Page, name: string, args: Record<string, unknown>) {
  return await page.evaluate(
    async ([name, args]) => {
      const response = await fetch(`/api/cmd/${name}`, {
        method: "POST",
        headers: { "content-type": "application/json", "sec-fetch-site": "same-origin" },
        body: JSON.stringify({ key: crypto.randomUUID(), ...(args as object) }),
      });
      return { status: response.status, body: await response.json() };
    },
    [name, args] as const,
  );
}

/** A unique address per run: the deployment keeps its rows. */
function unique(prefix: string) {
  return `${prefix}-${Date.now()}-${Math.floor(Math.random() * 1e6)}@example.invalid`;
}

test.describe.configure({ mode: "serial" });

test("an operator rules two registrations into one person, and ends a session", async ({
  browser,
}) => {
  test.setTimeout(120_000);

  const contexts: BrowserContext[] = [];
  const open = async () => {
    const context = await browser.newContext();
    contexts.push(context);
    return await context.newPage();
  };

  try {
    // --- the operator, seeded by tools/seed.sh -----------------------------
    const operator = await open();
    await signIn(operator, operatorEmail);
    await operator.goto(`${site}/platform`);
    await expect(operator.getByRole("heading", { name: "Parties" })).toBeVisible();

    // --- two registrations that share a name and nothing else -------------
    const first = await open();
    const firstEmail = unique("twice-a");
    await register(first, NAME, firstEmail);
    const second = await open();
    await register(second, NAME, unique("twice-b"));

    const a = (await whoami(first)).identity.public_id;
    const b = (await whoami(second)).identity.public_id;
    const person = (await whoami(first)).person.public_id;
    expect(a).not.toEqual(b);

    // A member holds no operator relation, so the tier is not there at all.
    await first.goto(`${site}/platform`);
    await expect(first.locator("body")).not.toContainText("Parties");
    await first.goto(`${site}/app`);

    // --- the candidate arrives live ---------------------------------------
    await operator.goto(`${site}/platform/matches`);
    await expect(operator.getByRole("heading", { name: "Matches" })).toBeVisible();
    // Nothing is reloaded after this point: the row has to arrive over the
    // subscription the open page already holds.
    const proposed = await command(operator, "propose-match", {
      identity_a: a,
      identity_b: b,
      signal: "name_and_group",
    });
    expect(proposed.status).toBe(200);
    const candidate = proposed.body.result.candidate as string;

    const row = operator.getByRole("row", { name: new RegExp(candidate) });
    await expect(row).toBeVisible({ timeout: 15_000 });
    await expect(row).toContainText("proposed");

    // --- ruled, with evidence ---------------------------------------------
    await row.click();
    const panel = operator.locator(".panel");
    await expect(panel).toBeVisible();

    // Empty evidence is refused before it is sent, and the reason is text.
    await expect(panel).toContainText("Evidence is required");
    await expect(panel.getByRole("button", { name: "Same person" })).toBeDisabled();

    const evidence = `passport and a support call, ${new Date().toISOString()}`;
    await panel.getByLabel("Evidence").fill(evidence);
    await panel.getByRole("button", { name: "Same person" }).click();

    // --- the merge is legible: link rows, the alias, and one person --------
    await expect
      .poll(async () => await panel.locator(".evidence").count(), { timeout: 15_000 })
      .toBeGreaterThan(0);
    await expect(panel).toContainText(evidence);
    // One link row, not two: `Register` writes none — a registration that has
    // never been merged has nothing to record — so the row that exists is the
    // absorbed identity moving onto the survivor, under the operator's method.
    await expect(panel.locator(".line.link")).toHaveCount(1);
    await expect(panel.locator(".line.link")).toContainText("operator");
    await expect(panel).toContainText("one person");
    // person_alias keeps the absorbed id resolving.
    await expect(panel.locator("h3", { hasText: "Alias" })).toContainText("1");

    await operator.goto(`${site}/platform/identities`);
    // A deployment this suite has run against before has more registrations
    // than one page holds, so each row is found the way an operator finds one.
    await operator.getByLabel("Filter rows").fill(a);
    const rowA = operator.getByRole("row", { name: new RegExp(a) });
    await expect(rowA).toBeVisible();
    await expect(rowA).toContainText(NAME);
    await operator.getByLabel("Filter rows").fill(b);
    const rowB = operator.getByRole("row", { name: new RegExp(b) });
    await expect(rowB).toBeVisible();
    await expect(rowB).toContainText(NAME);

    // --- the ruling is in the log, with what it relied on ------------------
    await operator.goto(`${site}/platform/audit`);
    const ruling = operator.getByRole("row", { name: /rule-match/ }).first();
    await expect(ruling).toBeVisible();
    await ruling.click();
    const entry = operator.locator(".panel");
    await expect(entry.locator(".payload")).toContainText(evidence);
    await expect(entry.locator(".payload")).toContainText("operator");

    // --- a session revoked from /platform/sessions ends its context --------
    //
    // The operator's own second window, because `RevokeSession` ends a session
    // of the *acting* identity: the kernel's guard is the `AND identity_id = ?`
    // in its delete, and an operator revoking a stranger's session is declined
    // by it. Ending somebody else's sessions is `Disable`, below.
    const otherWindow = await open();
    await signIn(otherWindow, operatorEmail);
    await expect(otherWindow).toHaveURL(`${site}/app`);
    // The window names its own session rather than the operator's console
    // guessing which row it is: `sessions` marks the device reading it.
    const otherSession = await otherWindow.evaluate(async () => {
      const rows = await (await fetch("/api/q/sessions")).json();
      return rows.find((row: { current: boolean }) => row.current).public_id as string;
    });

    await operator.goto(`${site}/platform/sessions`);
    await operator.getByLabel("Filter rows").fill(otherSession);
    const sessionRow = operator.getByRole("row", { name: new RegExp(otherSession) });
    await expect(sessionRow).toBeVisible();
    await sessionRow.click();
    const sessionPanel = operator.locator(".panel");
    await expect(sessionPanel).toBeVisible();
    await sessionPanel.getByRole("button", { name: "Revoke" }).click();
    // The row leaves the live list, which is the only thing a deletion can
    // look like: a session row *is* the session.
    await expect(sessionRow).toHaveCount(0, { timeout: 15_000 });

    await otherWindow.goto(`${site}/app`);
    await expect(otherWindow).toHaveURL(/\/auth/);

    // --- and a stranger's session ends by disabling the party --------------
    await operator.goto(`${site}/platform`);
    await operator.getByLabel("Filter rows").fill(person);
    const partyRow = operator.getByRole("row", { name: new RegExp(person) });
    await expect(partyRow).toBeVisible();
    await partyRow.click();
    const partyPanel = operator.locator(".panel");
    await expect(partyPanel).toContainText("A reason is required");
    await partyPanel.getByLabel("Reason").fill("merged in error, checked by hand");
    await partyPanel.getByRole("button", { name: "Disable" }).click();
    await expect
      .poll(async () => await partyPanel.locator(".state").first().innerText(), {
        timeout: 15_000,
      })
      .toBe("disabled");

    await first.goto(`${site}/app`);
    await expect(first).toHaveURL(/\/auth/);

    // Put it back, so a second run of this suite starts where the first did.
    await operator.goto(`${site}/platform`);
    await operator.getByLabel("Filter rows").fill(person);
    const again = operator.getByRole("row", { name: new RegExp(person) });
    await again.click();
    await operator.locator(".panel").getByRole("button", { name: "Enable" }).click();
    await expect
      .poll(async () => await operator.locator(".panel .state").first().innerText(), {
        timeout: 15_000,
      })
      .toBe("active");
  } finally {
    for (const context of contexts) {
      await context.close();
    }
  }
});

test("the cluster page reports what this node witnessed", async ({ browser }) => {
  const context = await browser.newContext();
  try {
    const page = await context.newPage();
    await signIn(page, operatorEmail);
    await page.goto(`${site}/platform/cluster`);
    await expect(page.getByRole("heading", { name: "Cluster" })).toBeVisible();

    // Both raft groups, separately: a readiness answer that reported only one
    // of them is the blind spot the two rows exist to close.
    await expect(page.getByRole("row", { name: /^sqlite/ })).toContainText("formed");
    await expect(page.getByRole("row", { name: /^cache/ })).toContainText("formed");
    // The feed head is where a fresh subscriber is seeded, so by now it is
    // past zero: this deployment has served commands. It is a reading rather
    // than a row — a fact list is not a table — so it is read from the fact
    // it sits in.
    const head = await page
      .locator(".fact", { hasText: "Feed head" })
      .locator(".fact-value")
      .innerText();
    expect(Number(head.replace(/\D/g, ""))).toBeGreaterThan(0);
  } finally {
    await context.close();
  }
});

test("an operator ends a stranger's session and takes an organization out of service", async ({
  browser,
}) => {
  test.setTimeout(120_000);
  const contexts: BrowserContext[] = [];
  const open = async () => {
    const context = await browser.newContext();
    contexts.push(context);
    return await context.newPage();
  };

  try {
    const operator = await open();
    await signIn(operator, operatorEmail);

    // --- a stranger, and their session -------------------------------------
    //
    // Not the operator's own second window: `RevokeSession` reads the operator
    // relation, so ending somebody else's session is the reach a deployment
    // needs and not a hole in the rule that a session is its identity's.
    const stranger = await open();
    const strangerEmail = unique("stranger");
    await register(stranger, "Stranger", strangerEmail);
    const strangerSession = await stranger.evaluate(async () => {
      const rows = await (await fetch("/api/q/sessions")).json();
      return rows.find((row: { current: boolean }) => row.current).public_id as string;
    });

    await operator.goto(`${site}/platform/sessions`);
    // Every run of this suite leaves sessions behind, so the list is paged.
    // Finding the row is what the filter box is for.
    await operator.getByLabel("Filter rows").fill(strangerSession);
    const row = operator.getByRole("row", { name: new RegExp(strangerSession) });
    await expect(row).toBeVisible({ timeout: 15_000 });
    await row.click();
    await operator.locator(".panel").getByRole("button", { name: "Revoke" }).click();
    // A session row *is* the session, so a deletion looks like a row leaving.
    await expect(row).toHaveCount(0, { timeout: 15_000 });

    // The stranger finds out on their next navigation.
    await stranger.goto(`${site}/app`);
    await expect(stranger).toHaveURL(/\/auth/);

    // --- an organization, disabled -----------------------------------------
    //
    // `Disable` takes any party. An organization is not a person and the
    // console offers it anyway, because the kernel decides authority by what
    // the party is rather than by refusing everything that is not human.
    const founder = await open();
    await register(founder, "Founder", unique("founder"));
    const org = await founder.evaluate(async () => {
      const response = await fetch("/api/cmd/create-organization", {
        method: "POST",
        headers: { "content-type": "application/json", "sec-fetch-site": "same-origin" },
        body: JSON.stringify({ key: crypto.randomUUID(), display_name: "Wound Up" }),
      });
      return (await response.json()).result.organization as string;
    });

    await operator.goto(`${site}/platform`);
    await operator.getByLabel("Filter rows").fill(org);
    const partyRow = operator.getByRole("row", { name: new RegExp(org) });
    await expect(partyRow).toBeVisible({ timeout: 15_000 });
    await partyRow.click();
    const panel = operator.locator(".panel");
    await expect(panel).toContainText("A reason is required");
    await panel.getByLabel("Reason").fill("wound up at the founder's request");
    await panel.getByRole("button", { name: "Disable" }).click();
    await expect
      .poll(async () => await panel.locator(".state").first().innerText(), { timeout: 15_000 })
      .toBe("disabled");

    // Put it back, so a second run of this suite starts where the first did.
    await panel.getByRole("button", { name: "Enable" }).click();
    await expect
      .poll(async () => await panel.locator(".state").first().innerText(), { timeout: 15_000 })
      .toBe("active");
  } finally {
    for (const context of contexts) {
      await context.close();
    }
  }
});
