import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./specs",
  use: {
    baseURL: "http://localhost:8080",
    screenshot: "only-on-failure",
  },
  webServer: {
    command: "trunk serve",
    url: "http://localhost:8080",
    reuseExistingServer: !process.env.CI,
    cwd: "../../crates/ferrite-webapp",
  },
  projects: [
    { name: "chromium", use: { browserName: "chromium" } },
  ],
});
