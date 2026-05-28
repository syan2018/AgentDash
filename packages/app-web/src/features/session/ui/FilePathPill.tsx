/**
 * 文件路径展示组件
 *
 * 长路径用「父目录中段省略 + basename 完整」的策略，
 * 避免 truncate 把文件名截掉。父目录靠 dir="rtl" 让省略号落在前缀。
 *
 * 可选 range 渲染为尾随的 `L<from>-<to>` 小字。
 */

import type { ReactNode } from "react";

export interface FilePathPillProps {
  path: string;
  range?: { from: number; to: number } | null;
  /** 强制只显示 basename（不显示父目录） */
  baseOnly?: boolean;
}

export function FilePathPill({ path, range, baseOnly }: FilePathPillProps): ReactNode {
  const { dir, base } = splitPath(path);
  const showDir = !baseOnly && dir.length > 0;

  return (
    <span className="inline-flex min-w-0 max-w-full items-baseline gap-0.5 font-mono text-sm">
      {showDir && (
        <span
          className="min-w-0 flex-shrink truncate text-muted-foreground/55"
          dir="rtl"
          style={{ textAlign: "left" }}
          title={path}
        >
          {dir}/
        </span>
      )}
      <span className="shrink-0 text-foreground" title={path}>
        {base}
      </span>
      {range && (
        <span className="ml-1 shrink-0 tabular-nums text-xs text-muted-foreground/60">
          L{range.from}-{range.to}
        </span>
      )}
    </span>
  );
}

function splitPath(path: string): { dir: string; base: string } {
  // 同时兼容 / 和 \\，并对 URI scheme（agentdash://、ld-km://）保留 scheme 开头
  const normalized = path.replace(/\\/g, "/");
  const lastSlash = normalized.lastIndexOf("/");
  if (lastSlash < 0) {
    return { dir: "", base: normalized };
  }
  const base = normalized.slice(lastSlash + 1);
  const dir = normalized.slice(0, lastSlash);
  return { dir, base: base || normalized };
}
