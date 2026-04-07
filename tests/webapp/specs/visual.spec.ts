import { test, expect } from "@playwright/test";

async function waitForApp(page: any) {
  await page.waitForFunction(
    () => typeof (window as any).__ferrite !== "undefined",
    { timeout: 15000 }
  );
}

test("canvas is visible and rendering", async ({ page }) => {
  await page.goto("/");
  await waitForApp(page);
  await page.waitForTimeout(300);

  const canvas = page.locator("canvas#the_canvas_id");
  await expect(canvas).toBeVisible();

  // Verify eframe is actually rendering (canvas has non-zero dimensions)
  const dims = await canvas.evaluate((el: HTMLCanvasElement) => ({
    w: el.width, h: el.height,
  }));
  expect(dims.w).toBeGreaterThan(0);
  expect(dims.h).toBeGreaterThan(0);
});
