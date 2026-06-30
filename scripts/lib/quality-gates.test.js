import assert from "node:assert/strict";
import test from "node:test";

import {
  gateCommand,
  gateNames,
  resolveGateSteps,
  validateQualityGateManifest,
} from "./quality-gates.js";

test("quality gate manifest exposes the required gates", () => {
  assert.deepEqual(gateNames().sort(), [
    "deployment_contract",
    "desktop_check",
    "full_local",
    "migration_history",
    "pr_quick",
  ]);

  const result = validateQualityGateManifest();
  assert.equal(result.ok, true, result.errors.join("\n"));
});

test("pr_quick composes migration, shared, frontend, and backend checks", () => {
  assert.deepEqual(
    resolveGateSteps("pr_quick").map((step) => step.id),
    ["migration_guard", "shared_check", "frontend_check", "backend_check"],
  );

  assert.equal(
    gateCommand("pr_quick"),
    "pnpm run migration:guard && pnpm run shared:check && pnpm run frontend:check && pnpm run backend:check",
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
      "cloud_image_dry_run",
    ],
  );

  assert.match(gateCommand("deployment_contract"), /deploy\/compose\/docker-compose\.yml/);
  assert.match(gateCommand("deployment_contract"), /quality-gates\.js expect-failure/);
  assert.match(gateCommand("deployment_contract"), /pnpm run docker:cloud:build -- --dry-run/);
});

test("full_local includes migration, contract, backend, frontend, desktop, and e2e checks", () => {
  const stepIds = resolveGateSteps("full_local").map((step) => step.id);

  assert.deepEqual(stepIds, [
    "migration_guard",
    "contracts_check",
    "backend_check",
    "backend_clippy",
    "backend_test",
    "shared_check",
    "frontend_check",
    "frontend_lint",
    "frontend_test",
    "desktop_check",
    "critical_e2e",
  ]);

  assert.equal(resolveGateSteps("desktop_check").at(0)?.run, "pnpm run desktop:check");
  assert.match(gateCommand("full_local"), /pnpm run contracts:check/);
  assert.match(gateCommand("full_local"), /pnpm run e2e:test:critical/);
});
