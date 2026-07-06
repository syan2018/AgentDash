import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import {
  gateCommand,
  gateNames,
  resolveGateSteps,
  validateQualityGateManifest,
} from "./quality-gates.js";

const REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

test("quality gate manifest exposes the required gates", () => {
  assert.deepEqual(gateNames().sort(), [
    "cloud_image_preflight",
    "deployment_contract",
    "desktop_check",
    "full_local",
    "heavy_check",
    "migration_history",
    "pr_quick",
  ]);

  const result = validateQualityGateManifest();
  assert.equal(result.ok, true, result.errors.join("\n"));
});

test("pr_quick composes migration, test support, shared, frontend, and backend checks", () => {
  assert.deepEqual(
    resolveGateSteps("pr_quick").map((step) => step.id),
    ["migration_guard", "test_support_guard", "shared_check", "frontend_check", "backend_check"],
  );

  assert.equal(
    gateCommand("pr_quick"),
    "pnpm run migration:guard && pnpm run test-support:guard && pnpm run shared:check && pnpm run frontend:check && pnpm run backend:check",
  );
});

test("cloud_image_preflight reuses pr_quick checks before packaging", () => {
  assert.deepEqual(
    resolveGateSteps("cloud_image_preflight").map((step) => step.id),
    ["migration_guard", "test_support_guard", "shared_check", "frontend_check", "backend_check"],
  );
});

test("deployment_contract keeps deployment command membership in one manifest", () => {
  assert.deepEqual(
    resolveGateSteps("deployment_contract").map((step) => step.id),
    [
      "deploy_compose_config",
      "deploy_managed_postgres_config",
      "deploy_update_dry_run",
      "deploy_managed_postgres_update_dry_run",
      "deploy_managed_postgres_backup_boundary",
      "release_metadata",
      "release_metadata_test",
      "cloud_image_dry_run",
    ],
  );

  assert.match(gateCommand("deployment_contract"), /deploy\/compose\/docker-compose\.yml/);
  assert.match(gateCommand("deployment_contract"), /quality-gates\.js expect-failure/);
  assert.match(gateCommand("deployment_contract"), /release-metadata\.test\.js/);
  assert.match(gateCommand("deployment_contract"), /pnpm run docker:cloud:build -- --dry-run/);
});

test("heavy_check keeps manual CI command membership in the manifest", () => {
  assert.deepEqual(
    resolveGateSteps("heavy_check").map((step) => step.id),
    ["backend_clippy", "backend_test", "frontend_test"],
  );
});

test("full_local includes migration, contract, backend, frontend, desktop, and e2e checks", () => {
  const stepIds = resolveGateSteps("full_local").map((step) => step.id);

  assert.deepEqual(stepIds, [
    "migration_guard",
    "test_support_guard",
    "contracts_check",
    "backend_check",
    "backend_clippy",
    "backend_test",
    "shared_check",
    "frontend_check",
    "frontend_lint",
    "frontend_test",
    "desktop_icons_generate",
    "desktop_frontend_check",
    "desktop_shell_check",
    "critical_e2e",
  ]);

  assert.deepEqual(
    resolveGateSteps("desktop_check").map((step) => step.id),
    ["desktop_icons_generate", "shared_check", "desktop_frontend_check", "desktop_shell_check"],
  );
  assert.match(gateCommand("full_local"), /pnpm run contracts:check/);
  assert.match(gateCommand("full_local"), /pnpm run e2e:test:critical/);
});

test("quality gate CLI run entry can expand a gate without executing it", () => {
  const result = spawnQualityGates(["run", "pr_quick", "--dry-run"]);

  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /> \[1\/5\] migration_guard: pnpm run migration:guard/);
  assert.match(result.stdout, /> \[5\/5\] backend_check: pnpm run backend:check/);
});

test("quality gate CLI tolerates a forwarded argument separator", () => {
  const result = spawnQualityGates(["--", "run", "pr_quick", "--dry-run"]);

  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /> \[1\/5\] migration_guard: pnpm run migration:guard/);
});

