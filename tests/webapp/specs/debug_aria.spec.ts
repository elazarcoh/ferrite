import { test, expect } from "@playwright/test";

// Temporary diagnostic test — remove after understanding the accessibility tree
test("debug: dump aria snapshot after ferrite ready", async ({ page }) => {
  await page.goto("/");
  await page.waitForFunction(
    () => typeof (window as any).__ferrite !== "undefined",
    { timeout: 15000 }
  );
  await page.waitForTimeout(1000);

  const buttonCount = await page.getByRole("button").count();
  const allRoleCount = await page.locator("[role]").count();
  const bodyHtml = await page.locator("body").innerHTML();

  console.log("=== ARIA DEBUG ===");
  console.log("button count:", buttonCount);
  console.log("elements with [role]:", allRoleCount);
  console.log("body innerHTML (first 2000 chars):", bodyHtml.slice(0, 2000));

  if (buttonCount > 0) {
    const labels = await page.getByRole("button").evaluateAll(
      (els) => els.map((el) => `${el.getAttribute("aria-label")} | ${el.textContent}`)
    );
    console.log("button labels:", JSON.stringify(labels));
  }

  const ariaSnapshot = await page.locator("body").ariaSnapshot();
  console.log("aria snapshot (first 2000):", ariaSnapshot.slice(0, 2000));

  // Always pass — we only care about the console output
  expect(true).toBe(true);
});
