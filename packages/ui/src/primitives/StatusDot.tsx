import type { HTMLAttributes } from 'react'

import { cn } from '../utils/cn'

export type StatusDotTone = 'success' | 'warning' | 'danger' | 'info' | 'muted' | 'primary'
export type StatusDotSize = 'sm' | 'md'

export interface StatusDotProps extends HTMLAttributes<HTMLSpanElement> {
  tone?: StatusDotTone
  size?: StatusDotSize
  pulse?: boolean
}

const toneClass: Record<StatusDotTone, string> = {
  success: 'bg-success',
  warning: 'bg-warning',
  danger: 'bg-destructive',
  info: 'bg-info',
  primary: 'bg-primary',
  muted: 'bg-muted-foreground/30',
}

const sizeClass: Record<StatusDotSize, string> = {
  sm: 'h-1.5 w-1.5',
  md: 'h-2 w-2',
}

export function StatusDot({
  className,
  pulse = false,
  size = 'sm',
  tone = 'muted',
  ...props
}: StatusDotProps) {
  return (
    <span className={cn('relative inline-flex', className)} {...props}>
      {pulse && (
        <span
          className={cn(
            'absolute inline-flex rounded-full opacity-60 animate-ping',
            sizeClass[size],
            toneClass[tone],
          )}
        />
      )}
      <span
        className={cn('relative inline-block rounded-full', sizeClass[size], toneClass[tone])}
      />
    </span>
  )
}
