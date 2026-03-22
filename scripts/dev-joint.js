#!/usr/bin/env node
/**
 * AgentDash 联合启动脚本（Node 版）
 * 目标：
 * 1. 先清理遗留端口，减少重启时的干扰
 * 2. 先统一编译，再按顺序启动 server -> local -> frontend
 * 3. 统一接管 Ctrl+C，确保子进程树被一并清理
 */

import http from 'node:http';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawn } from 'node:child_process';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const root = path.resolve(__dirname, '..');
const isWindows = process.platform === 'win32';

const config = parseArgs(process.argv.slice(2));

if (config.help) {
  printHelp();
  process.exit(0);
}

const managedChildren = [];
let shuttingDown = false;

process.on('SIGINT', () => {
  shutdown(0).catch((error) => {
    console.error(error);
    process.exit(1);
  });
});

process.on('SIGTERM', () => {
  shutdown(0).catch((error) => {
    console.error(error);
    process.exit(1);
  });
});

process.on('uncaughtException', (error) => {
  console.error(error);
  shutdown(1).catch(() => process.exit(1));
});

process.on('unhandledRejection', (reason) => {
  console.error(reason);
  shutdown(1).catch(() => process.exit(1));
});

await main();

async function main() {
  printBanner();

  await runStep0KillPorts();

  if (!config.skipBuild) {
    console.log('[1/4] 构建二进制...');
    await runCommand('cargo', ['build', '--bin', 'agentdash-server', '--bin', 'agentdash-local'], {
      cwd: root,
      label: 'cargo build'
    });
    console.log('  构建完成');
  } else {
    console.log('[1/4] 跳过构建（--skip-build）');
  }

  const serverBinary = resolveBinary('agentdash-server');
  if (!config.skipServer) {
    console.log(`[2/4] 启动 agentdash-server (:${config.serverPort})...`);
    startManagedProcess(serverBinary, [], 'agentdash-server');
  } else {
    console.log(`[2/4] 跳过 agentdash-server，等待现有服务 (:${config.serverPort})...`);
  }
  await waitForHttpReady(config.serverPort, '/api/health', 120);

  if (!config.skipLocal) {
    const backend = await ensureLocalBackendConfig(
      config.serverPort,
      config.backendId,
      config.backendName
    );
    const localBinary = resolveBinary('agentdash-local');
    const localArgs = [
      '--cloud-url', `ws://127.0.0.1:${config.serverPort}/ws/backend`,
      '--token', backend.auth_token,
      '--accessible-roots', config.accessibleRoots,
      '--name', config.backendName,
      '--backend-id', config.backendId
    ];
    if (config.noExecutor) {
      localArgs.push('--no-executor');
    }

    console.log('[3/4] 启动 agentdash-local...');
    startManagedProcess(localBinary, localArgs, 'agentdash-local');
    await waitForLocalRegistration(config.serverPort, config.backendId, 20, 500);
  } else {
    console.log('[3/4] 跳过 agentdash-local（--skip-local）');
  }

  if (!config.skipFrontend) {
    console.log('[4/4] 启动前端 (:5380)...');
    startFrontendProcess();
  } else {
    console.log('[4/4] 跳过前端（--skip-frontend）');
  }

  console.log('');
  console.log('  ╔══════════════════════════════════════╗');
  console.log('  ║       所有服务已就绪                 ║');
  console.log('  ╚══════════════════════════════════════╝');
  console.log(`  API:      http://localhost:${config.serverPort}`);
  console.log('  Frontend: http://localhost:5380');
  console.log(`  WS:       ws://localhost:${config.serverPort}/ws/backend`);
  console.log('');
  console.log('  按 Ctrl+C 停止全部服务');
  console.log('');

  await waitForAnyChildExit();
  await shutdown(1);
}

