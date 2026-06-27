import { defineConfig } from 'vite'
import type { Plugin } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'

const desktopApiOrigin = process.env.VITE_API_ORIGIN ?? 'http://127.0.0.1:17301'
const desktopDefaultsJson = process.env.AGENTDASH_DESKTOP_DEFAULTS_JSON ?? '{}'
const desktopDefaults = parseDesktopDefaults(desktopDefaultsJson)

export default defineConfig({
  plugins: [react(), tailwindcss(), desktopDefaultsAssetPlugin(desktopDefaults)],
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

interface DesktopDefaults {
  default_cloud_origin?: string
}

function parseDesktopDefaults(value: string): DesktopDefaults {
  try {
    const parsed: unknown = JSON.parse(value)
    if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) {
      return {}
    }
    const record = parsed as Record<string, unknown>
    return typeof record.default_cloud_origin === 'string' && record.default_cloud_origin.trim()
      ? { default_cloud_origin: record.default_cloud_origin.trim().replace(/\/+$/, '') }
      : {}
  } catch {
    return {}
  }
}

function desktopDefaultsAssetPlugin(defaults: DesktopDefaults): Plugin {
  return {
    name: 'agentdash-desktop-defaults',
    generateBundle() {
      this.emitFile({
        type: 'asset',
        fileName: 'agentdash-desktop-defaults.json',
        source: `${JSON.stringify(defaults, null, 2)}\n`,
      })
    },
  }
}
