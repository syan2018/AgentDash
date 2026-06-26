import { useEffect, useMemo, useState } from 'react'
import WebDashboardApp from 'app-web'
import { Button, StatusScreen } from '@agentdash/ui'
import { invoke } from '@tauri-apps/api/core'
import { DesktopTitlebar } from './DesktopTitlebar'
import { createTauriDesktopAppBridge, type DesktopAppBridge } from './desktopSettings'
import { createTauriLocalRuntimeClient, tauriBrowseDirectory } from './runtimeApi'
import type { LocalRuntimeClient } from '@agentdash/core/local-runtime'
import type { BrowseDirectoryResult } from '@agentdash/views/directory-browser'

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
    __AGENTDASH_DESKTOP_BROWSE_DIRECTORY__?: (path?: string) => Promise<BrowseDirectoryResult>
    __AGENTDASH_DESKTOP_OPEN_EXTERNAL__?: (url: string) => Promise<void>
    __AGENTDASH_DESKTOP_APP__?: DesktopAppBridge
  }
}

function App() {
  const client = useMemo(() => createTauriLocalRuntimeClient(), [])
  const desktopApp = useMemo(() => createTauriDesktopAppBridge(), [])

  useEffect(() => {
    window.__AGENTDASH_DESKTOP_LOCAL_RUNTIME__ = client
    window.__AGENTDASH_DESKTOP_BROWSE_DIRECTORY__ = tauriBrowseDirectory
    window.__AGENTDASH_DESKTOP_OPEN_EXTERNAL__ = (url: string) => invoke<void>('open_external_url', { url })
    window.__AGENTDASH_DESKTOP_APP__ = desktopApp
    return () => {
      delete window.__AGENTDASH_DESKTOP_LOCAL_RUNTIME__
      delete window.__AGENTDASH_DESKTOP_BROWSE_DIRECTORY__
      delete window.__AGENTDASH_DESKTOP_OPEN_EXTERNAL__
      delete window.__AGENTDASH_DESKTOP_APP__
    }
  }, [client, desktopApp])

  return (
    <div className="flex h-screen min-w-[960px] flex-col bg-background text-foreground">
      <DesktopTitlebar />
      <div className="min-h-0 flex-1">
        <DashboardHost />
      </div>
    </div>
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

  const unavailable = state === 'unavailable'
  return (
    <StatusScreen
      tone={unavailable ? 'danger' : 'loading'}
      title={unavailable ? 'Dashboard API 暂不可用' : '正在启动本机服务…'}
      description={dashboardApiMessage(state, apiSnapshot)}
      action={
        unavailable ? (
          <Button variant="secondary" onClick={() => setAttempt((value) => value + 1)}>
            重试
          </Button>
        ) : undefined
      }
    />
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
