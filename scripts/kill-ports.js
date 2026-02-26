#!/usr/bin/env node
/**
 * 端口清理脚本
 * 自动查找并终止占用指定端口的进程
 *
 * 用法:
 *   node kill-ports.js [port1] [port2] ...
 *   不传参数时默认清理: 3001, 5173, 5174, 5175
 */

import { execSync } from 'child_process';
import { platform } from 'os';

const DEFAULT_PORTS = [3001, 5173, 5174, 5175];
const parsedPorts = process.argv.slice(2).map(Number).filter(Boolean);
const ports = parsedPorts.length > 0 ? parsedPorts : DEFAULT_PORTS;

const isWindows = platform() === 'win32';

function log(message, type = 'info') {
  const colors = {
    info: '\x1b[36m',    // 青色
    success: '\x1b[32m', // 绿色
    warning: '\x1b[33m', // 黄色
    error: '\x1b[31m',   // 红色
    reset: '\x1b[0m'
  };
  const prefix = {
    info: '[INFO]',
    success: '[OK]',
    warning: '[WARN]',
    error: '[ERR]'
  };
  console.log(`${colors[type]}${prefix[type]}${colors.reset} ${message}`);
}

function killPortWindows(port) {
  try {
    // 使用 PowerShell 查找占用端口的进程 PID
    const findCmd = `powershell -Command "& {try { $conn = Get-NetTCPConnection -LocalPort ${port} -ErrorAction Stop; $proc = Get-Process -Id $conn.OwningProcess -ErrorAction SilentlyContinue; Write-Output $($conn.OwningProcess) } catch { Write-Output '' }}"`;
    const result = execSync(findCmd, { encoding: 'utf8', stdio: ['pipe', 'pipe', 'pipe'] }).trim();

    if (!result) {
      return { found: false };
    }

    const pid = parseInt(result, 10);
    if (!pid || isNaN(pid)) {
      return { found: false };
    }

    // 获取进程信息
    let processName = 'unknown';
    try {
      const nameCmd = `powershell -Command "(Get-Process -Id ${pid} -ErrorAction SilentlyContinue).ProcessName"`;
      processName = execSync(nameCmd, { encoding: 'utf8', stdio: ['pipe', 'pipe', 'ignore'] }).trim() || 'unknown';
    } catch { /* ignore */ }

    // 终止进程
    try {
      execSync(`taskkill /F /PID ${pid} 2>nul`, { stdio: 'ignore' });
      return { found: true, killed: true, pid, name: processName };
    } catch {
      // taskkill 失败时，尝试使用 PowerShell Stop-Process 兜底
      try {
        execSync(`powershell -Command "Stop-Process -Id ${pid} -Force -ErrorAction SilentlyContinue"`, { stdio: 'ignore' });
        return { found: true, killed: true, pid, name: processName };
      } catch {
        return { found: true, killed: false, pid, name: processName };
      }
    }
  } catch (error) {
    return { found: false, error: error.message };
  }
}

function killPortUnix(port) {
  try {
    // 使用 lsof 查找占用端口的进程
    let pid = null;
    let processName = 'unknown';

    try {
      const lsofCmd = `lsof -t -i:${port} -sTCP:LISTEN 2>/dev/null || true`;
      const result = execSync(lsofCmd, { encoding: 'utf8', stdio: ['pipe', 'pipe', 'ignore'] }).trim();
      if (result) {
        pid = parseInt(result.split('\n')[0], 10);
      }
    } catch { /* ignore */ }

    if (!pid) {
      // 尝试使用 netstat 作为备选
      try {
        const netstatCmd = `netstat -tlnp 2>/dev/null | grep ':${port} ' | awk '{print $7}' | cut -d'/' -f1 | head -1`;
        const result = execSync(netstatCmd, { encoding: 'utf8', shell: '/bin/bash', stdio: ['pipe', 'pipe', 'ignore'] }).trim();
        if (result) {
          pid = parseInt(result, 10);
        }
      } catch { /* ignore */ }
    }

    if (!pid || isNaN(pid)) {
      return { found: false };
    }

    // 获取进程名
    try {
      const nameCmd = `ps -p ${pid} -o comm= 2>/dev/null || echo 'unknown'`;
      processName = execSync(nameCmd, { encoding: 'utf8', stdio: ['pipe', 'pipe', 'ignore'] }).trim() || 'unknown';
    } catch { /* ignore */ }

    // 终止进程
    try {
      process.kill(pid, 'SIGKILL');
      return { found: true, killed: true, pid, name: processName };
    } catch {
      return { found: true, killed: false, pid, name: processName };
    }
  } catch (error) {
    return { found: false, error: error.message };
  }
}

function killPort(port) {
  return isWindows ? killPortWindows(port) : killPortUnix(port);
}

// 主程序
log(`开始清理端口: ${ports.join(', ')}`);
log(`当前平台: ${isWindows ? 'Windows' : 'Unix/Linux/Mac'}`);

let killedCount = 0;
let notFoundCount = 0;
let failedCount = 0;

for (const port of ports) {
  const result = killPort(port);

  if (!result.found) {
    log(`端口 ${port} 未被占用`, 'info');
    notFoundCount++;
  } else if (result.killed) {
    log(`端口 ${port} - 已终止进程 ${result.name}(PID: ${result.pid})`, 'success');
    killedCount++;
  } else {
    log(`端口 ${port} - 终止失败 ${result.name}(PID: ${result.pid})`, 'error');
    failedCount++;
  }
}

// 额外清理：尝试杀掉可能的僵尸进程
if (isWindows) {
  try {
    // 清理 node 和 cargo 相关进程
    const cleanupCmds = [
      'taskkill /F /IM "agentdash-server.exe" 2>nul',
      'taskkill /F /FI "WINDOWTITLE eq pnpm*" 2>nul',
      'taskkill /F /FI "WINDOWTITLE eq vite*" 2>nul'
    ];
    for (const cmd of cleanupCmds) {
      try { execSync(cmd, { stdio: 'ignore' }); } catch { /* ignore */ }
    }
  } catch { /* ignore */ }
}

log('');
log(`清理完成: ${killedCount} 个已终止, ${notFoundCount} 个未占用, ${failedCount} 个失败`,
  failedCount > 0 ? 'warning' : 'success'
);

process.exit(0);
