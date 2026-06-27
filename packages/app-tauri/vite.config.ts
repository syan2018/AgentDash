import { defineConfig } from 'vite'
import type { Plugin } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'

const desktopApiOrigin = process.env.VITE_API_ORIGIN ?? process.env.AGENTDASH_DEFAULT_CLOUD_ORIGIN ?? ''
const desktopDefaultsJson = process.env.AGENTDASH_DESKTOP_DEFAULTS_JSON ?? '{}'
const desktopDefaults = withDefaultCloudOrigin(
  parseDesktopDefaults(desktopDefaultsJson),
  process.env.AGENTDASH_DEFAULT_CLOUD_ORIGIN,
)

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

function withDefaultCloudOrigin(defaults: DesktopDefaults, value: string | undefined): DesktopDefaults {
  if (defaults.default_cloud_origin || !value?.trim()) {
    return defaults
  }
  return {
    ...defaults,
    default_cloud_origin: value.trim().replace(/\/+$/, ''),
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
