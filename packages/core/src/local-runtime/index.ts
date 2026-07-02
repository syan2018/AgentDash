export type LocalRuntimeState =
  | 'idle'
  | 'disabled'
  | 'waiting_for_auth'
  | 'waiting_for_api'
  | 'claiming'
  | 'starting'
  | 'running'
  | 'retrying'
  | 'stopping'
  | 'stopped'
  | 'error'

export interface LocalCapabilityHealthAction {
  kind: string
  label: string
}

export interface LocalCapabilityHealthItem {
  id: string
  domain: string
  status: 'ready' | 'degraded' | 'unavailable'
  label: string
  summary: string
  actions: LocalCapabilityHealthAction[]
}

export interface LocalRuntimeStatus {
  state: LocalRuntimeState
  owner: string
  registration_source: RegistrationSource | null
  backend_id: string
  name: string
  workspace_roots: string[]
  executor_enabled: boolean
  mcp_server_count: number
  capability_health: LocalCapabilityHealthItem[]
  message: string | null
  last_error: string | null
  last_attempt_at: string | null
  next_retry_at: string | null
  retry_count: number | null
  relay_connection?: RelayConnectionStatus | null
  registration?: RuntimeRegistrationStatus | null
}

export interface LocalLogEvent {
  sequence: number
  timestamp: string
  level: string
  target: string
  message: string
}

export type LayerState = 'unknown' | 'checking' | 'healthy' | 'degraded' | 'unavailable' | 'disabled'

export type RelayConnectionState =
  | 'not_configured'
  | 'connecting'
  | 'registered'
  | 'reconnecting'
  | 'disconnected'
  | 'error'

export type RegistrationSource = 'desktop_access_token' | 'runner_registration_token'

export interface ApiLayerStatus {
  state: LayerState
  label: string
  target: string | null
  message: string | null
}

export interface DesktopApiSnapshot {
  state: 'starting' | 'running' | 'error' | 'stopped'
  origin: string
  message?: string | null
  database_url?: string | null
}

export interface DesktopApiLayerStatus extends ApiLayerStatus {
  raw_state: DesktopApiSnapshot['state'] | null
}

export interface LocalRuntimeLayerStatus extends ApiLayerStatus {
  raw_state: LocalRuntimeState | null
  owner: string | null
  registration_source: RegistrationSource | null
  backend_id: string | null
  name: string | null
  workspace_roots: string[]
  executor_enabled: boolean
  mcp_server_count: number
  capability_health: LocalCapabilityHealthItem[]
  last_error: string | null
  last_attempt_at: string | null
  next_retry_at: string | null
  retry_count: number | null
}

export interface RunnerLayerStatus extends ApiLayerStatus {
  backend_id: string
  name: string
  online: boolean
  allocatable: boolean | null
  active_session_count: number | null
}

export interface RelayConnectionStatus {
  state: RelayConnectionState
  target: string | null
  last_connected_at: string | null
  last_disconnected_at: string | null
  last_error: string | null
  retry_count: number | null
  next_retry_at: string | null
  registered_backend_id: string | null
}

export interface RuntimeRegistrationStatus {
  source: RegistrationSource
  backend_id: string
  profile_id: string | null
  machine_id: string | null
  machine_label: string | null
  share_scope_kind: string | null
  share_scope_id: string | null
  capability_slot: string | null
  claimed_at: string | null
  registered_at: string | null
  last_seen_at: string | null
}

export interface DesktopRuntimeSettings {
  launch_at_login: boolean
  start_minimized_to_tray: boolean
  auto_connect_local_runtime: boolean
}

export interface DesktopAutostartStatus {
  supported: boolean
  enabled: boolean
  message?: string | null
}

export interface DesktopRuntimeSettingsClient {
  loadSettings(): Promise<DesktopRuntimeSettings>
  saveSettings(settings: DesktopRuntimeSettings): Promise<DesktopRuntimeSettings>
  getAutostartStatus(): Promise<DesktopAutostartStatus>
  setAutostartEnabled(enabled: boolean): Promise<DesktopAutostartStatus>
}

export interface RuntimeDiagnosticsSnapshot {
  generated_at: string
  cloud_api: ApiLayerStatus
  desktop_api: DesktopApiLayerStatus | null
  local_runtime: LocalRuntimeLayerStatus | null
  runner: RunnerLayerStatus | null
  relay_connection: RelayConnectionStatus | null
  registration: RuntimeRegistrationStatus | null
  logs: LocalLogEvent[]
  settings: DesktopRuntimeSettings | null
}

