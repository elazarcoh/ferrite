import { test, expect } from "@playwright/test";

// Temporary diagnostic test — remove after understanding the accessibility tree
test("debug: dump aria snapshot after ferrite ready", async ({ page }) => {
  // Capture console messages
  const consoleMsgs: string[] = [];
  page.on("console", (msg) => consoleMsgs.push(`[${msg.type()}] ${msg.text()}`));
  page.on("pageerror", (err) => consoleMsgs.push(`[pageerror] ${err.message}`));

  await page.goto("/");
  await page.waitForFunction(
    () => typeof (window as any).__ferrite !== "undefined",
    { timeout: 15000 }
  );
  await page.waitForTimeout(1000);

  const domInfo = await page.evaluate(() => ({
    bodyChildCount: document.body.childElementCount,
    bodyInnerHTML: document.body.innerHTML.slice(0, 500),
    canvasExists: !!document.getElementById("the_canvas_id"),
    canvasInBody: document.body.contains(document.getElementById("the_canvas_id")),
    allIds: Array.from(document.querySelectorAll("[id]")).map(e => e.id).slice(0, 20),
    allRoles: Array.from(document.querySelectorAll("[role]")).map(e => `${e.tagName}[role=${e.getAttribute("role")}]`).slice(0, 20),
    windowKeys: Object.keys(window).filter(k => k.startsWith("__")).slice(0, 10),
  }));

  console.log("=== DOM DEBUG ===");
  console.log(JSON.stringify(domInfo, null, 2));
  console.log("=== CONSOLE MESSAGES ===");
  consoleMsgs.slice(0, 20).forEach(m => console.log(m));

  const buttonCount = await page.getByRole("button").count();
  console.log("Playwright button count:", buttonCount);

  const ariaSnapshot = await page.locator("body").ariaSnapshot();
  console.log("aria snapshot:", ariaSnapshot.slice(0, 1000));

  expect(true).toBe(true);
});
