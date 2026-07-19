/**
 * 工具卡片标题区通用模板
 *
 * 极简两行结构：
 *   行 1：badge + primary（路径 / 命令 / server/tool / 查询词等，由 renderer 自由构造）
 *   行 2：secondary（参数摘要：cwd / 行范围 / args / target，灰色小字）
 *
 * 不再渲染与 badge 重复的 verb（kind.label）。
 */

import type { ReactNode } from "react";
import type { KindMeta } from "../model/threadItemKind";

export interface ToolCardHeaderModel {
  /** 主信息行：renderer 自由构造 ReactNode。
   *  约定：不要重复 badge 已表达的 verb（badge=READ 时不要再写 "Read"）。 */
  primary: ReactNode;
  /** 参数摘要行：cwd / 行范围 / args / target 之类，灰色小字。 */
  secondary?: ReactNode;
}

export interface ToolCardHeaderProps {
  kind: KindMeta;
  header: ToolCardHeaderModel;
}

export function ToolCardHeader({ kind, header }: ToolCardHeaderProps): ReactNode {
  return (
    <div className="flex min-w-0 flex-1 items-start gap-2.5">
      <span className="mt-0.5 inline-flex shrink-0 rounded-[6px] border border-border bg-secondary px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-[0.1em] text-muted-foreground">
        {kind.badge}
      </span>
      <div className="min-w-0 flex-1">
        <div className="min-w-0 truncate text-sm font-medium text-foreground">
          {header.primary}
        </div>
        {header.secondary != null && header.secondary !== "" && (
          <div className="min-w-0 truncate text-xs text-muted-foreground/70">
            {header.secondary}
          </div>
        )}
      </div>
    </div>
  );
}
