#!/usr/bin/env node
/**
 * AgentDash 联合启动脚本（Node 版）
 * 目标：
 * 1. 先清理遗留端口，减少重启时的干扰
 * 2. 先统一编译，再按顺序启动 server -> local -> frontend
 * 3. 统一接管 Ctrl+C，确保子进程树被一并清理
 */

import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { randomUUID } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { execSync, spawnSync } from 'node:child_process';
import {
  createProcessSupervisor,
  fetchJson,
  installShutdownHandlers,
  isPostgresUrl,
  isWindows,
  killProcessTreeByName,
  requestJson,
  runAgentDashDevRustBuild,
  sleep,
  startDebugBinary,
  startPnpmFilterScript,
  waitForHttpReady,
} from './lib/dev-process.js';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const root = path.resolve(__dirname, '..');
const devRuntimeProfilePath = path.join(root, '.agentdash', 'dev-joint-runtime-profile.json');

const config = parseArgs(process.argv.slice(2));

if (config.help) {
  printHelp();
  process.exit(0);
}

if (config.databaseUrl && !isPostgresUrl(config.databaseUrl)) {
  throw new Error(`--database-url / DATABASE_URL 必须是 PostgreSQL URL，收到: ${config.databaseUrl}`);
}

const rustBuild = configureRustBuild(config);
const supervisor = createProcessSupervisor({
  root,
  shutdownMessage: '正在停止所有服务...',
  stoppedMessage: '全部已停止',
  afterStop: async () => {
    // 兜底：确保 embedded PostgreSQL 子进程不会成为僵尸。
    await killEmbeddedPostgres().catch(() => {});
  },
});
const {
  runCommand,
  shutdown,
  waitForAnyChildExit,
} = supervisor;

installShutdownHandlers(shutdown);

await main();

async function main() {
  printBanner();

  await runStep0Cleanup();

  if (!config.skipBuild) {
    console.log('[1/4] 构建 dev Rust 目标...');
    await runAgentDashDevRustBuild(runCommand, { env: rustBuild.env });
    console.log('  构建完成');
  } else {
    console.log('[1/4] 跳过构建（--skip-build）');
  }

  if (!config.skipServer) {
    console.log(`[2/4] 启动 agentdash-server (:${config.serverPort})...`);
    const serverEnv = {
      ...process.env,
      HOST: config.serverHost,
      PORT: String(config.serverPort),
      DATABASE_URL: undefined,
    };
    // 仅当明确提供 PostgreSQL URL 时透传，避免 sqlite 默认值误导运行时判断。
    if (isPostgresUrl(config.databaseUrl)) {
      serverEnv.DATABASE_URL = config.databaseUrl;
    }
    startDebugBinary(supervisor, root, 'agentdash-server', { env: serverEnv });
  } else {
    console.log(`[2/4] 跳过 agentdash-server，等待现有服务 (:${config.serverPort})...`);
  }
  await waitForHttpReady(config.serverPort, '/api/health', 120);

  if (!config.skipLocal) {
    const backend = await ensureDevLocalRuntimeClaim(config.serverPort, config);
    const localArgs = [
      '--cloud-url', backend.relay_ws_url,
      '--token', backend.auth_token,
      '--accessible-roots', config.accessibleRoots,
      '--name', backend.name || config.backendName,
      '--backend-id', backend.backend_id
    ];
    if (config.noExecutor) {
      localArgs.push('--no-executor');
    }

    console.log('[3/4] 启动 agentdash-local...');
    startDebugBinary(supervisor, root, 'agentdash-local', {
      args: localArgs,
      label: 'agentdash-local',
    });
    await waitForLocalRegistration(config.serverPort, backend.backend_id, 20, 500);
  } else {
    console.log('[3/4] 跳过 agentdash-local（--skip-local）');
  }

  if (!config.skipFrontend) {
    console.log(`[4/4] 启动前端 (${config.frontendMode}, :${config.frontendPort})...`);
    startFrontendProcess();
  } else {
    console.log('[4/4] 跳过前端（--skip-frontend）');
  }

  console.log('');
  console.log('  ╔══════════════════════════════════════╗');
  console.log('  ║       所有服务已就绪                 ║');
  console.log('  ╚══════════════════════════════════════╝');
  console.log(`  API:      http://${config.serverHost}:${config.serverPort}`);
  console.log(`  Frontend: http://${config.frontendHost}:${config.frontendPort}`);
  console.log(`  WS:       ws://${config.serverHost}:${config.serverPort}/ws/backend`);
  console.log('');
  console.log('  按 Ctrl+C 停止全部服务');
  console.log('');

  await waitForAnyChildExit();
  await shutdown(1);
}

