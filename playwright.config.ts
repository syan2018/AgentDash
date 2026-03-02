import { defineConfig, devices } from "@playwright/test";

export default defineConfig({
  testDir: "./tests/e2e",
  timeout: 45_000,
  expect: {
    timeout: 10_000,
  },
  fullyParallel: false,
  retries: 0,
  reporter: [["list"], ["html", { open: "never" }]],
  use: {
    baseURL: "http://127.0.0.1:5199",
    trace: "on-first-retry",
    screenshot: "only-on-failure",
    video: "retain-on-failure",
  },
  webServer: [
    {
      command:
        'powershell -NoProfile -Command "$env:HOST=\'127.0.0.1\'; $env:PORT=\'3011\'; $env:DATABASE_URL=\'sqlite:agentdash-e2e.db?mode=rwc\'; cargo run --bin agentdash-server"',
      url: "http://127.0.0.1:3011/api/health",
      timeout: 120_000,
      reuseExistingServer: false,
    },
    {
      command:
        "powershell -NoProfile -Command \"$env:VITE_API_ORIGIN='http://127.0.0.1:3011'; pnpm --filter frontend dev -- --host 127.0.0.1 --port 5199 --strictPort\"",
      url: "http://127.0.0.1:5199",
      timeout: 120_000,
      reuseExistingServer: false,
    },
  ],
  projects: [
    {
      name: "chromium",
      use: { ...devices["Desktop Chrome"] },
    },
  ],
});
