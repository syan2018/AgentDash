import type { InputHTMLAttributes } from 'react'

import { cn } from '../utils/cn'

export interface TextInputProps extends InputHTMLAttributes<HTMLInputElement> {}

export function TextInput({ className, ...props }: TextInputProps) {
  return (
    <input
      className={cn(
        'min-h-9 w-full rounded-[8px] border border-border bg-background px-3 py-2 text-sm text-foreground outline-none transition-colors placeholder:text-muted-foreground/60 focus:border-primary/40 focus:ring-1 focus:ring-primary/30 disabled:opacity-50',
        className,
      )}
      {...props}
    />
  )
}
