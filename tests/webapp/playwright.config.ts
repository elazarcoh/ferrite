import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./specs",
  use: {
    baseURL: "http://localhost:8080",
    screenshot: "only-on-failure",
  },
  webServer: {
    command: "npx serve -l 8080 ../../crates/ferrite-webapp/dist",
    url: "http://localhost:8080",
    reuseExistingServer: !process.env.CI,
  },
  projects: [
    { name: "chromium", use: { browserName: "chromium" } },
  ],
});
