/**
 * McpPresetCategoryPanel — Assets 页 MCP Preset 类目实装（PR4 只做列表 + 预览）。
 *
 * 本 PR 做什么：
 * - 加载 `listMcpPresets(projectId)` 全量，builtin / user 一起展示
 * - 每行 chip 区分来源（builtin / user）+ transport 类型（http / sse / stdio）
 * - 只读预览卡：
 *   - http / sse：URL + headers 计数 + relay 标记
 *   - stdio：command + args 预览 + env 计数 + relay 标记
 * - 提供"装载内置 Preset"按钮——否则新项目下面板永远空态，无法看出 builtin 存在
 *
 * 本 PR 不做（PR5）：
 * - 创建 / 编辑 / 删除 / 复制表单——按钮以 "PR5 实装" 文字禁用占位
 *
 * 依赖：
 * - `frontend/src/services/mcpPreset.ts` 的 `fetchProjectMcpPresets`、`bootstrapMcpPresets`
 * - 字段命名严格 snake_case，与后端 DTO 对齐
 */

import { useCallback, useEffect, useState } from "react";

import { useProjectStore } from "../../../stores/projectStore";
import {
  bootstrapMcpPresets,
  fetchProjectMcpPresets,
} from "../../../services/mcpPreset";
import type { McpPresetDto, McpServerDecl } from "../../../types";

export function McpPresetCategoryPanel() {
  const currentProjectId = useProjectStore((s) => s.currentProjectId);

  const [presets, setPresets] = useState<McpPresetDto[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [isBootstrapping, setIsBootstrapping] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);

  const loadPresets = useCallback(
    async (projectId: string) => {
      setIsLoading(true);
      setError(null);
      try {
        const next = await fetchProjectMcpPresets(projectId);
        setPresets(next);
      } catch (e) {
        setError(e instanceof Error ? e.message : "加载 MCP Preset 失败");
      } finally {
        setIsLoading(false);
      }
    },
    [],
  );

  useEffect(() => {
    if (!currentProjectId) return;
    void loadPresets(currentProjectId);
  }, [currentProjectId, loadPresets]);

  useEffect(() => {
    if (!message) return;
    const t = setTimeout(() => setMessage(null), 4000);
    return () => clearTimeout(t);
  }, [message]);

  const handleBootstrap = useCallback(async () => {
    if (!currentProjectId) return;
    setIsBootstrapping(true);
    setError(null);
    setMessage(null);
    try {
      const created = await bootstrapMcpPresets(currentProjectId, {});
      if (created.length === 0) {
        setMessage("未装载任何内置 Preset（可能已全部装载或后端无内置定义）");
      } else {
        setMessage(`已装载 ${created.length} 个内置 Preset：${created.map((p) => p.name).join("、")}`);
      }
      await loadPresets(currentProjectId);
    } catch (e) {
      setError(e instanceof Error ? e.message : "装载内置 Preset 失败");
    } finally {
      setIsBootstrapping(false);
    }
  }, [currentProjectId, loadPresets]);

  if (!currentProjectId) {
    return (
      <div className="flex h-full items-center justify-center p-6">
        <div className="text-center text-sm text-muted-foreground">
          请选择项目后查看 MCP Preset
        </div>
      </div>
    );
  }

  const builtinCount = presets.filter((p) => p.source === "builtin").length;
  const userCount = presets.length - builtinCount;

  return (
    <div className="flex h-full flex-col gap-4 p-6">
      <header className="flex flex-wrap items-center justify-between gap-3">
        <div className="space-y-1">
          <h2 className="text-base font-semibold tracking-tight text-foreground">MCP Preset 资产</h2>
          <p className="text-xs text-muted-foreground">
            {builtinCount} 个 builtin · {userCount} 个 user · 单个 MCP Server 条目粒度，供 agent 装配复用
          </p>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <button
            type="button"
            onClick={() => void loadPresets(currentProjectId)}
            disabled={isLoading}
            className="h-9 rounded-[10px] border border-border bg-background px-3.5 text-sm text-foreground transition-colors hover:bg-secondary disabled:cursor-not-allowed disabled:opacity-60"
          >
            {isLoading ? "刷新中…" : "刷新"}
          </button>
          <button
            type="button"
            onClick={() => void handleBootstrap()}
            disabled={isBootstrapping}
            className="h-9 rounded-[10px] border border-border bg-background px-3.5 text-sm text-foreground transition-colors hover:bg-secondary disabled:cursor-not-allowed disabled:opacity-60"
            title="从内置 JSON 模板装载常用 MCP Server 定义（幂等，已装载会跳过）"
          >
            {isBootstrapping ? "装载中…" : "装载内置 Preset"}
          </button>
          <button
            type="button"
            disabled
            className="h-9 cursor-not-allowed rounded-[10px] border border-border bg-background px-3.5 text-sm text-muted-foreground opacity-60"
            title="PR5 实装：用户自定义 MCP Preset 创建表单"
          >
            + Preset（PR5 实装）
          </button>
        </div>
      </header>

      {/* 反馈消息 */}
      {message && (
        <div className="flex items-center justify-between rounded-[10px] border border-emerald-300/30 bg-emerald-500/5 px-3 py-2">
          <p className="text-xs text-emerald-600">{message}</p>
          <button
            type="button"
            onClick={() => setMessage(null)}
            className="ml-2 text-xs text-emerald-600/60 hover:text-emerald-600"
          >
            ×
          </button>
        </div>
      )}
      {error && (
        <div className="rounded-[10px] border border-destructive/30 bg-destructive/5 px-3 py-2">
          <p className="text-xs text-destructive">{error}</p>
        </div>
      )}

      {/* 列表 */}
      {isLoading && presets.length === 0 ? (
        <div className="rounded-[12px] border border-dashed border-border bg-secondary/20 px-6 py-10 text-center">
          <p className="text-sm text-muted-foreground">正在加载 MCP Preset…</p>
        </div>
      ) : presets.length === 0 ? (
        <div className="rounded-[12px] border border-dashed border-border bg-secondary/20 px-6 py-10 text-center">
          <p className="text-sm text-foreground">当前项目还没有任何 MCP Preset</p>
          <p className="mt-1 text-xs text-muted-foreground">
            点击上方「装载内置 Preset」快速装载常用 MCP Server 模板（http / sse / stdio）。
          </p>
        </div>
      ) : (
        <McpPresetGrid presets={presets} />
      )}
    </div>
  );
}

