import { test, expect } from "@playwright/test";
import path from "path";

const FIXTURE = path.join(__dirname, "../fixtures/test_bundle.petbundle");

test("export bundle from sprites tab triggers download", async ({ page }) => {
  await page.goto("/");
  await page.waitForLoadState("networkidle");

  await page.getByRole("tab", { name: /Sprites/i }).click();
  await page.waitForTimeout(200);

  const downloadPromise = page.waitForEvent("download");
  await page.getByRole("button", { name: /Export Bundle/i }).click();
  const download = await downloadPromise;
  expect(download.suggestedFilename()).toMatch(/\.petbundle$/);
});
