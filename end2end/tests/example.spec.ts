import { test, expect } from "@playwright/test";

test("homepage has title and starscape chrome", async ({ page }) => {
  await page.goto("http://127.0.0.1:3004/");

  await expect(page).toHaveTitle("Ronit Nath");
  await expect(page.locator("h1")).toHaveText("Ronit Nath");
  await expect(page.getByRole("link", { name: "Isoastra" })).toHaveAttribute(
    "href",
    "https://isoastra.com",
  );
  await expect(page.getByRole("link", { name: "GitHub" })).toBeVisible();
  await expect(page.getByRole("link", { name: "Instagram" })).toBeVisible();
  await expect(page.getByRole("link", { name: "LinkedIn" })).toBeVisible();
  await expect(page.getByRole("link", { name: "Email" })).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Toggle color theme" }),
  ).toBeVisible();
  await expect(page.getByRole("link", { name: "Authenticate" })).toBeVisible();
  await expect(page.locator("canvas.starscape")).toBeAttached();
  await expect(page.locator(".mini-globe")).toBeAttached();
});

test("auth page has login form island", async ({ page }) => {
  await page.goto("http://127.0.0.1:3004/auth");

  await expect(page.locator("h1")).toHaveText("Authenticate");
  await expect(page.locator('input[type="email"]')).toBeVisible();
  await expect(page.locator('input[type="password"]')).toBeVisible();
  await expect(page.getByRole("button", { name: "Log in" })).toBeVisible();
});
