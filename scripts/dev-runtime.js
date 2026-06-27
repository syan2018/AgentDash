#!/usr/bin/env node
/**
 * AgentDash 统一开发启动入口。
 *
 * 运行形态由 profile 决定：
 * - web：启动 Dashboard API、本机 runtime、app-web
 * - desktop：启动 Dashboard API、app-tauri、Tauri 桌面壳
 */

import fs from 'node:fs';
import path from 'node:path';
import { execSync, spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
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

const WEB_FRONTEND_PORT = 5380;
const DESKTOP_FRONTEND_PORT = 5381;
const DESKTOP_PREVIEW_PORT = 5382;
const DEFAULT_SERVER_PORT = 3001;

const config = applyProfileDefaults(parseArgs(process.argv.slice(2)));

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
  shutdownMessage: `正在停止 AgentDash ${profileLabel(config.profile)} 开发进程...`,
  stoppedMessage: `AgentDash ${profileLabel(config.profile)} 开发进程已停止`,
  afterStop: async () => {
    await killEmbeddedPostgres().catch(() => {});
  },
});
const {
  hasManagedChildren,
  runCommand,
  shutdown,
  waitForAnyChildExit,
} = supervisor;

installShutdownHandlers(shutdown);

await main();

async function main() {
  printBanner();

  if (!config.skipClean) {
    await runCleanup();
  } else {
    console.log('[0] 跳过清理（--skip-clean）');
  }

  if (!config.skipBuild) {
    console.log('[1] 构建 dev Rust 目标...');
    await runAgentDashDevRustBuild(runCommand, { env: rustBuild.env });
    console.log('  Rust 目标构建完成');
  } else {
    console.log('[1] 跳过 Rust 构建（--skip-build）');
  }

  if (!config.skipServer) {
    console.log(`[2] 启动 agentdash-server (:${config.serverPort})...`);
    startAgentDashServer();
  } else {
    console.log(`[2] 跳过 agentdash-server，复用现有服务 (:${config.serverPort})...`);
  }
  await waitForHttpReady(config.serverPort, '/api/health', 120, {
    label: 'agentdash-server',
  });

  if (config.profile === 'web') {
    await maybeStartLocalRuntime();
  }

  if (!config.skipFrontend) {
    console.log(`[${config.profile === 'web' ? 4 : 3}] 启动前端 ${config.frontendPackage} (:${config.frontendPort})...`);
    startFrontendProcess();
  } else {
    console.log(`[${config.profile === 'web' ? 4 : 3}] 跳过前端（--skip-frontend）`);
  }

  if (config.profile === 'desktop') {
    await waitForHttpReady(config.frontendPort, '/', 60, {
      label: 'desktop frontend',
      acceptStatus: (statusCode) => statusCode >= 200 && statusCode < 500,
    });
    if (!config.skipShell) {
      console.log('[4] 启动 Tauri 桌面壳 agentdash-local-tauri...');
      startDesktopShell();
    } else {
      console.log('[4] 跳过 Tauri 桌面壳（--skip-shell）');
    }
  }

  printReady();

  if (!hasManagedChildren()) {
    return;
  }
  await waitForAnyChildExit();
  await shutdown(1);
}

async function maybeStartLocalRuntime() {
  if (config.skipLocal) {
    console.log('[3] 跳过 agentdash-local（--skip-local）');
    return;
  }

  const backend = await ensureDevLocalRuntimeClaim(config.serverPort, config);
  const localArgs = [
    'run',
    '--relay-ws-url', backend.relay_ws_url,
    '--auth-token', backend.auth_token,
    '--runner-name', backend.name || config.backendName,
    '--backend-id', backend.backend_id,
  ];
  const workspaceRoots = splitWorkspaceRoots(config.workspaceRoots);
  if (workspaceRoots.length > 0) {
    localArgs.push('--workspace-root', workspaceRoots.join(','));
  }
  if (config.noExecutor) {
    localArgs.push('--no-executor');
  }

  console.log('[3] 启动 agentdash-local...');
  startDebugBinary(supervisor, root, 'agentdash-local', {
    args: localArgs,
    label: 'agentdash-local',
  });
  await waitForLocalRegistration(config.serverPort, backend.backend_id, 20, 500);
}

