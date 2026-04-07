import { test, expect } from "@playwright/test";

async function waitForApp(page: any) {
  await page.waitForFunction(
    () => typeof (window as any).__ferrite !== "undefined",
    { timeout: 15000 }
  );
  await page.waitForTimeout(500);
}

test("JS bridge returns initial app state", async ({ page }) => {
  await page.goto("/");
  await waitForApp(page);

  const state = await page.evaluate(
    () => (window as any).__ferrite.get_state()
  );
  expect(state).toHaveProperty("pets");
  expect(state).toHaveProperty("dark_mode");
  expect(Array.isArray(state.pets)).toBe(true);
});

test("SM editor renders without errors", async ({ page }) => {
  await page.goto("/");
  await waitForApp(page);

  const errors: string[] = [];
  page.on("console", (msg) => {
    if (msg.type() === "error") errors.push(msg.text());
  });

  await page.getByRole("button", { name: /State Machine/i }).click();
  await page.waitForTimeout(200);

  expect(errors.filter(e => !e.includes("favicon"))).toHaveLength(0);
});
