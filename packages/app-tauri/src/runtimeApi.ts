import { invoke } from '@tauri-apps/api/core'
import type {
  LocalLogEvent,
  LocalRuntimeClient,
  LocalRuntimeStatus,
  McpLocalServerEntry,
  McpProbeResult,
  RuntimeStartRequest,
} from '@agentdash/core/local-runtime'

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown
  }
}

export function createTauriLocalRuntimeClient(): LocalRuntimeClient {
  return {
    runtimeSnapshot,
    runtimeStart,
    runtimeStop,
    runtimeRestart,
    logsTail,
    logsClear,
    mcpServersLoad,
    mcpServersSave,
    mcpServerProbe,
  }
}

async function runtimeSnapshot(): Promise<LocalRuntimeStatus | null> {
  if (!isTauriHost()) return null
  return invoke('runtime_snapshot')
}

async function runtimeStart(request: RuntimeStartRequest): Promise<LocalRuntimeStatus> {
  ensureTauriHost()
  return invoke('runtime_start', { request })
}

async function runtimeStop(): Promise<void> {
  ensureTauriHost()
  return invoke('runtime_stop')
}

async function runtimeRestart(): Promise<LocalRuntimeStatus> {
  ensureTauriHost()
  return invoke('runtime_restart')
}

async function logsTail(limit = 200): Promise<LocalLogEvent[]> {
  if (!isTauriHost()) return []
  return invoke('logs_tail', { limit })
}

async function logsClear(): Promise<void> {
  ensureTauriHost()
  return invoke('logs_clear')
}

async function mcpServersLoad(root: string): Promise<McpLocalServerEntry[]> {
  ensureTauriHost()
  return invoke('mcp_servers_load', { root })
}

async function mcpServersSave(root: string, servers: McpLocalServerEntry[]): Promise<void> {
  ensureTauriHost()
  return invoke('mcp_servers_save', { root, servers })
}

async function mcpServerProbe(server: McpLocalServerEntry): Promise<McpProbeResult> {
  ensureTauriHost()
  return invoke('mcp_server_probe', { server })
}

function isTauriHost() {
  return typeof window !== 'undefined' && window.__TAURI_INTERNALS__ !== undefined
}

function ensureTauriHost() {
  if (!isTauriHost()) {
    throw new Error('当前页面未运行在 Tauri 宿主中')
  }
}