function parseArgs(args) {
  const result = {
    backendName: 'dev-local',
    databaseUrl: process.env.DATABASE_URL || null,
    frontendHost: '127.0.0.1',
    frontendMode: 'dev',
    frontendPort: null,
    help: false,
    noExecutor: false,
    profile: 'web',
    sccacheDir: process.env.SCCACHE_DIR || null,
    sccacheMode: 'auto',
    serverHost: '127.0.0.1',
    serverPort: DEFAULT_SERVER_PORT,
    skipBuild: false,
    skipClean: false,
    skipFrontend: false,
    skipLocal: false,
    skipServer: false,
    skipShell: false,
    workspaceRoots: '',
  };

  for (let index = 0; index < args.length; index += 1) {
    const arg = args[index];
    if (arg === 'web' || arg === 'desktop') {
      result.profile = arg;
      continue;
    }
    if (arg === '--profile') {
      result.profile = parseProfile(readNextValue(args, ++index, arg), arg);
      continue;
    }
    if (arg.startsWith('--profile=')) {
      result.profile = parseProfile(arg.slice('--profile='.length), arg);
      continue;
    }
    if (arg === '--help' || arg === '-h') {
      result.help = true;
      continue;
    }
    if (arg === '--skip-build') {
      result.skipBuild = true;
      continue;
    }
    if (arg === '--skip-clean') {
      result.skipClean = true;
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
    if (arg === '--skip-shell') {
      result.skipShell = true;
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
    if (arg.startsWith('--workspace-roots=')) {
      result.workspaceRoots = arg.slice('--workspace-roots='.length);
      continue;
    }
    if (arg === '--workspace-roots') {
      result.workspaceRoots = readNextValue(args, ++index, arg);
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

function applyProfileDefaults(options) {
  const config = { ...options };
  if (config.profile === 'web') {
    config.frontendPackage = 'app-web';
    config.frontendPort ??= WEB_FRONTEND_PORT;
    return config;
  }
  if (config.profile === 'desktop') {
    config.frontendPackage = 'app-tauri';
    config.frontendMode = 'dev';
    config.frontendPort ??= DESKTOP_FRONTEND_PORT;
    config.skipLocal = true;
    return config;
  }
  throw new Error(`不支持的 profile: ${config.profile}`);
}

function readNextValue(args, index, flagName) {
  const value = args[index];
  if (!value) {
    throw new Error(`${flagName} 缺少取值`);
  }
  return value;
}

function parseProfile(value, flagName) {
  if (value === 'web' || value === 'desktop') {
    return value;
  }
  throw new Error(`${flagName} 不是合法 profile: ${value}`);
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
  console.log('AgentDash 统一开发启动脚本');
  console.log('');
  console.log('用法:');
  console.log('  node ./scripts/dev-runtime.js --profile web [options]');
  console.log('  node ./scripts/dev-runtime.js --profile desktop [options]');
  console.log('');
  console.log('常用参数:');
  console.log('  --profile <web|desktop>   指定启动形态');
  console.log('  --skip-clean              不清理端口和残留进程');
  console.log('  --skip-build              跳过 cargo build');
  console.log('  --skip-local              web profile 不启动 agentdash-local');
  console.log('  --skip-server             不启动 server，复用现有服务');
  console.log('  --skip-frontend           不启动前端');
  console.log('  --skip-shell              desktop profile 不启动 Tauri 壳');
  console.log('  --no-executor             local 追加 --no-executor');
  console.log('  --sccache                 强制使用 sccache，未安装时报错');
  console.log('  --no-sccache              关闭自动 sccache 检测');
  console.log('  --sccache-dir <path>      指定 SCCACHE_DIR');
  console.log('  --workspace-roots <val>   指定 workspace roots');
  console.log('  --backend-name <val>      指定本机 runtime 展示名称');
  console.log('  --database-url <val>      指定 PostgreSQL DATABASE_URL');
  console.log('  --server-host <val>       指定后端绑定 host');
  console.log('  --server-port <port>      指定 server 端口');
  console.log('  --frontend-host <val>     指定前端 host');
  console.log('  --frontend-mode <mode>    web 前端模式（dev | preview）');
  console.log('  --frontend-port <port>    指定前端端口');
}

function printBanner() {
  console.log('');
  console.log('  ╔══════════════════════════════════════╗');
  console.log(`  ║   AgentDash ${profileLabel(config.profile)}开发启动${bannerPadding(config.profile)}║`);
  console.log('  ╚══════════════════════════════════════╝');
  console.log(`  root:       ${root}`);
  console.log(`  profile:    ${config.profile}`);
  console.log(`  frontend:   ${config.frontendPackage} (${config.frontendMode}, :${config.frontendPort})`);
  console.log(`  server:     ${config.serverHost}:${config.serverPort}`);
  if (config.profile === 'web') {
    console.log(`  runtime:    ${config.skipLocal ? '(跳过)' : config.backendName}`);
    console.log(`  roots:      ${config.workspaceRoots || '(未显式配置)'}`);
  }
  console.log(`  db:         ${formatDatabaseMode(config.databaseUrl)}`);
  console.log(`  rust cache: ${rustBuild.description}`);
  console.log('');
}

function bannerPadding(profile) {
  return profile === 'desktop' ? '        ' : '            ';
}

function profileLabel(profile) {
  return profile === 'desktop' ? '桌面端 ' : 'Web ';
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
      env,
    };
  }

  const existingWrapper = normalizeOptionalValue(env.RUSTC_WRAPPER);
  if (existingWrapper && options.sccacheMode !== 'required') {
    return {
      description: `RUSTC_WRAPPER=${existingWrapper}${formatCacheDirSuffix(env.SCCACHE_DIR)}`,
      env,
    };
  }

  const sccachePath = resolveExecutable('sccache');
  if (sccachePath) {
    env.RUSTC_WRAPPER = sccachePath;
    return {
      description: formatSccacheDescription(sccachePath, env.SCCACHE_DIR),
      env,
    };
  }

  if (options.sccacheMode === 'required') {
    throw new Error('--sccache 需要先安装 sccache，并确保 sccache 在 PATH 中可执行');
  }

  return {
    description: '未检测到 sccache，已退化为普通 rustc',
    env,
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
    windowsHide: true,
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

async function runCleanup() {
  console.log('[0] 启动前环境检测...');

  if (!config.skipServer) {
    const serverConflict = await detectProcessByName('agentdash-server');
    if (serverConflict) {
      console.log('  [warn] 检测到残留 agentdash-server，正在强制终止...');
      await forceKillProcessByName('agentdash-server');
      await sleep(1000);
    }

    const pgConflict = await detectEmbeddedPostgres();
    if (pgConflict) {
      console.log('  [warn] 检测到残留 embedded PostgreSQL，正在强制终止...');
      await killEmbeddedPostgres();
      await sleep(1500);
      const stillAlive = await detectEmbeddedPostgres();
      if (stillAlive) {
        console.log('  [warn] postgres 仍存活，二次强制终止...');
        await killEmbeddedPostgres();
        await sleep(1500);
      }
    }

    cleanupPostgresLockFiles();
  }

  if (config.profile === 'web' && !config.skipLocal) {
    const localConflict = await detectProcessByName('agentdash-local');
    if (localConflict) {
      console.log('  [warn] 检测到残留 agentdash-local 进程，正在终止以避免重复注册...');
      await forceKillProcessByName('agentdash-local');
    }
  }

  if (!config.skipBuild || (config.profile === 'desktop' && !config.skipShell)) {
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
    ports.push(WEB_FRONTEND_PORT, DESKTOP_FRONTEND_PORT, DESKTOP_PREVIEW_PORT, config.frontendPort);
    if (config.profile === 'web') {
      ports.push(config.frontendPort + 1, config.frontendPort + 2);
    }
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
      label: 'kill-ports',
      allowNonZeroExit: true,
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
        { encoding: 'utf8', timeout: 5000 },
      ).trim();
      return Number.parseInt(out, 10) > 0;
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
      const psScript = `@(Get-CimInstance Win32_Process -Filter "Name = 'postgres.exe'" -ErrorAction SilentlyContinue | Where-Object { $cl = $_.CommandLine + ' ' + $_.ExecutablePath; ($cl -match '\\.theseus[/\\\\]') -or ($cl -match '\\.agentdash[/\\\\]') }).Count`;
      const out = execSync(
        `powershell -NoProfile -Command "${psScript.replace(/"/g, '\\"')}"`,
        { encoding: 'utf8', timeout: 8000 },
      ).trim();
      return Number.parseInt(out, 10) > 0;
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
          { encoding: 'utf8', timeout: 5000 },
        ).trim();
        if (Number.parseInt(out, 10) > 0) {
          occupied.push(port);
        }
      } else {
        execSync(`lsof -ti:${port}`, { timeout: 5000 });
        occupied.push(port);
      }
    } catch {
      // 端口空闲
    }
  }
  return occupied;
}

async function forceKillProcessByName(name) {
  await killProcessTreeByName(name, { root, runCommand });
  console.log(`  [run] 已强制终止进程树 ${name}`);
}

async function killEmbeddedPostgres() {
  if (isWindows) {
    const psScript = [
      `$procs = Get-CimInstance Win32_Process -Filter "Name = 'postgres.exe'" -ErrorAction SilentlyContinue`,
      `| Where-Object { $cl = $_.CommandLine + ' ' + $_.ExecutablePath; ($cl -match '\\.theseus[/\\\\]') -or ($cl -match '\\.agentdash[/\\\\]') }`,
      `; foreach ($p in $procs) { taskkill /F /T /PID $p.ProcessId 2>$null | Out-Null }`,
    ].join(' ');
    await runCommand('powershell', ['-NoProfile', '-Command', psScript], {
      cwd: root,
      label: 'kill-embedded-postgres',
      allowNonZeroExit: true,
      windowsHide: true,
    });
  } else {
    await runCommand('pkill', ['-9', '-f', '.theseus.*postgres'], {
      cwd: root,
      label: 'kill-embedded-postgres',
      allowNonZeroExit: true,
    });
  }
  console.log('  [run] 已强制终止 embedded PostgreSQL 进程树');
}

function cleanupPostgresLockFiles() {
  const dataRoot = process.env.AGENTDASH_DATA_ROOT
    ? path.resolve(process.env.AGENTDASH_DATA_ROOT)
    : root;
  const embeddedDir = path.join(dataRoot, '.agentdash', 'embedded-postgres');

  let serviceDirs;
  try {
    serviceDirs = fs.readdirSync(embeddedDir, { withFileTypes: true })
      .filter((entry) => entry.isDirectory())
      .map((entry) => path.join(embeddedDir, entry.name, 'data'));
  } catch {
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

function startAgentDashServer() {
  const env = {
    ...process.env,
    HOST: config.serverHost,
    PORT: String(config.serverPort),
  };
  if (isPostgresUrl(config.databaseUrl)) {
    env.DATABASE_URL = config.databaseUrl;
  } else {
    delete env.DATABASE_URL;
  }
  startDebugBinary(supervisor, root, 'agentdash-server', { env });
}

function startFrontendProcess() {
  const env = {
    ...process.env,
    VITE_API_ORIGIN: serverOrigin(),
  };
  const scriptArgs = config.profile === 'web'
    ? ['--host', config.frontendHost, '--port', String(config.frontendPort), '--strictPort']
    : ['--', '--host', config.frontendHost, '--port', String(config.frontendPort), '--strictPort'];
  startPnpmFilterScript(supervisor, {
    packageName: config.frontendPackage,
    scriptName: config.frontendMode,
    scriptArgs,
    label: config.profile === 'desktop' ? 'desktop-frontend' : 'frontend',
    env,
  });
}

function startDesktopShell() {
  const env = {
    ...process.env,
    AGENTDASH_DESKTOP_API_MODE: 'external',
    AGENTDASH_DESKTOP_API_ORIGIN: serverOrigin(),
  };
  startDebugBinary(supervisor, root, 'agentdash-local-tauri', { env });
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
  const profile = loadLocalMachineIdentity();
  const backend = await requestJson(port, 'POST', '/api/local-runtime/ensure', {
    machine_id: profile.machine_id,
    machine_label: profile.machine_label,
    profile_id: formatDevRuntimeProfileId(options),
    scope: { kind: 'user' },
    capability_slot: 'default',
    name: options.backendName,
    workspace_roots: splitWorkspaceRoots(options.workspaceRoots),
    executor_enabled: !options.noExecutor,
    client_version: 'dev-runtime',
    device: {
      app: 'agentdash-dev-runtime',
      root,
      profile: options.profile,
      platform: process.platform,
      arch: process.arch,
      pid: process.pid,
    },
    rotate_token: false,
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

  console.log(
    `  [ready] 本机运行时已领取 (backend_id=${backend.backend_id}, machine=${backend.machine_label ?? profile.machine_label})`,
  );
  return backend;
}

function loadLocalMachineIdentity() {
  const result = spawnSync(localBinaryPath(), ['machine-identity'], {
    cwd: root,
    encoding: 'utf8',
    windowsHide: true,
  });
  if (result.status !== 0) {
    const message = result.stderr.trim() || result.stdout.trim() || `exit=${result.status}`;
    throw new Error(`读取 agentdash-local 机器身份失败: ${message}`);
  }

  try {
    const identity = JSON.parse(result.stdout);
    if (!normalizeOptionalValue(identity?.machine_id)) {
      throw new Error('缺少 machine_id');
    }
    if (!normalizeOptionalValue(identity?.machine_label)) {
      throw new Error('缺少 machine_label');
    }
    return {
      machine_id: identity.machine_id.trim(),
      machine_label: identity.machine_label.trim(),
    };
  } catch (error) {
    throw new Error(`agentdash-local machine-identity 输出不是合法身份 JSON: ${error.message}`);
  }
}

function localBinaryPath() {
  return path.join(root, 'target', 'debug', isWindows ? 'agentdash-local.exe' : 'agentdash-local');
}

function splitWorkspaceRoots(value) {
  return String(value || '')
    .split(new RegExp(`[${escapeRegExp(path.delimiter)},]`))
    .map((item) => item.trim())
    .filter(Boolean);
}

function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

function formatDevRuntimeProfileId(options) {
  return `dev-runtime:${options.profile}:${options.serverHost}:${options.serverPort}`;
}

function serverOrigin() {
  return `http://${config.serverHost}:${config.serverPort}`;
}

function printReady() {
  console.log('');
  console.log('  ╔══════════════════════════════════════╗');
  console.log(`  ║   AgentDash ${profileLabel(config.profile)}开发环境已就绪${readyPadding(config.profile)}║`);
  console.log('  ╚══════════════════════════════════════╝');
  console.log(`  API:      ${serverOrigin()}`);
  console.log(`  Frontend: http://${config.frontendHost}:${config.frontendPort}`);
  if (config.profile === 'web') {
    console.log(`  WS:       ws://${config.serverHost}:${config.serverPort}/ws/backend`);
  }
  console.log('');
  console.log('  按 Ctrl+C 停止全部服务');
  console.log('');
}

function readyPadding(profile) {
  return profile === 'desktop' ? '      ' : '          ';
}
