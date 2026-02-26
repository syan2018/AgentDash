import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'

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
            const code = (err as any)?.code as string | undefined
            if (code === 'ECONNRESET' || code === 'EPIPE') return

            if (res && !res.headersSent) {
              res.writeHead(502, { 'Content-Type': 'text/plain; charset=utf-8' })
            }
            res?.end('Vite proxy error')
          })
        },
      },
      '/api': {
        target: 'http://localhost:3001',
        changeOrigin: true,
        ws: true,
        configure: (proxy) => {
          proxy.removeAllListeners('error')
          proxy.on('error', (err, _req, res) => {
            const code = (err as any)?.code as string | undefined
            if (code === 'ECONNRESET' || code === 'EPIPE') return

            if (res && !res.headersSent) {
              res.writeHead(502, { 'Content-Type': 'text/plain; charset=utf-8' })
            }
            res?.end('Vite proxy error')
          })
        },
      },
    },
  },
})
