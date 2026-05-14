#!/usr/bin/env node
/**
 * AgentDash 桌面端联合启动脚本。
 *
 * 启动顺序：
 * 1. 清理桌面端相关端口和残留进程
 * 2. 先统一编译本轮需要的 Rust 目标
 * 3. 启动独立 agentdash-server，便于开发期调试后端日志与断点
 * 4. 启动 app-tauri Vite dev server
 * 5. 启动 agentdash-local-tauri 桌面壳，并复用外部 agentdash-server
 */

import path from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  createProcessSupervisor,
  installShutdownHandlers,
  isPostgresUrl,
  killProcessTreeByName,
  runAgentDashDevRustBuild,
  startDebugBinary,
  startPnpmFilterScript,
  waitForHttpReady,
} from './lib/dev-process.js';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const root = path.resolve(__dirname, '..');

const DESKTOP_FRONTEND_PORT = 5381;
const DESKTOP_PREVIEW_PORT = 5382;
const DESKTOP_API_PORT = 3001;
const DESKTOP_API_ORIGIN = `http://127.0.0.1:${DESKTOP_API_PORT}`;

const config = parseArgs(process.argv.slice(2));
const supervisor = createProcessSupervisor({
  root,
  shutdownMessage: '正在停止桌面端开发进程...',
  stoppedMessage: '桌面端开发进程已停止',
});
const {
  hasManagedChildren,
  runCommand,
  shutdown,
  waitForAnyChildExit,
} = supervisor;

installShutdownHandlers(shutdown);

if (config.help) {
  printHelp();
  process.exit(0);
}

await main();

async function main() {
  printBanner();

  if (!config.skipClean) {
    await cleanupDesktopEnvironment();
  } else {
    console.log('[0/5] 跳过清理（--skip-clean）');
  }

  if (!config.skipBuild) {
    await buildRustTargets();
  } else {
    console.log('[1/5] 跳过 Rust 构建（--skip-build）');
  }

  if (!config.skipServer) {
    console.log(`[2/5] 启动 agentdash-server (:${DESKTOP_API_PORT})...`);
    startAgentDashServer();
  } else {
    console.log(`[2/5] 跳过 agentdash-server，复用现有服务 (:${DESKTOP_API_PORT})...`);
  }
  await waitForHttpReady(DESKTOP_API_PORT, '/api/health', 120, {
    label: 'agentdash-server',
  });

  if (!config.skipFrontend) {
    console.log(`[3/5] 启动桌面前端 app-tauri (:${DESKTOP_FRONTEND_PORT})...`);
    startDesktopFrontend();
  } else {
    console.log(`[3/5] 跳过桌面前端，复用现有服务 (:${DESKTOP_FRONTEND_PORT})...`);
  }
  await waitForHttpReady(DESKTOP_FRONTEND_PORT, '/', 60, {
    label: 'desktop frontend',
    acceptStatus: (statusCode) => statusCode >= 200 && statusCode < 500,
  });

  if (!config.skipShell) {
    console.log('[4/5] 启动 Tauri 桌面壳 agentdash-local-tauri...');
    startDesktopShell();
  } else {
    console.log('[4/5] 跳过 Tauri 桌面壳（--skip-shell）');
  }

  console.log('');
  console.log('  ╔══════════════════════════════════════╗');
  console.log('  ║       桌面端开发环境已就绪           ║');
  console.log('  ╚══════════════════════════════════════╝');
  console.log(`  Desktop UI:  http://127.0.0.1:${DESKTOP_FRONTEND_PORT}`);
  console.log(`  Server API:  ${DESKTOP_API_ORIGIN}`);
  console.log('');
  console.log('  按 Ctrl+C 停止 agentdash-server、桌面前端和 Tauri 壳');
  console.log('');

  if (!hasManagedChildren()) {
    return;
  }
  await waitForAnyChildExit();
  await shutdown(1);
}

