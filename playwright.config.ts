import { defineConfig, devices } from "@playwright/test";
import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";

const serverPort = 3011;
const frontendPort = 5199;
const serverHost = "127.0.0.1";
const userScopeId = process.env.PLAYWRIGHT_USER_ID ?? "local-user";
const runId = process.env.PLAYWRIGHT_RUN_ID ?? `${process.pid}`;
const repoRoot = process.cwd();
const backendId = process.env.PLAYWRIGHT_BACKEND_ID ?? resolvePlaywrightBackendId();
const shellQuote = (value: string) => `"${value.replace(/(["\\$`])/g, "\\$1")}"`;
const configuredDatabaseUrl = resolvePlaywrightDatabaseUrl();
const skipWebServer = process.env.PLAYWRIGHT_SKIP_WEBSERVER === "1";
const reuseExistingServer = process.env.PLAYWRIGHT_REUSE_EXISTING_SERVER === "1";
const webServerArgs = [
  "node",
  "./scripts/dev-runtime.js",
  "--profile",
  "web",
  "--skip-build",
  "--frontend-mode",
  "preview",
  "--server-host",
  serverHost,
  "--server-port",
  String(serverPort),
  "--frontend-host",
  serverHost,
  "--frontend-port",
  String(frontendPort),
  "--backend-name",
  `e2e-local-${runId}`,
  "--workspace-roots",
  repoRoot,
];

if (configuredDatabaseUrl) {
  webServerArgs.push("--database-url", configuredDatabaseUrl);
}

const webServerCommand = webServerArgs.map(shellQuote).join(" ");
const webServer = skipWebServer
  ? undefined
  : {
      command: webServerCommand,
      url: `http://127.0.0.1:${frontendPort}`,
      timeout: 180_000,
      reuseExistingServer,
    };

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

function resolvePlaywrightBackendId(): string {
  const binary = process.platform === "win32"
    ? "target/debug/agentdash-local.exe"
    : "target/debug/agentdash-local";
  const result = spawnSync(binary, ["machine-identity"], {
    cwd: repoRoot,
    encoding: "utf8",
    windowsHide: true,
  });
  if (result.status !== 0) {
    const message = result.stderr.trim() || result.stdout.trim() || `exit=${result.status}`;
    throw new Error(`读取 Playwright local backend identity 失败: ${message}`);
  }
  const identity = JSON.parse(result.stdout) as { machine_id?: string };
  const machineId = identity.machine_id?.trim();
  if (!machineId) {
    throw new Error("Playwright local backend identity 缺少 machine_id");
  }
  const hash = createHash("sha256");
  hash.update(machineId);
  hash.update("\n");
  hash.update("user");
  hash.update("\n");
  hash.update(userScopeId);
  hash.update("\n");
  hash.update("default");
  return `local_${hash.digest("hex").slice(0, 24)}`;
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
    baseURL: `http://${serverHost}:${frontendPort}`,
    trace: "on-first-retry",
    screenshot: "only-on-failure",
    video: "retain-on-failure",
  },
  webServer,
  projects: [
    {
      name: "chromium",
      use: { ...devices["Desktop Chrome"] },
    },
  ],
});