test("quality gate CLI reports unknown gate names", () => {
  const result = spawnQualityGates(["run", "missing_gate", "--dry-run"]);

  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /Unknown quality gate: missing_gate/);
});

test("root scripts delegate gate composition to the manifest runner", () => {
  const packageJson = JSON.parse(readRepoFile("package.json"));

  assert.equal(packageJson.scripts.check, "node scripts/quality-gates.js run full_local");
  assert.equal(packageJson.scripts["desktop:check"], "node scripts/quality-gates.js run desktop_check");
  assert.equal(packageJson.scripts["test-support:guard"], "node scripts/check-test-support-boundaries.js");
  assert.equal(packageJson.scripts["check:quick"], "node scripts/quality-gates.js run pr_quick");
  assert.equal(
    packageJson.scripts["check:deployment"],
    "node scripts/quality-gates.js run deployment_contract",
  );
  assert.equal(packageJson.scripts["check:heavy"], "node scripts/quality-gates.js run heavy_check");
});

test("CI workflows delegate quality gate selection to the manifest runner", () => {
  const prQuick = readRepoFile(".github/workflows/pr-quick.yml");
  const deployment = readRepoFile(".github/workflows/deploy-contract.yml");
  const heavy = readRepoFile(".github/workflows/heavy-check.yml");
  const cloudImage = readRepoFile(".github/workflows/cloud-image.yml");

  assert.match(prQuick, /node scripts\/quality-gates\.js run pr_quick/);
  assert.doesNotMatch(prQuick, /pnpm run (migration:guard|shared:check|frontend:check|backend:check)/);

  assert.match(deployment, /node scripts\/quality-gates\.js run deployment_contract/);
  assert.doesNotMatch(deployment, /docker compose -f deploy\/compose\/docker-compose\.yml/);
  assert.doesNotMatch(deployment, /node deploy\/compose\/update\.mjs/);

  assert.match(heavy, /node scripts\/quality-gates\.js run heavy_check/);
  assert.match(heavy, /node scripts\/quality-gates\.js run-step critical_e2e/);
  assert.doesNotMatch(heavy, /pnpm run (backend:clippy|backend:test|frontend:test|e2e:test:critical)/);

  assert.match(cloudImage, /node scripts\/quality-gates\.js run cloud_image_preflight/);
  assert.doesNotMatch(cloudImage, /pnpm run (backend:check|frontend:check)/);
});

test("CI pnpm setup follows the packageManager field", () => {
  const packageJson = JSON.parse(readRepoFile("package.json"));
  assert.match(packageJson.packageManager, /^pnpm@\d+\.\d+\.\d+$/);

  for (const workflowPath of [
    ".github/workflows/pr-quick.yml",
    ".github/workflows/deploy-contract.yml",
    ".github/workflows/heavy-check.yml",
    ".github/workflows/cloud-image.yml",
  ]) {
    const workflow = readRepoFile(workflowPath);
    const setupBlocks = pnpmSetupBlocks(workflow);

    assert.ok(setupBlocks.length > 0, `${workflowPath} should setup pnpm`);
    for (const setupBlock of setupBlocks) {
      assert.match(setupBlock, /run_install: false/);
      assert.doesNotMatch(setupBlock, /\bversion:/);
    }
  }
});

function spawnQualityGates(args) {
  return spawnSync(process.execPath, [resolve(REPO_ROOT, "scripts/quality-gates.js"), ...args], {
    cwd: REPO_ROOT,
    encoding: "utf8",
  });
}

function readRepoFile(path) {
  return readFileSync(resolve(REPO_ROOT, path), "utf8");
}

function pnpmSetupBlocks(workflow) {
  const blockPattern =
    /- name: Setup pnpm\r?\n\s+uses: pnpm\/action-setup@v4\r?\n(?<body>(?:\s{8,}.+\r?\n?)*)/g;
  return Array.from(workflow.matchAll(blockPattern), (match) => match[0]);
}
