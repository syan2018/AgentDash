import type { ButtonHTMLAttributes } from 'react'

import { cn } from '../utils/cn'

export type ButtonVariant = 'primary' | 'secondary' | 'danger' | 'ghost'
export type ButtonSize = 'sm' | 'md' | 'icon'

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant
  size?: ButtonSize
}

const variantClass: Record<ButtonVariant, string> = {
  primary: 'border-primary bg-primary text-primary-foreground hover:opacity-95',
  secondary: 'border-border bg-background text-foreground hover:bg-secondary',
  danger: 'border-destructive bg-destructive text-destructive-foreground hover:opacity-95',
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
