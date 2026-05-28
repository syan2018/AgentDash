import { type KeyboardEvent, type ReactNode } from 'react'

import { cn } from '../utils/cn'

export interface AssetCardProps {
  /** 卡片主标题（display_name）。 */
  title: ReactNode
  /** 标题下方的小字行（key / mount_id / extensions/<key> 等）。 */
  subtitle?: ReactNode
  /** 可选的两行截断描述。null/undefined 时不渲染。 */
  description?: string | null
  /** header 右侧 slot：PublishedBadge / OriginBadge / CardMenu 等。 */
  headerRight?: ReactNode
  /** 卡片底部的小字 footer（"更新于 ..." 等）。带顶部分隔线。 */
  footer?: ReactNode
  /** 点击或键盘 Enter / Space 触发，对应"打开详情 / 进入编辑"。 */
  onOpen: () => void
  /** 鼠标悬停 tooltip & 无障碍 title，例如"编辑" / "查看" / "查看详情"。 */
  openTitle?: string
  /** 额外的外层 className（例如覆盖 padding / 圆角）。 */
  className?: string
  /** 描述与 footer 之间的 body 区，通常放 `<MetaTagList>` 或自定义内容。 */
  children?: ReactNode
}

/**
 * 资产卡片骨架：
 *
 *   ┌─────────────────────────────────────┐
 *   │ title              [headerRight]    │
 *   │ subtitle                            │
 *   │ description (line-clamp-2)          │
 *   │ children                            │
 *   │ ───────────────────────────────────  │
 *   │ footer                              │
 *   └─────────────────────────────────────┘
 *
 * 与设计语言对齐：rounded-[8px] / border / cursor-pointer / hover & focus 视觉。
 * 整张卡片对外只暴露一个 `onOpen` 入口（点击或键盘激活），
 * 其它操作走 `headerRight` 里的 `<CardMenu>`。
 */
export function AssetCard({
  title,
  subtitle,
  description,
  headerRight,
  footer,
  onOpen,
  openTitle,
  className,
  children,
}: AssetCardProps) {
  const handleKeyDown = (event: KeyboardEvent<HTMLElement>) => {
    if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault()
      onOpen()
    }
  }

  return (
    <article
      role="button"
      tabIndex={0}
      onClick={onOpen}
      onKeyDown={handleKeyDown}
      title={openTitle}
      className={cn(
        'flex cursor-pointer flex-col rounded-[8px] border border-border bg-background p-3.5 text-left transition-colors',
        'hover:border-primary/25 hover:bg-secondary/30',
        'focus:outline-none focus-visible:ring-2 focus-visible:ring-primary/40',
        className,
      )}
    >
      <header className="flex items-start justify-between gap-2">
        <div className="min-w-0">
          <p className="truncate text-sm font-medium leading-6 text-foreground">{title}</p>
          {subtitle != null && (
            <div className="mt-0.5 truncate text-xs text-muted-foreground">{subtitle}</div>
          )}
        </div>
        {headerRight != null && (
          <div className="flex shrink-0 items-center gap-1">{headerRight}</div>
        )}
      </header>

      {description && (
        <p className="mt-1.5 line-clamp-2 text-xs leading-5 text-muted-foreground">{description}</p>
      )}

      {children}

      {footer != null && (
        <footer className="mt-3 border-t border-border/70 pt-2.5 text-[11px] text-muted-foreground">
          {footer}
        </footer>
      )}
    </article>
  )
}
