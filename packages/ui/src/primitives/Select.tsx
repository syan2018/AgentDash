import type { SelectHTMLAttributes } from 'react'

import { cn } from '../utils/cn'

export interface SelectProps extends SelectHTMLAttributes<HTMLSelectElement> {}

export function Select({ className, ...props }: SelectProps) {
  return (
    <select
      className={cn(
        'min-h-9 rounded-[8px] border border-border bg-background px-3 py-2 text-sm text-foreground outline-none transition-colors focus:border-primary/40 focus:ring-1 focus:ring-primary/30 disabled:opacity-50',
        className,
      )}
      {...props}
    />
  )
}
