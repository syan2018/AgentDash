import { type ReactNode } from "react";

import { DismissibleNotice, type DismissibleNoticeData } from "@agentdash/ui";

export interface CategoryPageShellProps {
  /** 顶部标题（"Extension 资产" / "Skill 资产" 等）。 */
  title: string;
  /** 标题下方的小字（资产计数 / 文案说明）。 */
  stats?: ReactNode;
  /** header 右侧 slot：刷新按钮 / CreateButton 等。 */
  actions?: ReactNode;
  /** 通用反馈条；null 时不渲染。 */
  notice?: DismissibleNoticeData | null;
  onDismissNotice?: () => void;
  /** 主体内容（loading / empty / grid / dialogs 都放在 children）。 */
  children: ReactNode;
}

/**
 * Assets 页 CategoryPanel 的共享外壳：
 * - 全高 flex 容器 + 一致 padding
 * - header（title + stats + actions）
 * - DismissibleNotice 槽位
 *
 * 不接管 loading / empty / error 渲染——这些状态在各 panel 之间差异较大，
 * 由调用方按需放在 children 里。
 */
export function CategoryPageShell({
  title,
  stats,
  actions,
  notice,
  onDismissNotice,
  children,
}: CategoryPageShellProps) {
  return (
    <div className="flex h-full flex-col gap-4 p-6">
      <header className="flex flex-wrap items-center justify-between gap-3">
        <div className="space-y-1">
          <h2 className="text-base font-semibold tracking-tight text-foreground">
            {title}
          </h2>
          {stats != null && <p className="text-xs text-muted-foreground">{stats}</p>}
        </div>
        {actions != null && <div className="flex items-center gap-2">{actions}</div>}
      </header>

      {notice !== undefined && onDismissNotice && (
        <DismissibleNotice notice={notice ?? null} onDismiss={onDismissNotice} />
      )}

      {children}
    </div>
  );
}

export default CategoryPageShell;
