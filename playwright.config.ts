import path from "node:path";
import { defineConfig, devices } from "@playwright/test";

const serverPort = 3011;
const frontendPort = 5199;
const runId = process.env.PLAYWRIGHT_RUN_ID ?? `${process.pid}`;
const backendId = process.env.PLAYWRIGHT_BACKEND_ID ?? `e2e-local-${runId}`;
const repoRoot = process.cwd();
const dbFile = path.join(repoRoot, "tmp", `agentdash-e2e-${runId}.db`);
const databaseUrl = `sqlite:${dbFile.replace(/\\/g, "/")}?mode=rwc`;
const shellQuote = (value: string) => `"${value.replace(/(["\\$`])/g, "\\$1")}"`;
const webServerCommand = [
  "node",
  "./scripts/dev-joint.js",
  "--skip-build",
  "--frontend-mode",
  "preview",
  "--server-host",
  "127.0.0.1",
  "--server-port",
  String(serverPort),
  "--frontend-host",
  "127.0.0.1",
  "--frontend-port",
  String(frontendPort),
  "--database-url",
  databaseUrl,
  "--backend-id",
  backendId,
  "--backend-name",
  `e2e-local-${runId}`,
  "--accessible-roots",
  repoRoot,
].map(shellQuote).join(" ");

process.env.PLAYWRIGHT_RUN_ID = runId;
process.env.PLAYWRIGHT_BACKEND_ID = backendId;
process.env.PLAYWRIGHT_E2E_ROOT = repoRoot;
process.env.PLAYWRIGHT_SERVER_PORT = String(serverPort);

export default defineConfig({
  testDir: "./tests/e2e",
  timeout: 60_000,
  expect: {
    timeout: 15_000,
  },
  fullyParallel: false,
  retries: 0,
  reporter: [["list"], ["html", { open: "never" }]],
  use: {
    baseURL: `http://127.0.0.1:${frontendPort}`,
    trace: "on-first-retry",
    screenshot: "only-on-failure",
    video: "retain-on-failure",
  },
  webServer: {
    command: webServerCommand,
    url: `http://127.0.0.1:${frontendPort}`,
    timeout: 180_000,
    reuseExistingServer: false,
  },
  projects: [
    {
      name: "chromium",
      use: { ...devices["Desktop Chrome"] },
    },
  ],
});
