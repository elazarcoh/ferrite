import { test, expect } from "@playwright/test";
import path from "path";

const FIXTURE = path.join(__dirname, "../fixtures/test_bundle.petbundle");

async function waitForApp(page: any) {
  await page.waitForFunction(
    () => typeof (window as any).__ferrite !== "undefined",
    { timeout: 15000 }
  );
  await page.waitForTimeout(500);
}

test("export bundle from sprites tab triggers download", async ({ page }) => {
  await page.goto("/");
  await waitForApp(page);

  await page.getByRole("button", { name: /Sprites/i }).click();
  await page.waitForTimeout(200);

  const downloadPromise = page.waitForEvent("download");
  await page.getByRole("button", { name: /Export Bundle/i }).click();
  const download = await downloadPromise;
  expect(download.suggestedFilename()).toMatch(/\.petbundle$/);
});
