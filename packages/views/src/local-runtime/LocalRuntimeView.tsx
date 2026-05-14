import { useEffect, useMemo, useState } from 'react'
import type { FormEvent } from 'react'
import {
  DEFAULT_LOCAL_RUNTIME_BACKEND_NAME,
  DEFAULT_LOCAL_RUNTIME_PROFILE_ID,
  DEFAULT_LOCAL_RUNTIME_SERVER_URL,
  formatLocalLogLine,
  normalizeMcpLocalServer,
  parseRuntimeEnv,
  parseRuntimeLines,
} from '@agentdash/core/local-runtime'
import type {
  LocalLogEvent,
  LocalRuntimeClient,
  LocalRuntimeProfile,
  LocalRuntimeStatus,
  McpLocalServerEntry,
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
  Textarea,
  TextInput,
  cn,
} from '@agentdash/ui'

export interface LocalRuntimeViewProps {
  client: LocalRuntimeClient
  defaultServerUrl?: string
  defaultAccessToken?: string
  defaultProfileId?: string
  defaultBackendName?: string
}

export function LocalRuntimeView({
  client,
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
  const [legacyMachineIds, setLegacyMachineIds] = useState('')
  const [backendName, setBackendName] = useState(defaultBackendName)
  const [accessibleRoots, setAccessibleRoots] = useState('')
  const [executorEnabled, setExecutorEnabled] = useState(true)
  const [autoStart, setAutoStart] = useState(false)
  const [isBusy, setIsBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [profileMessage, setProfileMessage] = useState<string | null>(null)
  const [mcpRoot, setMcpRoot] = useState('')
  const [mcpServers, setMcpServers] = useState<McpLocalServerEntry[]>([])
  const [mcpMessage, setMcpMessage] = useState<string | null>(null)
  const [probingIndex, setProbingIndex] = useState<number | null>(null)
  const [logs, setLogs] = useState<LocalLogEvent[]>([])
  const [logLevel, setLogLevel] = useState('all')

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

  const roots = useMemo(() => parseRuntimeLines(accessibleRoots), [accessibleRoots])

  useEffect(() => {
    if (!mcpRoot && roots[0]) {
      setMcpRoot(roots[0])
    }
  }, [mcpRoot, roots])

  const effectiveMcpRoot = mcpRoot.trim() || roots[0] || ''
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
        parseRuntimeLines(legacyMachineIds),
        backendName,
        roots,
        executorEnabled,
      )
      setSnapshot(await client.runtimeStart(request))
    } catch (err) {
      setError(formatError(err))
    } finally {
      setIsBusy(false)
    }
  }

  async function handleLoadProfile() {
    setProfileMessage(null)
    try {
      const profile = await client.profileLoad()
      if (!profile) {
        setProfileMessage('尚未保存本机 profile')
        return
      }
      applyProfile(profile)
      setProfileMessage('已加载本机 profile')
    } catch (err) {
      setProfileMessage(formatError(err))
    }
  }

  async function handleSaveProfile() {
    setProfileMessage(null)
    try {
      const profile = await client.profileSave(buildProfile())
      applyProfile(profile)
      setProfileMessage('已保存本机 profile')
    } catch (err) {
      setProfileMessage(formatError(err))
    }
  }

  async function handleDeleteProfile() {
    setProfileMessage(null)
    try {
      await client.profileDelete()
      setProfileMessage('已删除本机 profile')
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
    setLegacyMachineIds((profile.legacy_machine_ids ?? []).join('\n'))
    setBackendName(profile.name ?? defaultBackendName)
    setAccessibleRoots(profile.accessible_roots.join('\n'))
    setExecutorEnabled(profile.executor_enabled)
    setAutoStart(profile.auto_start)
    if (profile.accessible_roots[0]) {
      setMcpRoot(profile.accessible_roots[0])
    }
  }

  function buildProfile(): LocalRuntimeProfile {
    return {
      ...buildStartRequest(
        serverUrl,
        accessToken,
        profileId,
        machineId,
        machineLabel,
        parseRuntimeLines(legacyMachineIds),
        backendName,
        roots,
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

  async function handleLoadMcpServers() {
    setMcpMessage(null)
    try {
      setMcpServers(await client.mcpServersLoad(effectiveMcpRoot))
      setMcpMessage('已加载 MCP servers')
    } catch (err) {
      setMcpMessage(formatError(err))
    }
  }

  async function handleSaveMcpServers() {
    setMcpMessage(null)
    try {
      await client.mcpServersSave(effectiveMcpRoot, mcpServers.map(normalizeMcpLocalServer))
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

  function addMcpServer(transport: McpLocalServerEntry['transport']) {
    const baseName = `mcp-${mcpServers.length + 1}`
    setMcpServers((current) => [
      ...current,
      transport === 'stdio'
        ? { name: baseName, transport, command: '', args: [], env: [] }
        : { name: baseName, transport, url: '' },
    ])
  }

  function updateMcpServer(index: number, patch: Partial<McpLocalServerEntry>) {
    setMcpServers((current) =>
      current.map((server, currentIndex) =>
        currentIndex === index ? { ...server, ...patch } : server,
      ),
    )
  }

  function removeMcpServer(index: number) {
    setMcpServers((current) => current.filter((_, currentIndex) => currentIndex !== index))
  }

  const stateLabel = snapshot ? stateText(snapshot.state) : '未启动'

  return (
    <section className="min-w-0 p-6">
      <header className="mb-5 flex items-center justify-between gap-4">
        <div>
          <h1 className="text-xl font-semibold tracking-normal text-foreground">Local Runtime</h1>
          <p className="mt-1 text-sm text-muted-foreground">机器身份由桌面端持久化，server 按 scope 领取本机 runtime</p>
        </div>
        <Badge variant={stateBadgeVariant(snapshot?.state)}>{stateLabel}</Badge>
      </header>

      <div className="grid gap-4 xl:grid-cols-[minmax(320px,0.95fr)_minmax(420px,1.35fr)]">
        <Card>
          <CardHeader
            actions={
              <Button onClick={() => void client.runtimeSnapshot().then(setSnapshot)} disabled={isBusy}>
                刷新
              </Button>
            }
          >
            <h2 className="text-sm font-semibold text-foreground">Runtime Snapshot</h2>
          </CardHeader>

          <dl className="grid gap-3">
            <RuntimeStat label="Backend" value={snapshot?.backend_id ?? '—'} />
            <RuntimeStat label="Name" value={snapshot?.name ?? backendName} />
            <RuntimeStat label="Machine" value={machineLabel || machineId || '由桌面端自动生成'} />
            <RuntimeStat label="Scope" value="Personal / private / default" />
            <RuntimeStat label="Executors" value={snapshot ? (snapshot.executor_enabled ? 'enabled' : 'disabled') : '—'} />
            <RuntimeStat label="MCP Servers" value={String(snapshot?.mcp_server_count ?? 0)} />
          </dl>

          <div className="mt-4 grid gap-2 border-t border-border pt-4">
            {(snapshot?.accessible_roots.length ? snapshot.accessible_roots : roots).map((root) => (
              <code key={root} className="rounded-[6px] bg-secondary px-2.5 py-2 text-xs text-muted-foreground wrap-anywhere">
                {root}
              </code>
            ))}
            {!snapshot?.accessible_roots.length && roots.length === 0 ? (
              <span className="rounded-[6px] bg-secondary px-2.5 py-2 text-xs text-muted-foreground">未配置 accessible roots</span>
            ) : null}
          </div>
        </Card>

        <Card as="form" onSubmit={handleStart}>
          <CardHeader
            actions={
              <div className="flex flex-wrap items-center justify-end gap-3">
                <CheckboxField checked={executorEnabled} label="Executor" onChange={(event) => setExecutorEnabled(event.target.checked)} />
                <CheckboxField checked={autoStart} label="Auto start" onChange={(event) => setAutoStart(event.target.checked)} />
              </div>
            }
          >
            <h2 className="text-sm font-semibold text-foreground">Start Runtime</h2>
          </CardHeader>

          <div className="grid gap-3">
            <Field label="Server URL">
              <TextInput value={serverUrl} onChange={(event) => setServerUrl(event.target.value)} />
            </Field>

            <Field label="Access Token (optional)">
              <TextInput
                autoComplete="current-password"
                value={accessToken}
                onChange={(event) => setAccessToken(event.target.value)}
                type="password"
              />
            </Field>

            <div className="grid gap-3 md:grid-cols-2">
              <Field label="Profile ID">
                <TextInput value={profileId} onChange={(event) => setProfileId(event.target.value)} />
              </Field>

              <Field label="Machine Label">
                <TextInput value={machineLabel} onChange={(event) => setMachineLabel(event.target.value)} placeholder="默认使用本机 hostname" />
              </Field>
            </div>

            <Field label="Machine ID">
              <TextInput value={machineId || '保存 profile 后由桌面端生成'} readOnly />
            </Field>

            <Field label="Legacy Machine IDs">
              <Textarea
                value={legacyMachineIds}
                onChange={(event) => setLegacyMachineIds(event.target.value)}
                placeholder="每行一个旧 hostname / device id，用于身份合并"
              />
            </Field>

            <Field label="Backend Name">
              <TextInput value={backendName} onChange={(event) => setBackendName(event.target.value)} />
            </Field>

            <Field label="Accessible Roots">
              <Textarea
                value={accessibleRoots}
                onChange={(event) => setAccessibleRoots(event.target.value)}
                placeholder="每行一个绝对路径"
              />
            </Field>
          </div>

          {error ? <Notice className="mt-3" tone="danger">{error}</Notice> : null}
          {profileMessage ? <Notice className="mt-3">{profileMessage}</Notice> : null}

          <div className="mt-4 flex flex-wrap justify-end gap-2">
            <Button type="submit" variant="primary" disabled={isBusy || !serverUrl.trim()}>
              启动
            </Button>
            <Button variant="danger" onClick={() => void handleStop()} disabled={isBusy || !snapshot}>
              停止
            </Button>
            <Button onClick={() => void handleRestart()} disabled={isBusy || snapshot?.state !== 'running'}>
              重启
            </Button>
          </div>

          <div className="mt-2 flex flex-wrap justify-end gap-2">
            <Button onClick={() => void handleLoadProfile()}>加载 profile</Button>
            <Button onClick={() => void handleSaveProfile()} disabled={!serverUrl.trim()}>
              保存 profile
            </Button>
            <Button variant="danger" onClick={() => void handleDeleteProfile()}>
              删除 profile
            </Button>
          </div>
        </Card>

        <Card className="xl:col-span-2">
          <CardHeader
            actions={
              <>
                <Button onClick={() => addMcpServer('stdio')}>添加 stdio</Button>
                <Button onClick={() => addMcpServer('http')}>添加 HTTP</Button>
              </>
            }
          >
            <h2 className="text-sm font-semibold text-foreground">MCP Servers</h2>
          </CardHeader>

          <div className="mb-4 grid gap-3 md:grid-cols-[minmax(360px,1fr)_auto] md:items-end">
            <Field label="Config Root">
              <TextInput
                value={mcpRoot}
                onChange={(event) => setMcpRoot(event.target.value)}
                placeholder="保存到 root/.agentdash/local-backend.json"
              />
            </Field>
            <div className="flex gap-2">
              <Button onClick={() => void handleLoadMcpServers()} disabled={!effectiveMcpRoot}>
                加载
              </Button>
              <Button variant="primary" onClick={() => void handleSaveMcpServers()} disabled={!effectiveMcpRoot}>
                保存
              </Button>
            </div>
          </div>

          <div className="grid gap-3">
            {mcpServers.map((server, index) => (
              <Card as="article" className="bg-background/60" key={`${server.name}-${index}`}>
                <div className="grid gap-3 lg:grid-cols-[minmax(0,1fr)_auto]">
                  <div className="grid gap-3 md:grid-cols-[minmax(140px,0.8fr)_minmax(110px,0.55fr)_repeat(2,minmax(180px,1fr))]">
                    <Field label="Name">
                      <TextInput value={server.name} onChange={(event) => updateMcpServer(index, { name: event.target.value })} />
                    </Field>

                    <Field label="Transport">
                      <Select
                        value={server.transport}
                        onChange={(event) => updateMcpServer(index, { transport: event.target.value as McpLocalServerEntry['transport'] })}
                      >
                        <option value="stdio">stdio</option>
                        <option value="http">http</option>
                        <option value="sse">sse</option>
                      </Select>
                    </Field>

                    {server.transport === 'stdio' ? (
                      <>
                        <Field label="Command" className="md:col-span-2">
                          <TextInput value={server.command ?? ''} onChange={(event) => updateMcpServer(index, { command: event.target.value })} />
                        </Field>
                        <Field label="Args">
                          <Textarea
                            value={(server.args ?? []).join('\n')}
                            onChange={(event) => updateMcpServer(index, { args: parseRuntimeLines(event.target.value) })}
                            placeholder="每行一个参数"
                          />
                        </Field>
                        <Field label="Env">
                          <Textarea
                            value={(server.env ?? []).map((entry) => `${entry.name}=${entry.value}`).join('\n')}
                            onChange={(event) => updateMcpServer(index, { env: parseRuntimeEnv(event.target.value) })}
                            placeholder="NAME=value"
                          />
                        </Field>
                      </>
                    ) : (
                      <Field label="URL" className="md:col-span-2">
                        <TextInput value={server.url ?? ''} onChange={(event) => updateMcpServer(index, { url: event.target.value })} />
                      </Field>
                    )}
                  </div>

                  <div className="flex items-start gap-2">
                    <Button onClick={() => void handleProbeMcpServer(index)} disabled={probingIndex === index}>
                      {probingIndex === index ? '探测中' : '探测'}
                    </Button>
                    <Button variant="danger" onClick={() => removeMcpServer(index)}>
                      删除
                    </Button>
                  </div>
                </div>
              </Card>
            ))}
            {mcpServers.length === 0 ? <EmptyState>当前 root 未配置 MCP servers</EmptyState> : null}
          </div>

          {mcpMessage ? <Notice className="mt-3">{mcpMessage}</Notice> : null}
        </Card>

        <Card className="xl:col-span-2">
          <CardHeader
            actions={
              <>
                <Select value={logLevel} onChange={(event) => setLogLevel(event.target.value)}>
                  <option value="all">全部</option>
                  <option value="info">info</option>
                  <option value="warn">warn</option>
                  <option value="error">error</option>
                </Select>
                <Button onClick={() => void handleRefreshLogs()}>刷新</Button>
                <Button onClick={() => void handleCopyLogs()} disabled={visibleLogs.length === 0}>
                  复制
                </Button>
                <Button variant="danger" onClick={() => void handleClearLogs()}>
                  清空
                </Button>
              </>
            }
          >
            <h2 className="text-sm font-semibold text-foreground">Runtime Logs</h2>
          </CardHeader>

          <div className="grid gap-2">
            {visibleLogs.map((log) => (
              <div
                className={cn(
                  'grid gap-2 rounded-[8px] border-l-4 bg-secondary/35 px-3 py-2 text-xs md:grid-cols-[92px_58px_110px_minmax(0,1fr)]',
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
      </div>
    </section>
  )
}

function RuntimeStat({ label, value }: { label: string; value: string }) {
  return (
    <div className="grid gap-1">
      <dt className="text-[11px] font-semibold uppercase tracking-wide text-muted-foreground">{label}</dt>
      <dd className="m-0 text-sm text-foreground wrap-anywhere">{value}</dd>
    </div>
  )
}

function buildStartRequest(
  serverUrl: string,
  accessToken: string,
  profileId: string,
  machineId: string,
  machineLabel: string,
  legacyMachineIds: string[],
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
    legacy_machine_ids: legacyMachineIds,
    name: backendName.trim() || undefined,
    accessible_roots: roots,
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

function stateBadgeVariant(state?: LocalRuntimeStatus['state']) {
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
