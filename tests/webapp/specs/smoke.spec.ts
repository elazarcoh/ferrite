import { test, expect } from "@playwright/test";

async function waitForApp(page: any) {
  await page.waitForFunction(
    () => typeof (window as any).__ferrite !== "undefined",
    { timeout: 15000 }
  );
}

test("app loads and canvas renders", async ({ page }) => {
  await page.goto("/");
  await waitForApp(page);

  // eframe renders to a canvas — verify it's present and has dimensions
  const canvas = page.locator("canvas#the_canvas_id");
  await expect(canvas).toBeVisible();
  const width = await canvas.evaluate((el: HTMLCanvasElement) => el.width);
  const height = await canvas.evaluate((el: HTMLCanvasElement) => el.height);
  expect(width).toBeGreaterThan(0);
  expect(height).toBeGreaterThan(0);
});

test("window.__ferrite is available", async ({ page }) => {
  await page.goto("/");
  await waitForApp(page);
  const hasFerriteGlobal = await page.evaluate(
    () => typeof (window as any).__ferrite !== "undefined"
  );
  expect(hasFerriteGlobal).toBe(true);
});
