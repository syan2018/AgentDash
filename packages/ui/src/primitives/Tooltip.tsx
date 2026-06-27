import type { ReactNode } from 'react'
import { cn, type ClassValue } from '../utils/cn'

export interface TooltipProps {
  content: string
  children: ReactNode
  className?: ClassValue
  /** 气泡方向，默认 top */
  side?: 'top' | 'bottom'
}

/**
 * 轻量级 CSS-only Tooltip
 *
 * 使用 `group/tip` + hover 伪元素实现，无 JS 状态、无 portal。
 * 长文本自动换行，最大宽度 20rem。
 */
export function Tooltip({ content, children, className, side = 'top' }: TooltipProps) {
  return (
    <span className={cn('group/tip relative inline-flex', className)}>
      {children}
      <span
        role="tooltip"
        className={cn(
          'pointer-events-none absolute left-1/2 z-50 -translate-x-1/2 whitespace-pre-wrap',
          'max-w-80 rounded-[6px] border border-border bg-popover px-2 py-1',
          'text-[11px] leading-relaxed text-popover-foreground shadow-md',
          'opacity-0 transition-opacity duration-150 group-hover/tip:opacity-100',
          side === 'top' ? 'bottom-full mb-1.5' : 'top-full mt-1.5',
        )}
      >
        {content}
      </span>
    </span>
  )
}
