import { test, expect } from "@playwright/test";

async function waitForApp(page: any) {
  await page.waitForFunction(
    () => typeof (window as any).__ferrite !== "undefined",
    { timeout: 15000 }
  );
}

test("inject_event is accepted without error", async ({ page }) => {
  await page.goto("/");
  await waitForApp(page);

  // Verify the bridge accepts a grab event without throwing
  const result = await page.evaluate(() => {
    try {
      (window as any).__ferrite.inject_event('{"type":"grab","pet_id":"esheep"}');
      return { ok: true };
    } catch (e: any) {
      return { ok: false, error: e.message };
    }
  });
  expect(result.ok).toBe(true);
});
