import { invoke } from '@tauri-apps/api/core'

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown
  }
}

export type LocalRuntimeState = 'starting' | 'running' | 'stopping' | 'stopped' | 'error'

export interface LocalRuntimeStatus {
  state: LocalRuntimeState
  backend_id: string
  name: string
  accessible_roots: string[]
  executor_enabled: boolean
  mcp_server_count: number
  message: string | null
}

export interface LocalLogEvent {
  sequence: number
  timestamp: string
  level: string
  target: string
  message: string
}

export interface RuntimeStartRequest {
  cloud_url: string
  token: string
  backend_id?: string
  name?: string
  accessible_roots: string[]
  executor_enabled: boolean
}

export interface McpEnvEntry {
  name: string
  value: string
}

export interface McpLocalServerEntry {
  name: string
  transport: 'stdio' | 'http' | 'sse'
  command?: string | null
  args?: string[] | null
  env?: McpEnvEntry[] | null
  url?: string | null
}

export interface McpProbeResult {
  ok: boolean
  tool_count: number
  message: string
}

export async function runtimeSnapshot(): Promise<LocalRuntimeStatus | null> {
  if (!isTauriHost()) return null
  return invoke('runtime_snapshot')
}

export async function runtimeStart(request: RuntimeStartRequest): Promise<LocalRuntimeStatus> {
  ensureTauriHost()
  return invoke('runtime_start', { request })
}

export async function runtimeStop(): Promise<void> {
  ensureTauriHost()
  return invoke('runtime_stop')
}

export async function logsTail(limit = 200): Promise<LocalLogEvent[]> {
  if (!isTauriHost()) return []
  return invoke('logs_tail', { limit })
}

export async function logsClear(): Promise<void> {
  ensureTauriHost()
  return invoke('logs_clear')
}

export async function mcpServersLoad(root: string): Promise<McpLocalServerEntry[]> {
  ensureTauriHost()
  return invoke('mcp_servers_load', { root })
}

export async function mcpServersSave(root: string, servers: McpLocalServerEntry[]): Promise<void> {
  ensureTauriHost()
  return invoke('mcp_servers_save', { root, servers })
}

export async function mcpServerProbe(server: McpLocalServerEntry): Promise<McpProbeResult> {
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
