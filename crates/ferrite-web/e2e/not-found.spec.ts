import { test, expect } from '@playwright/test';

test.describe('404 Not Found', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/ferrite/does-not-exist');
  });

  test('unknown route shows 404 heading', async ({ page }) => {
    await expect(page.getByRole('heading', { name: '404' })).toBeVisible();
  });

  test('404 page shows Page not found message', async ({ page }) => {
    await expect(page.getByText(/Page not found/)).toBeVisible();
  });
});
