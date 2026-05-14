import type { InputHTMLAttributes, ReactNode } from 'react'

import { cn } from '../utils/cn'

export interface CheckboxFieldProps extends Omit<InputHTMLAttributes<HTMLInputElement>, 'type'> {
  label: ReactNode
}

export function CheckboxField({ className, label, ...props }: CheckboxFieldProps) {
  return (
    <label className={cn('inline-flex items-center gap-2 text-xs font-semibold text-muted-foreground', className)}>
      <input className="h-4 w-4 rounded border-border" type="checkbox" {...props} />
      <span>{label}</span>
    </label>
  )
}
