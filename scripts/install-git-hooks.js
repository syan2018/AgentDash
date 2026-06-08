#!/usr/bin/env node

import { execFileSync } from "node:child_process";
import { chmodSync, existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname } from "node:path";

const MANAGED_MARKER = "# AgentDashboard managed pre-commit hook";

function git(args) {
  return execFileSync("git", args, { encoding: "utf8" }).trim();
}

function main() {
  const force = process.argv.includes("--force");
  const hookPath = git(["rev-parse", "--git-path", "hooks/pre-commit"]);
  const hookBody = `#!/bin/sh
${MANAGED_MARKER}

pnpm run migration:guard
`;

  if (existsSync(hookPath)) {
    const current = readFileSync(hookPath, "utf8");
    if (!current.includes(MANAGED_MARKER) && !force) {
      console.error(`pre-commit hook already exists at ${hookPath}`);
      console.error("Re-run with --force to replace it.");
      process.exit(1);
    }
  }

  mkdirSync(dirname(hookPath), { recursive: true });
  writeFileSync(hookPath, hookBody, "utf8");
  chmodSync(hookPath, 0o755);
  console.log(`installed pre-commit hook: ${hookPath}`);
}

main();