function parseArgs(args) {
  const result = {
    help: false,
    skipBuild: false,
    skipClean: false,
    skipFrontend: false,
    skipServer: false,
    skipShell: false,
  };

  for (const arg of args) {
    if (arg === '--help' || arg === '-h') {
      result.help = true;
      continue;
    }
    if (arg === '--skip-clean') {
      result.skipClean = true;
      continue;
    }
    if (arg === '--skip-build') {
      result.skipBuild = true;
      continue;
    }
    if (arg === '--skip-frontend') {
      result.skipFrontend = true;
      continue;
    }
    if (arg === '--skip-server') {
      result.skipServer = true;
      continue;
    }
    if (arg === '--skip-shell') {
      result.skipShell = true;
      continue;
    }
    throw new Error(`不支持的参数: ${arg}`);
  }

  return result;
}

function printHelp() {
  console.log('AgentDash 桌面端联合启动脚本');
  console.log('');
  console.log('用法:');
  console.log('  pnpm dev:desktop [options]');
  console.log('');
  console.log('选项:');
  console.log('  --skip-clean      不清理端口和残留 server / Tauri 壳进程');
  console.log('  --skip-build      跳过 Rust 构建，直接启动已有 binary');
  console.log('  --skip-server     不启动 agentdash-server，复用现有 :3001');
  console.log('  --skip-frontend   不启动 app-tauri Vite，复用现有 :5381');
  console.log('  --skip-shell      不启动 Tauri 壳，只启动/检查桌面前端');
  console.log('  --help, -h        显示帮助');
}

function printBanner() {
  console.log('');
  console.log('  ╔══════════════════════════════════════╗');
  console.log('  ║   AgentDash 桌面端联合启动           ║');
  console.log('  ╚══════════════════════════════════════╝');
  console.log(`  root:         ${root}`);
  console.log(`  frontend:     127.0.0.1:${DESKTOP_FRONTEND_PORT}`);
  console.log(`  server api:   127.0.0.1:${DESKTOP_API_PORT}`);
  console.log('');
}

async function cleanupDesktopEnvironment() {
  console.log('[0/5] 清理桌面端开发环境...');
  await killProcessTreeByName('agentdash-server', { root, runCommand });
  await killProcessTreeByName('agentdash-local-tauri', { root, runCommand });
  await runCommand(process.execPath, [
    path.join(root, 'scripts', 'kill-ports.js'),
    String(DESKTOP_API_PORT),
    String(DESKTOP_FRONTEND_PORT),
    String(DESKTOP_PREVIEW_PORT),
  ], {
    cwd: root,
    label: 'kill desktop ports',
    allowNonZeroExit: true,
  });
}

async function buildRustTargets() {
  console.log('[1/5] 构建 dev Rust 目标...');
  await runAgentDashDevRustBuild(runCommand);
  console.log('  Rust 目标构建完成');
}

function startAgentDashServer() {
  const env = {
    ...process.env,
    HOST: '127.0.0.1',
    PORT: String(DESKTOP_API_PORT),
  };
  if (!isPostgresUrl(env.DATABASE_URL)) {
    delete env.DATABASE_URL;
  }

  startDebugBinary(supervisor, root, 'agentdash-server', { env });
}

function startDesktopFrontend() {
  const env = {
    ...process.env,
    VITE_API_ORIGIN: DESKTOP_API_ORIGIN,
  };
  startPnpmFilterScript(supervisor, {
    packageName: 'app-tauri',
    scriptName: 'dev',
    label: 'desktop-frontend',
    env,
  });
}

function startDesktopShell() {
  const env = {
    ...process.env,
    AGENTDASH_DESKTOP_API_MODE: 'external',
    AGENTDASH_DESKTOP_API_ORIGIN: DESKTOP_API_ORIGIN,
  };
  startDebugBinary(supervisor, root, 'agentdash-local-tauri', { env });
}
