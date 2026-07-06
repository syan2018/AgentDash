#!/usr/bin/env node

import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const root = path.resolve(__dirname, '..');

const config = parseArgs(process.argv.slice(2));
const packageJson = JSON.parse(fs.readFileSync(path.join(root, 'package.json'), 'utf8'));
const version = packageJson.version;
const gitSha = readCommand('git', ['rev-parse', '--short=12', 'HEAD']);
const buildTime = process.env.AGENTDASH_BUILD_TIME || new Date().toISOString();
const tags = config.tags.length > 0 ? config.tags : [`agentdash-cloud:${version}`];

const args = [
  'build',
  '-f',
  'deploy/docker/Dockerfile.cloud',
  '--build-arg',
  `AGENTDASH_GIT_SHA=${gitSha}`,
  '--build-arg',
  `AGENTDASH_BUILD_TIME=${buildTime}`,
  '--build-arg',
  `AGENTDASH_VERSION=${version}`,
];
for (const tag of tags) {
  args.push('-t', tag);
}
args.push('.');

console.log(`[cloud-image-build] version: ${version}`);
console.log(`[cloud-image-build] git sha: ${gitSha}`);
console.log(`[cloud-image-build] build time: ${buildTime}`);
for (const tag of tags) {
  console.log(`[cloud-image-build] tag: ${tag}`);
}

if (config.dryRun) {
  console.log(`[cloud-image-build] command: docker ${args.join(' ')}`);
  process.exit(0);
}

const result = spawnSync('docker', args, {
  cwd: root,
  stdio: 'inherit',
  windowsHide: true,
});
if (result.error) {
  throw result.error;
}
process.exit(result.status ?? 0);

function parseArgs(values) {
  const result = { dryRun: false, tags: [] };
  for (let index = 0; index < values.length; index += 1) {
    const value = values[index];
    if (value === '--') {
      continue;
    }
    if (value === '--dry-run') {
      result.dryRun = true;
      continue;
    }
    if (value === '--tag' || value === '-t') {
      const tag = values[index + 1];
      if (!tag) {
        throw new Error(`${value} 缺少镜像标签`);
      }
      result.tags.push(tag);
      index += 1;
      continue;
    }
    if (value.startsWith('--tag=')) {
      const tag = value.slice('--tag='.length);
      if (!tag) {
        throw new Error('--tag 不能为空');
      }
      result.tags.push(tag);
      continue;
    }
    throw new Error(`未知参数: ${value}`);
  }
  return result;
}

function readCommand(command, args) {
  const result = spawnSync(command, args, {
    cwd: root,
    encoding: 'utf8',
    windowsHide: true,
  });
  if (result.status !== 0) {
    throw new Error(`${command} ${args.join(' ')} 执行失败: ${result.stderr}`);
  }
  return result.stdout.trim();
}
