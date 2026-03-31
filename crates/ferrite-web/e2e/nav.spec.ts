import { test, expect } from '@playwright/test';

test.describe('Navigation bar', () => {
  test.describe('from home page', () => {
    test.beforeEach(async ({ page }) => {
      await page.goto('/ferrite/');
    });

    test('nav bar is visible on home', async ({ page }) => {
      await expect(page.locator('nav')).toBeVisible();
    });

    test('Guides link navigates to guides index', async ({ page }) => {
      await page.locator('nav').getByText('Guides').click();
      await expect(page).toHaveURL(/\/ferrite\/guides$/);
    });

    test('GitHub link has correct href and target', async ({ page }) => {
      const github = page.locator('nav a[href*="github.com"]');
      await expect(github).toBeVisible();
      await expect(github).toHaveAttribute('target', '_blank');
    });

    test('GitHub link opens new tab and does not navigate current page', async ({ page, context }) => {
      const [newPage] = await Promise.all([
        context.waitForEvent('page'),
        page.locator('nav a[href*="github.com"]').click(),
      ]);
      await expect(page).toHaveURL(/\/ferrite\/?$/);
      await newPage.waitForLoadState('domcontentloaded').catch(() => {});
      await newPage.close();
    });
  });

  test.describe('from guides page', () => {
    test.beforeEach(async ({ page }) => {
      await page.goto('/ferrite/guides');
    });

    test('Home link navigates to home page', async ({ page }) => {
      await page.locator('nav').getByText('Home').click();
      await expect(page).toHaveURL(/\/ferrite\/?$/);
    });
  });
});
