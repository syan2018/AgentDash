import { useEffect, useMemo, useState } from 'react'
import WebDashboardApp from 'app-web'
import { LocalRuntimeView } from '@agentdash/views/local-runtime'
import { createTauriLocalRuntimeClient } from './runtimeApi'

type DesktopView = 'runtime' | 'dashboard'
type DashboardApiState = 'checking' | 'ready' | 'unavailable'

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

  useEffect(() => {
    let alive = true
    setState('checking')
    fetch(`${API_ORIGIN}/api/health`)
      .then((response) => {
        if (!alive) return
        setState(response.ok ? 'ready' : 'unavailable')
      })
      .catch(() => {
        if (alive) setState('unavailable')
      })
    return () => {
      alive = false
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
          <p>{state === 'checking' ? '正在检查 127.0.0.1:3001' : '127.0.0.1:3001 暂不可用'}</p>
        </div>
        <button className="secondary-button" type="button" onClick={() => setAttempt((value) => value + 1)}>
          重试
        </button>
      </div>
    </div>
  )
}

export default App
