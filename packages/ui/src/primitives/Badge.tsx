import type { HTMLAttributes, ReactNode } from 'react'

import { cn } from '../utils/cn'

export type BadgeVariant = 'neutral' | 'primary' | 'success' | 'warning' | 'danger'

export interface BadgeProps extends HTMLAttributes<HTMLSpanElement> {
  children: ReactNode
  variant?: BadgeVariant
}

const variantClass: Record<BadgeVariant, string> = {
  neutral: 'border-border bg-secondary text-muted-foreground',
  primary: 'border-primary/25 bg-primary/10 text-primary',
  success: 'border-success/25 bg-success/10 text-success',
  warning: 'border-warning/30 bg-warning/10 text-warning',
  danger: 'border-destructive/25 bg-destructive/10 text-destructive',
}

export function Badge({ children, className, variant = 'neutral', ...props }: BadgeProps) {
  return (
    <span
      className={cn(
        'inline-flex min-h-6 items-center justify-center rounded-full border px-2 py-0.5 text-[11px] font-medium',
        variantClass[variant],
        className,
      )}
      {...props}
    >
      {children}
    </span>
  )
}
