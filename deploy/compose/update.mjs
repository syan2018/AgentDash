#!/usr/bin/env node

import { existsSync, mkdirSync, readFileSync } from 'node:fs';
import http from 'node:http';
import https from 'node:https';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');

function printHelp() {
  console.log(`Usage: node deploy/compose/update.mjs [options]

Options:
  --env-file <path>          Compose env file (default: deploy/compose/.env)
  --compose-file <path>      Compose file, repeatable (default: deploy/compose/docker-compose.yml)
  --version <version>        Target AGENTDASH_VERSION
  --image-repository <repo>  Target AGENTDASH_IMAGE_REPOSITORY
  --managed-postgres         Add managed PostgreSQL override
  --skip-backup              Skip Compose postgres backup
  --skip-pull                Skip image pull
  --dry-run                  Print commands and checks without executing them
  --help                     Show this help
`);
}

function parseArgs(argv) {
  const options = {
    envFile: 'deploy/compose/.env',
    composeFiles: [],
    version: undefined,
    imageRepository: undefined,
    managedPostgres: false,
    skipBackup: false,
    skipPull: false,
    dryRun: false,
    help: false,
  };

  const readValue = (index, name) => {
    const value = argv[index + 1];
    if (!value || value.startsWith('--')) {
      throw new Error(`${name} requires a value`);
    }
    return value;
  };

  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    switch (arg) {
      case '--env-file':
        options.envFile = readValue(index, arg);
        index += 1;
        break;
      case '--compose-file':
        options.composeFiles.push(readValue(index, arg));
        index += 1;
        break;
      case '--version':
        options.version = readValue(index, arg);
        index += 1;
        break;
      case '--image-repository':
        options.imageRepository = readValue(index, arg);
        index += 1;
        break;
      case '--managed-postgres':
        options.managedPostgres = true;
        break;
      case '--skip-backup':
        options.skipBackup = true;
        break;
      case '--skip-pull':
        options.skipPull = true;
        break;
      case '--dry-run':
        options.dryRun = true;
        break;
      case '--help':
        options.help = true;
        break;
      default:
        throw new Error(`Unknown option: ${arg}`);
    }
  }

  if (options.composeFiles.length === 0) {
    options.composeFiles.push('deploy/compose/docker-compose.yml');
  }

  return options;
}

function resolveExisting(inputPath) {
  const absolutePath = path.isAbsolute(inputPath) ? inputPath : path.join(repoRoot, inputPath);
  if (!existsSync(absolutePath)) {
    throw new Error(`路径不存在: ${inputPath}`);
  }
  return path.resolve(absolutePath);
}

function readEnvFile(filePath) {
  const entries = {};
  for (const line of readFileSync(filePath, 'utf8').split(/\r?\n/)) {
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith('#')) {
      continue;
    }
    const separator = trimmed.indexOf('=');
    if (separator === -1) {
      continue;
    }
    const key = trimmed.slice(0, separator).trim();
    let value = trimmed.slice(separator + 1).trim();
    if (
      (value.startsWith('"') && value.endsWith('"')) ||
      (value.startsWith("'") && value.endsWith("'"))
    ) {
      value = value.slice(1, -1);
    }
    entries[key] = value;
  }
  return entries;
}

function firstNonEmpty(...values) {
  for (const value of values) {
    if (typeof value === 'string' && value.trim().length > 0) {
      return value.trim();
    }
  }
  return undefined;
}

function quoteArg(value) {
  return /\s/.test(value) ? `"${value.replaceAll('"', '\\"')}"` : value;
}

function formatCommand(command, args) {
  return [command, ...args].map(quoteArg).join(' ');
}

function createRunner({ dryRun, env }) {
  return function run(command, args) {
    console.log(`[run] ${formatCommand(command, args)}`);
    if (dryRun) {
      return;
    }
    const result = spawnSync(command, args, {
      cwd: repoRoot,
      env,
      stdio: 'inherit',
      shell: false,
    });
    if (result.error) {
      throw result.error;
    }
    if (result.status !== 0) {
      throw new Error(`${command} failed with exit code ${result.status}`);
    }
  };
}

function createComposeRunner({ composeFiles, envFile, runner }) {
  const baseArgs = ['compose'];
  for (const composeFile of composeFiles) {
    baseArgs.push('-f', composeFile);
  }
  baseArgs.push('--env-file', envFile);

  return function compose(args) {
    runner('docker', [...baseArgs, ...args]);
  };
}

function timestampForFileName() {
  const date = new Date();
  const pad = (value) => String(value).padStart(2, '0');
  return [
    date.getFullYear(),
    pad(date.getMonth() + 1),
    pad(date.getDate()),
    '-',
    pad(date.getHours()),
    pad(date.getMinutes()),
    pad(date.getSeconds()),
  ].join('');
}

