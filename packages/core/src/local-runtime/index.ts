export type LocalRuntimeState = 'starting' | 'running' | 'stopping' | 'stopped' | 'error'

export interface LocalRuntimeStatus {
  state: LocalRuntimeState
  backend_id: string
  name: string
  workspace_roots: string[]
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
  machine_id: string
  machine_label?: string | null
  name?: string
  workspace_roots: string[]
  executor_enabled: boolean
}

export interface LocalRuntimeProfile extends RuntimeStartRequest {
  auto_start: boolean
  backend_id?: string | null
  relay_ws_url?: string | null
}

export interface McpEnvVar {
  name: string
  value: string
}

export interface McpHttpHeader {
  name: string
  value: string
}

export type McpTransportConfig =
  | { type: 'http'; url: string; headers?: McpHttpHeader[] }
  | { type: 'sse'; url: string; headers?: McpHttpHeader[] }
  | { type: 'stdio'; command: string; args?: string[]; env?: McpEnvVar[] }

export interface McpLocalServerEntry {
  name: string
  transport: McpTransportConfig
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
  mcpServersLoad(): Promise<McpLocalServerEntry[]>
  mcpServersSave(servers: McpLocalServerEntry[]): Promise<void>
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

export function parseRuntimeEnv(value: string): McpEnvVar[] {
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
  const t = server.transport

  if (t.type === 'stdio') {
    const args = t.args?.map((a) => a.trim()).filter(Boolean) ?? []
    const env = t.env?.filter((e) => e.name.trim()) ?? []
    return {
      name,
      transport: {
        type: 'stdio',
        command: t.command.trim(),
        ...(args.length ? { args } : {}),
        ...(env.length ? { env } : {}),
      },
    }
  }

  return {
    name,
    transport: { type: t.type, url: t.url.trim(), ...(t.headers?.length ? { headers: t.headers } : {}) },
  }
}

/** 创建指定 transport 类型的空白 MCP Server 条目 */
export function createDefaultMcpLocalServer(
  transportType: McpTransportConfig['type'],
  name: string,
): McpLocalServerEntry {
  switch (transportType) {
    case 'stdio':
      return { name, transport: { type: 'stdio', command: '', args: [], env: [] } }
    case 'http':
      return { name, transport: { type: 'http', url: '' } }
    case 'sse':
      return { name, transport: { type: 'sse', url: '' } }
  }
}

export function formatLocalLogLine(log: LocalLogEvent) {
  return `${log.timestamp} ${log.level.toUpperCase()} ${log.target} ${log.message}`
}
