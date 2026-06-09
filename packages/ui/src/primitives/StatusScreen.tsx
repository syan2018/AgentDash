import type { HTMLAttributes, ReactNode } from 'react'

import { cn } from '../utils/cn'

export type StatusScreenTone = 'loading' | 'info' | 'warning' | 'danger'

export interface StatusScreenProps extends HTMLAttributes<HTMLDivElement> {
  /** loading 渲染旋转器；其余 tone 渲染对应语义色指示点（可被 icon 覆盖） */
  tone?: StatusScreenTone
  title: string
  description?: ReactNode
  /** 品牌 logo 或自定义图标；覆盖默认的 spinner / tone 点 */
  icon?: ReactNode
  /** 操作区，通常是一个重试 / 重载按钮 */
  action?: ReactNode
}

const dotToneClass: Record<Exclude<StatusScreenTone, 'loading'>, string> = {
  info: 'bg-primary',
  warning: 'bg-warning',
  danger: 'bg-destructive',
}

function StatusVisual({ tone, icon }: { tone: StatusScreenTone; icon?: ReactNode }) {
  if (icon) return <>{icon}</>
  if (tone === 'loading') {
    return (
      // eslint-disable-next-line no-restricted-syntax -- 加载旋转器必须为圆形
      <span className="h-7 w-7 animate-spin rounded-full border-2 border-primary border-t-transparent" />
    )
  }
  return (
    // eslint-disable-next-line no-restricted-syntax -- 状态指示点为圆形
    <span className={cn('h-2.5 w-2.5 rounded-full', dotToneClass[tone])} />
  )
}

/**
 * 统一状态屏：splash / loading / api-unavailable / 崩溃 等全屏态共用一套视觉。
 * 纯展示组件，不含业务逻辑；填满父容器并居中。
 */
export function StatusScreen({
  tone = 'loading',
  title,
  description,
  icon,
  action,
  className,
  ...props
}: StatusScreenProps) {
  return (
    <div
      className={cn('grid min-h-full w-full place-items-center bg-background p-6', className)}
      {...props}
    >
      <div className="flex w-full max-w-[420px] flex-col items-center text-center">
        <div className="mb-4 flex h-12 items-center justify-center">
          <StatusVisual tone={tone} icon={icon} />
        </div>
        <h1 className="text-base font-semibold text-foreground">{title}</h1>
        {description ? (
          <p className="mt-1.5 text-sm text-muted-foreground">{description}</p>
        ) : null}
        {action ? <div className="mt-5">{action}</div> : null}
      </div>
    </div>
  )
}
