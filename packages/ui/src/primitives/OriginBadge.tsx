import type { HTMLAttributes, ReactNode } from 'react'

import { cn } from '../utils/cn'

export type OriginBadgeTone =
  | 'neutral'
  | 'accent'
  | 'success'
  | 'info'
  | 'warning'

export interface OriginBadgeProps extends HTMLAttributes<HTMLSpanElement> {
  label: ReactNode
  tone?: OriginBadgeTone
  url?: string | null
  maxWidth?: string
}

const toneClass: Record<OriginBadgeTone, string> = {
  neutral: 'border-border bg-secondary/50 text-muted-foreground',
  accent: 'border-violet-500/30 bg-violet-500/10 text-violet-700 dark:text-violet-300',
  success: 'border-emerald-500/30 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300',
  info: 'border-sky-500/30 bg-sky-500/10 text-sky-700 dark:text-sky-300',
  warning: 'border-orange-500/30 bg-orange-500/10 text-orange-700 dark:text-orange-300',
}

export function OriginBadge({
  className,
  label,
  maxWidth = '180px',
  title,
  tone = 'neutral',
  url,
  ...props
}: OriginBadgeProps) {
  const shortUrl = url
    ? url
        .replace(/^https?:\/\//, '')
        .replace(/^github\.com\//, '')
        .slice(0, 36)
    : null

  return (
    <span
      className={cn(
        'inline-flex items-center gap-1 truncate rounded-[6px] border px-1.5 py-0.5 text-[10px]',
        toneClass[tone],
        className,
      )}
      style={{ maxWidth }}
      title={title ?? url ?? undefined}
      {...props}
    >
      <span className="truncate">{label}</span>
      {shortUrl && (
        <span className="truncate opacity-70">· {shortUrl}</span>
      )}
    </span>
  )
}