function parseArgs(args) {
  const result = {
    accessibleRoots: root,
    backendName: 'dev-local',
    databaseUrl: process.env.DATABASE_URL || null,
    frontendMode: 'dev',
    frontendHost: '127.0.0.1',
    frontendPort: 5380,
    help: false,
    noExecutor: false,
    sccacheMode: 'auto',
    sccacheDir: process.env.SCCACHE_DIR || null,
    serverHost: '127.0.0.1',
    serverPort: 3001,
    skipBuild: false,
    skipFrontend: false,
    skipLocal: false,
    skipServer: false
  };

  for (let index = 0; index < args.length; index += 1) {
    const arg = args[index];
    if (arg === '--help' || arg === '-h') {
      result.help = true;
      continue;
    }
    if (arg === '--skip-build') {
      result.skipBuild = true;
      continue;
    }
    if (arg === '--skip-local') {
      result.skipLocal = true;
      continue;
    }
    if (arg === '--skip-server') {
      result.skipServer = true;
      continue;
    }
    if (arg === '--skip-frontend') {
      result.skipFrontend = true;
      continue;
    }
    if (arg === '--no-executor') {
      result.noExecutor = true;
      continue;
    }
    if (arg === '--sccache') {
      result.sccacheMode = 'required';
      continue;
    }
    if (arg === '--no-sccache') {
      result.sccacheMode = 'disabled';
      continue;
    }
    if (arg.startsWith('--sccache-dir=')) {
      result.sccacheDir = arg.slice('--sccache-dir='.length);
      continue;
    }
    if (arg === '--sccache-dir') {
      result.sccacheDir = readNextValue(args, ++index, arg);
      continue;
    }
    if (arg.startsWith('--accessible-roots=')) {
      result.accessibleRoots = arg.slice('--accessible-roots='.length);
      continue;
    }
    if (arg === '--accessible-roots') {
      result.accessibleRoots = readNextValue(args, ++index, arg);
      continue;
    }
    if (arg.startsWith('--backend-name=')) {
      result.backendName = arg.slice('--backend-name='.length);
      continue;
    }
    if (arg === '--backend-name') {
      result.backendName = readNextValue(args, ++index, arg);
      continue;
    }
    if (arg.startsWith('--server-port=')) {
      result.serverPort = parsePort(arg.slice('--server-port='.length), arg);
      continue;
    }
    if (arg === '--server-port') {
      result.serverPort = parsePort(readNextValue(args, ++index, arg), arg);
      continue;
    }
    if (arg.startsWith('--server-host=')) {
      result.serverHost = arg.slice('--server-host='.length);
      continue;
    }
    if (arg === '--server-host') {
      result.serverHost = readNextValue(args, ++index, arg);
      continue;
    }
    if (arg.startsWith('--frontend-port=')) {
      result.frontendPort = parsePort(arg.slice('--frontend-port='.length), arg);
      continue;
    }
    if (arg === '--frontend-port') {
      result.frontendPort = parsePort(readNextValue(args, ++index, arg), arg);
      continue;
    }
    if (arg.startsWith('--frontend-host=')) {
      result.frontendHost = arg.slice('--frontend-host='.length);
      continue;
    }
    if (arg === '--frontend-host') {
      result.frontendHost = readNextValue(args, ++index, arg);
      continue;
    }
    if (arg.startsWith('--frontend-mode=')) {
      result.frontendMode = parseFrontendMode(arg.slice('--frontend-mode='.length), arg);
      continue;
    }
    if (arg === '--frontend-mode') {
      result.frontendMode = parseFrontendMode(readNextValue(args, ++index, arg), arg);
      continue;
    }
    if (arg.startsWith('--database-url=')) {
      result.databaseUrl = arg.slice('--database-url='.length);
      continue;
    }
    if (arg === '--database-url') {
      result.databaseUrl = readNextValue(args, ++index, arg);
      continue;
    }
    throw new Error(`不支持的参数: ${arg}`);
  }

  return result;
}

function readNextValue(args, index, flagName) {
  const value = args[index];
  if (!value) {
    throw new Error(`${flagName} 缺少取值`);
  }
  return value;
}

