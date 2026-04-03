import { defineConfig, devices } from "@playwright/test";

const serverPort = 3011;
const frontendPort = 5199;
const runId = process.env.PLAYWRIGHT_RUN_ID ?? `${process.pid}`;
const backendId = process.env.PLAYWRIGHT_BACKEND_ID ?? `e2e-local-${runId}`;
const repoRoot = process.cwd();
const shellQuote = (value: string) => `"${value.replace(/(["\\$`])/g, "\\$1")}"`;
const configuredDatabaseUrl = resolvePlaywrightDatabaseUrl();
const webServerArgs = [
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
  "--backend-id",
  backendId,
  "--backend-name",
  `e2e-local-${runId}`,
  "--accessible-roots",
  repoRoot,
];

if (configuredDatabaseUrl) {
  webServerArgs.push("--database-url", configuredDatabaseUrl);
}

const webServerCommand = webServerArgs.map(shellQuote).join(" ");

process.env.PLAYWRIGHT_RUN_ID = runId;
process.env.PLAYWRIGHT_BACKEND_ID = backendId;
process.env.PLAYWRIGHT_E2E_ROOT = repoRoot;
process.env.PLAYWRIGHT_SERVER_PORT = String(serverPort);

function resolvePlaywrightDatabaseUrl(): string | null {
  const raw = process.env.PLAYWRIGHT_DATABASE_URL ?? process.env.DATABASE_URL ?? "";
  const databaseUrl = raw.trim();
  if (!databaseUrl) return null;
  if (!/^postgres(ql)?:\/\//i.test(databaseUrl)) {
    throw new Error(`PLAYWRIGHT_DATABASE_URL / DATABASE_URL 必须是 PostgreSQL URL，收到: ${databaseUrl}`);
  }
  return databaseUrl;
}

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
