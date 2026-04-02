import { test, expect } from '@playwright/test';

test.describe('Home page', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/ferrite/');
  });

  test('hero heading shows Ferrite', async ({ page }) => {
    await expect(page.locator('h1').first()).toContainText('Ferrite');
  });

  test('hero tagline mentions Windows desktop', async ({ page }) => {
    await expect(page.getByText(/Windows desktop/)).toBeVisible();
  });

  test('Download for Windows button is visible', async ({ page }) => {
    await expect(page.getByText('Download for Windows').first()).toBeVisible();
  });

  test('Download button links to GitHub releases', async ({ page }) => {
    const btn = page.getByText('Download for Windows').first();
    const href = await btn.getAttribute('href');
    expect(href).toMatch(/github\.com/);
    expect(href).toMatch(/releases/);
  });

  test('three feature cards are visible', async ({ page }) => {
    await expect(page.getByRole('heading', { name: 'Animated' })).toBeVisible();
    await expect(page.getByRole('heading', { name: 'Custom Sprites' })).toBeVisible();
    await expect(page.getByRole('heading', { name: 'Scriptable' })).toBeVisible();
  });

  test('Read the Guides link navigates to guides index', async ({ page }) => {
    await page.getByText('Read the Guides').click();
    await expect(page).toHaveURL(/\/ferrite\/guides$/);
  });
});