function runPostgresBackup({ compose, dryRun }) {
  const timestamp = timestampForFileName();
  const backupDir = path.join(repoRoot, 'deploy', 'compose', 'backups');
  const backupFile = path.join(backupDir, `agentdash-${timestamp}.dump`);
  const containerFile = `/tmp/agentdash-${timestamp}.dump`;
  if (!dryRun) {
    mkdirSync(backupDir, { recursive: true });
  }

  console.log(`[deploy] backup: ${backupFile}`);
  compose([
    'exec',
    '-T',
    'postgres',
    'sh',
    '-c',
    `pg_dump -U "$POSTGRES_USER" -d "$POSTGRES_DB" --format=custom --no-owner --file=${containerFile}`,
  ]);
  compose(['cp', `postgres:${containerFile}`, backupFile]);
  compose(['exec', '-T', 'postgres', 'rm', '-f', containerFile]);
}

function checkHttp(url, { dryRun }) {
  console.log(`[check] ${url}`);
  if (dryRun) {
    return Promise.resolve();
  }

  return new Promise((resolve, reject) => {
    const client = url.startsWith('https:') ? https : http;
    const request = client.get(url, { timeout: 20_000 }, (response) => {
      response.resume();
      response.on('end', () => {
        if (response.statusCode >= 200 && response.statusCode < 300) {
          resolve();
        } else {
          reject(new Error(`HTTP check failed: ${url} -> ${response.statusCode}`));
        }
      });
    });
    request.on('timeout', () => {
      request.destroy(new Error(`HTTP check timed out: ${url}`));
    });
    request.on('error', reject);
  });
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  if (options.help) {
    printHelp();
    return;
  }

  const envFile = resolveExisting(options.envFile);
  const composeFiles = options.composeFiles.map(resolveExisting);
  if (options.managedPostgres) {
    const managedFile = resolveExisting('deploy/compose/docker-compose.managed-postgres.yml');
    if (!composeFiles.includes(managedFile)) {
      composeFiles.push(managedFile);
    }
    if (!options.skipBackup) {
      throw new Error(
        'Managed PostgreSQL 模式无法通过 Compose postgres 容器执行备份；请先完成外部数据库快照，然后追加 --skip-backup。',
      );
    }
  }

  const envValues = readEnvFile(envFile);
  const targetVersion = firstNonEmpty(options.version, envValues.AGENTDASH_VERSION);
  const imageRepository = firstNonEmpty(
    options.imageRepository,
    envValues.AGENTDASH_IMAGE_REPOSITORY,
    'agentdash-cloud',
  );
  const publicOrigin = firstNonEmpty(
    envValues.AGENTDASH_PUBLIC_ORIGIN,
    'http://127.0.0.1:8080',
  ).replace(/\/+$/, '');

  if (!targetVersion) {
    throw new Error('缺少 AGENTDASH_VERSION；请在 env file 中配置或传入 --version。');
  }

  const env = {
    ...process.env,
    AGENTDASH_VERSION: targetVersion,
    AGENTDASH_IMAGE_REPOSITORY: imageRepository,
  };
  const runner = createRunner({ dryRun: options.dryRun, env });
  const compose = createComposeRunner({ composeFiles, envFile, runner });

  console.log(`[deploy] version: ${targetVersion}`);
  console.log(`[deploy] image: ${imageRepository}:${targetVersion}`);
  console.log(`[deploy] env: ${envFile}`);
  console.log(`[deploy] mode: ${options.managedPostgres ? 'managed-postgres' : 'compose-postgres'}`);

  compose(['config']);

  if (options.skipPull) {
    console.log('[deploy] skip pull');
  } else {
    compose(['pull', 'migrate', 'agentdash-cloud', 'reverse-proxy']);
  }

  if (options.skipBackup) {
    console.log('[deploy] skip backup');
  } else {
    runPostgresBackup({ compose, dryRun: options.dryRun });
  }

  compose(['run', '--rm', 'migrate']);
  compose(['up', '-d', 'agentdash-cloud', 'reverse-proxy']);
  await checkHttp(`${publicOrigin}/api/health`, { dryRun: options.dryRun });
  await checkHttp(`${publicOrigin}/api/version`, { dryRun: options.dryRun });
  compose(['run', '--rm', 'agentdash-cloud', 'doctor']);

  console.log('[deploy] update completed');
}

main().catch((error) => {
  console.error(`[deploy:error] ${error.message}`);
  process.exitCode = 1;
});
