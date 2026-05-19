import type { ButtonHTMLAttributes } from 'react'

import { cn } from '../utils/cn'

export type ButtonVariant = 'primary' | 'secondary' | 'danger' | 'ghost'
export type ButtonSize = 'sm' | 'md' | 'icon'

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant
  size?: ButtonSize
}

const variantClass: Record<ButtonVariant, string> = {
  primary: 'border-primary/60 bg-transparent text-primary hover:border-primary hover:bg-primary/8',
  secondary: 'border-border bg-background text-foreground hover:border-foreground/30 hover:bg-secondary',
  danger: 'border-destructive/60 bg-transparent text-destructive hover:border-destructive hover:bg-destructive/8',
  ghost: 'border-transparent bg-transparent text-muted-foreground hover:bg-secondary hover:text-foreground',
}

const sizeClass: Record<ButtonSize, string> = {
  sm: 'min-h-8 px-2.5 py-1.5 text-xs',
  md: 'min-h-9 px-3.5 py-2 text-sm',
  icon: 'h-9 w-9 p-0',
}

export function Button({
  className,
  size = 'md',
  type = 'button',
  variant = 'secondary',
  ...props
}: ButtonProps) {
  return (
    <button
      className={cn(
        'inline-flex shrink-0 items-center justify-center rounded-[8px] border font-medium transition-colors disabled:cursor-not-allowed disabled:opacity-50',
        variantClass[variant],
        sizeClass[size],
        className,
      )}
      type={type}
      {...props}
    />
  )
}
