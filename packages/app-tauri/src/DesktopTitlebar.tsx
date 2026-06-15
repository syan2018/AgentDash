import { useEffect, useState, type ReactNode } from 'react'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { cn } from '@agentdash/ui'

// 自定义标题栏：decorations:false 后承担拖拽 + 窗口控制。
// 仅在 Tauri 宿主内有效（app-tauri 始终运行于 Tauri）。

function TitlebarButton({
  onClick,
  label,
  danger = false,
  children,
}: {
  onClick: () => void
  label: string
  danger?: boolean
  children: ReactNode
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      onClick={onClick}
      className={cn(
        'inline-flex h-9 w-11 items-center justify-center text-muted-foreground transition-colors',
        danger
          ? 'hover:bg-destructive hover:text-destructive-foreground'
          : 'hover:bg-secondary hover:text-foreground',
      )}
    >
      {children}
    </button>
  )
}

export function DesktopTitlebar() {
  const [maximized, setMaximized] = useState(false)

  useEffect(() => {
    const appWindow = getCurrentWindow()
    let unlisten: (() => void) | undefined
    let alive = true

    const sync = () => {
      appWindow
        .isMaximized()
        .then((value) => {
          if (alive) setMaximized(value)
        })
        .catch(() => {})
    }
    sync()
    appWindow
      .onResized(() => sync())
      .then((fn) => {
        if (alive) unlisten = fn
        else fn()
      })
      .catch(() => {})

    return () => {
      alive = false
      unlisten?.()
    }
  }, [])

  return (
    <div
      data-tauri-drag-region
      className="flex h-9 shrink-0 select-none items-center justify-between bg-sidebar pl-3 shadow-sm"
    >
      <div data-tauri-drag-region className="pointer-events-none flex items-center gap-2">
        <img src="/app-icon.svg" alt="" draggable={false} className="h-4 w-4 shrink-0" />
        <span className="text-xs font-semibold tracking-tight text-sidebar-foreground">
          AgentDash
        </span>
      </div>
      <div className="flex items-center">
        <TitlebarButton label="最小化" onClick={() => void getCurrentWindow().minimize()}>
          <svg width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="1.2">
            <path d="M2.5 6h7" strokeLinecap="round" />
          </svg>
        </TitlebarButton>
        <TitlebarButton
          label={maximized ? '还原' : '最大化'}
          onClick={() => void getCurrentWindow().toggleMaximize()}
        >
          {maximized ? (
            <svg width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="1.1">
              <rect x="3.2" y="3.2" width="5.6" height="5.6" rx="0.8" />
              <path d="M4.6 3.2V2.2a.8.8 0 0 1 .8-.8h4a.8.8 0 0 1 .8.8v4a.8.8 0 0 1-.8.8h-1" />
            </svg>
          ) : (
            <svg width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="1.1">
              <rect x="2.6" y="2.6" width="6.8" height="6.8" rx="0.8" />
            </svg>
          )}
        </TitlebarButton>
        <TitlebarButton label="关闭" danger onClick={() => void getCurrentWindow().close()}>
          <svg width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="1.2">
            <path d="M3 3l6 6M9 3l-6 6" strokeLinecap="round" />
          </svg>
        </TitlebarButton>
      </div>
    </div>
  )
}
