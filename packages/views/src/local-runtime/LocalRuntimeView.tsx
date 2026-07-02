import { useEffect, useMemo, useState } from 'react'
import type { FormEvent } from 'react'
import {
  DEFAULT_LOCAL_RUNTIME_PROFILE_ID,
  DEFAULT_LOCAL_RUNTIME_SERVER_URL,
  createRuntimeDiagnosticsSnapshot,
  createDefaultMcpLocalServer,
  formatLocalLogLine,
  normalizeMcpLocalServer,
} from '@agentdash/core/local-runtime'
import type {
  DesktopApiSnapshot,
  DesktopAutostartStatus,
  DesktopRuntimeSettings,
  DesktopRuntimeSettingsClient,
  LayerState,
  LocalCapabilityHealthItem,
  LocalLogEvent,
  LocalRuntimeClient,
  LocalRuntimeProfile,
  LocalRuntimeStatus,
  McpLocalServerEntry,
  McpTransportConfig,
  RuntimeDiagnosticsBackendFact,
  RuntimeDiagnosticsCloudApiInput,
  RuntimeDiagnosticsRuntimeSummaryFact,
  RuntimeDiagnosticsSnapshot,
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
  StatusDot,
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
  desktopApp?: DesktopRuntimeSettingsClient
  diagnosticsContext?: LocalRuntimeDiagnosticsContext
  defaultServerUrl?: string
  defaultAccessToken?: string
  defaultProfileId?: string
  defaultBackendName?: string
}

export interface LocalRuntimeDiagnosticsContext {
  cloud_api: RuntimeDiagnosticsCloudApiInput
  desktop_api_snapshot: DesktopApiSnapshot | null
  backends: RuntimeDiagnosticsBackendFact[]
  runtime_summaries: RuntimeDiagnosticsRuntimeSummaryFact[]
}

