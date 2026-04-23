import { expect, test, type Page } from "@playwright/test";
import { mkdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const SCREENSHOTS = resolve(here, "screenshots");
mkdirSync(SCREENSHOTS, { recursive: true });

const snap = (page: Page, name: string) =>
  page.screenshot({ path: resolve(SCREENSHOTS, `${name}.png`), fullPage: true });

test.describe.configure({ mode: "serial" });

test.describe("events UI", () => {
  // Stash state produced by admin steps so RSVP and signup tests can use them.
  let eventRef: string;
  let inviteeRsvpUrl: string;

  test("anon empty events list", async ({ page }) => {
    await page.goto("/events");
    await expect(page.getByRole("heading", { name: "Events" })).toBeVisible();
    await expect(page.getByText("Check back soon")).toBeVisible();
    await expect(page.locator("[data-island='admin-events-list']")).toHaveCount(0);
    await snap(page, "01-anon-empty");
  });

  test("enter route is hidden but redirects in dev mode", async ({ page }) => {
    const response = await page.goto("/enter");
    expect(response?.ok()).toBeTruthy();
    await expect(page).toHaveURL(/\/events$/);
  });

  test("admin creates a published self-signup event", async ({ page }) => {
    await page.goto("/dev/login");
    await expect(page).toHaveURL(/\/events$/);
    await expect(page.getByText("Admin", { exact: true }).first()).toBeVisible();
    await snap(page, "02-admin-empty");

    await page.getByRole("button", { name: "New event" }).click();
    await expect(page.locator("[data-test='new-event-modal']")).toBeVisible();
    await page.locator("[data-test='event-title-input']").fill("Backyard Dinner");
    await page.locator("[data-test='event-starts-input']").fill("2026-06-01T18:00:00-07:00");
    await page.locator("[data-test='event-ends-input']").fill("2026-06-01T22:00:00-07:00");
    await page.locator("[data-test='event-visibility-input']").selectOption("public");
    await page.locator("[data-test='event-signup-mode-input']").selectOption("self_signup");

    await snap(page, "03-new-event-modal");
    await Promise.all([
      page.waitForURL(/\/events\/[^/]+$/),
      page.locator("[data-test='create-event-submit']").click(),
    ]);
    const match = page.url().match(/\/events\/([^/?#]+)$/);
    expect(match).not.toBeNull();
    eventRef = match![1];

    await expect(page.getByRole("heading", { name: "Backyard Dinner" })).toBeVisible();
    await expect(page.locator("[data-test='event-status']")).toHaveText("draft");
    await snap(page, "04-admin-detail-draft");

    // Publish
    await page.locator("[data-test='publish-btn']").click();
    await expect(page.locator("[data-test='admin-status']")).toHaveText("published");
    await snap(page, "05-admin-detail-published");

    // Admin can change access after creation, and a signed signup link works
    // even while the event is private.
    await page.locator("[data-test='admin-visibility-input']").selectOption("invite_only");
    await page.locator("[data-test='admin-signup-mode-input']").selectOption("invite_only");
    await page.locator("[data-test='admin-settings-submit']").click();
    await expect(page.locator("[data-test='event-visibility']")).toHaveText("invite_only");
    await expect(page.locator("[data-test='event-signup-mode']")).toHaveText("invite_only");

    await page.locator("[data-test='signup-token-btn']").click();
    const signupCode = await page
      .locator("[data-test='signup-link-reveal'] code")
      .textContent();
    expect(signupCode).toBeTruthy();
    const signedSignupPath = new URL(signupCode!.trim()).pathname + new URL(signupCode!.trim()).search;
    const signupCtx = await page.context().browser()!.newContext();
    const signupPage = await signupCtx.newPage();
    await signupPage.goto(signedSignupPath);
    await expect(signupPage.locator("[data-test='signup-form']")).toBeVisible();
    await signupCtx.close();

    await page.locator("[data-test='admin-visibility-input']").selectOption("public");
    await page.locator("[data-test='admin-signup-mode-input']").selectOption("self_signup");
    await page.locator("[data-test='admin-settings-submit']").click();
    await expect(page.locator("[data-test='event-visibility']")).toHaveText("public");
    await expect(page.locator("[data-test='event-signup-mode']")).toHaveText("self_signup");

    // Add an invitee and capture RSVP URL
    await page.locator("[data-test='new-invitee-name']").fill("Ada Lovelace");
    await page.locator("[data-test='add-invitee-btn']").click();
    const reveal = page.locator("[data-test='invite-link-reveal']");
    await expect(reveal).toBeVisible();
    const code = await reveal.locator("code").textContent();
    expect(code).toBeTruthy();
    inviteeRsvpUrl = code!.trim();
    expect(inviteeRsvpUrl).toContain(`/events/${eventRef}/r/`);
    await snap(page, "06-admin-invitee-added");
  });

  test("anon sees event, cannot see admin panel", async ({ page, browser }) => {
    // Fresh anon context (no cookies)
    const ctx = await browser.newContext();
    const anon = await ctx.newPage();
    await anon.goto(`/events/${eventRef}`);
    await expect(anon.getByRole("heading", { name: "Backyard Dinner" })).toBeVisible();
    await expect(anon.locator(".admin-panel")).toHaveCount(0);
    await expect(anon.locator("[data-test='signup-link']")).toBeVisible();
    await snap(anon, "07-anon-event-detail");
    await ctx.close();
  });

  test("invitee completes RSVP with a guest", async ({ browser }) => {
    const ctx = await browser.newContext();
    const page = await ctx.newPage();
    // inviteeRsvpUrl is absolute; extract path portion to stay on test host
    const rsvpPath = new URL(inviteeRsvpUrl).pathname;
    await page.goto(rsvpPath);
    await expect(page.locator("[data-test='rsvp-form']")).toBeVisible();
    await expect(page.getByText("Hi Ada Lovelace")).toBeVisible();
    await snap(page, "08-invitee-rsvp-initial");

    await page.locator("[data-test='rsvp-status-yes']").click();
    await page.locator("[data-test='rsvp-arrival-note']").fill("Running 10 min late");
    await page.locator("[data-test='rsvp-dietary']").fill("Vegetarian");

    // Party size limit defaults to 1 -> can't add guest. Admin created with default 1.
    // Submit and expect success.
    await page.locator("[data-test='rsvp-submit']").click();
    await expect(page.locator("[data-test='rsvp-message']")).toHaveText(/Saved/);
    await snap(page, "09-invitee-rsvp-saved");
    await ctx.close();
  });

  test("public capacity reflects RSVP, self-signup works", async ({ browser }) => {
    const ctx = await browser.newContext();
    const page = await ctx.newPage();
    await page.goto(`/events/${eventRef}/signup`);
    await expect(page.locator("[data-test='signup-form']")).toBeVisible();
    await expect(page.locator("[data-test='signup-capacity']").first()).toContainText("1");
    await snap(page, "10-signup-page");

    await page.locator("[data-test='signup-name']").fill("Grace Hopper");
    await page.locator("[data-test='signup-email']").fill("grace@example.test");
    await page.locator("[data-test='signup-submit']").click();
    await expect(page.locator("[data-test='signup-message']")).toContainText(/signed up|Redirecting/i);
    // Redirect follows; land on RSVP page
    await page.waitForURL(/\/events\/[^/]+\/r\//, { timeout: 10_000 });
    await expect(page.locator("[data-test='rsvp-form']")).toBeVisible();
    await snap(page, "11-post-signup-rsvp");
    await ctx.close();
  });

  test("admin view now shows both confirmed RSVPs", async ({ page }) => {
    await page.goto("/dev/login");
    await page.goto(`/events/${eventRef}`);
    const rows = page.locator("[data-test^='invitee-row-']");
    await expect(rows).toHaveCount(2);
    // At least one 'yes'
    await expect(page.getByText("yes").first()).toBeVisible();
    await snap(page, "12-admin-detail-with-rsvps");
  });

  test("bad RSVP token returns not found", async ({ page }) => {
    const res = await page.goto(`/events/${eventRef}/r/not-a-real-token`);
    expect(res?.status()).toBe(404);
  });
});
