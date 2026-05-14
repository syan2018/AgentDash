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
  server_url: string
  access_token: string
  profile_id: string
  device_id: string
  name?: string
  accessible_roots: string[]
  executor_enabled: boolean
}

export interface LocalRuntimeProfile extends RuntimeStartRequest {
  auto_start: boolean
  backend_id?: string | null
  relay_ws_url?: string | null
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

export interface LocalRuntimeClient {
  profileLoad(): Promise<LocalRuntimeProfile | null>
  profileSave(profile: LocalRuntimeProfile): Promise<LocalRuntimeProfile>
  profileDelete(): Promise<void>
  runtimeSnapshot(): Promise<LocalRuntimeStatus | null>
  runtimeStart(request: RuntimeStartRequest): Promise<LocalRuntimeStatus>
  runtimeStop(): Promise<void>
  runtimeRestart(): Promise<LocalRuntimeStatus>
  logsTail(limit?: number): Promise<LocalLogEvent[]>
  logsClear(): Promise<void>
  mcpServersLoad(root: string): Promise<McpLocalServerEntry[]>
  mcpServersSave(root: string, servers: McpLocalServerEntry[]): Promise<void>
  mcpServerProbe(server: McpLocalServerEntry): Promise<McpProbeResult>
}

export const DEFAULT_LOCAL_RUNTIME_SERVER_URL = 'http://127.0.0.1:3001'
export const DEFAULT_LOCAL_RUNTIME_PROFILE_ID = 'default'
export const DEFAULT_LOCAL_RUNTIME_BACKEND_NAME = 'desktop-local-backend'

export function parseRuntimeLines(value: string) {
  return value
    .split('\n')
    .map((line) => line.trim())
    .filter(Boolean)
}

export function parseRuntimeEnv(value: string): McpEnvEntry[] {
  return value
    .split('\n')
    .map((line) => line.trim())
    .filter(Boolean)
    .map((line) => {
      const separatorIndex = line.indexOf('=')
      if (separatorIndex === -1) {
        return { name: line, value: '' }
      }
      return {
        name: line.slice(0, separatorIndex).trim(),
        value: line.slice(separatorIndex + 1),
      }
    })
    .filter((entry) => entry.name)
}

export function normalizeMcpLocalServer(server: McpLocalServerEntry): McpLocalServerEntry {
  const name = server.name.trim()
  if (server.transport === 'stdio') {
    const args = server.args?.map((arg) => arg.trim()).filter(Boolean) ?? []
    const env = server.env?.filter((entry) => entry.name.trim()) ?? []
    return {
      name,
      transport: 'stdio',
      command: server.command?.trim() || null,
      args: args.length ? args : null,
      env: env.length ? env : null,
      url: null,
    }
  }

  return {
    name,
    transport: server.transport,
    command: null,
    args: null,
    env: null,
    url: server.url?.trim() || null,
  }
}

export function formatLocalLogLine(log: LocalLogEvent) {
  return `${log.timestamp} ${log.level.toUpperCase()} ${log.target} ${log.message}`
}