export function LocalRuntimeView({
  client,
  onBrowseDirectory,
  desktopApp,
  diagnosticsContext,
  defaultServerUrl = DEFAULT_LOCAL_RUNTIME_SERVER_URL,
  defaultAccessToken = '',
  defaultProfileId = DEFAULT_LOCAL_RUNTIME_PROFILE_ID,
  defaultBackendName = '',
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
  const [desktopSettings, setDesktopSettings] = useState<DesktopRuntimeSettings | null>(null)
  const [desktopSettingsDraft, setDesktopSettingsDraft] = useState<DesktopRuntimeSettings | null>(null)
  const [autostartStatus, setAutostartStatus] = useState<DesktopAutostartStatus | null>(null)
  const [desktopSettingsMessage, setDesktopSettingsMessage] = useState<string | null>(null)
  const [desktopSettingsBusy, setDesktopSettingsBusy] = useState(false)
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

  useEffect(() => {
    if (!desktopApp) return
    let alive = true
    const loadDesktopSettings = async () => {
      try {
        const [settings, autostart] = await Promise.all([
          desktopApp.loadSettings(),
          desktopApp.getAutostartStatus(),
        ])
        if (!alive) return
        const normalized = { ...settings, launch_at_login: autostart.enabled }
        setDesktopSettings(normalized)
        setDesktopSettingsDraft(normalized)
        setAutostartStatus(autostart)
      } catch (err) {
        if (alive) setDesktopSettingsMessage(formatError(err))
      }
    }
    void loadDesktopSettings()
    return () => { alive = false }
  }, [desktopApp])

  const visibleLogs = useMemo(
    () => logs.filter((log) => logLevel === 'all' || log.level === logLevel),
    [logLevel, logs],
  )

  const diagnostics = useMemo(
    () => createRuntimeDiagnosticsSnapshot({
      cloud_api: diagnosticsContext?.cloud_api ?? {
        state: 'unknown',
        target: null,
        message: null,
      },
      desktop_api_snapshot: diagnosticsContext?.desktop_api_snapshot ?? null,
      local_runtime: snapshot,
      backends: diagnosticsContext?.backends ?? [],
      runtime_summaries: diagnosticsContext?.runtime_summaries ?? [],
      logs,
      settings: desktopSettings,
    }),
    [desktopSettings, diagnosticsContext, logs, snapshot],
  )

  async function startRuntimeFromCurrentProfile() {
    setIsBusy(true)
    setError(null)
    try {
      const request = buildStartRequest(
        serverUrl,
        accessToken,
        profileId,
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

  async function handleStart(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    await startRuntimeFromCurrentProfile()
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
    setBackendName(profile.name ?? profile.machine_label ?? defaultBackendName)
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
        machineLabel,
        backendName,
        workspaceRoots,
        executorEnabled,
      ),
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

  function updateDesktopSettingsDraft(update: Partial<DesktopRuntimeSettings>) {
    setDesktopSettingsDraft((current) => {
      const base = current ?? {
        launch_at_login: false,
        start_minimized_to_tray: false,
        auto_connect_local_runtime: true,
      }
      return { ...base, ...update }
    })
  }

  async function handleSaveDesktopSettings() {
    if (!desktopApp || !desktopSettingsDraft) return
    setDesktopSettingsBusy(true)
    setDesktopSettingsMessage(null)
    try {
      const saved = await desktopApp.saveSettings(desktopSettingsDraft)
      const autostart = await desktopApp.getAutostartStatus()
      const normalized = { ...saved, launch_at_login: autostart.enabled }
      setDesktopSettings(normalized)
      setDesktopSettingsDraft(normalized)
      setAutostartStatus(autostart)
      setDesktopSettingsMessage('桌面设置已保存')
    } catch (err) {
      setDesktopSettingsMessage(formatError(err))
    } finally {
      setDesktopSettingsBusy(false)
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
      <RuntimeDiagnosticsOverview
        diagnostics={diagnostics}
        manualRetryDisabled={isBusy}
        onManualRetry={() => void startRuntimeFromCurrentProfile()}
      />

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
          <RuntimeStat label="本机身份" value={machineLabel || machineId || '保存 profile 后生成'} />
          <RuntimeStat label="Scope" value="Personal / private" />
          <RuntimeStat label="能力" value={`${snapshot?.mcp_server_count ?? 0} MCP · ${snapshot?.executor_enabled ? 'Executor 开启' : 'Executor 关闭'}`} />
        </div>

        <div className="mt-4 flex flex-wrap justify-end gap-2 border-t border-border pt-4">
          <Button size="sm" onClick={() => void client.runtimeSnapshot().then(setSnapshot)} disabled={isBusy}>
            刷新状态
          </Button>
          <Button size="sm" variant="primary" onClick={() => void startRuntimeFromCurrentProfile()} disabled={isBusy}>
            启动/重试
          </Button>
          <Button size="sm" onClick={() => void handleRestart()} disabled={isBusy || !isRunning}>
            重启
          </Button>
          <Button size="sm" onClick={() => void handleStop()} disabled={isBusy || !snapshot}>
            停止
          </Button>
        </div>
      </Card>

      <CapabilityHealthSection items={snapshot?.capability_health ?? []} />

      {desktopApp && (
        <Card>
          <CardHeader
            actions={
              <Button
                size="sm"
                variant="primary"
                onClick={() => void handleSaveDesktopSettings()}
                disabled={desktopSettingsBusy || !desktopSettingsDraft}
              >
                {desktopSettingsBusy ? '保存中…' : '保存设置'}
              </Button>
            }
          >
            <h2 className="text-base font-semibold text-foreground">桌面设置</h2>
            <p className="mt-0.5 text-xs text-muted-foreground">
              当前桌面壳的启动偏好与本机 runtime 自动连接行为。
            </p>
          </CardHeader>

          <div className="grid gap-3 md:grid-cols-3">
            <CheckboxField
              label="开机自启动"
              checked={desktopSettingsDraft?.launch_at_login ?? false}
              onChange={(event) => updateDesktopSettingsDraft({ launch_at_login: event.currentTarget.checked })}
              disabled={desktopSettingsBusy || autostartStatus?.supported === false}
            />
            <CheckboxField
              label="启动到托盘"
              checked={desktopSettingsDraft?.start_minimized_to_tray ?? false}
              onChange={(event) => updateDesktopSettingsDraft({ start_minimized_to_tray: event.currentTarget.checked })}
              disabled={desktopSettingsBusy}
            />
            <CheckboxField
              label="启动后自动连接 runtime"
              checked={desktopSettingsDraft?.auto_connect_local_runtime ?? true}
              onChange={(event) => updateDesktopSettingsDraft({ auto_connect_local_runtime: event.currentTarget.checked })}
              disabled={desktopSettingsBusy}
            />
          </div>

          {autostartStatus?.supported === false ? (
            <Notice className="mt-3" tone="warning">
              {autostartStatus.message ?? '当前平台暂不支持开机自启动设置'}
            </Notice>
          ) : null}
          {desktopSettingsMessage ? <Notice className="mt-3">{desktopSettingsMessage}</Notice> : null}
        </Card>
      )}

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
            <Field label="本机身份 ID">
              <TextInput value={machineId || '保存 profile 后由桌面端生成'} readOnly />
              <p className="mt-1 text-[11px] text-muted-foreground">
                只读 local runtime identity，保存或启动时由桌面端覆盖。
              </p>
            </Field>
            <Field label="Backend 名称">
              <TextInput
                value={backendName}
                onChange={(event) => setBackendName(event.target.value)}
                placeholder="默认使用机器标签"
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

function RuntimeDiagnosticsOverview({
  diagnostics,
  manualRetryDisabled,
  onManualRetry,
}: {
  diagnostics: RuntimeDiagnosticsSnapshot
  manualRetryDisabled: boolean
  onManualRetry: () => void
}) {
  const relayState = diagnostics.relay_connection?.state ?? 'not_configured'
  const chain = [
    diagnostics.cloud_api,
    diagnostics.desktop_api,
    diagnostics.local_runtime,
    diagnostics.runner,
    diagnostics.relay_connection
      ? {
          state: relayLayerState(diagnostics.relay_connection.state),
          label: 'Relay',
          target: diagnostics.relay_connection.target,
          message: diagnostics.relay_connection.last_error,
        }
      : {
          state: 'unknown' as LayerState,
          label: 'Relay',
          target: null,
          message: '等待 runtime/runner 结构化上报',
        },
  ].filter((item): item is { state: LayerState; label: string; target: string | null; message: string | null } => item !== null)
  const worst = chain.reduce<LayerState>((current, layer) => (
    layerSeverity(layer.state) > layerSeverity(current) ? layer.state : current
  ), 'healthy')

  return (
    <Card>
      <CardHeader
        actions={
          <div className="flex items-center gap-2">
            <Button size="sm" onClick={onManualRetry} disabled={manualRetryDisabled}>
              手动重试
            </Button>
            <Badge variant={layerBadgeVariant(worst)}>{layerStateText(worst)}</Badge>
          </div>
        }
      >
        <h2 className="text-base font-semibold text-foreground">运行状态诊断</h2>
        <p className="mt-0.5 text-xs text-muted-foreground">
          按事实源区分 Cloud API、Desktop API、Local Runtime、Runner 与 relay 连接。
        </p>
      </CardHeader>

      <div className="grid gap-2 md:grid-cols-5">
        {chain.map((layer) => (
          <div key={layer.label} className="rounded-[8px] border border-border bg-background/80 px-3 py-2">
            <div className="flex items-center gap-2">
              <StatusDot tone={layerDotTone(layer.state)} />
              <span className="text-xs font-medium text-foreground">{layer.label}</span>
            </div>
            <p className="mt-1 text-xs text-muted-foreground">{layerStateText(layer.state)}</p>
            {layer.target ? (
              <p className="mt-1 truncate font-mono text-[11px] text-foreground/80" title={layer.target}>
                {layer.target}
              </p>
            ) : null}
            {layer.message ? (
              <p className="mt-1 line-clamp-2 text-[11px] text-muted-foreground" title={layer.message}>
                {layer.message}
              </p>
            ) : null}
          </div>
        ))}
      </div>

      <div className="mt-4 grid gap-3 md:grid-cols-3">
        <div className="rounded-[8px] border border-border bg-background/80 px-3 py-2">
          <p className="text-xs font-medium text-muted-foreground">原生 Supervisor</p>
          {diagnostics.local_runtime ? (
            <div className="mt-2 grid grid-cols-[96px_minmax(0,1fr)] gap-x-3 gap-y-1 text-xs">
              <DiagnosticsRow label="状态" value={runtimeStateText(diagnostics.local_runtime.raw_state)} />
              <DiagnosticsRow label="宿主" value={diagnostics.local_runtime.owner ?? '-'} mono />
              <DiagnosticsRow label="最后错误" value={diagnostics.local_runtime.last_error ?? '-'} />
              <DiagnosticsRow label="重试" value={retryText(diagnostics.local_runtime.retry_count, diagnostics.local_runtime.next_retry_at)} />
              <DiagnosticsRow label="上次尝试" value={formatOptionalTimestamp(diagnostics.local_runtime.last_attempt_at)} />
            </div>
          ) : (
            <p className="mt-2 text-xs text-muted-foreground">等待桌面宿主上报本机 runtime snapshot。</p>
          )}
        </div>

        <div className="rounded-[8px] border border-border bg-background/80 px-3 py-2">
          <p className="text-xs font-medium text-muted-foreground">注册与身份</p>
          {diagnostics.registration ? (
            <div className="mt-2 grid grid-cols-[120px_minmax(0,1fr)] gap-x-3 gap-y-1 text-xs">
              <DiagnosticsRow label="来源" value={registrationSourceText(diagnostics.registration.source)} />
              <DiagnosticsRow label="Backend" value={diagnostics.registration.backend_id} mono />
              <DiagnosticsRow label="机器" value={diagnostics.registration.machine_label ?? diagnostics.registration.machine_id ?? '-'} />
              <DiagnosticsRow label="Scope" value={scopeText(diagnostics.registration.share_scope_kind, diagnostics.registration.share_scope_id)} />
              <DiagnosticsRow label="能力槽" value={diagnostics.registration.capability_slot ?? 'default'} mono />
              <DiagnosticsRow label="Last seen" value={formatOptionalTimestamp(diagnostics.registration.last_seen_at)} />
            </div>
          ) : (
            <p className="mt-2 text-xs text-muted-foreground">
              暂无明确注册来源；需要 server/runner 返回 `registration_source` 后展示。
            </p>
          )}
        </div>

        <div className="rounded-[8px] border border-border bg-background/80 px-3 py-2">
          <p className="text-xs font-medium text-muted-foreground">Runner 交接</p>
          {diagnostics.runner ? (
            <div className="mt-2 space-y-1 text-xs text-muted-foreground">
              <p className="text-foreground">{diagnostics.runner.name}</p>
              <p>状态：{layerStateText(diagnostics.runner.state)} · {diagnostics.runner.online ? 'registry 在线' : 'registry 离线'}</p>
              <p>执行占用：{diagnostics.runner.active_session_count ?? 0} 个活跃会话</p>
              <p>独立 runner 由 systemd / Windows Service 或前台进程管理，桌面 UI 仅展示状态交接。</p>
            </div>
          ) : (
            <p className="mt-2 text-xs text-muted-foreground">
              未发现独立 runner backend；桌面托管 runtime 可使用下方按钮管理。
            </p>
          )}
        </div>
      </div>

      {!diagnostics.relay_connection ? (
        <Notice className="mt-3" tone="info">
          Relay 连接状态需要 local runtime 或 runner 提供结构化 snapshot；当前不会从日志或 backend.online 推断。
        </Notice>
      ) : (
        <Notice className="mt-3" tone={relayState === 'registered' ? 'success' : 'warning'}>
          Relay：{relayStateText(relayState)}
        </Notice>
      )}
    </Card>
  )
}

function DiagnosticsRow({
  label,
  value,
  mono = false,
}: {
  label: string
  value: string
  mono?: boolean
}) {
  return (
    <>
      <span className="text-muted-foreground">{label}</span>
      <span className={cn('truncate text-foreground', mono && 'font-mono')} title={value}>{value}</span>
    </>
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
  machineLabel: string,
  backendName: string,
  roots: string[],
  executorEnabled: boolean,
): RuntimeStartRequest {
  return {
    server_url: serverUrl.trim(),
    access_token: accessToken.trim(),
    profile_id: profileId.trim() || DEFAULT_LOCAL_RUNTIME_PROFILE_ID,
    machine_id: '',
    machine_label: machineLabel.trim() || null,
    name: backendName.trim() || machineLabel.trim() || undefined,
    workspace_roots: roots.map((root) => root.trim()).filter(Boolean),
    executor_enabled: executorEnabled,
  }
}

function stateText(state: LocalRuntimeStatus['state']) {
  switch (state) {
    case 'idle':
      return '待命'
    case 'disabled':
      return '未启用'
    case 'waiting_for_auth':
      return '等待登录'
    case 'waiting_for_api':
      return '等待 API'
    case 'claiming':
      return '领取凭据中'
    case 'starting':
      return '启动中'
    case 'running':
      return '运行中'
    case 'retrying':
      return '重试中'
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
    case 'waiting_for_auth':
    case 'waiting_for_api':
    case 'claiming':
    case 'starting':
    case 'retrying':
    case 'stopping':
      return 'primary'
    case 'error':
      return 'danger'
    case 'stopped':
    default:
      return 'neutral'
  }
}

function runtimeStateText(state: LocalRuntimeStatus['state'] | null): string {
  return state ? stateText(state) : '-'
}

function retryText(retryCount: number | null, nextRetryAt: string | null): string {
  if (retryCount === null && !nextRetryAt) return '-'
  const countText = retryCount === null ? '未知次数' : `${retryCount} 次`
  return `${countText} · 下次 ${formatOptionalTimestamp(nextRetryAt)}`
}

function layerSeverity(state: LayerState): number {
  switch (state) {
    case 'unavailable':
      return 5
    case 'degraded':
      return 4
    case 'unknown':
      return 3
    case 'checking':
      return 2
    case 'disabled':
      return 1
    case 'healthy':
      return 0
  }
}

function layerStateText(state: LayerState): string {
  switch (state) {
    case 'healthy':
      return '正常'
    case 'checking':
      return '检查中'
    case 'degraded':
      return '降级'
    case 'unavailable':
      return '不可用'
    case 'disabled':
      return '未启用'
    case 'unknown':
      return '未知'
  }
}

function layerBadgeVariant(state: LayerState): 'success' | 'primary' | 'danger' | 'warning' | 'neutral' {
  switch (state) {
    case 'healthy':
      return 'success'
    case 'checking':
      return 'primary'
    case 'degraded':
      return 'warning'
    case 'unavailable':
      return 'danger'
    case 'disabled':
    case 'unknown':
      return 'neutral'
  }
}

function layerDotTone(state: LayerState): 'success' | 'warning' | 'danger' | 'info' | 'muted' {
  switch (state) {
    case 'healthy':
      return 'success'
    case 'checking':
      return 'info'
    case 'degraded':
      return 'warning'
    case 'unavailable':
      return 'danger'
    case 'disabled':
    case 'unknown':
      return 'muted'
  }
}

function relayLayerState(state: string): LayerState {
  switch (state) {
    case 'registered':
      return 'healthy'
    case 'connecting':
    case 'reconnecting':
      return 'checking'
    case 'error':
    case 'disconnected':
      return 'unavailable'
    case 'not_configured':
    default:
      return 'unknown'
  }
}

function relayStateText(state: string): string {
  switch (state) {
    case 'registered':
      return '已注册并连接'
    case 'connecting':
      return '连接中'
    case 'reconnecting':
      return '重连中'
    case 'disconnected':
      return '已断开'
    case 'error':
      return '连接错误'
    case 'not_configured':
    default:
      return '未配置'
  }
}

function registrationSourceText(source: string): string {
  if (source === 'desktop_access_token') return '桌面登录授权'
  if (source === 'runner_registration_token') return 'Runner 注册令牌'
  return source
}

function scopeText(kind: string | null, id: string | null): string {
  if (!kind) return '-'
  if (!id) return kind
  return `${kind} / ${id}`
}

function formatOptionalTimestamp(value: string | null) {
  if (!value) return '-'
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return value
  return date.toLocaleString()
}

function formatError(error: unknown) {
  return error instanceof Error ? error.message : String(error)
}

function formatTime(timestamp: string) {
  const date = new Date(timestamp)
  if (Number.isNaN(date.getTime())) return timestamp
  return date.toLocaleTimeString()
}

// ─── Capability Health ───────────────────────────────────────────────────────

export function CapabilityHealthNotice({ items }: { items: LocalCapabilityHealthItem[] }) {
  const unhealthy = items.filter((item) => item.status !== 'ready')
  if (unhealthy.length === 0) return null

  const names = unhealthy.map((item) => item.label).join('、')
  return (
    <Notice tone="warning">
      {unhealthy.length} 个声明能力不可用（{names}），相关工具本次对话可能缺失。
    </Notice>
  )
}

function CapabilityHealthSection({ items }: { items: LocalCapabilityHealthItem[] }) {
  const unhealthy = items.filter((item) => item.status !== 'ready')
  if (unhealthy.length === 0) return null

  return (
    <Card>
      <CardHeader>声明能力状态</CardHeader>
      <div className="space-y-2 px-4 pb-4">
        {unhealthy.map((item) => (
          <div
            key={item.id}
            className="flex items-start gap-2 rounded-md border border-border px-3 py-2 overflow-hidden"
          >
            <StatusDot tone={item.status === 'degraded' ? 'warning' : 'danger'} className="mt-1 shrink-0" />
            <div className="min-w-0 flex-1">
              <div className="flex items-center gap-2">
                <span className="text-sm font-medium truncate">{item.label}</span>
                <Badge variant={item.status === 'degraded' ? 'warning' : 'danger'}>
                  {item.status === 'degraded' ? '降级' : '不可用'}
                </Badge>
              </div>
              <p className="mt-0.5 text-xs text-muted truncate">{item.summary}</p>
            </div>
          </div>
        ))}
      </div>
    </Card>
  )
}
