import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'

const desktopApiOrigin = process.env.VITE_API_ORIGIN ?? 'http://127.0.0.1:17301'

export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  define: {
    'import.meta.env.VITE_API_ORIGIN': JSON.stringify(desktopApiOrigin),
  },
  server: {
    host: '127.0.0.1',
    port: 5381,
    strictPort: true,
  },
})
