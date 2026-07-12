import { useEffect, useState } from 'react'
import WebDashboardApp from 'app-web'
import { Button, StatusScreen } from '@agentdash/ui'
import { invoke } from '@tauri-apps/api/core'
import { relaunch } from '@tauri-apps/plugin-process'
import { DesktopTitlebar } from './DesktopTitlebar'
import { createTauriDesktopAppBridge } from './desktopSettings'
import type { DesktopAppBridge } from './desktopSettings'
import { createTauriLocalRuntimeClient, tauriBrowseDirectory } from './runtimeApi'
import type {
  DesktopApiSnapshot,
  DesktopUpdatePolicySnapshot,
  LocalRuntimeClient,
} from '@agentdash/core/local-runtime'
import type { BrowseDirectoryResult } from '@agentdash/views/directory-browser'

type DashboardApiState = 'checking' | 'ready' | 'unavailable'

const API_ORIGIN = (import.meta.env.VITE_API_ORIGIN ?? '').replace(/\/+$/, '')

declare global {
  interface Window {
    __AGENTDASH_DESKTOP_LOCAL_RUNTIME__?: LocalRuntimeClient
    __AGENTDASH_DESKTOP_BROWSE_DIRECTORY__?: (path?: string) => Promise<BrowseDirectoryResult>
    __AGENTDASH_DESKTOP_OPEN_EXTERNAL__?: (url: string) => Promise<void>
    __AGENTDASH_DESKTOP_APP__?: DesktopAppBridge
  }
}

const desktopRuntimeClient = createTauriLocalRuntimeClient()
const desktopAppBridge = createTauriDesktopAppBridge()

window.__AGENTDASH_DESKTOP_LOCAL_RUNTIME__ = desktopRuntimeClient
window.__AGENTDASH_DESKTOP_BROWSE_DIRECTORY__ = tauriBrowseDirectory
window.__AGENTDASH_DESKTOP_OPEN_EXTERNAL__ = (url: string) => invoke<void>('open_external_url', { url })
window.__AGENTDASH_DESKTOP_APP__ = desktopAppBridge

function App() {
  return (
    <div className="flex h-screen min-w-[960px] flex-col bg-background text-foreground">
      <DesktopTitlebar />
      <div className="min-h-0 flex-1">
        <DashboardHost desktopApp={desktopAppBridge} />
      </div>
    </div>
  )
}

interface DashboardHostProps {
  desktopApp: DesktopAppBridge
}

function DashboardHost({ desktopApp }: DashboardHostProps) {
  const [state, setState] = useState<DashboardApiState>('checking')
  const [attempt, setAttempt] = useState(0)
  const [apiSnapshot, setApiSnapshot] = useState<DesktopApiSnapshot | null>(null)
  const [updatePolicy, setUpdatePolicy] = useState<DesktopUpdatePolicySnapshot | null>(null)
  const [updateActionState, setUpdateActionState] = useState<'idle' | 'installing' | 'ready_to_restart' | 'restarting'>('idle')
  const [updateActionError, setUpdateActionError] = useState<string | null>(null)

  useEffect(() => {
    let alive = true
    let timer: number | undefined
    setState('checking')
    setUpdateActionError(null)

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
        .then(async (response) => {
          if (!alive) return
          if (!response.ok) {
            setState('unavailable')
            return
          }
          const policy = await desktopApp.refreshUpdatePolicy().catch(() => null)
          if (!alive) return
          setUpdatePolicy(policy)
          setState('ready')
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
  }, [attempt, desktopApp])

  if (state === 'ready') {
    if (updatePolicy?.force_update_required) {
      return (
        <ForceUpdateScreen
          policy={updatePolicy}
          actionState={updateActionState}
          error={updateActionError}
          onRetry={() => setAttempt((value) => value + 1)}
          onQuit={() => {
            void desktopApp.quit()
          }}
          onRestart={async () => {
            setUpdateActionState('restarting')
            await relaunch()
          }}
          onInstall={async () => {
            setUpdateActionError(null)
            setUpdateActionState('installing')
            try {
              const result = await desktopApp.installUpdate()
              if (!result.installed) {
                setUpdateActionState('idle')
                setUpdateActionError(result.message)
                return
              }
              setUpdateActionState('ready_to_restart')
            } catch (error) {
              setUpdateActionState('idle')
              setUpdateActionError(error instanceof Error ? error.message : String(error))
            }
          }}
        />
      )
    }
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

interface ForceUpdateScreenProps {
  policy: DesktopUpdatePolicySnapshot
  actionState: 'idle' | 'installing' | 'ready_to_restart' | 'restarting'
  error: string | null
  onInstall(): Promise<void>
  onRestart(): Promise<void>
  onRetry(): void
  onQuit(): void
}

function ForceUpdateScreen({
  policy,
  actionState,
  error,
  onInstall,
  onRestart,
  onRetry,
  onQuit,
}: ForceUpdateScreenProps) {
  const busy = actionState === 'installing' || actionState === 'restarting'
  const detail = [
    `当前版本 ${policy.current_version}`,
    policy.min_desktop_version ? `最低要求 ${policy.min_desktop_version}` : null,
    policy.latest_version ? `最新版本 ${policy.latest_version}` : null,
    actionState === 'ready_to_restart' ? '更新已安装，等待重启' : null,
    error ?? policy.last_error ?? policy.diagnostics_message,
  ].filter((item): item is string => Boolean(item))

  return (
    <StatusScreen
      tone="warning"
      title="需要更新桌面端"
      description={detail.join(' · ')}
      action={
        <div className="flex flex-wrap items-center justify-center gap-2">
          <Button
            variant="primary"
            disabled={busy}
            onClick={() => {
              if (actionState === 'ready_to_restart') {
                void onRestart()
              } else {
                void onInstall()
              }
            }}
          >
            {actionState === 'installing'
              ? '正在安装'
              : actionState === 'restarting'
                ? '正在重启'
                : actionState === 'ready_to_restart'
                  ? '重启完成更新'
                  : '安装更新'}
          </Button>
          <Button variant="secondary" disabled={busy} onClick={onRetry}>
            重试检查
          </Button>
          <Button variant="secondary" disabled={busy} onClick={onQuit}>
            退出
          </Button>
        </div>
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
