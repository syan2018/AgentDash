import type { HTMLAttributes, ReactNode } from 'react'

import { cn } from '../utils/cn'

export interface SectionTitleProps extends Omit<HTMLAttributes<HTMLElement>, 'title'> {
  title: ReactNode
  subtitle?: ReactNode
  badge?: ReactNode
  actions?: ReactNode
  sticky?: boolean
}

export function SectionTitle({
  actions,
  badge,
  className,
  sticky = false,
  subtitle,
  title,
  ...props
}: SectionTitleProps) {
  return (
    <header
      className={cn(
        'flex items-center justify-between gap-3 border-b border-border/60 px-4 py-3',
        sticky &&
          'sticky top-0 z-10 bg-secondary/10 backdrop-blur supports-[backdrop-filter]:bg-secondary/30',
        className,
      )}
      {...props}
    >
      <div className="min-w-0">
        <p className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
          {title}
        </p>
        {subtitle && (
          <p className="mt-0.5 truncate font-mono text-[11px] text-foreground/80">{subtitle}</p>
        )}
      </div>
      {(actions || badge) && (
        <div className="flex shrink-0 items-center gap-2">
          {badge}
          {actions}
        </div>
      )}
    </header>
  )
}
