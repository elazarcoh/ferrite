import { test, expect } from '@playwright/test';

test.describe('Animated pet canvas', () => {
  test('pet canvas element is present in DOM', async ({ page }) => {
    await page.goto('/ferrite/');
    await expect(page.locator('#pet-canvas')).toBeVisible();
  });

  test('canvas draws non-zero pixels after wasm loads', async ({ page }) => {
    await page.goto('/ferrite/');

    await expect.poll(
      () =>
        page.evaluate(() => {
          const c = document.getElementById('pet-canvas') as HTMLCanvasElement | null;
          if (!c) return false;
          const ctx = c.getContext('2d');
          if (!ctx) return false;
          const data = ctx.getImageData(0, 0, c.width, c.height).data;
          return Array.from(data).some(v => v > 0);
        }),
      { timeout: 10_000, intervals: [500] }
    ).toBe(true);
  });

  test('no console errors during home page load', async ({ page }) => {
    const errors: string[] = [];
    page.on('console', msg => {
      if (msg.type() === 'error') errors.push(msg.text());
    });

    await page.goto('/ferrite/');

    // Allow time for wasm to fully initialise
    await page.waitForTimeout(3_000);

    expect(errors, `unexpected console errors: ${errors.join(', ')}`).toHaveLength(0);
  });
});