function parsePort(value, flagName) {
  const port = Number.parseInt(value, 10);
  if (!Number.isInteger(port) || port <= 0) {
    throw new Error(`${flagName} 不是合法端口: ${value}`);
  }
  return port;
}

function parseFrontendMode(value, flagName) {
  if (value === 'dev' || value === 'preview') {
    return value;
  }
  throw new Error(`${flagName} 不是合法前端模式: ${value}`);
}

function printHelp() {
  console.log('AgentDash 联合启动脚本（Node 版）');
  console.log('');
  console.log('用法:');
  console.log('  node ./scripts/dev-joint.js [options]');
  console.log('');
  console.log('常用参数:');
  console.log('  --skip-build              跳过 cargo build');
  console.log('  --skip-local              只启动 server + frontend');
  console.log('  --skip-server             不启动 server，复用现有服务');
  console.log('  --skip-frontend           不启动前端');
  console.log('  --no-executor             local 追加 --no-executor');
  console.log('  --sccache                 强制使用 sccache，未安装时报错');
  console.log('  --no-sccache              关闭自动 sccache 检测');
  console.log('  --sccache-dir <path>      指定 SCCACHE_DIR');
  console.log('  --accessible-roots <val>  指定 accessible roots');
  console.log('  --backend-name <val>      指定本机运行时展示名称');
  console.log('  --database-url <val>      指定 DATABASE_URL');
  console.log('  --server-host <val>       指定后端绑定 host');
  console.log('  --server-port <port>      指定 server 端口');
  console.log('  --frontend-host <val>     指定前端 host');
  console.log('  --frontend-mode <mode>    指定前端模式（dev | preview）');
  console.log('  --frontend-port <port>    指定前端端口');
}

function printBanner() {
  console.log('');
  console.log('  ╔══════════════════════════════════════╗');
  console.log('  ║   AgentDash 联合启动（保序模式）     ║');
  console.log('  ╚══════════════════════════════════════╝');
  console.log(`  root:       ${root}`);
  console.log(`  roots:      ${config.accessibleRoots}`);
  console.log(`  runtime:    ${config.backendName}`);
  console.log(`  frontend:   ${config.frontendMode}`);
  console.log(`  db:         ${formatDatabaseMode(config.databaseUrl)}`);
  console.log(`  rust cache: ${rustBuild.description}`);
  console.log('');
}

function formatDatabaseMode(value) {
  return isPostgresUrl(value) ? value : 'embedded-postgresql';
}

function configureRustBuild(options) {
  const env = { ...process.env };
  const configuredSccacheDir = normalizeOptionalValue(options.sccacheDir);
  if (configuredSccacheDir) {
    env.SCCACHE_DIR = path.isAbsolute(configuredSccacheDir)
      ? configuredSccacheDir
      : path.resolve(root, configuredSccacheDir);
  }

  if (options.sccacheMode === 'disabled') {
    delete env.RUSTC_WRAPPER;
    return {
      description: '已关闭',
      env
    };
  }

  const existingWrapper = normalizeOptionalValue(env.RUSTC_WRAPPER);
  if (existingWrapper && options.sccacheMode !== 'required') {
    return {
      description: `RUSTC_WRAPPER=${existingWrapper}${formatCacheDirSuffix(env.SCCACHE_DIR)}`,
      env
    };
  }

  const sccachePath = resolveExecutable('sccache');
  if (sccachePath) {
    env.RUSTC_WRAPPER = sccachePath;
    return {
      description: formatSccacheDescription(sccachePath, env.SCCACHE_DIR),
      env
    };
  }

  if (options.sccacheMode === 'required') {
    throw new Error('--sccache 需要先安装 sccache，并确保 sccache 在 PATH 中可执行');
  }

  return {
    description: '未检测到 sccache，已退化为普通 rustc',
    env
  };
}

function normalizeOptionalValue(value) {
  if (typeof value !== 'string') {
    return null;
  }
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : null;
}

function resolveExecutable(name) {
  const command = isWindows ? 'where.exe' : 'sh';
  const args = isWindows ? [name] : ['-lc', `command -v ${name}`];
  const result = spawnSync(command, args, {
    cwd: root,
    encoding: 'utf8',
    windowsHide: true
  });
  if (result.status !== 0) {
    return null;
  }
  const firstLine = result.stdout
    .split(/\r?\n/)
    .map((line) => line.trim())
    .find(Boolean);
  return firstLine || null;
}

