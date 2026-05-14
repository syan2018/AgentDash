import type { HTMLAttributes, ReactNode } from 'react'

import { cn } from '../utils/cn'

export interface EmptyStateProps extends HTMLAttributes<HTMLDivElement> {
  children: ReactNode
}

export function EmptyState({ children, className, ...props }: EmptyStateProps) {
  return (
    <div
      className={cn('rounded-[8px] border border-dashed border-border px-3 py-2 text-center text-sm text-muted-foreground', className)}
      {...props}
    >
      {children}
    </div>
  )
}
