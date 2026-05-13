import { FormEvent, useEffect, useMemo, useState } from 'react'
import {
  LocalRuntimeStatus,
  RuntimeStartRequest,
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

  useEffect(() => {
    let alive = true
    const refresh = async () => {
      try {
        const next = await runtimeSnapshot()
        if (alive) setSnapshot(next)
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

export default App
