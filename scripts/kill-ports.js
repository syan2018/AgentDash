#!/usr/bin/env node
/**
 * 端口清理脚本
 * 自动查找并终止占用指定端口的进程
 *
 * 用法:
 *   node kill-ports.js [port1] [port2] ...
 *   不传参数时默认清理: 3001, 5380, 5381, 5382
 */

import { execSync } from 'child_process';
import { platform } from 'os';

const DEFAULT_PORTS = [3001, 5380, 5381, 5382];
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
    return killPortsWindowsBatch([port])[0] ?? { found: false };
  } catch (error) {
    return { found: false, error: error.message };
  }
}

function killPortsWindowsBatch(targetPorts) {
  const portList = targetPorts.join(',');
  const psScript = `
$ports = @(${portList})
$conns = @(Get-NetTCPConnection -State Listen -ErrorAction SilentlyContinue | Where-Object { $ports -contains $_.LocalPort })
$pidToName = @{}
$allPids = @($conns | Select-Object -ExpandProperty OwningProcess -Unique)
if ($allPids.Count -gt 0) {
  Get-Process -Id $allPids -ErrorAction SilentlyContinue | ForEach-Object {
    $pidToName[[string]$_.Id] = $_.ProcessName
  }
  foreach ($pid in $allPids) {
    Stop-Process -Id $pid -Force -ErrorAction SilentlyContinue
  }
}
$results = foreach ($port in $ports) {
  $portConns = @($conns | Where-Object { $_.LocalPort -eq $port })
  if ($portConns.Count -eq 0) {
    [pscustomobject]@{
      port = $port
      found = $false
      killed = $false
      pids = @()
      names = @()
    }
    continue
  }

  $pids = @($portConns | Select-Object -ExpandProperty OwningProcess -Unique)
  [pscustomobject]@{
    port = $port
    found = $true
    killed = $true
    pids = $pids
    names = @($pids | ForEach-Object {
      if ($pidToName.ContainsKey([string]$_)) { $pidToName[[string]$_] } else { "unknown" }
    })
  }
}
$results | ConvertTo-Json -Depth 4 -Compress
`.trim();

  const command = `powershell -NoProfile -Command "${psScript.replace(/"/g, '\\"')}"`;
  const output = execSync(command, {
    encoding: 'utf8',
    stdio: ['pipe', 'pipe', 'pipe']
  }).trim();

  if (!output) {
    return targetPorts.map((port) => ({ port, found: false, killed: false, pids: [], names: [] }));
  }

  const parsed = JSON.parse(output);
  const list = Array.isArray(parsed) ? parsed : [parsed];
  return list.map((item) => ({
    port: item.port,
    found: Boolean(item.found),
    killed: Boolean(item.killed),
    pids: Array.isArray(item.pids) ? item.pids : [],
    names: Array.isArray(item.names) ? item.names : []
  }));
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
const results = isWindows ? killPortsWindowsBatch(ports) : ports.map((port) => ({ port, ...killPortUnix(port) }));

for (const [index, port] of ports.entries()) {
  const result = results[index] ?? { found: false };

  if (!result.found) {
    log(`端口 ${port} 未被占用`, 'info');
    notFoundCount++;
  } else if (result.killed) {
    const pairs = result.pids
      .map((pid, pidIndex) => `${result.names[pidIndex] || 'unknown'}(PID: ${pid})`)
      .join(', ');
    log(`端口 ${port} - 已终止进程 ${pairs}`, 'success');
    killedCount++;
  } else {
    const pairs = (result.pids || [])
      .map((pid, pidIndex) => `${result.names?.[pidIndex] || 'unknown'}(PID: ${pid})`)
      .join(', ') || 'unknown';
    log(`端口 ${port} - 终止失败 ${pairs}`, 'error');
    failedCount++;
  }
}

log('');
log(`清理完成: ${killedCount} 个已终止, ${notFoundCount} 个未占用, ${failedCount} 个失败`,
  failedCount > 0 ? 'warning' : 'success'
);

process.exit(0);
