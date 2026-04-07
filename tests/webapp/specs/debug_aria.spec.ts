import { test, expect } from "@playwright/test";

// Temporary diagnostic test — remove after understanding the accessibility tree
test("debug: dump aria snapshot after ferrite ready", async ({ page }) => {
  await page.goto("/");
  // Wait until the JS bridge is attached (signals WASM fully initialized)
  await page.waitForFunction(
    () => typeof (window as any).__ferrite !== "undefined",
    { timeout: 15000 }
  );
  await page.waitForTimeout(500);

  const buttonCount = await page.getByRole("button").count();
  const allRoles = await page.locator("[role]").count();
  const bodyLen = (await page.locator("body").innerHTML()).length;
  const snapshot = await page.accessibility.snapshot();

  console.log("=== ARIA DEBUG ===");
  console.log("button count:", buttonCount);
  console.log("elements with [role]:", allRoles);
  console.log("body innerHTML length:", bodyLen);
  console.log("accessibility snapshot:", JSON.stringify(snapshot, null, 2).slice(0, 2000));

  // Always pass — we only care about the console output
  expect(true).toBe(true);
});
