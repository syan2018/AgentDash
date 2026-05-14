import type { TextareaHTMLAttributes } from 'react'

import { cn } from '../utils/cn'

export interface TextareaProps extends TextareaHTMLAttributes<HTMLTextAreaElement> {}

export function Textarea({ className, ...props }: TextareaProps) {
  return (
    <textarea
      className={cn(
        'min-h-24 w-full resize-y rounded-[8px] border border-border bg-background px-3 py-2 text-sm leading-relaxed text-foreground outline-none transition-colors placeholder:text-muted-foreground/60 focus:border-primary/40 focus:ring-1 focus:ring-primary/30 disabled:opacity-50',
        className,
      )}
      {...props}
    />
  )
}
