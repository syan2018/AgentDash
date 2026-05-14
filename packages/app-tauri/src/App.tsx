import { useEffect, useMemo, useState } from 'react'
import WebDashboardApp from 'app-web'
import { Button, Card, cn } from '@agentdash/ui'
import { invoke } from '@tauri-apps/api/core'
import { createTauriLocalRuntimeClient } from './runtimeApi'
import type { LocalRuntimeClient } from '@agentdash/core/local-runtime'

type DashboardApiState = 'checking' | 'ready' | 'unavailable'
type DesktopApiSnapshot = {
  state: 'starting' | 'running' | 'error' | 'stopped'
  origin: string
  message?: string | null
  database_url?: string | null
}

const API_ORIGIN = (import.meta.env.VITE_API_ORIGIN ?? '').replace(/\/+$/, '')

declare global {
  interface Window {
    __AGENTDASH_DESKTOP_LOCAL_RUNTIME__?: LocalRuntimeClient
  }
}

function App() {
  const client = useMemo(() => createTauriLocalRuntimeClient(), [])

  useEffect(() => {
    window.__AGENTDASH_DESKTOP_LOCAL_RUNTIME__ = client
    return () => {
      delete window.__AGENTDASH_DESKTOP_LOCAL_RUNTIME__
    }
  }, [client])

  return (
    <main className="min-h-screen min-w-[960px] bg-background text-foreground">
      <DashboardHost />
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
    <div className="grid min-h-screen place-items-center bg-background p-6">
      <Card className="grid w-full max-w-[520px] grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-3.5">
        <span className={cn('h-2.5 w-2.5 rounded-full bg-muted-foreground', dashboardHostDotClass(state))} />
        <div>
          <h1 className="text-base font-semibold text-foreground">Dashboard API</h1>
          <p className="mt-1 text-sm text-muted-foreground">{dashboardApiMessage(state, apiSnapshot)}</p>
        </div>
        <Button onClick={() => setAttempt((value) => value + 1)}>
          重试
        </Button>
      </Card>
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

function dashboardHostDotClass(state: DashboardApiState): string {
  switch (state) {
    case 'checking':
      return 'bg-warning'
    case 'ready':
      return 'bg-success'
    case 'unavailable':
      return 'bg-destructive'
  }
}

export default App
