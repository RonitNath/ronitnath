import { expect, test } from "@playwright/test";

test.use({ hasTouch: true });

test("a mobile tap on a drawer link reaches the calendar", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto("http://127.0.0.1:3130/");

  await page.getByRole("button", { name: "Open menu" }).tap();
  const calendar = page.getByRole("link", { name: "Calendar" });
  await expect(calendar).toBeVisible();
  await calendar.tap();

  await expect(page).toHaveURL("http://127.0.0.1:3130/calendar");
  await expect(page.getByRole("heading", { name: "Agenda — next 12 months" })).toBeVisible();
});
