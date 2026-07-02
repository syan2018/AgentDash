#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const repoRoot = path.resolve(__dirname, "..");

const scanRoots = [
  "crates/agentdash-local",
  "crates/agentdash-local-tauri",
  "crates/agentdash-executor",
  "crates/agentdash-infrastructure",
  "crates/agentdash-process",
];

const substrateFile = path.normalize("crates/agentdash-process/src/lib.rs");

const forbiddenPatterns = [
  /\bCommand::new\s*\(/,
  /\bstd::process::Command::new\s*\(/,
  /\btokio::process::Command::new\s*\(/,
  /\bStdCommand::new\s*\(/,
  /\bTokioCommand::new\s*\(/,
  /\b[A-Za-z_][A-Za-z0-9_]*Command::new\s*\(/,
  /\.creation_flags\s*\(/,
  /\bCREATE_NO_WINDOW\b/,
  /\bCreateProcessW\b/,
];

function walk(dir, files = []) {
  if (!fs.existsSync(dir)) {
    return files;
  }
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      walk(full, files);
    } else if (entry.isFile() && entry.name.endsWith(".rs")) {
      files.push(full);
    }
  }
  return files;
}

function isAllowedFile(relativePath) {
  const normalized = path.normalize(relativePath);
  if (normalized === substrateFile) {
    return true;
  }
  const segments = normalized.split(path.sep);
  if (segments.some((segment) => segment === "tests" || segment === "test")) {
    return true;
  }
  return /(^|[._-])test(s)?\.rs$/.test(path.basename(normalized));
}

const findings = [];

for (const root of scanRoots) {
  const absRoot = path.join(repoRoot, root);
  for (const file of walk(absRoot)) {
    const relativePath = path.relative(repoRoot, file);
    if (isAllowedFile(relativePath)) {
      continue;
    }
    const lines = fs.readFileSync(file, "utf8").split(/\r?\n/);
    lines.forEach((line, index) => {
      if (forbiddenPatterns.some((pattern) => pattern.test(line))) {
        findings.push({
          file: relativePath,
          line: index + 1,
          text: line.trim(),
        });
      }
    });
  }
}

if (findings.length > 0) {
  console.error("发现未收束的后台进程启动点：");
  for (const finding of findings) {
    console.error(`${finding.file}:${finding.line}: ${finding.text}`);
  }
  console.error(
    "后台执行请使用 agentdash_process::{background_std_command, background_tokio_command}；显式用户入口请使用 user_visible_* helper。"
  );
  process.exit(1);
}

console.log("background process spawn guard passed");