export default McpPresetCategoryPanel;

/* ─── 列表 ─── */

function McpPresetGrid({ presets }: { presets: McpPresetDto[] }) {
  const sorted = presets
    .slice()
    .sort((a, b) => {
      // builtin 优先展示在前，然后按 name 排
      if (a.source !== b.source) {
        return a.source === "builtin" ? -1 : 1;
      }
      return a.name.localeCompare(b.name, "zh-CN");
    });

  return (
    <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
      {sorted.map((preset) => (
        <McpPresetCard key={preset.id} preset={preset} />
      ))}
    </div>
  );
}

function McpPresetCard({ preset }: { preset: McpPresetDto }) {
  const decl = preset.server_decl;

  return (
    <article className="flex flex-col rounded-[12px] border border-border bg-background p-3.5 transition-colors hover:border-primary/25 hover:bg-secondary/30">
      <header className="flex items-start justify-between gap-2">
        <div className="min-w-0">
          <p className="truncate text-sm font-medium leading-6 text-foreground">{preset.name}</p>
          <p className="mt-0.5 truncate text-xs text-muted-foreground">server: {decl.name}</p>
        </div>
        <div className="flex shrink-0 items-center gap-1">
          <TransportBadge transport={decl.type} />
          <SourceBadge source={preset.source} />
        </div>
      </header>

      {preset.description && (
        <p className="mt-1.5 line-clamp-2 text-xs leading-5 text-muted-foreground">
          {preset.description}
        </p>
      )}

      <ServerPreview decl={decl} />

      <footer className="mt-3 flex items-center justify-between border-t border-border/70 pt-2.5 text-[11px] text-muted-foreground">
        <span>更新于 {formatDateTime(preset.updated_at)}</span>
        <div className="flex gap-1">
          <button
            type="button"
            disabled
            className="cursor-not-allowed rounded-[6px] px-1.5 py-0.5 text-[11px] text-muted-foreground opacity-60"
            title="PR5 实装：编辑 / 复制为 user / 删除"
          >
            {preset.source === "builtin" ? "查看（PR5）" : "编辑（PR5）"}
          </button>
        </div>
      </footer>
    </article>
  );
}

