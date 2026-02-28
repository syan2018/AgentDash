import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'

function getProxyErrorCode(error: unknown): string | undefined {
  if (!error || typeof error !== 'object') return undefined
  const withCode = error as { code?: unknown }
  return typeof withCode.code === 'string' ? withCode.code : undefined
}

export default defineConfig({
  plugins: [react(), tailwindcss()],
  server: {
    port: 5173,
    proxy: {
      // SSE 是长连接；避免 dev 代理因超时/断流导致 ECONNRESET 噪音或中断
      '/api/events/stream': {
        target: 'http://localhost:3001',
        changeOrigin: true,
        ws: false,
        timeout: 0,
        proxyTimeout: 0,
        configure: (proxy) => {
          // 用我们自己的 handler 替换掉 Vite 默认的 error 日志（ECONNRESET 在 HMR/刷新时很常见）
          proxy.removeAllListeners('error')
          proxy.on('error', (err, _req, res) => {
            const code = getProxyErrorCode(err)
            // 后端启动/编译期间 ECONNREFUSED 很常见，不应导致 dev server 退出
            if (code === 'ECONNRESET' || code === 'EPIPE' || code === 'ECONNREFUSED' || code === 'ECONNABORTED') return

            // res 在某些错误分支可能不是 Node 的 ServerResponse（例如是 Socket），需要守卫
            const anyRes = res as unknown as {
              headersSent?: boolean
              writeHead?: (statusCode: number, headers: Record<string, string>) => void
              end?: (chunk?: string) => void
              destroy?: () => void
            } | null

            if (anyRes?.writeHead && !anyRes.headersSent) {
              anyRes.writeHead(502, { 'Content-Type': 'text/plain; charset=utf-8' })
            }
            if (anyRes?.end) anyRes.end('Vite proxy error')
            else anyRes?.destroy?.()
          })
        },
      },
      '/api': {
        target: 'http://localhost:3001',
        changeOrigin: true,
        // NDJSON/SSE 是普通 HTTP 长连接，不需要 WebSocket 代理；ws 由上面的精确规则单独处理
        ws: false,
        // /api 下含 SSE/NDJSON 长连接（例如 acp session stream），统一关闭超时
        timeout: 0,
        proxyTimeout: 0,
        configure: (proxy) => {
          proxy.removeAllListeners('error')
          proxy.on('error', (err, _req, res) => {
            const code = getProxyErrorCode(err)
            // ECONNRESET/EPIPE/ECONNABORTED：HMR 刷新、页面切换、长连接断开时的正常现象
            if (
              code === 'ECONNRESET' ||
              code === 'EPIPE' ||
              code === 'ECONNREFUSED' ||
              code === 'ECONNABORTED'
            ) return

            const anyRes = res as unknown as {
              headersSent?: boolean
              writeHead?: (statusCode: number, headers: Record<string, string>) => void
              end?: (chunk?: string) => void
              destroy?: () => void
            } | null

            if (anyRes?.writeHead && !anyRes.headersSent) {
              anyRes.writeHead(502, { 'Content-Type': 'text/plain; charset=utf-8' })
            }
            if (anyRes?.end) anyRes.end('Vite proxy error')
            else anyRes?.destroy?.()
          })
        },
      },
    },
  },
})
