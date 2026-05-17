import { invoke } from '@tauri-apps/api/core'
import type {
  LocalLogEvent,
  LocalRuntimeClient,
  LocalRuntimeProfile,
  LocalRuntimeStatus,
  McpLocalServerEntry,
  McpProbeResult,
  RuntimeStartRequest,
} from '@agentdash/core/local-runtime'
import type { BrowseDirectoryResult } from '@agentdash/views/directory-browser'

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown
  }
}

export function createTauriLocalRuntimeClient(): LocalRuntimeClient {
  return {
    profileLoad,
    profileSave,
    profileDelete,
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

async function profileLoad(): Promise<LocalRuntimeProfile | null> {
  if (!isTauriHost()) return null
  return invoke('profile_load')
}

async function profileSave(profile: LocalRuntimeProfile): Promise<LocalRuntimeProfile> {
  ensureTauriHost()
  return invoke('profile_save', { profile })
}

async function profileDelete(): Promise<void> {
  ensureTauriHost()
  return invoke('profile_delete')
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

async function mcpServersLoad(): Promise<McpLocalServerEntry[]> {
  ensureTauriHost()
  return invoke('mcp_servers_load')
}

async function mcpServersSave(servers: McpLocalServerEntry[]): Promise<void> {
  ensureTauriHost()
  return invoke('mcp_servers_save', { servers })
}

async function mcpServerProbe(server: McpLocalServerEntry): Promise<McpProbeResult> {
  ensureTauriHost()
  return invoke('mcp_server_probe', { server })
}

export async function tauriBrowseDirectory(path?: string): Promise<BrowseDirectoryResult> {
  ensureTauriHost()
  return invoke('desktop_browse_directory', { path: path ?? null })
}

function isTauriHost() {
  return typeof window !== 'undefined' && window.__TAURI_INTERNALS__ !== undefined
}

function ensureTauriHost() {
  if (!isTauriHost()) {
    throw new Error('当前页面未运行在 Tauri 宿主中')
  }
}
