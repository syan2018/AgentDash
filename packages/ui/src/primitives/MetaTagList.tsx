import { type ReactNode } from 'react'

import { cn } from '../utils/cn'

export type MetaTagTone = 'neutral' | 'muted' | 'warning' | 'danger' | 'success'

export interface MetaTagItem {
  key: string
  label: ReactNode
  tone?: MetaTagTone
  /** tooltip / 悬停标题（例如 digest / 完整路径）。 */
  title?: string
}

export interface MetaTagListProps {
  items: MetaTagItem[]
  className?: string
}

const toneClass: Record<MetaTagTone, string> = {
  neutral: 'border-border bg-secondary/40 text-muted-foreground',
  muted: 'border-border bg-secondary/30 text-muted-foreground/70',
  warning: 'border-warning/30 bg-warning/10 text-warning',
  danger: 'border-destructive/30 bg-destructive/10 text-destructive',
  success: 'border-success/25 bg-success/10 text-success',
}

/**
 * 资产卡片 / 详情页的小灰 chip 标签条。
 *
 * 用于"3 个文件 / 5 step / imported / explicit only / target: ..."
 * 这种带可选语义色调的元数据展示。空数组时不渲染。
 */
export function MetaTagList({ items, className }: MetaTagListProps) {
  if (items.length === 0) return null
  return (
    <div className={cn('mt-3 flex flex-wrap gap-1.5 text-[11px]', className)}>
      {items.map((item) => (
        <MetaTag key={item.key} tone={item.tone ?? 'neutral'} title={item.title}>
          {item.label}
        </MetaTag>
      ))}
    </div>
  )
}

interface MetaTagProps {
  tone: MetaTagTone
  title?: string
  children: ReactNode
}

function MetaTag({ tone, title, children }: MetaTagProps) {
  return (
    <span
      title={title}
      className={cn('rounded-[6px] border px-1.5 py-0.5', toneClass[tone])}
    >
      {children}
    </span>
  )
}