/* ─── 只读预览：按 transport 区分 ─── */

function ServerPreview({ decl }: { decl: McpServerDecl }) {
  if (decl.type === "http" || decl.type === "sse") {
    const headerCount = decl.headers?.length ?? 0;
    return (
      <div className="mt-3 space-y-1.5 rounded-[10px] border border-border/70 bg-secondary/20 p-2.5 text-[11px]">
        <div className="flex items-center gap-1.5 text-muted-foreground">
          <span className="shrink-0 text-foreground/70">URL</span>
          <span className="min-w-0 truncate font-mono" title={decl.url}>
            {decl.url || "(未配置)"}
          </span>
        </div>
        <div className="flex flex-wrap gap-1.5 text-muted-foreground">
          <span className="rounded-[6px] border border-border bg-background px-1.5 py-0.5">
            {headerCount} header{headerCount === 1 ? "" : "s"}
          </span>
          {decl.relay && (
            <span className="rounded-[6px] border border-sky-500/30 bg-sky-500/10 px-1.5 py-0.5 text-sky-700 dark:text-sky-300">
              relay
            </span>
          )}
        </div>
      </div>
    );
  }

  // stdio
  const args = decl.args ?? [];
  const envCount = decl.env?.length ?? 0;
  const argsPreview = args.length > 0 ? args.join(" ") : "(无参数)";

  return (
    <div className="mt-3 space-y-1.5 rounded-[10px] border border-border/70 bg-secondary/20 p-2.5 text-[11px]">
      <div className="flex items-center gap-1.5 text-muted-foreground">
        <span className="shrink-0 text-foreground/70">command</span>
        <span className="min-w-0 truncate font-mono" title={decl.command}>
          {decl.command || "(未配置)"}
        </span>
      </div>
      <div className="flex items-center gap-1.5 text-muted-foreground">
        <span className="shrink-0 text-foreground/70">args</span>
        <span className="min-w-0 truncate font-mono" title={argsPreview}>
          {argsPreview}
        </span>
      </div>
      <div className="flex flex-wrap gap-1.5 text-muted-foreground">
        <span className="rounded-[6px] border border-border bg-background px-1.5 py-0.5">
          {envCount} env
        </span>
        {decl.relay && (
          <span className="rounded-[6px] border border-sky-500/30 bg-sky-500/10 px-1.5 py-0.5 text-sky-700 dark:text-sky-300">
            relay
          </span>
        )}
      </div>
    </div>
  );
}

/* ─── Badges ─── */

function TransportBadge({ transport }: { transport: "http" | "sse" | "stdio" }) {
  const style =
    transport === "http"
      ? "border-emerald-500/30 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300"
      : transport === "sse"
        ? "border-violet-500/30 bg-violet-500/10 text-violet-700 dark:text-violet-300"
        : "border-orange-500/30 bg-orange-500/10 text-orange-700 dark:text-orange-300";
  return (
    <span className={`shrink-0 rounded-[6px] border px-1.5 py-0.5 text-[10px] font-medium ${style}`}>
      {transport}
    </span>
  );
}

function SourceBadge({ source }: { source: "builtin" | "user" }) {
  if (source === "builtin") {
    return (
      <span className="shrink-0 rounded-[6px] border border-amber-500/30 bg-amber-500/10 px-1.5 py-0.5 text-[10px] font-medium text-amber-700 dark:text-amber-300">
        builtin
      </span>
    );
  }
  return (
    <span className="shrink-0 rounded-[6px] border border-border bg-secondary/70 px-1.5 py-0.5 text-[10px] font-medium text-muted-foreground">
      user
    </span>
  );
}

/* ─── Utils ─── */

function formatDateTime(value: string): string {
  const time = new Date(value);
  if (Number.isNaN(time.getTime())) return value;
  return time.toLocaleString("zh-CN", {
    hour12: false,
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}
