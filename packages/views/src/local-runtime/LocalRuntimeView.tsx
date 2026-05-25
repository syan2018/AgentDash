import { useEffect, useMemo, useState } from 'react'
import type { FormEvent } from 'react'
import {
  DEFAULT_LOCAL_RUNTIME_BACKEND_NAME,
  DEFAULT_LOCAL_RUNTIME_PROFILE_ID,
  DEFAULT_LOCAL_RUNTIME_SERVER_URL,
  createDefaultMcpLocalServer,
  formatLocalLogLine,
  normalizeMcpLocalServer,
} from '@agentdash/core/local-runtime'
import type {
  LocalLogEvent,
  LocalRuntimeClient,
  LocalRuntimeProfile,
  LocalRuntimeStatus,
  McpLocalServerEntry,
  McpTransportConfig,
  RuntimeStartRequest,
} from '@agentdash/core/local-runtime'
import {
  Badge,
  Button,
  Card,
  CardHeader,
  CheckboxField,
  EmptyState,
  Field,
  Notice,
  Select,
  TextInput,
  cn,
} from '@agentdash/ui'
import type { BrowseDirectoryResult } from '../directory-browser'
import { DirectoryBrowserDialog } from '../directory-browser'
import { McpTransportConfigEditor } from '../mcp-shared'

export interface LocalRuntimeViewProps {
  client: LocalRuntimeClient
  /** 注入目录浏览 API（由宿主层提供 backend browse 能力） */
  onBrowseDirectory?: (path?: string) => Promise<BrowseDirectoryResult>
  defaultServerUrl?: string
  defaultAccessToken?: string
  defaultProfileId?: string
  defaultBackendName?: string
}