function formatSccacheDescription(sccachePath, cacheDir) {
  return `sccache (${sccachePath})${formatCacheDirSuffix(cacheDir)}`;
}

function formatCacheDirSuffix(cacheDir) {
  const normalized = normalizeOptionalValue(cacheDir);
  return normalized ? `，SCCACHE_DIR=${normalized}` : '';
}

async function runStep0Cleanup() {
  console.log('[0/4] 启动前环境检测...');

  if (!config.skipServer) {
    // 先杀残留 agentdash-server — 它是 embedded postgres 的父进程，
    // 杀父进程比杀子进程更可靠，也能释放端口和文件句柄。
    const serverConflict = await detectProcessByName('agentdash-server');
    if (serverConflict) {
      console.log('  [warn] 检测到残留 agentdash-server，正在强制终止...');
      await forceKillProcessByName('agentdash-server');
      await sleep(1000);
    }

    // 再杀残留 embedded PostgreSQL 子进程
    const pgConflict = await detectEmbeddedPostgres();
    if (pgConflict) {
      console.log('  [warn] 检测到残留 embedded PostgreSQL，正在强制终止...');
      await killEmbeddedPostgres();
      // 等待 Windows 释放文件句柄，然后验证是否真的死了
      await sleep(1500);
      const stillAlive = await detectEmbeddedPostgres();
      if (stillAlive) {
        console.log('  [warn] postgres 仍存活，二次强制终止...');
        await killEmbeddedPostgres();
        await sleep(1500);
      }
    }

    // 清理 postmaster.pid 和锁文件，避免新实例启动时被卡住
    cleanupPostgresLockFiles();
  }

  if (!config.skipLocal) {
    const localConflict = await detectProcessByName('agentdash-local');
    if (localConflict) {
      console.log('  [warn] 检测到残留 agentdash-local 进程，正在终止以避免重复注册...');
      await forceKillProcessByName('agentdash-local');
    }
  } else {
    console.log('  [skip] 保留现有 agentdash-local（--skip-local）');
  }

  if (!config.skipBuild) {
    const tauriConflict = await detectProcessByName('agentdash-local-tauri');
    if (tauriConflict) {
      console.log('  [warn] 检测到残留 agentdash-local-tauri 进程，正在终止以避免锁定 debug binary...');
      await forceKillProcessByName('agentdash-local-tauri');
    }
  }

  const ports = [];
  if (!config.skipServer) {
    ports.push(config.serverPort);
  }
  if (!config.skipFrontend) {
    ports.push(5380, 5381, 5382, config.frontendPort, config.frontendPort + 1, config.frontendPort + 2);
  }

  const uniquePorts = [...new Set(ports)];

  if (uniquePorts.length === 0) {
    console.log('  [skip] 当前模式无需端口检测');
    return;
  }

  const occupiedPorts = await detectOccupiedPorts(uniquePorts);
  if (occupiedPorts.length > 0) {
    console.log(`  [warn] 端口被占用: ${occupiedPorts.join(', ')}，正在释放...`);
    await runCommand(process.execPath, [path.join(root, 'scripts', 'kill-ports.js'), ...occupiedPorts.map(String)], {
      cwd: root,
      label: 'kill-ports'
    });
  } else {
    console.log(`  [ok] 所需端口均可用: ${uniquePorts.join(', ')}`);
  }
}

async function detectProcessByName(name) {
  try {
    if (isWindows) {
      const out = execSync(
        `powershell -NoProfile -Command "(Get-Process -Name '${name}' -ErrorAction SilentlyContinue).Count"`,
        { encoding: 'utf8', timeout: 5000 }
      ).trim();
      return parseInt(out, 10) > 0;
    }
    execSync(`pgrep -f ${name}`, { timeout: 5000 });
    return true;
  } catch {
    return false;
  }
}

async function detectEmbeddedPostgres() {
  try {
    if (isWindows) {
      // 匹配 .theseus 和 .agentdash 路径；
      // CommandLine 可能用正斜杠 (/) 或反斜杠 (\)，两种都要匹配。
      const psScript = `@(Get-CimInstance Win32_Process -Filter "Name = 'postgres.exe'" -ErrorAction SilentlyContinue | Where-Object { $cl = $_.CommandLine + ' ' + $_.ExecutablePath; ($cl -match '\\.theseus[/\\\\]') -or ($cl -match '\\.agentdash[/\\\\]') }).Count`;
      const out = execSync(
        `powershell -NoProfile -Command "${psScript.replace(/"/g, '\\"')}"`,
        { encoding: 'utf8', timeout: 8000 }
      ).trim();
      return parseInt(out, 10) > 0;
    }
    execSync('pgrep -f ".theseus.*postgres"', { timeout: 5000 });
    return true;
  } catch {
    return false;
  }
}