function parseArgs(args) {
  const result = {
    accessibleRoots: root,
    backendId: 'local-dev-1',
    backendName: 'dev-local',
    help: false,
    noExecutor: false,
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
    if (arg.startsWith('--accessible-roots=')) {
      result.accessibleRoots = arg.slice('--accessible-roots='.length);
      continue;
    }
    if (arg === '--accessible-roots') {
      result.accessibleRoots = readNextValue(args, ++index, arg);
      continue;
    }
    if (arg.startsWith('--backend-id=')) {
      result.backendId = arg.slice('--backend-id='.length);
      continue;
    }
    if (arg === '--backend-id') {
      result.backendId = readNextValue(args, ++index, arg);
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
  console.log('  --accessible-roots <val>  指定 accessible roots');
  console.log('  --backend-id <val>        指定 backend id');
  console.log('  --backend-name <val>      指定 backend name');
  console.log('  --server-port <port>      指定 server 端口');
}

function printBanner() {
  console.log('');
  console.log('  ╔══════════════════════════════════════╗');
  console.log('  ║   AgentDash 联合启动（保序模式）     ║');
  console.log('  ╚══════════════════════════════════════╝');
  console.log(`  root:       ${root}`);
  console.log(`  roots:      ${config.accessibleRoots}`);
  console.log(`  backend_id: ${config.backendId}`);
  console.log('');
}

async function runStep0KillPorts() {
  const ports = [];
  if (!config.skipServer) {
    ports.push(config.serverPort);
  }
  if (!config.skipFrontend) {
    ports.push(5380, 5381, 5382);
  }

  if (ports.length === 0) {
    console.log('[0/4] 跳过端口预清理（当前模式无需清理）');
    return;
  }

  console.log('[0/4] 端口预清理...');
  await runCommand(process.execPath, [path.join(root, 'scripts', 'kill-ports.js'), ...ports.map(String)], {
    cwd: root,
    label: 'kill-ports'
  });
}

function resolveBinary(name) {
  return path.join(root, 'target', 'debug', isWindows ? `${name}.exe` : name);
}

function startManagedProcess(command, args, label, options = {}) {
  const child = spawn(command, args, {
    cwd: root,
    stdio: 'inherit',
    windowsHide: false,
    detached: !isWindows
  });

  child.on('exit', (code, signal) => {
    if (shuttingDown) {
      return;
    }
    const suffix = signal ? `signal=${signal}` : `code=${code ?? 0}`;
    console.log(`\n  进程 ${label} 已退出 (${suffix})`);
    shutdown(1).catch((error) => {
      console.error(error);
      process.exit(1);
    });
  });

  child.on('error', (error) => {
    if (shuttingDown) {
      return;
    }
    console.error(`启动 ${label} 失败:`, error);
    shutdown(1).catch(() => process.exit(1));
  });

  managedChildren.push({ child, label });
  return child;
}

function runCommand(command, args, options = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, {
      cwd: options.cwd ?? root,
      stdio: 'inherit',
      windowsHide: false
    });

    child.on('error', reject);
    child.on('exit', (code, signal) => {
      if (code === 0) {
        resolve();
        return;
      }
      reject(new Error(`${options.label ?? command} 失败 (${signal ? `signal=${signal}` : `code=${code ?? 0}`})`));
    });
  });
}

function startFrontendProcess() {
  if (isWindows) {
    startManagedProcess('cmd.exe', ['/d', '/s', '/c', 'pnpm --filter frontend dev'], 'frontend');
    return;
  }
  startManagedProcess('pnpm', ['--filter', 'frontend', 'dev'], 'frontend');
}

