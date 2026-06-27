#!/usr/bin/env node

import fs from 'node:fs';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const root = path.resolve(__dirname, '..');

const args = parseArgs(process.argv.slice(2));
const packageJson = readJson(path.join(root, 'package.json'));
const cargoMetadata = readCargoMetadata();
const gitSha = readGitSha();
const buildTime = process.env.AGENTDASH_BUILD_TIME || new Date().toISOString();

const workspaceVersions = collectWorkspaceVersions(cargoMetadata);
if (!workspaceVersions.includes(packageJson.version)) {
  throw new Error(
    `根 package.json version (${packageJson.version}) 未出现在 Cargo workspace package versions: ${workspaceVersions.join(', ')}`,
  );
}

const manifest = {
  product: 'AgentDash',
  version: packageJson.version,
  git_sha: gitSha,
  build_time: buildTime,
  package_manager: packageJson.packageManager,
  cargo_versions: workspaceVersions,
  artifacts: {
    server_binary: 'agentdash-server',
    cloud_image: `agentdash-cloud:${packageJson.version}`,
    web_dist: 'packages/app-web/dist',
    desktop_installer: `AgentDash_${packageJson.version}_x64-setup.exe`,
  },
};

const output = `${JSON.stringify(manifest, null, 2)}\n`;
if (args.out) {
  const outPath = path.resolve(root, args.out);
  fs.mkdirSync(path.dirname(outPath), { recursive: true });
  fs.writeFileSync(outPath, output, 'utf8');
} else {
  process.stdout.write(output);
}

function parseArgs(values) {
  const result = { out: null };
  for (let index = 0; index < values.length; index += 1) {
    const value = values[index];
    if (value === '--out') {
      const next = values[index + 1];
      if (!next) {
        throw new Error('--out 缺少输出路径');
      }
      result.out = next;
      index += 1;
      continue;
    }
    if (value.startsWith('--out=')) {
      result.out = value.slice('--out='.length);
      if (!result.out) {
        throw new Error('--out 不能为空');
      }
      continue;
    }
    throw new Error(`未知参数: ${value}`);
  }
  return result;
}

function readJson(filePath) {
  return JSON.parse(fs.readFileSync(filePath, 'utf8'));
}

function readCargoMetadata() {
  const stdout = execFileSync('cargo', ['metadata', '--format-version', '1', '--no-deps'], {
    cwd: root,
    encoding: 'utf8',
    windowsHide: true,
  });
  return JSON.parse(stdout);
}

function readGitSha() {
  return execFileSync('git', ['rev-parse', '--short=12', 'HEAD'], {
    cwd: root,
    encoding: 'utf8',
    windowsHide: true,
  }).trim();
}

function collectWorkspaceVersions(metadata) {
  const memberIds = new Set(metadata.workspace_members);
  const versions = new Set();
  for (const pkg of metadata.packages) {
    if (memberIds.has(pkg.id)) {
      versions.add(pkg.version);
    }
  }
  return [...versions].sort();
}
