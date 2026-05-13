import { FormEvent, useEffect, useMemo, useState } from 'react'
import {
  LocalLogEvent,
  LocalRuntimeStatus,
  McpLocalServerEntry,
  RuntimeStartRequest,
  logsClear,
  logsTail,
  mcpServerProbe,
  mcpServersLoad,
  mcpServersSave,
  runtimeSnapshot,
  runtimeStart,
  runtimeStop,
} from './runtimeApi'

const DEFAULT_CLOUD_URL = 'ws://127.0.0.1:3001/ws/backend'

function App() {
  const [snapshot, setSnapshot] = useState<LocalRuntimeStatus | null>(null)
  const [cloudUrl, setCloudUrl] = useState(DEFAULT_CLOUD_URL)
  const [token, setToken] = useState('')
  const [backendName, setBackendName] = useState('desktop-local-backend')
  const [accessibleRoots, setAccessibleRoots] = useState('')
  const [executorEnabled, setExecutorEnabled] = useState(true)
  const [isBusy, setIsBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [mcpRoot, setMcpRoot] = useState('')
  const [mcpServers, setMcpServers] = useState<McpLocalServerEntry[]>([])
  const [mcpMessage, setMcpMessage] = useState<string | null>(null)
  const [probingIndex, setProbingIndex] = useState<number | null>(null)
  const [logs, setLogs] = useState<LocalLogEvent[]>([])
  const [logLevel, setLogLevel] = useState('all')

  useEffect(() => {
    let alive = true
    const refresh = async () => {
      try {
        const next = await runtimeSnapshot()
        if (alive) setSnapshot(next)
        const nextLogs = await logsTail()
        if (alive) setLogs(nextLogs)
      } catch (err) {
        if (alive) setError(formatError(err))
      }
    }
    void refresh()
    const timer = window.setInterval(refresh, 1500)
    return () => {
      alive = false
      window.clearInterval(timer)
    }
  }, [])

  const roots = useMemo(
    () =>
      accessibleRoots
        .split('\n')
        .map((root) => root.trim())
        .filter(Boolean),
    [accessibleRoots],
  )

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
      const request: RuntimeStartRequest = {
        cloud_url: cloudUrl.trim(),
        token: token.trim(),
        name: backendName.trim() || undefined,
        accessible_roots: roots,
        executor_enabled: executorEnabled,
      }
      setSnapshot(await runtimeStart(request))
    } catch (err) {
      setError(formatError(err))
    } finally {
      setIsBusy(false)
    }
  }

  async function handleStop() {
    setIsBusy(true)
    setError(null)
    try {
      await runtimeStop()
      setSnapshot(await runtimeSnapshot())
    } catch (err) {
      setError(formatError(err))
    } finally {
      setIsBusy(false)
    }
  }

  async function handleRefreshLogs() {
    try {
      setLogs(await logsTail())
    } catch (err) {
      setError(formatError(err))
    }
  }

  async function handleClearLogs() {
    try {
      await logsClear()
      setLogs(await logsTail())
    } catch (err) {
      setError(formatError(err))
    }
  }

  async function handleCopyLogs() {
    try {
      const content = visibleLogs.map(formatLogLine).join('\n')
      await navigator.clipboard.writeText(content)
    } catch (err) {
      setError(formatError(err))
    }
  }

  async function handleLoadMcpServers() {
    setMcpMessage(null)
    try {
      setMcpServers(await mcpServersLoad(effectiveMcpRoot))
      setMcpMessage('已加载 MCP servers')
    } catch (err) {
      setMcpMessage(formatError(err))
    }
  }

  async function handleSaveMcpServers() {
    setMcpMessage(null)
    try {
      await mcpServersSave(effectiveMcpRoot, mcpServers.map(normalizeMcpServer))
      setMcpMessage('已保存 MCP servers')
    } catch (err) {
      setMcpMessage(formatError(err))
    }
  }

  async function handleProbeMcpServer(index: number) {
    setProbingIndex(index)
    setMcpMessage(null)
    try {
      const result = await mcpServerProbe(normalizeMcpServer(mcpServers[index]))
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
    <main className="desktop-shell">
      <aside className="sidebar">
        <div className="brand">AgentDash</div>
        <nav className="nav-list" aria-label="桌面端导航">
          <button className="nav-item active" type="button">Runtime</button>
          <button className="nav-item" type="button" disabled>Dashboard</button>
          <button className="nav-item" type="button" disabled>Settings</button>
        </nav>
      </aside>

      <section className="workspace">
        <header className="topbar">
          <div>
            <h1>Local Runtime</h1>
            <p>状态源：Tauri command → agentdash-local library</p>
          </div>
          <div className={`status-pill state-${snapshot?.state ?? 'stopped'}`}>{stateLabel}</div>
        </header>

        <div className="content-grid">
          <section className="panel runtime-panel">
            <div className="panel-header">
              <h2>Runtime Snapshot</h2>
              <button className="secondary-button" type="button" onClick={() => void runtimeSnapshot().then(setSnapshot)} disabled={isBusy}>
                刷新
              </button>
            </div>
            <dl className="status-list">
              <div>
                <dt>Backend</dt>
                <dd>{snapshot?.backend_id ?? '—'}</dd>
              </div>
              <div>
                <dt>Name</dt>
                <dd>{snapshot?.name ?? backendName}</dd>
              </div>
              <div>
                <dt>Executors</dt>
                <dd>{snapshot ? (snapshot.executor_enabled ? 'enabled' : 'disabled') : '—'}</dd>
              </div>
              <div>
                <dt>MCP Servers</dt>
                <dd>{snapshot?.mcp_server_count ?? 0}</dd>
              </div>
            </dl>
            <div className="roots-box">
              {(snapshot?.accessible_roots.length ? snapshot.accessible_roots : roots).map((root) => (
                <code key={root}>{root}</code>
              ))}
              {!snapshot?.accessible_roots.length && roots.length === 0 ? <span>未配置 accessible roots</span> : null}
            </div>
          </section>

          <form className="panel control-panel" onSubmit={handleStart}>
            <div className="panel-header">
              <h2>Start Runtime</h2>
              <label className="switch">
                <input
                  checked={executorEnabled}
                  onChange={(event) => setExecutorEnabled(event.target.checked)}
                  type="checkbox"
                />
                <span>Executor</span>
              </label>
            </div>

            <label className="field">
              <span>Cloud WebSocket URL</span>
              <input value={cloudUrl} onChange={(event) => setCloudUrl(event.target.value)} />
            </label>

            <label className="field">
              <span>Backend Token</span>
              <input
                autoComplete="current-password"
                value={token}
                onChange={(event) => setToken(event.target.value)}
                type="password"
              />
            </label>

            <label className="field">
              <span>Backend Name</span>
              <input value={backendName} onChange={(event) => setBackendName(event.target.value)} />
            </label>

            <label className="field">
              <span>Accessible Roots</span>
              <textarea
                value={accessibleRoots}
                onChange={(event) => setAccessibleRoots(event.target.value)}
                placeholder="每行一个绝对路径"
              />
            </label>

            {error ? <div className="error-box">{error}</div> : null}

            <div className="actions">
              <button className="primary-button" type="submit" disabled={isBusy || !cloudUrl.trim() || !token.trim()}>
                启动
              </button>
              <button className="danger-button" type="button" onClick={() => void handleStop()} disabled={isBusy || !snapshot}>
                停止
              </button>
            </div>
          </form>

          <section className="panel mcp-panel">
            <div className="panel-header">
              <h2>MCP Servers</h2>
              <div className="inline-actions">
                <button className="secondary-button" type="button" onClick={() => addMcpServer('stdio')}>
                  添加 stdio
                </button>
                <button className="secondary-button" type="button" onClick={() => addMcpServer('http')}>
                  添加 HTTP
                </button>
              </div>
            </div>

            <div className="mcp-toolbar">
              <label className="field compact-field">
                <span>Config Root</span>
                <input value={mcpRoot} onChange={(event) => setMcpRoot(event.target.value)} placeholder="保存到 root/.agentdash/local-backend.json" />
              </label>
              <div className="actions toolbar-actions">
                <button className="secondary-button" type="button" onClick={() => void handleLoadMcpServers()} disabled={!effectiveMcpRoot}>
                  加载
                </button>
                <button className="primary-button" type="button" onClick={() => void handleSaveMcpServers()} disabled={!effectiveMcpRoot}>
                  保存
                </button>
              </div>
            </div>

            <div className="server-list">
              {mcpServers.map((server, index) => (
                <article className="server-row" key={`${server.name}-${index}`}>
                  <div className="server-fields">
                    <label className="field compact-field">
                      <span>Name</span>
                      <input value={server.name} onChange={(event) => updateMcpServer(index, { name: event.target.value })} />
                    </label>

                    <label className="field compact-field">
                      <span>Transport</span>
                      <select
                        value={server.transport}
                        onChange={(event) => updateMcpServer(index, { transport: event.target.value as McpLocalServerEntry['transport'] })}
                      >
                        <option value="stdio">stdio</option>
                        <option value="http">http</option>
                        <option value="sse">sse</option>
                      </select>
                    </label>

                    {server.transport === 'stdio' ? (
                      <>
                        <label className="field compact-field wide-field">
                          <span>Command</span>
                          <input value={server.command ?? ''} onChange={(event) => updateMcpServer(index, { command: event.target.value })} />
                        </label>
                        <label className="field compact-field">
                          <span>Args</span>
                          <textarea
                            value={(server.args ?? []).join('\n')}
                            onChange={(event) => updateMcpServer(index, { args: parseLines(event.target.value) })}
                            placeholder="每行一个参数"
                          />
                        </label>
                        <label className="field compact-field">
                          <span>Env</span>
                          <textarea
                            value={(server.env ?? []).map((entry) => `${entry.name}=${entry.value}`).join('\n')}
                            onChange={(event) => updateMcpServer(index, { env: parseEnv(event.target.value) })}
                            placeholder="NAME=value"
                          />
                        </label>
                      </>
                    ) : (
                      <label className="field compact-field wide-field">
                        <span>URL</span>
                        <input value={server.url ?? ''} onChange={(event) => updateMcpServer(index, { url: event.target.value })} />
                      </label>
                    )}
                  </div>

                  <div className="server-actions">
                    <button className="secondary-button" type="button" onClick={() => void handleProbeMcpServer(index)} disabled={probingIndex === index}>
                      {probingIndex === index ? '探测中' : '探测'}
                    </button>
                    <button className="danger-button" type="button" onClick={() => removeMcpServer(index)}>
                      删除
                    </button>
                  </div>
                </article>
              ))}
              {mcpServers.length === 0 ? <div className="empty-state">当前 root 未配置 MCP servers</div> : null}
            </div>

            {mcpMessage ? <div className="message-box">{mcpMessage}</div> : null}
          </section>

          <section className="panel logs-panel">
            <div className="panel-header">
              <h2>Runtime Logs</h2>
              <div className="inline-actions">
                <select className="compact-select" value={logLevel} onChange={(event) => setLogLevel(event.target.value)}>
                  <option value="all">全部</option>
                  <option value="info">info</option>
                  <option value="warn">warn</option>
                  <option value="error">error</option>
                </select>
                <button className="secondary-button" type="button" onClick={() => void handleRefreshLogs()}>
                  刷新
                </button>
                <button className="secondary-button" type="button" onClick={() => void handleCopyLogs()} disabled={visibleLogs.length === 0}>
                  复制
                </button>
                <button className="danger-button" type="button" onClick={() => void handleClearLogs()}>
                  清空
                </button>
              </div>
            </div>

            <div className="log-list">
              {visibleLogs.map((log) => (
                <div className={`log-row log-${log.level}`} key={log.sequence}>
                  <time>{formatTime(log.timestamp)}</time>
                  <span>{log.level}</span>
                  <code>{log.target}</code>
                  <p>{log.message}</p>
                </div>
              ))}
              {visibleLogs.length === 0 ? <div className="empty-state">暂无本机 runtime 日志</div> : null}
            </div>
          </section>
        </div>
      </section>
    </main>
  )
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

function formatError(error: unknown) {
  return error instanceof Error ? error.message : String(error)
}

function formatLogLine(log: LocalLogEvent) {
  return `${log.timestamp} ${log.level.toUpperCase()} ${log.target} ${log.message}`
}

function formatTime(timestamp: string) {
  const date = new Date(timestamp)
  if (Number.isNaN(date.getTime())) return timestamp
  return date.toLocaleTimeString()
}

function parseLines(value: string) {
  return value
    .split('\n')
    .map((line) => line.trim())
    .filter(Boolean)
}

function parseEnv(value: string) {
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

function normalizeMcpServer(server: McpLocalServerEntry): McpLocalServerEntry {
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

export default App
