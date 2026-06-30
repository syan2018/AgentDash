const REQUIRED_GATE_NAMES = [
  "full_local",
  "pr_quick",
  "deployment_contract",
  "migration_history",
  "desktop_check",
  "heavy_check",
  "cloud_image_preflight",
];

export const QUALITY_GATE_STEPS = Object.freeze({
  migration_guard: Object.freeze({
    label: "Migration history guard",
    run: "pnpm run migration:guard",
  }),
  contracts_check: Object.freeze({
    label: "Generated contract drift check",
    run: "pnpm run contracts:check",
  }),
  backend_check: Object.freeze({
    label: "Rust workspace check",
    run: "pnpm run backend:check",
  }),
  backend_clippy: Object.freeze({
    label: "Rust clippy",
    run: "pnpm run backend:clippy",
  }),
  backend_test: Object.freeze({
    label: "Rust workspace tests",
    run: "pnpm run backend:test",
  }),
  shared_check: Object.freeze({
    label: "Shared package typecheck",
    run: "pnpm run shared:check",
  }),
  frontend_check: Object.freeze({
    label: "Web typecheck",
    run: "pnpm run frontend:check",
  }),
  frontend_lint: Object.freeze({
    label: "Web lint",
    run: "pnpm run frontend:lint",
  }),
  frontend_test: Object.freeze({
    label: "Web tests",
    run: "pnpm run frontend:test",
  }),
  critical_e2e: Object.freeze({
    label: "Critical Playwright e2e",
    run: "pnpm run e2e:test:critical",
  }),
  desktop_icons_generate: Object.freeze({
    label: "Desktop icon generation",
    run: "pnpm run icons:generate",
  }),
  desktop_frontend_check: Object.freeze({
    label: "Desktop frontend typecheck",
    run: "pnpm run desktop:frontend:check",
  }),
  desktop_shell_check: Object.freeze({
    label: "Desktop shell Rust check",
    run: "pnpm run desktop:shell:check",
  }),
  deploy_compose_config: Object.freeze({
    label: "Compose config",
    run: "docker compose -f deploy/compose/docker-compose.yml --env-file deploy/compose/.env.example config",
  }),
  deploy_managed_postgres_config: Object.freeze({
    label: "Managed PostgreSQL compose config",
    run: "docker compose -f deploy/compose/docker-compose.yml -f deploy/compose/docker-compose.managed-postgres.yml --env-file deploy/compose/.env.example config",
  }),
  deploy_update_dry_run: Object.freeze({
    label: "Compose update dry run",
    run: "node deploy/compose/update.mjs --env-file deploy/compose/.env.example --dry-run --skip-pull",
  }),
  deploy_managed_postgres_update_dry_run: Object.freeze({
    label: "Managed PostgreSQL update dry run",
    run: "node deploy/compose/update.mjs --env-file deploy/compose/.env.example --managed-postgres --skip-backup --dry-run --skip-pull",
  }),
  deploy_managed_postgres_backup_boundary: Object.freeze({
    label: "Managed PostgreSQL backup boundary",
    run: "node scripts/quality-gates.js expect-failure -- node deploy/compose/update.mjs --env-file deploy/compose/.env.example --managed-postgres --dry-run --skip-pull",
  }),
  release_metadata: Object.freeze({
    label: "Release metadata",
    run: "pnpm run release:metadata",
  }),
  cloud_image_dry_run: Object.freeze({
    label: "Cloud image build dry run",
    run: "pnpm run docker:cloud:build -- --dry-run",
  }),
});

