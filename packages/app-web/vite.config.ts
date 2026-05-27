import { defineConfig, type ProxyOptions } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'

function getProxyErrorCode(error: unknown): string | undefined {
  if (!error || typeof error !== 'object') return undefined
  const withCode = error as { code?: unknown }
  return typeof withCode.code === 'string' ? withCode.code : undefined
}

const apiProxyTarget = (process.env.VITE_API_ORIGIN ?? 'http://127.0.0.1:3001').trim()

function apiProxyConfig(): ProxyOptions {
  return {
    target: apiProxyTarget,
    changeOrigin: true,
    // NDJSON 是普通 HTTP 长连接，不需要 WebSocket 代理；ws 由后端 relay 端点单独处理
    ws: false,
    // /api 下含 NDJSON 长连接（project/session stream），统一关闭超时
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
        const message = code === 'ECONNREFUSED'
          ? `Vite proxy target unavailable: ${apiProxyTarget}`
          : 'Vite proxy error'
        if (anyRes?.end) anyRes.end(message)
        else anyRes?.destroy?.()
      })
    },
  }
}

export default defineConfig({
  plugins: [react(), tailwindcss()],
  build: {
    rollupOptions: {
      output: {
        manualChunks(id) {
          if (id.includes('node_modules')) {
            if (id.includes('react-router-dom') || id.includes('@remix-run') || id.includes('react-dom') || id.includes('/react/')) {
              return 'react-vendor'
            }
            if (id.includes('streamdown') || id.includes('@streamdown/') || id.includes('mdast') || id.includes('micromark') || id.includes('shiki') || id.includes('katex')) {
              return 'markdown-vendor'
            }
            if (id.includes('@agentclientprotocol/sdk') || id.includes('fast-json-patch')) {
              return 'acp-vendor'
            }
          }
          return undefined
        },
      },
    },
  },
  server: {
    host: '127.0.0.1',
    port: 5380,
    proxy: {
      '/api': apiProxyConfig(),
    },
  },
  preview: {
    host: '127.0.0.1',
    port: 5380,
    proxy: {
      '/api': apiProxyConfig(),
    },
  },
})