async function detectOccupiedPorts(ports) {
  const occupied = [];
  for (const port of ports) {
    try {
      if (isWindows) {
        const out = execSync(
          `powershell -NoProfile -Command "(Get-NetTCPConnection -LocalPort ${port} -State Listen -ErrorAction SilentlyContinue).Count"`,
          { encoding: 'utf8', timeout: 5000 }
        ).trim();
        if (parseInt(out, 10) > 0) occupied.push(port);
      } else {
        execSync(`lsof -ti:${port}`, { timeout: 5000 });
        occupied.push(port);
      }
    } catch {
      // port is free
    }
  }
  return occupied;
}

/**
 * 强制杀掉按名称匹配的进程及其整个进程树。
 * Windows 上使用 Get-CimInstance + taskkill /F /T 确保子进程一并清理。
 */
async function forceKillProcessByName(name) {
  await killProcessTreeByName(name, { root, runCommand });
  console.log(`  [run] 已强制终止进程树 ${name}`);
}

async function killEmbeddedPostgres() {
  if (isWindows) {
    // 使用 Get-CimInstance + taskkill /F /T 杀掉 postgres 进程树
    // 用 -match 正则同时匹配正斜杠和反斜杠路径（CommandLine 两种都可能出现）
    const psScript = [
      `$procs = Get-CimInstance Win32_Process -Filter "Name = 'postgres.exe'" -ErrorAction SilentlyContinue`,
      `| Where-Object { $cl = $_.CommandLine + ' ' + $_.ExecutablePath; ($cl -match '\\.theseus[/\\\\]') -or ($cl -match '\\.agentdash[/\\\\]') }`,
      `; foreach ($p in $procs) { taskkill /F /T /PID $p.ProcessId 2>$null | Out-Null }`
    ].join(' ');
    await runCommand(
      'powershell',
      ['-NoProfile', '-Command', psScript],
      { cwd: root, label: 'kill-embedded-postgres', allowNonZeroExit: true }
    );
  } else {
    await runCommand('pkill', ['-9', '-f', '.theseus.*postgres'], {
      cwd: root,
      label: 'kill-embedded-postgres',
      allowNonZeroExit: true
    });
  }
  console.log('  [run] 已强制终止 embedded PostgreSQL 进程树');
}

/**
 * 清理 embedded PostgreSQL 的 postmaster.pid 和锁文件。
 * 这些文件在进程被强杀后可能残留，导致新实例启动卡住。
 */
function cleanupPostgresLockFiles() {
  const dataRoot = process.env.AGENTDASH_DATA_ROOT
    ? path.resolve(process.env.AGENTDASH_DATA_ROOT)
    : root;
  const embeddedDir = path.join(dataRoot, '.agentdash', 'embedded-postgres');

  let serviceDirs;
  try {
    serviceDirs = fs.readdirSync(embeddedDir, { withFileTypes: true })
      .filter((d) => d.isDirectory())
      .map((d) => path.join(embeddedDir, d.name, 'data'));
  } catch {
    // embedded-postgres 目录不存在，无需清理
    return;
  }

  const lockFiles = ['postmaster.pid', 'postmaster.opts'];
  let cleaned = 0;

  for (const dataDir of serviceDirs) {
    for (const lockFile of lockFiles) {
      const lockPath = path.join(dataDir, lockFile);
      try {
        fs.unlinkSync(lockPath);
        cleaned += 1;
        console.log(`  [clean] 已删除 ${path.relative(root, lockPath)}`);
      } catch {
        // 文件不存在或无权限，跳过
      }
    }
  }

  if (cleaned === 0) {
    console.log('  [ok] 无残留 PostgreSQL 锁文件');
  }
}

function startFrontendProcess() {
  const frontendEnv = {
    ...process.env,
    VITE_API_ORIGIN: `http://${config.serverHost}:${config.serverPort}`
  };
  startPnpmFilterScript(supervisor, {
    packageName: 'app-web',
    scriptName: config.frontendMode,
    scriptArgs: ['--', '--host', config.frontendHost, '--port', String(config.frontendPort), '--strictPort'],
    label: 'frontend',
    env: frontendEnv,
  });
}

