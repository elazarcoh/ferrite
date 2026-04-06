import { test, expect } from "@playwright/test";

test("app loads and shows tabs", async ({ page }) => {
  await page.goto("/");
  await page.waitForLoadState("networkidle");

  await expect(page.getByRole("tab", { name: /Config/i })).toBeVisible();
  await expect(page.getByRole("tab", { name: /Sprites/i })).toBeVisible();
  await expect(page.getByRole("tab", { name: /State Machine/i })).toBeVisible();
  await expect(page.getByRole("tab", { name: /Simulation/i })).toBeVisible();
});

test("window.__ferrite is available", async ({ page }) => {
  await page.goto("/");
  await page.waitForLoadState("networkidle");
  const hasFerriteGlobal = await page.evaluate(
    () => typeof (window as any).__ferrite !== "undefined"
  );
  expect(hasFerriteGlobal).toBe(true);
});