async function waitForHttpReady(port, requestPath, timeoutSec) {
  const startedAt = Date.now();
  const deadline = startedAt + timeoutSec * 1000;
  let attempt = 0;

  while (Date.now() < deadline) {
    attempt += 1;
    const statusCode = await probeHttp(port, requestPath);
    if (statusCode >= 200 && statusCode < 400) {
      const elapsed = ((Date.now() - startedAt) / 1000).toFixed(1);
      console.log(`  [ready] :${port}${requestPath} → ${statusCode} (${elapsed}s)`);
      return;
    }
    if (attempt % 10 === 0) {
      console.log(`  [wait]  :${port} 第 ${attempt} 次探测...`);
    }
    await sleep(500);
  }

  throw new Error(`:${port}${requestPath} 未在 ${timeoutSec}s 内就绪`);
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

async function ensureLocalBackendConfig(port, backendId, backendName) {
  const backend = await requestJson(port, 'POST', '/api/backends', {
    id: backendId,
    name: backendName,
    endpoint: '',
    backend_type: 'local'
  });

  if (!backend || typeof backend !== 'object' || backend.__error__) {
    const message = backend?.message || '未知错误';
    throw new Error(`确保本地 backend 失败: ${message}`);
  }

  const token = typeof backend.auth_token === 'string' ? backend.auth_token.trim() : '';
  if (!token) {
    throw new Error(`backend ${backendId} 未返回可用 auth_token`);
  }

  console.log(`  [ready] backend 已确保 (backend_id=${backendId})`);
  return backend;
}

function probeHttp(port, requestPath) {
  return new Promise((resolve) => {
    const req = http.get({
      hostname: '127.0.0.1',
      port,
      path: requestPath,
      timeout: 2000
    }, (res) => {
      const { statusCode = 0 } = res;
      res.resume();
      resolve(statusCode);
    });

    req.on('error', () => resolve(0));
    req.on('timeout', () => {
      req.destroy();
      resolve(0);
    });
  });
}

function fetchJson(port, requestPath) {
  return requestJson(port, 'GET', requestPath);
}

function requestJson(port, method, requestPath, payload = undefined) {
  return new Promise((resolve) => {
    const body = payload === undefined ? null : JSON.stringify(payload);
    const req = http.request({
      hostname: '127.0.0.1',
      port,
      method,
      path: requestPath,
      timeout: 2000,
      headers: body ? {
        'Content-Type': 'application/json',
        'Content-Length': Buffer.byteLength(body)
      } : undefined
    }, (res) => {
      let data = '';
      res.setEncoding('utf8');
      res.on('data', (chunk) => {
        data += chunk;
      });
      res.on('end', () => {
        if ((res.statusCode ?? 500) < 200 || (res.statusCode ?? 500) >= 300) {
          resolve({
            __error__: true,
            status: res.statusCode ?? 500,
            message: data.trim() || `HTTP ${res.statusCode ?? 500}`
          });
          return;
        }
        try {
          resolve(JSON.parse(data));
        } catch {
          resolve(data ? { __raw__: data } : null);
        }
      });
    });

    req.on('error', () => resolve(null));
    req.on('timeout', () => {
      req.destroy();
      resolve(null);
    });
    if (body) {
      req.write(body);
    }
    req.end();
  });
}

async function waitForAnyChildExit() {
  await new Promise((resolve) => {
    for (const { child } of managedChildren) {
      child.once('exit', resolve);
    }
  });
}

async function shutdown(exitCode) {
  if (shuttingDown) {
    return;
  }
  shuttingDown = true;

  console.log('');
  console.log('  正在停止所有服务...');
  for (const { child } of [...managedChildren].reverse()) {
    await stopProcessTree(child);
  }
  console.log('  全部已停止');
  process.exit(exitCode);
}

function stopProcessTree(child) {
  return new Promise((resolve) => {
    if (!child || child.killed || child.exitCode !== null) {
      resolve();
      return;
    }

    if (isWindows) {
      const killer = spawn('taskkill', ['/T', '/F', '/PID', String(child.pid)], {
        stdio: 'ignore',
        windowsHide: true
      });
      killer.on('exit', () => resolve());
      killer.on('error', () => resolve());
      return;
    }

    try {
      process.kill(-child.pid, 'SIGTERM');
    } catch {
      try {
        child.kill('SIGTERM');
      } catch {
        resolve();
        return;
      }
    }

    const timeout = setTimeout(() => {
      try {
        process.kill(-child.pid, 'SIGKILL');
      } catch {
        try {
          child.kill('SIGKILL');
        } catch { /* ignore */ }
      }
    }, 2000);

    child.once('exit', () => {
      clearTimeout(timeout);
      resolve();
    });
  });
}

function sleep(ms) {
  return new Promise((resolve) => {
    setTimeout(resolve, ms);
  });
}
