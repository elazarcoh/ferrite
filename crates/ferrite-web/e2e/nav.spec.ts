import { test, expect } from '@playwright/test';

test.beforeEach(async ({ page }) => {
  await page.goto('/ferrite/');
});

test('nav bar is visible on home', async ({ page }) => {
  await expect(page.locator('nav')).toBeVisible();
});

test('Home link navigates to home page', async ({ page }) => {
  // Go to guides first so the click is meaningful
  await page.goto('/ferrite/guides');
  await page.locator('nav').getByText('Home').click();
  await expect(page).toHaveURL(/\/ferrite\/?$/);
});

test('Guides link navigates to guides index', async ({ page }) => {
  await page.locator('nav').getByText('Guides').click();
  await expect(page).toHaveURL(/\/ferrite\/guides$/);
});

test('GitHub link has correct href and target', async ({ page }) => {
  const github = page.locator('nav a[href*="github.com"]');
  await expect(github).toBeVisible();
  await expect(github).toHaveAttribute('target', '_blank');
  const href = await github.getAttribute('href');
  expect(href).toMatch(/github\.com/);
});

test('GitHub link opens new tab and does not navigate current page', async ({ page, context }) => {
  const [newPage] = await Promise.all([
    context.waitForEvent('page'),
    page.locator('nav a[href*="github.com"]').click(),
  ]);
  await expect(page).toHaveURL(/\/ferrite\/?$/);
  await newPage.close();
});
