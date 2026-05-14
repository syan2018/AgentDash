import type { HTMLAttributes, ReactNode } from 'react'

import { cn } from '../utils/cn'

export type NoticeTone = 'info' | 'success' | 'warning' | 'danger'

export interface NoticeProps extends HTMLAttributes<HTMLDivElement> {
  children: ReactNode
  tone?: NoticeTone
}

const toneClass: Record<NoticeTone, string> = {
  info: 'border-primary/25 bg-primary/10 text-primary',
  success: 'border-success/25 bg-success/10 text-success',
  warning: 'border-warning/30 bg-warning/10 text-warning',
  danger: 'border-destructive/30 bg-destructive/10 text-destructive',
}

export function Notice({ children, className, tone = 'info', ...props }: NoticeProps) {
  return (
    <div
      className={cn('rounded-[8px] border px-3 py-2 text-sm wrap-anywhere', toneClass[tone], className)}
      {...props}
    >
      {children}
    </div>
  )
}