export function LocalRuntimeView({
  client,
  onBrowseDirectory,
  defaultServerUrl = DEFAULT_LOCAL_RUNTIME_SERVER_URL,
  defaultAccessToken = '',
  defaultProfileId = DEFAULT_LOCAL_RUNTIME_PROFILE_ID,
  defaultBackendName = DEFAULT_LOCAL_RUNTIME_BACKEND_NAME,
}: LocalRuntimeViewProps) {
  const [snapshot, setSnapshot] = useState<LocalRuntimeStatus | null>(null)
  const [serverUrl, setServerUrl] = useState(defaultServerUrl)
  const [accessToken, setAccessToken] = useState(defaultAccessToken)
  const [profileId, setProfileId] = useState(defaultProfileId)
  const [machineId, setMachineId] = useState('')
  const [machineLabel, setMachineLabel] = useState('')
  const [backendName, setBackendName] = useState(defaultBackendName)
  const [workspaceRoots, setWorkspaceRoots] = useState<string[]>([])
  const [executorEnabled, setExecutorEnabled] = useState(true)
  const [autoStart, setAutoStart] = useState(false)
  const [isEditing, setIsEditing] = useState(false)
  const [isBusy, setIsBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [profileMessage, setProfileMessage] = useState<string | null>(null)
  const [mcpServers, setMcpServers] = useState<McpLocalServerEntry[]>([])
  const [mcpMessage, setMcpMessage] = useState<string | null>(null)
  const [mcpDirty, setMcpDirty] = useState(false)
  const [probingIndex, setProbingIndex] = useState<number | null>(null)
  const [expandedMcpIndex, setExpandedMcpIndex] = useState<number | null>(null)
  const [logs, setLogs] = useState<LocalLogEvent[]>([])
  const [logLevel, setLogLevel] = useState('all')
  const [browseDialogOpen, setBrowseDialogOpen] = useState(false)
  const [browseTargetIndex, setBrowseTargetIndex] = useState<number | null>(null)

  useEffect(() => {
    let alive = true
    const loadProfile = async () => {
      try {
        const profile = await client.profileLoad()
        if (!alive || !profile) return
        applyProfile(profile)
        setProfileMessage('已加载本机 profile')
        if (profile.auto_start && profile.server_url.trim()) {
          setSnapshot(await client.runtimeStart(profile))
          setProfileMessage('已加载本机 profile 并自动启动 runtime')
        }
      } catch (err) {
        if (alive) setProfileMessage(formatError(err))
      }
    }
    const refresh = async () => {
      try {
        const next = await client.runtimeSnapshot()
        if (alive) setSnapshot(next)
        const nextLogs = await client.logsTail()
        if (alive) setLogs(nextLogs)
      } catch (err) {
        if (alive) setError(formatError(err))
      }
    }
    void loadProfile()
    void refresh()
    const timer = window.setInterval(refresh, 1500)
    return () => {
      alive = false
      window.clearInterval(timer)
    }
  }, [client])

  useEffect(() => {
    let alive = true
    void client.mcpServersLoad().then((servers) => {
      if (alive) {
        setMcpServers(servers)
        setMcpDirty(false)
      }
    }).catch(() => {})
    return () => { alive = false }
  }, [client])

  const visibleLogs = useMemo(
    () => logs.filter((log) => logLevel === 'all' || log.level === logLevel),
    [logLevel, logs],
  )

  async function handleStart(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    setIsBusy(true)
    setError(null)
    try {
      const request = buildStartRequest(
        serverUrl,
        accessToken,
        profileId,
        machineId,
        machineLabel,
        backendName,
        workspaceRoots,
        executorEnabled,
      )
      setSnapshot(await client.runtimeStart(request))
    } catch (err) {
      setError(formatError(err))
    } finally {
      setIsBusy(false)
    }
  }

  async function handleSaveProfile() {
    setProfileMessage(null)
    try {
      const profile = await client.profileSave(buildProfile())
      applyProfile(profile)
      setProfileMessage('已保存本机 profile')
      setIsEditing(false)
    } catch (err) {
      setProfileMessage(formatError(err))
    }
  }

  function applyProfile(profile: LocalRuntimeProfile) {
    setServerUrl(profile.server_url)
    setAccessToken(profile.access_token || defaultAccessToken)
    setProfileId(profile.profile_id || defaultProfileId)
    setMachineId(profile.machine_id || '')
    setMachineLabel(profile.machine_label ?? '')
    setBackendName(profile.name ?? defaultBackendName)
    setWorkspaceRoots(profile.workspace_roots)
    setExecutorEnabled(profile.executor_enabled)
    setAutoStart(profile.auto_start)
  }

  function buildProfile(): LocalRuntimeProfile {
    return {
      ...buildStartRequest(
        serverUrl,
        accessToken,
        profileId,
        machineId,
        machineLabel,
        backendName,
        workspaceRoots,
        executorEnabled,
      ),
      legacy_machine_ids: [],
      auto_start: autoStart,
      backend_id: snapshot?.backend_id ?? null,
    }
  }

  async function handleStop() {
    setIsBusy(true)
    setError(null)
    try {
      await client.runtimeStop()
      setSnapshot(await client.runtimeSnapshot())
    } catch (err) {
      setError(formatError(err))
    } finally {
      setIsBusy(false)
    }
  }

  async function handleRestart() {
    setIsBusy(true)
    setError(null)
    try {
      setSnapshot(await client.runtimeRestart())
      setLogs(await client.logsTail())
    } catch (err) {
      setError(formatError(err))
    } finally {
      setIsBusy(false)
    }
  }

  async function handleRefreshLogs() {
    try {
      setLogs(await client.logsTail())
    } catch (err) {
      setError(formatError(err))
    }
  }

  async function handleClearLogs() {
    try {
      await client.logsClear()
      setLogs(await client.logsTail())
    } catch (err) {
      setError(formatError(err))
    }
  }

  async function handleCopyLogs() {
    try {
      const content = visibleLogs.map(formatLocalLogLine).join('\n')
      await navigator.clipboard.writeText(content)
    } catch (err) {
      setError(formatError(err))
    }
  }

  async function handleSaveMcpServers() {
    setMcpMessage(null)
    try {
      await client.mcpServersSave(mcpServers.map(normalizeMcpLocalServer))
      setMcpDirty(false)
      setMcpMessage('已保存 MCP servers')
    } catch (err) {
      setMcpMessage(formatError(err))
    }
  }

  async function handleProbeMcpServer(index: number) {
    setProbingIndex(index)
    setMcpMessage(null)
    try {
      const result = await client.mcpServerProbe(normalizeMcpLocalServer(mcpServers[index]))
      setMcpMessage(`${result.ok ? '探测成功' : '探测失败'}：${result.message}`)
    } catch (err) {
      setMcpMessage(formatError(err))
    } finally {
      setProbingIndex(null)
    }
  }

  function addMcpServer() {
    const baseName = `mcp-${mcpServers.length + 1}`
    setMcpServers((current) => [...current, createDefaultMcpLocalServer('stdio', baseName)])
    setExpandedMcpIndex(mcpServers.length)
    setMcpDirty(true)
  }

  function updateMcpServerName(index: number, name: string) {
    setMcpServers((current) =>
      current.map((server, i) => (i === index ? { ...server, name } : server)),
    )
    setMcpDirty(true)
  }

  function updateMcpServerTransport(index: number, transport: McpTransportConfig) {
    setMcpServers((current) =>
      current.map((server, i) => (i === index ? { ...server, transport } : server)),
    )
    setMcpDirty(true)
  }

  function removeMcpServer(index: number) {
    setMcpServers((current) => current.filter((_, currentIndex) => currentIndex !== index))
    setExpandedMcpIndex(null)
    setMcpDirty(true)
  }

  const stateLabel = snapshot ? stateText(snapshot.state) : '未启动'
  const isRunning = snapshot?.state === 'running'

  return (
    <div className="space-y-4">
      {/* ── 状态概览 ── */}
      <Card>
        <CardHeader
          actions={
            <Badge variant={stateBadgeVariant(snapshot?.state)}>{stateLabel}</Badge>
          }
        >
          <h2 className="text-base font-semibold text-foreground">本机运行时</h2>
          <p className="mt-0.5 text-xs text-muted-foreground">
            当前桌面端的本机执行环境，机器身份由 Tauri 持久化。
          </p>
        </CardHeader>

        <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
          <RuntimeStat label="Backend" value={snapshot?.backend_id ?? '—'} />
          <RuntimeStat label="机器" value={machineLabel || machineId || '保存 profile 后生成'} />
          <RuntimeStat label="Scope" value="Personal / private" />
          <RuntimeStat label="能力" value={`${snapshot?.mcp_server_count ?? 0} MCP · ${snapshot?.executor_enabled ? 'Executor 开启' : 'Executor 关闭'}`} />
        </div>

        <div className="mt-4 flex flex-wrap justify-end gap-2 border-t border-border pt-4">
          <Button size="sm" onClick={() => void client.runtimeSnapshot().then(setSnapshot)} disabled={isBusy}>
            刷新状态
          </Button>
          <Button size="sm" onClick={() => void handleRestart()} disabled={isBusy || !isRunning}>
            重启
          </Button>
          <Button size="sm" onClick={() => void handleStop()} disabled={isBusy || !snapshot}>
            停止
          </Button>
        </div>
      </Card>

      {/* ── 连接配置 (Profile) ── */}
      <Card as="form" onSubmit={handleStart}>
        <CardHeader
          actions={
            isEditing
              ? (
                  <div className="flex items-center gap-2">
                    <Button size="sm" onClick={() => setIsEditing(false)}>取消</Button>
                    <Button size="sm" variant="primary" onClick={() => void handleSaveProfile()} disabled={!serverUrl.trim()}>
                      保存配置
                    </Button>
                  </div>
                )
              : (
                  <Button size="sm" onClick={() => setIsEditing(true)}>编辑</Button>
                )
          }
        >
          <h2 className="text-base font-semibold text-foreground">连接配置</h2>
          <p className="mt-0.5 text-xs text-muted-foreground">
            桌面端本地 profile，保存后下次启动自动使用。
          </p>
        </CardHeader>

        <div className="grid gap-3">
          <div className="grid gap-3 md:grid-cols-2">
            <Field label="Server URL">
              <TextInput
                value={serverUrl}
                onChange={(event) => setServerUrl(event.target.value)}
                readOnly={!isEditing}
              />
            </Field>
            <Field label="Access Token">
              <TextInput
                autoComplete="current-password"
                value={isEditing ? accessToken : (accessToken ? '••••••••' : '')}
                onChange={(event) => setAccessToken(event.target.value)}
                placeholder={isEditing ? 'Personal auth 下可留空' : '（未设置）'}
                type={isEditing ? 'password' : 'text'}
                readOnly={!isEditing}
              />
            </Field>
          </div>

          <div className="grid gap-3 md:grid-cols-2">
            <Field label="Profile ID">
              <TextInput
                value={profileId}
                onChange={(event) => setProfileId(event.target.value)}
                readOnly={!isEditing}
              />
            </Field>
            <Field label="机器标签">
              <TextInput
                value={machineLabel}
                onChange={(event) => setMachineLabel(event.target.value)}
                placeholder="默认使用本机 hostname"
                readOnly={!isEditing}
              />
            </Field>
          </div>

          <div className="grid gap-3 md:grid-cols-2">
            <Field label="Machine ID">
              <TextInput value={machineId || '保存 profile 后由桌面端生成'} readOnly />
            </Field>
            <Field label="Backend 名称">
              <TextInput
                value={backendName}
                onChange={(event) => setBackendName(event.target.value)}
                readOnly={!isEditing}
              />
            </Field>
          </div>

          {(isEditing || workspaceRoots.length > 0) && (
            <Field label="Workspace roots">
              <div className="space-y-1.5">
                {workspaceRoots.map((root, i) => (
                  <div key={i} className="flex items-center gap-1.5">
                    <TextInput
                      className="flex-1"
                      value={root}
                      onChange={(e) => {
                        const next = [...workspaceRoots]
                        next[i] = e.target.value
                        setWorkspaceRoots(next)
                      }}
                      placeholder="绝对路径"
                      readOnly={!isEditing}
                    />
                    {isEditing && onBrowseDirectory && (
                      <Button
                        size="sm"
                        type="button"
                        onClick={() => { setBrowseTargetIndex(i); setBrowseDialogOpen(true) }}
                      >
                        浏览
                      </Button>
                    )}
                    {isEditing && (
                      <Button
                        size="sm"
                        type="button"
                        onClick={() => setWorkspaceRoots(workspaceRoots.filter((_, j) => j !== i))}
                      >
                        ×
                      </Button>
                    )}
                  </div>
                ))}
                {isEditing && (
                  <Button
                    size="sm"
                    type="button"
                    onClick={() => {
                      if (onBrowseDirectory) {
                        setBrowseTargetIndex(workspaceRoots.length)
                        setWorkspaceRoots([...workspaceRoots, ''])
                        setBrowseDialogOpen(true)
                      } else {
                        setWorkspaceRoots([...workspaceRoots, ''])
                      }
                    }}
                  >
                    添加目录
                  </Button>
                )}
              </div>
            </Field>
          )}

          <div className="flex items-center gap-4">
            <CheckboxField
              label="Executor"
              checked={executorEnabled}
              onChange={(event) => setExecutorEnabled(event.currentTarget.checked)}
              disabled={!isEditing}
            />
            <CheckboxField
              label="自动启动"
              checked={autoStart}
              onChange={(event) => setAutoStart(event.currentTarget.checked)}
              disabled={!isEditing}
            />
          </div>
        </div>

        {error ? <Notice className="mt-3" tone="danger">{error}</Notice> : null}
        {profileMessage ? <Notice className="mt-3">{profileMessage}</Notice> : null}

        {isEditing && (
          <div className="mt-4 flex justify-end border-t border-border pt-4">
            <Button size="sm" type="submit" variant="primary" disabled={isBusy || !serverUrl.trim()}>
              启动 runtime
            </Button>
          </div>
        )}
      </Card>

      {/* ── MCP Servers ── */}
      <Card>
        <CardHeader
          actions={
            <div className="flex items-center gap-2">
              {mcpDirty && (
                <Button size="sm" variant="primary" onClick={() => void handleSaveMcpServers()}>
                  保存更改
                </Button>
              )}
              <Button size="sm" onClick={addMcpServer}>
                添加
              </Button>
            </div>
          }
        >
          <h2 className="text-base font-semibold text-foreground">MCP Servers</h2>
          <p className="mt-0.5 text-xs text-muted-foreground">
            本机 MCP Server 配置，随本机 runtime 暴露给会话执行面。
          </p>
        </CardHeader>

        <div className="space-y-2">
          {mcpServers.map((server, index) => (
            <McpServerCard
              key={`${server.name}-${index}`}
              server={server}
              expanded={expandedMcpIndex === index}
              isProbing={probingIndex === index}
              onToggle={() => setExpandedMcpIndex(expandedMcpIndex === index ? null : index)}
              onNameChange={(name) => updateMcpServerName(index, name)}
              onTransportChange={(transport) => updateMcpServerTransport(index, transport)}
              onProbe={() => void handleProbeMcpServer(index)}
              onRemove={() => removeMcpServer(index)}
            />
          ))}
          {mcpServers.length === 0 && (
            <EmptyState>未配置 MCP servers</EmptyState>
          )}
        </div>

        {mcpMessage ? <Notice className="mt-3">{mcpMessage}</Notice> : null}
      </Card>

      {/* ── 诊断日志 ── */}
      <Card>
        <CardHeader
          actions={
            <div className="flex flex-wrap gap-2">
              <Select value={logLevel} onChange={(event) => setLogLevel(event.target.value)}>
                <option value="all">全部</option>
                <option value="info">info</option>
                <option value="warn">warn</option>
                <option value="error">error</option>
              </Select>
              <Button size="sm" onClick={() => void handleRefreshLogs()}>刷新</Button>
              <Button size="sm" onClick={() => void handleCopyLogs()} disabled={visibleLogs.length === 0}>
                复制
              </Button>
              <Button size="sm" onClick={() => void handleClearLogs()} disabled={visibleLogs.length === 0}>
                清空
              </Button>
            </div>
          }
        >
          <h2 className="text-base font-semibold text-foreground">诊断日志</h2>
          <p className="mt-0.5 text-xs text-muted-foreground">
            来自桌面端本机 runtime manager 的日志，用于排查 ensure、注册和 MCP 探测。
          </p>
        </CardHeader>

        <div className="space-y-1">
          {visibleLogs.map((log) => (
            <div
              className={cn(
                'grid gap-2 rounded-[8px] border-l-4 bg-background/80 px-3 py-2 text-xs md:grid-cols-[92px_58px_110px_minmax(0,1fr)]',
                log.level === 'warn' && 'border-l-warning bg-warning/5',
                log.level === 'error' && 'border-l-destructive bg-destructive/5',
                log.level === 'info' && 'border-l-muted-foreground/40',
              )}
              key={log.sequence}
            >
              <time className="text-muted-foreground">{formatTime(log.timestamp)}</time>
              <span className="font-semibold uppercase text-muted-foreground">{log.level}</span>
              <code className="text-muted-foreground">{log.target}</code>
              <p className="m-0 wrap-anywhere text-foreground">{log.message}</p>
            </div>
          ))}
          {visibleLogs.length === 0 ? <EmptyState>暂无本机 runtime 日志</EmptyState> : null}
        </div>
      </Card>

      {onBrowseDirectory && (
        <DirectoryBrowserDialog
          open={browseDialogOpen}
          onBrowse={onBrowseDirectory}
          onSelect={(path) => {
            if (browseTargetIndex !== null) {
              const next = [...workspaceRoots]
              if (browseTargetIndex < next.length) {
                next[browseTargetIndex] = path
              } else {
                next.push(path)
              }
              setWorkspaceRoots(next)
            }
            setBrowseTargetIndex(null)
          }}
          onClose={() => { setBrowseDialogOpen(false); setBrowseTargetIndex(null) }}
        />
      )}
    </div>
  )
}

// ── MCP Server 单卡片 ──

function mcpTransportSummary(t: McpTransportConfig): string {
  if (t.type === 'stdio') return t.command || '（未设置命令）'
  return t.url || '（未设置 URL）'
}

function McpServerCard({
  server,
  expanded,
  isProbing,
  onToggle,
  onNameChange,
  onTransportChange,
  onProbe,
  onRemove,
}: {
  server: McpLocalServerEntry
  expanded: boolean
  isProbing: boolean
  onToggle: () => void
  onNameChange: (name: string) => void
  onTransportChange: (transport: McpTransportConfig) => void
  onProbe: () => void
  onRemove: () => void
}) {
  return (
    <div className="rounded-[8px] border border-border bg-card">
      {/* 折叠头：点击展开/收起 */}
      <button
        type="button"
        onClick={onToggle}
        className="flex w-full items-center justify-between gap-3 px-4 py-3 text-left transition-colors hover:bg-secondary/30"
      >
        <div className="flex min-w-0 items-center gap-2">
          <span className="shrink-0 text-xs text-muted-foreground">{expanded ? '▼' : '▶'}</span>
          <span className="truncate text-sm font-medium text-foreground">{server.name || '未命名'}</span>
          <Badge variant={server.transport.type === 'stdio' ? 'neutral' : 'primary'}>
            {server.transport.type}
          </Badge>
        </div>
        <span className="truncate text-xs text-muted-foreground">{mcpTransportSummary(server.transport)}</span>
      </button>

      {/* 展开内容 */}
      {expanded && (
        <div className="space-y-3 border-t border-border px-4 pb-4 pt-3">
          <Field label="名称">
            <TextInput
              value={server.name}
              onChange={(event) => onNameChange(event.target.value)}
              placeholder="Server 名称"
            />
          </Field>

          <McpTransportConfigEditor value={server.transport} onChange={onTransportChange} />

          <div className="flex items-center justify-end gap-2 border-t border-border pt-3">
            <Button size="sm" onClick={onProbe} disabled={isProbing}>
              {isProbing ? '探测中…' : '探测连接'}
            </Button>
            <Button
              size="sm"
              variant="ghost"
              onClick={onRemove}
              className="text-muted-foreground hover:text-destructive"
            >
              移除
            </Button>
          </div>
        </div>
      )}
    </div>
  )
}

// ── 状态概览小组件 ──

function RuntimeStat({ label, value }: { label: string; value: string }) {
  return (
    <div className="grid gap-0.5">
      <dt className="text-[11px] font-semibold uppercase tracking-wide text-muted-foreground">{label}</dt>
      <dd className="m-0 truncate text-sm text-foreground" title={value}>{value}</dd>
    </div>
  )
}

// ── 工具函数 ──

function buildStartRequest(
  serverUrl: string,
  accessToken: string,
  profileId: string,
  machineId: string,
  machineLabel: string,
  backendName: string,
  roots: string[],
  executorEnabled: boolean,
): RuntimeStartRequest {
  return {
    server_url: serverUrl.trim(),
    access_token: accessToken.trim(),
    profile_id: profileId.trim() || DEFAULT_LOCAL_RUNTIME_PROFILE_ID,
    machine_id: machineId.trim(),
    machine_label: machineLabel.trim() || null,
    legacy_machine_ids: [],
    name: backendName.trim() || undefined,
    workspace_roots: roots.map((root) => root.trim()).filter(Boolean),
    executor_enabled: executorEnabled,
  }
}

function stateText(state: LocalRuntimeStatus['state']) {
  switch (state) {
    case 'starting':
      return '启动中'
    case 'running':
      return '运行中'
    case 'stopping':
      return '停止中'
    case 'error':
      return '错误'
    case 'stopped':
      return '已停止'
  }
}

function stateBadgeVariant(state?: LocalRuntimeStatus['state']): 'success' | 'primary' | 'danger' | 'neutral' {
  switch (state) {
    case 'running':
      return 'success'
    case 'starting':
    case 'stopping':
      return 'primary'
    case 'error':
      return 'danger'
    case 'stopped':
    default:
      return 'neutral'
  }
}

function formatError(error: unknown) {
  return error instanceof Error ? error.message : String(error)
}

function formatTime(timestamp: string) {
  const date = new Date(timestamp)
  if (Number.isNaN(date.getTime())) return timestamp
  return date.toLocaleTimeString()
}
