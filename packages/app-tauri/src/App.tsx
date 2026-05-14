import { useEffect, useMemo, useState } from 'react'
import WebDashboardApp from 'app-web'
import { LocalRuntimeView } from '@agentdash/views/local-runtime'
import { invoke } from '@tauri-apps/api/core'
import { createTauriLocalRuntimeClient } from './runtimeApi'

type DesktopView = 'runtime' | 'dashboard'
type DashboardApiState = 'checking' | 'ready' | 'unavailable'
type DesktopApiSnapshot = {
  state: 'starting' | 'running' | 'error' | 'stopped'
  origin: string
  message?: string | null
  database_url?: string | null
}

const API_ORIGIN = (import.meta.env.VITE_API_ORIGIN ?? '').replace(/\/+$/, '')

function App() {
  const client = useMemo(() => createTauriLocalRuntimeClient(), [])
  const [activeView, setActiveView] = useState<DesktopView>('runtime')

  return (
    <main className="desktop-shell">
      <aside className="sidebar">
        <div className="brand">AgentDash</div>
        <nav className="nav-list" aria-label="桌面端导航">
          <button
            className={`nav-item ${activeView === 'runtime' ? 'active' : ''}`}
            type="button"
            onClick={() => setActiveView('runtime')}
          >
            Runtime
          </button>
          <button
            className={`nav-item ${activeView === 'dashboard' ? 'active' : ''}`}
            type="button"
            onClick={() => setActiveView('dashboard')}
          >
            Dashboard
          </button>
        </nav>
      </aside>

      <section className="desktop-content">
        {activeView === 'runtime' ? <LocalRuntimeView client={client} /> : <DashboardHost />}
      </section>
    </main>
  )
}

function DashboardHost() {
  const [state, setState] = useState<DashboardApiState>('checking')
  const [attempt, setAttempt] = useState(0)
  const [apiSnapshot, setApiSnapshot] = useState<DesktopApiSnapshot | null>(null)

  useEffect(() => {
    let alive = true
    let timer: number | undefined
    setState('checking')

    const check = async () => {
      const snapshot = await loadDesktopApiSnapshot()
      const origin = normalizeApiOrigin(snapshot?.origin ?? API_ORIGIN)

      if (!alive) return
      setApiSnapshot(snapshot)

      if (snapshot && snapshot.state !== 'running') {
        setState(snapshot.state === 'starting' ? 'checking' : 'unavailable')
        if (snapshot.state === 'starting') {
          timer = window.setTimeout(() => setAttempt((value) => value + 1), 1000)
        }
        return
      }

      fetch(`${origin}/api/health`)
        .then((response) => {
          if (!alive) return
          setState(response.ok ? 'ready' : 'unavailable')
        })
        .catch(() => {
          if (alive) setState('unavailable')
        })
    }

    void check()
    return () => {
      alive = false
      if (timer !== undefined) window.clearTimeout(timer)
    }
  }, [attempt])

  if (state === 'ready') {
    return <WebDashboardApp />
  }

  return (
    <div className="dashboard-host-state">
      <div className="dashboard-host-panel">
        <span className={`dashboard-host-dot ${state}`} />
        <div>
          <h1>Dashboard API</h1>
          <p>{dashboardApiMessage(state, apiSnapshot)}</p>
        </div>
        <button className="secondary-button" type="button" onClick={() => setAttempt((value) => value + 1)}>
          重试
        </button>
      </div>
    </div>
  )
}

async function loadDesktopApiSnapshot(): Promise<DesktopApiSnapshot | null> {
  try {
    return await invoke<DesktopApiSnapshot>('desktop_api_snapshot')
  } catch {
    return null
  }
}

function dashboardApiMessage(state: DashboardApiState, snapshot: DesktopApiSnapshot | null): string {
  if (state === 'checking') {
    return snapshot?.message ?? `正在检查 ${normalizeApiOrigin(API_ORIGIN)}`
  }
  return snapshot?.message ?? `${normalizeApiOrigin(API_ORIGIN)} 暂不可用`
}

function normalizeApiOrigin(value: string): string {
  return value.replace(/\/+$/, '')
}

export default App