export const QUALITY_GATES = Object.freeze({
  migration_history: Object.freeze({
    description: "Protects committed migration history from accidental rewrites.",
    entries: Object.freeze([{ step: "migration_guard" }]),
  }),
  desktop_check: Object.freeze({
    description: "Validates shared packages, Tauri frontend types, and local Tauri Rust crate.",
    entries: Object.freeze([
      { step: "desktop_icons_generate" },
      { step: "shared_check" },
      { step: "desktop_frontend_check" },
      { step: "desktop_shell_check" },
    ]),
  }),
  pr_quick: Object.freeze({
    description: "Fast pull-request signal for migration safety, TypeScript surfaces, and Rust check.",
    entries: Object.freeze([
      { gate: "migration_history" },
      { step: "shared_check" },
      { step: "frontend_check" },
      { step: "backend_check" },
    ]),
  }),
  deployment_contract: Object.freeze({
    description: "Validates compose deployment, update dry-runs, metadata, and cloud image command shape.",
    entries: Object.freeze([
      { step: "deploy_compose_config" },
      { step: "deploy_managed_postgres_config" },
      { step: "deploy_update_dry_run" },
      { step: "deploy_managed_postgres_update_dry_run" },
      { step: "deploy_managed_postgres_backup_boundary" },
      { step: "release_metadata" },
      { step: "cloud_image_dry_run" },
    ]),
  }),
  heavy_check: Object.freeze({
    description: "Manual heavier CI signal for clippy, Rust tests, and frontend tests.",
    entries: Object.freeze([
      { step: "backend_clippy" },
      { step: "backend_test" },
      { step: "frontend_test" },
    ]),
  }),
  cloud_image_preflight: Object.freeze({
    description: "Preflight checks before cloud image packaging.",
    entries: Object.freeze([{ gate: "pr_quick" }]),
  }),
  full_local: Object.freeze({
    description: "Full local quality pass for contract drift, backend, frontend, desktop, and critical e2e.",
    entries: Object.freeze([
      { gate: "migration_history" },
      { step: "contracts_check" },
      { step: "backend_check" },
      { step: "backend_clippy" },
      { step: "backend_test" },
      { step: "shared_check" },
      { step: "frontend_check" },
      { step: "frontend_lint" },
      { step: "frontend_test" },
      { gate: "desktop_check" },
      { step: "critical_e2e" },
    ]),
  }),
});

export function gateNames() {
  return Object.keys(QUALITY_GATES);
}

export function getGate(name) {
  const gate = QUALITY_GATES[name];
  if (!gate) {
    throw new Error(`Unknown quality gate: ${name}`);
  }
  return gate;
}

export function getStep(id) {
  const step = QUALITY_GATE_STEPS[id];
  if (!step) {
    throw new Error(`Unknown quality gate step: ${id}`);
  }
  return { id, ...step };
}

export function resolveGateSteps(name) {
  return resolveGateEntries(name, [], new Set());
}

export function gateCommand(name) {
  return resolveGateSteps(name)
    .map((step) => step.run)
    .join(" && ");
}

export function validateQualityGateManifest() {
  const missingRequired = REQUIRED_GATE_NAMES.filter((name) => !QUALITY_GATES[name]);
  const errors = missingRequired.map((name) => `Missing required gate: ${name}`);

  for (const name of gateNames()) {
    try {
      const steps = resolveGateSteps(name);
      if (steps.length === 0) {
        errors.push(`Gate has no steps: ${name}`);
      }
    } catch (error) {
      errors.push(error instanceof Error ? error.message : String(error));
    }
  }

  return {
    ok: errors.length === 0,
    errors,
    gate_count: gateNames().length,
    step_count: Object.keys(QUALITY_GATE_STEPS).length,
  };
}

function resolveGateEntries(name, visited, emittedStepIds) {
  if (visited.includes(name)) {
    throw new Error(`Quality gate cycle: ${[...visited, name].join(" -> ")}`);
  }

  const gate = getGate(name);
  const nextVisited = [...visited, name];
  const steps = [];

  for (const entry of gate.entries) {
    if ("gate" in entry) {
      steps.push(...resolveGateEntries(entry.gate, nextVisited, emittedStepIds));
      continue;
    }

    if (!("step" in entry)) {
      throw new Error(`Gate ${name} contains an entry without gate or step`);
    }

    try {
      const step = getStep(entry.step);
      if (emittedStepIds.has(entry.step)) {
        continue;
      }
      emittedStepIds.add(entry.step);
      steps.push(step);
    } catch {
      throw new Error(`Gate ${name} references unknown step: ${entry.step}`);
    }
  }

  return steps;
}
