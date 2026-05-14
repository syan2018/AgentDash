import http from 'node:http';
import { spawn } from 'node:child_process';

export const isWindows = process.platform === 'win32';

export function installShutdownHandlers(shutdown) {
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
}

export function createProcessSupervisor({
  root,
  shutdownMessage = '正在停止开发进程...',
  stoppedMessage = '开发进程已停止',
  afterStop = async () => {},
}) {
  const managedChildren = [];
  let shuttingDown = false;

  function startManagedProcess(command, args, label, options = {}) {
    const child = spawn(command, args, {
      cwd: options.cwd ?? root,
      env: options.env ?? process.env,
      stdio: options.stdio ?? 'inherit',
      windowsHide: options.windowsHide ?? false,
      detached: !isWindows,
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

  function hasManagedChildren() {
    return managedChildren.length > 0;
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
    console.log(`  ${shutdownMessage}`);
    for (const { child } of [...managedChildren].reverse()) {
      await stopProcessTree(child);
    }
    await afterStop();
    console.log(`  ${stoppedMessage}`);
    process.exit(exitCode);
  }

  return {
    hasManagedChildren,
    runCommand: (command, args, options = {}) => runCommand(command, args, { cwd: root, ...options }),
    shutdown,
    startManagedProcess,
    waitForAnyChildExit,
  };
}

export function runCommand(command, args, options = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, {
      cwd: options.cwd,
      env: options.env ?? process.env,
      stdio: options.stdio ?? 'inherit',
      windowsHide: options.windowsHide ?? false,
    });

    child.on('error', reject);
    child.on('exit', (code, signal) => {
      if (code === 0 || (options.allowNonZeroExit && code !== null)) {
        resolve();
        return;
      }
      reject(new Error(`${options.label ?? command} 失败 (${signal ? `signal=${signal}` : `code=${code ?? 0}`})`));
    });
  });
}

export async function killProcessTreeByName(name, { root, runCommand: runner = runCommand } = {}) {
  if (isWindows) {
    const psScript = [
      `$procs = Get-CimInstance Win32_Process -Filter "Name = '${name}.exe'" -ErrorAction SilentlyContinue`,
      'foreach ($p in $procs) { taskkill /F /T /PID $p.ProcessId 2>$null | Out-Null }',
    ].join('; ');
    await runner('powershell', ['-NoProfile', '-Command', psScript], {
      cwd: root,
      label: `kill-${name}`,
      allowNonZeroExit: true,
      windowsHide: true,
    });
    return;
  }

  await runner('pkill', ['-9', '-f', name], {
    cwd: root,
    label: `kill-${name}`,
    allowNonZeroExit: true,
  });
}

export function resolveDebugBinary(root, name) {
  return `${root}/target/debug/${isWindows ? `${name}.exe` : name}`;
}

export function startDebugBinary(
  supervisor,
  root,
  binaryName,
  { label = binaryName, args = [], env } = {},
) {
  supervisor.startManagedProcess(resolveDebugBinary(root, binaryName), args, label, { env });
}

export function isPostgresUrl(value) {
  if (!value || typeof value !== 'string') {
    return false;
  }
  const lower = value.toLowerCase();
  return lower.startsWith('postgres://') || lower.startsWith('postgresql://');
}

export function startPnpmFilterScript(
  supervisor,
  { packageName, scriptName, scriptArgs = [], label, env },
) {
  const { startManagedProcess } = supervisor;
  if (isWindows) {
    const command = ['pnpm', '--filter', packageName, scriptName, ...scriptArgs].join(' ');
    startManagedProcess('cmd.exe', ['/d', '/s', '/c', command], label, { env });
    return;
  }
  startManagedProcess('pnpm', ['--filter', packageName, scriptName, ...scriptArgs], label, { env });
}

export async function runCargoBuild(
  runner,
  { packages = [], bins = [], env, label = 'cargo build' },
) {
  const args = ['build'];
  for (const packageName of packages) {
    args.push('-p', packageName);
  }
  for (const binName of bins) {
    args.push('--bin', binName);
  }
  await runner('cargo', args, { env, label });
}

export async function runAgentDashDevRustBuild(runner, { env } = {}) {
  const packages = ['agentdash-api', 'agentdash-local', 'agentdash-local-tauri'];
  await runCargoBuild(runner, {
    packages,
    env,
    label: `cargo build ${packages.join(', ')}`,
  });
}

export async function waitForHttpReady(port, requestPath, timeoutSec, options = {}) {
  const {
    label = '',
    acceptStatus = (statusCode) => statusCode === 200,
  } = options;
  const startedAt = Date.now();
  const deadline = startedAt + timeoutSec * 1000;
  let attempt = 0;
  const name = label ? `${label} ` : '';

  while (Date.now() < deadline) {
    attempt += 1;
    const statusCode = await probeHttp(port, requestPath);
    if (acceptStatus(statusCode)) {
      const elapsed = ((Date.now() - startedAt) / 1000).toFixed(1);
      console.log(`  [ready] ${name}:${port}${requestPath} → ${statusCode} (${elapsed}s)`);
      return;
    }
    if (attempt % 10 === 0) {
      console.log(`  [wait]  ${name}:${port} 第 ${attempt} 次探测...`);
    }
    await sleep(500);
  }

  throw new Error(`${name}:${port}${requestPath} 未在 ${timeoutSec}s 内就绪`);
}

export function probeHttp(port, requestPath) {
  return new Promise((resolve) => {
    const req = http.get({
      hostname: '127.0.0.1',
      port,
      path: requestPath,
      timeout: 2000,
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

export function fetchJson(port, requestPath) {
  return requestJson(port, 'GET', requestPath);
}

export function requestJson(port, method, requestPath, payload = undefined) {
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
        'Content-Length': Buffer.byteLength(body),
      } : undefined,
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
            message: data.trim() || `HTTP ${res.statusCode ?? 500}`,
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

export function sleep(ms) {
  return new Promise((resolve) => {
    setTimeout(resolve, ms);
  });
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
        windowsHide: true,
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
