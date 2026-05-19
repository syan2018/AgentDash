import type { HTMLAttributes, ReactNode } from 'react'

import { cn } from '../utils/cn'

export interface InspectorRowProps extends HTMLAttributes<HTMLDivElement> {
  label: ReactNode
  value: ReactNode
  mono?: boolean
}

export function InspectorRow({ className, label, mono = false, value, ...props }: InspectorRowProps) {
  return (
    <div className={cn('space-y-1', className)} {...props}>
      <div className="agentdash-form-label">{label}</div>
      <div className={cn('break-words text-foreground/85', mono && 'font-mono text-[11px]')}>
        {value}
      </div>
    </div>
  )
}
