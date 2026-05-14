import type { HTMLAttributes, ReactNode } from 'react'

import { cn } from '../utils/cn'

export interface FieldProps extends HTMLAttributes<HTMLLabelElement> {
  children: ReactNode
  label: ReactNode
}

export function Field({ children, className, label, ...props }: FieldProps) {
  return (
    <label className={cn('grid gap-1.5 text-xs font-semibold text-muted-foreground', className)} {...props}>
      <span>{label}</span>
      {children}
    </label>
  )
}
