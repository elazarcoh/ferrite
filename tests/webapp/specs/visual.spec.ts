import { test, expect } from "@playwright/test";

async function waitForApp(page: any) {
  await page.waitForFunction(
    () => typeof (window as any).__ferrite !== "undefined",
    { timeout: 15000 }
  );
  await page.waitForTimeout(500);
}

test("simulation tab visual regression", async ({ page }) => {
  await page.goto("/");
  await waitForApp(page);

  await page.getByRole("button", { name: /Simulation/i }).click();
  await page.waitForTimeout(300);

  const canvas = page.locator("canvas#the_canvas_id");
  await expect(canvas).toBeVisible();
});
