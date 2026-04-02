import { test, expect } from '@playwright/test';

test.describe('Guides index', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/ferrite/guides');
  });

  test('heading is visible', async ({ page }) => {
    await expect(page.getByRole('heading', { name: 'Guides' })).toBeVisible();
  });

  test('all four guide links are present', async ({ page }) => {
    await expect(page.getByText('Getting Started')).toBeVisible();
    await expect(page.getByText('Custom Sprites')).toBeVisible();
    await expect(page.getByText('State Machines')).toBeVisible();
    await expect(page.getByText('Configuration')).toBeVisible();
  });

  test('Getting Started link goes to correct slug', async ({ page }) => {
    await page.getByText('Getting Started').click();
    await expect(page).toHaveURL(/\/ferrite\/guides\/getting-started$/);
  });

  test('Custom Sprites link goes to correct slug', async ({ page }) => {
    await page.getByText('Custom Sprites').click();
    await expect(page).toHaveURL(/\/ferrite\/guides\/custom-sprites$/);
  });

  test('State Machines link goes to correct slug', async ({ page }) => {
    await page.getByText('State Machines').click();
    await expect(page).toHaveURL(/\/ferrite\/guides\/state-machines$/);
  });

  test('Configuration link goes to correct slug', async ({ page }) => {
    await page.getByText('Configuration').click();
    await expect(page).toHaveURL(/\/ferrite\/guides\/configuration$/);
  });
});

test.describe('Guide detail pages', () => {
  test('Getting Started has an h1 heading', async ({ page }) => {
    await page.goto('/ferrite/guides/getting-started');
    await expect(page.locator('h1').first()).toBeVisible();
  });

  for (const slug of [
    'getting-started',
    'custom-sprites',
    'state-machines',
    'configuration',
  ]) {
    test(`${slug} page has substantial content`, async ({ page }) => {
      await page.goto(`/ferrite/guides/${slug}`);
      const text = await page.locator('div.prose').innerText();
      expect(text.length, `guide "${slug}" body should have > 100 chars`).toBeGreaterThan(100);
    });
  }
});