async function waitForLocalRegistration(port, backendId, maxAttempts, intervalMs) {
  await sleep(500);
  for (let attempt = 1; attempt <= maxAttempts; attempt += 1) {
    const data = await fetchJson(port, '/api/backends/online');
    const matched = Array.isArray(data)
      ? data.find((item) => item && item.backend_id === backendId)
      : null;
    if (matched) {
      console.log(`  [ready] local 已注册 (backend_id=${matched.backend_id ?? 'unknown'})`);
      return;
    }
    await sleep(intervalMs);
  }
  throw new Error(`local backend 未在预期时间内完成注册 (backend_id=${backendId})`);
}

async function ensureDevLocalRuntimeClaim(port, options) {
  const profile = loadOrCreateDevRuntimeProfile();
  const backend = await requestJson(port, 'POST', '/api/local-runtime/ensure', {
    machine_id: profile.machine_id,
    machine_label: profile.machine_label,
    legacy_machine_ids: profile.legacy_machine_ids,
    profile_id: formatDevRuntimeProfileId(options),
    scope: { kind: 'user' },
    capability_slot: 'default',
    name: options.backendName,
    accessible_roots: splitAccessibleRoots(options.accessibleRoots),
    executor_enabled: !options.noExecutor,
    client_version: 'dev-joint',
    device: {
      app: 'agentdash-dev-joint',
      root,
      hostname: os.hostname(),
      platform: process.platform,
      arch: process.arch,
      pid: process.pid
    },
    rotate_token: false
  });

  if (!backend || typeof backend !== 'object' || backend.__error__) {
    const message = backend?.message || '未知错误';
    throw new Error(`领取本机运行时失败: ${message}`);
  }

  const token = typeof backend.auth_token === 'string' ? backend.auth_token.trim() : '';
  if (!token) {
    throw new Error(`本机运行时 ${backend.backend_id ?? 'unknown'} 未返回可用 auth_token`);
  }
  if (typeof backend.backend_id !== 'string' || !backend.backend_id.trim()) {
    throw new Error('本机运行时领取响应缺少 backend_id');
  }
  if (typeof backend.relay_ws_url !== 'string' || !backend.relay_ws_url.trim()) {
    throw new Error(`本机运行时 ${backend.backend_id} 未返回 relay_ws_url`);
  }

  saveDevRuntimeProfile({
    ...profile,
    machine_label: normalizeOptionalValue(backend.machine_label) || profile.machine_label
  });

  console.log(
    `  [ready] 本机运行时已领取 (backend_id=${backend.backend_id}, machine=${backend.machine_label ?? profile.machine_label})`
  );
  return backend;
}

function loadOrCreateDevRuntimeProfile() {
  const existing = readJsonFile(devRuntimeProfilePath);
  const machineId = normalizeOptionalValue(existing?.machine_id) || randomUUID();
  const machineLabel = normalizeOptionalValue(existing?.machine_label)
    || normalizeOptionalValue(os.hostname())
    || 'dev-machine';
  const legacyMachineIds = Array.isArray(existing?.legacy_machine_ids)
    ? existing.legacy_machine_ids
        .map((value) => normalizeOptionalValue(value))
        .filter((value) => value && value !== machineId)
    : [];
  const profile = {
    machine_id: machineId,
    machine_label: machineLabel,
    legacy_machine_ids: [...new Set(legacyMachineIds)]
  };

  if (JSON.stringify(existing) !== JSON.stringify(profile)) {
    saveDevRuntimeProfile(profile);
  }
  return profile;
}

function readJsonFile(filePath) {
  if (!fs.existsSync(filePath)) {
    return null;
  }
  const raw = fs.readFileSync(filePath, 'utf8');
  try {
    return JSON.parse(raw);
  } catch (error) {
    throw new Error(`${filePath} 不是合法 JSON: ${error.message}`);
  }
}

function saveDevRuntimeProfile(profile) {
  fs.mkdirSync(path.dirname(devRuntimeProfilePath), { recursive: true });
  fs.writeFileSync(
    devRuntimeProfilePath,
    `${JSON.stringify(profile, null, 2)}\n`,
    'utf8'
  );
}

function splitAccessibleRoots(value) {
  return String(value || '')
    .split(path.delimiter)
    .map((item) => item.trim())
    .filter(Boolean);
}

function formatDevRuntimeProfileId(options) {
  return `dev-joint:${options.serverHost}:${options.serverPort}`;
}
