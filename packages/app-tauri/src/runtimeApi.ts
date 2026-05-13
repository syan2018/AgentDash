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

export interface RuntimeStartRequest {
  cloud_url: string
  token: string
  backend_id?: string
  name?: string
  accessible_roots: string[]
  executor_enabled: boolean
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

function isTauriHost() {
  return typeof window !== 'undefined' && window.__TAURI_INTERNALS__ !== undefined
}

function ensureTauriHost() {
  if (!isTauriHost()) {
    throw new Error('当前页面未运行在 Tauri 宿主中')
  }
}
