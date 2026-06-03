#!/usr/bin/env node

import { execFileSync } from "node:child_process";

const MIGRATION_PREFIX = "crates/agentdash-infrastructure/migrations/";
const ALLOW_REWRITE = process.env.ALLOW_MIGRATION_BASELINE_REWRITE === "1";

function git(args) {
  return execFileSync("git", args, { encoding: "utf8" }).trim();
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

  for (const line of stagedDiffNameStatus()) {
    const parts = splitTabLine(line);
    const status = parts[0] ?? "";
    const paths = changedMigrationPaths(parts);
    const touchesTracked = paths.some((path) => tracked.has(path));
    if (!touchesTracked) continue;

    if (status.startsWith("A")) continue;
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