export interface RuntimeDiagnosticsBackendFact {
  id: string
  name: string
  online: boolean
  backend_type: string
  profile_id?: string | null
  machine_id?: string | null
  machine_label?: string | null
  share_scope_kind?: string | null
  share_scope_id?: string | null
  capability_slot?: string | null
  last_claimed_at?: string | null
  registration_source?: string | null
  runtime_health?: {
    status: string
    profile_id?: string | null
    last_seen_at?: string | null
    connected_at?: string | null
    disconnected_at?: string | null
    disconnect_reason?: string | null
  } | null
}

export interface RuntimeDiagnosticsRuntimeSummaryFact {
  backend_id: string
  online: boolean
  allocatable: boolean
  active_session_count: number
}

export interface RuntimeDiagnosticsCloudApiInput {
  state: LayerState
  target: string | null
  message: string | null
  event_stream_state?: string | null
}

export interface RuntimeDiagnosticsInput {
  generated_at?: string
  cloud_api: RuntimeDiagnosticsCloudApiInput
  desktop_api_snapshot?: DesktopApiSnapshot | null
  local_runtime?: LocalRuntimeStatus | null
  backends?: RuntimeDiagnosticsBackendFact[]
  runtime_summaries?: RuntimeDiagnosticsRuntimeSummaryFact[]
  logs?: LocalLogEvent[]
  settings?: DesktopRuntimeSettings | null
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
  | { type: 'stdio'; command: string; args?: string[]; env?: McpEnvVar[]; cwd?: string }

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
    const cwd = t.cwd?.trim()
    return {
      name,
      transport: {
        type: 'stdio',
        command: t.command.trim(),
        ...(args.length ? { args } : {}),
        ...(env.length ? { env } : {}),
        ...(cwd ? { cwd } : {}),
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
      return { name, transport: { type: 'stdio', command: '', args: [], env: [], cwd: '' } }
    case 'http':
      return { name, transport: { type: 'http', url: '' } }
    case 'sse':
      return { name, transport: { type: 'sse', url: '' } }
  }
}

export function formatLocalLogLine(log: LocalLogEvent) {
  return redactRuntimeDiagnosticText(`${log.timestamp} ${log.level.toUpperCase()} ${log.target} ${log.message}`)
}

export function redactRuntimeDiagnosticText(value: string): string {
  const tokenNames = 'access_token|refresh_token|auth_token|relay_token|registration_token|token'
  return value
    .replace(/(\bBearer\s+)[^\s,;"]+/gi, '$1***')
    .replace(new RegExp(`(^|[?&;\\s,])((?:${tokenNames})\\s*=\\s*)[^\\s&;,]+`, 'gi'), '$1$2***')
    .replace(new RegExp(`(["'](?:${tokenNames})["']\\s*:\\s*["'])[^"']+(["'])`, 'gi'), '$1***$2')
}

export function createRuntimeDiagnosticsSnapshot(input: RuntimeDiagnosticsInput): RuntimeDiagnosticsSnapshot {
  const backends = input.backends ?? []
  const summaries = input.runtime_summaries ?? []
  const localBackend = input.local_runtime?.backend_id
    ? backends.find((backend) => backend.id === input.local_runtime?.backend_id)
    : undefined
  const runnerBackend = backends.find((backend) => (
    backend.id !== input.local_runtime?.backend_id
    && backend.registration_source === 'runner_registration_token'
  )) ?? backends.find((backend) => backend.backend_type === 'remote')
  const runnerSummary = runnerBackend
    ? summaries.find((summary) => summary.backend_id === runnerBackend.id)
    : undefined
  const registration = localBackend
    ? registrationFromBackend(localBackend)
    : runnerBackend
      ? registrationFromBackend(runnerBackend)
      : input.local_runtime?.registration ?? registrationFromLocalRuntime(input.local_runtime ?? null)

  return {
    generated_at: input.generated_at ?? new Date().toISOString(),
    cloud_api: {
      state: input.cloud_api.state,
      label: 'Cloud API',
      target: input.cloud_api.target,
      message: cloudApiMessage(input.cloud_api),
    },
    desktop_api: input.desktop_api_snapshot ? desktopApiLayer(input.desktop_api_snapshot) : null,
    local_runtime: localRuntimeLayer(input.local_runtime ?? null),
    runner: runnerBackend ? runnerLayer(runnerBackend, runnerSummary) : null,
    relay_connection: input.local_runtime?.relay_connection ?? null,
    registration,
    logs: input.logs ?? [],
    settings: input.settings ?? null,
  }
}

function cloudApiMessage(input: RuntimeDiagnosticsCloudApiInput): string | null {
  if (input.message) return input.message
  if (input.event_stream_state) return `Project event stream: ${input.event_stream_state}`
  return null
}

function desktopApiLayer(snapshot: DesktopApiSnapshot): DesktopApiLayerStatus {
  const state = desktopApiLayerState(snapshot.state)
  return {
    state,
    label: 'Desktop API',
    target: snapshot.origin || null,
    message: snapshot.message ?? null,
    raw_state: snapshot.state,
  }
}

function desktopApiLayerState(state: DesktopApiSnapshot['state']): LayerState {
  switch (state) {
    case 'running':
      return 'healthy'
    case 'starting':
      return 'checking'
    case 'error':
    case 'stopped':
      return 'unavailable'
  }
}

function localRuntimeLayer(status: LocalRuntimeStatus | null): LocalRuntimeLayerStatus | null {
  if (!status) {
    return {
      state: 'disabled',
      label: 'Local Runtime',
      target: null,
      message: '桌面托管 runtime 未启动',
      raw_state: null,
      owner: null,
      registration_source: null,
      backend_id: null,
      name: null,
      workspace_roots: [],
      executor_enabled: false,
      mcp_server_count: 0,
      capability_health: [],
      last_error: null,
      last_attempt_at: null,
      next_retry_at: null,
      retry_count: null,
    }
  }
  return {
    state: localRuntimeLayerState(status.state),
    label: 'Local Runtime',
    target: status.backend_id || null,
    message: status.message,
    raw_state: status.state,
    owner: status.owner || null,
    registration_source: status.registration_source ?? null,
    backend_id: status.backend_id || null,
    name: status.name || null,
    workspace_roots: status.workspace_roots,
    executor_enabled: status.executor_enabled,
    mcp_server_count: status.mcp_server_count,
    capability_health: status.capability_health ?? [],
    last_error: status.last_error,
    last_attempt_at: status.last_attempt_at,
    next_retry_at: status.next_retry_at,
    retry_count: status.retry_count,
  }
}

function localRuntimeLayerState(state: LocalRuntimeState): LayerState {
  switch (state) {
    case 'running':
      return 'healthy'
    case 'waiting_for_auth':
    case 'waiting_for_api':
    case 'claiming':
    case 'starting':
    case 'retrying':
    case 'stopping':
      return 'checking'
    case 'error':
      return 'unavailable'
    case 'idle':
    case 'disabled':
    case 'stopped':
      return 'disabled'
  }
}

function runnerLayer(
  backend: RuntimeDiagnosticsBackendFact,
  summary: RuntimeDiagnosticsRuntimeSummaryFact | undefined,
): RunnerLayerStatus {
  const active_session_count = summary?.active_session_count ?? null
  const allocatable = summary?.allocatable ?? null
  return {
    state: runnerLayerState(backend, summary),
    label: 'Runner',
    target: backend.id,
    message: backend.runtime_health?.disconnect_reason ?? null,
    backend_id: backend.id,
    name: backend.name,
    online: backend.online,
    allocatable,
    active_session_count,
  }
}

function runnerLayerState(
  backend: RuntimeDiagnosticsBackendFact,
  summary: RuntimeDiagnosticsRuntimeSummaryFact | undefined,
): LayerState {
  if (!backend.online) return 'unavailable'
  if (summary && !summary.allocatable) return 'degraded'
  return 'healthy'
}

function registrationFromBackend(backend: RuntimeDiagnosticsBackendFact): RuntimeRegistrationStatus | null {
  const source = normalizeRegistrationSource(backend.registration_source)
  if (!source) return null
  return {
    source,
    backend_id: backend.id,
    profile_id: backend.profile_id ?? backend.runtime_health?.profile_id ?? null,
    machine_id: backend.machine_id ?? null,
    machine_label: backend.machine_label ?? null,
    share_scope_kind: backend.share_scope_kind ?? null,
    share_scope_id: backend.share_scope_id ?? null,
    capability_slot: backend.capability_slot ?? null,
    claimed_at: backend.last_claimed_at ?? null,
    registered_at: backend.runtime_health?.connected_at ?? null,
    last_seen_at: backend.runtime_health?.last_seen_at ?? null,
  }
}

function registrationFromLocalRuntime(status: LocalRuntimeStatus | null): RuntimeRegistrationStatus | null {
  if (!status?.registration_source) return null
  return {
    source: status.registration_source,
    backend_id: status.backend_id,
    profile_id: null,
    machine_id: null,
    machine_label: null,
    share_scope_kind: null,
    share_scope_id: null,
    capability_slot: null,
    claimed_at: null,
    registered_at: status.relay_connection?.last_connected_at ?? null,
    last_seen_at: status.relay_connection?.last_connected_at ?? null,
  }
}

function normalizeRegistrationSource(source: string | null | undefined): RegistrationSource | null {
  if (source === 'desktop_access_token' || source === 'runner_registration_token') return source
  return null
}
