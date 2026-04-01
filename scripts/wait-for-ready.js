#!/usr/bin/env node
/**
 * 等待指定端口的 HTTP 服务就绪后退出
 *
 * 用法:
 *   node wait-for-ready.js <port> [path] [timeout_sec]
 *
 * 示例:
 *   node wait-for-ready.js 3001              # 等待 :3001/api/health 返回 200
 *   node wait-for-ready.js 3001 /api/health 30
 */
import http from 'node:http';

const port = parseInt(process.argv[2] || '3001', 10);
const path = process.argv[3] || '/api/health';
const timeoutSec = parseInt(process.argv[4] || '60', 10);

const intervalMs = 500;
const maxAttempts = Math.ceil((timeoutSec * 1000) / intervalMs);
let attempt = 0;
const startTime = Date.now();

function probe() {
  attempt++;
  const req = http.get({ hostname: '127.0.0.1', port, path, timeout: 2000 }, (res) => {
    if (res.statusCode === 200) {
      const elapsed = ((Date.now() - startTime) / 1000).toFixed(1);
      console.log(`[ready] :${port}${path} → ${res.statusCode} (${elapsed}s)`);
      process.exit(0);
    }
    res.resume();
    retry();
  });
  req.on('error', () => retry());
  req.on('timeout', () => { req.destroy(); retry(); });
}

function retry() {
  if (attempt >= maxAttempts) {
    const elapsed = ((Date.now() - startTime) / 1000).toFixed(1);
    console.error(`[timeout] :${port}${path} 未就绪 (${elapsed}s)`);
    process.exit(1);
  }
  if (attempt % 10 === 0) {
    console.log(`[wait] :${port} 第 ${attempt} 次探测...`);
  }
  setTimeout(probe, intervalMs);
}

console.log(`[wait] 等待 :${port}${path} 就绪 (超时 ${timeoutSec}s)...`);
probe();
