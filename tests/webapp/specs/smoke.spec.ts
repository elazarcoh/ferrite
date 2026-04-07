import { test, expect } from "@playwright/test";

test("app loads and shows tabs", async ({ page }) => {
  await page.goto("/");
  await page.waitForLoadState("networkidle");

  await expect(page.getByRole("button", { name: /Config/i })).toBeVisible();
  await expect(page.getByRole("button", { name: /Sprites/i })).toBeVisible();
  await expect(page.getByRole("button", { name: /State Machine/i })).toBeVisible();
  await expect(page.getByRole("button", { name: /Simulation/i })).toBeVisible();
});

test("window.__ferrite is available", async ({ page }) => {
  await page.goto("/");
  await page.waitForLoadState("networkidle");
  const hasFerriteGlobal = await page.evaluate(
    () => typeof (window as any).__ferrite !== "undefined"
  );
  expect(hasFerriteGlobal).toBe(true);
});
