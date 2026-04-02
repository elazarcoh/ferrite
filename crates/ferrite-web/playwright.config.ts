import { defineConfig } from '@playwright/test';

const port = parseInt(process.env.TEST_PORT ?? '8080');

export default defineConfig({
  testDir: './e2e',
  // Each test file gets a fresh browser context
  use: {
    baseURL: `http://localhost:${port}`,
  },
  webServer: {
    // Rebuild CSS then start dev server on the chosen port
    command: `npm run css && dx serve --port ${port}`,
    url: `http://localhost:${port}/ferrite/`,
    reuseExistingServer: !process.env.CI,
    timeout: 120_000,
  },
  projects: [
    { name: 'chromium', use: { browserName: 'chromium' } },
  ],
});
