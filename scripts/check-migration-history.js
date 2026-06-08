#!/usr/bin/env node

import { execFileSync } from "node:child_process";

const MIGRATION_PREFIX = "crates/agentdash-infrastructure/migrations/";
const ALLOW_REWRITE = process.env.ALLOW_MIGRATION_BASELINE_REWRITE === "1";
const BASE_REF = process.env.MIGRATION_HISTORY_BASE_REF || "origin/main";

function git(args) {
  return execFileSync("git", args, { encoding: "utf8" }).trim();
}

function gitMaybe(args) {
  try {
    return execFileSync("git", args, {
      encoding: "utf8",
      stdio: ["ignore", "pipe", "ignore"],
    }).trim();
  } catch {
    return "";
  }
}

function gitSucceeds(args) {
  try {
    execFileSync("git", args, { stdio: "ignore" });
    return true;
  } catch {
    return false;
  }
}

function splitTabLine(line) {
  return line.split("\t").filter(Boolean);
}

function stagedDiffNameStatus() {
  const output = git(["diff", "--cached", "--name-status", "--", MIGRATION_PREFIX]);
  return output ? output.split(/\r?\n/).filter(Boolean) : [];
}

function trackedMigrations() {
  const output = git(["ls-tree", "-r", "--name-only", "HEAD", MIGRATION_PREFIX]);
  return new Set(output ? output.split(/\r?\n/).filter(Boolean) : []);
}

function migrationRepairBase() {
  return gitMaybe(["merge-base", "HEAD", BASE_REF]);
}

function stagedContentMatchesRef(path, ref) {
  if (!ref) return false;
  const staged = gitMaybe(["show", `:${path}`]);
  const base = gitMaybe(["show", `${ref}:${path}`]);
  return staged !== "" && staged === base;
}

function pathExistsInRef(path, ref) {
  if (!ref) return false;
  return gitSucceeds(["cat-file", "-e", `${ref}:${path}`]);
}

function changedMigrationPaths(parts) {
  const status = parts[0] ?? "";
  if (status.startsWith("R") || status.startsWith("C")) {
    return parts.slice(1);
  }
  return parts.slice(1, 2);
}

function main() {
  if (ALLOW_REWRITE) {
    console.log("migration history guard bypassed by ALLOW_MIGRATION_BASELINE_REWRITE=1");
    return;
  }

  const tracked = trackedMigrations();
  const violations = [];
  const repairBase = migrationRepairBase();

  for (const line of stagedDiffNameStatus()) {
    const parts = splitTabLine(line);
    const status = parts[0] ?? "";
    const paths = changedMigrationPaths(parts);
    const touchesTracked = paths.some((path) => tracked.has(path));
    if (!touchesTracked) continue;

    if (status.startsWith("A")) continue;
    if (paths.every((path) => !pathExistsInRef(path, repairBase))) continue;
    if (
      status === "M" &&
      paths.length === 1 &&
      stagedContentMatchesRef(paths[0], repairBase)
    ) {
      console.log(`migration history guard allowed restoring ${paths[0]} to ${BASE_REF}`);
      continue;
    }
    violations.push(line);
  }

  if (violations.length === 0) {
    console.log("migration history guard passed");
    return;
  }

  console.error("禁止在普通任务中修改、删除或重命名已提交 migration：");
  for (const violation of violations) {
    console.error(`  ${violation}`);
  }
  console.error("");
  console.error("请新增下一号 migration 文件。只有明确授权的数据库 baseline squash/reset/merge 任务");
  console.error("才能设置 ALLOW_MIGRATION_BASELINE_REWRITE=1 绕过此检查。");
  process.exit(1);
}

main();
