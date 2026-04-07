import { test, expect } from "@playwright/test";

test("simulation tab visual regression", async ({ page }) => {
  await page.goto("/");
  await page.waitForLoadState("networkidle");

  await page.getByRole("button", { name: /Simulation/i }).click();
  await page.waitForTimeout(300);

  const canvas = page.locator("canvas#the_canvas_id");
  await expect(canvas).toBeVisible();
});
