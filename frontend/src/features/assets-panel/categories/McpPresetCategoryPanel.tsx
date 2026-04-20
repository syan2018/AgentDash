/**
 * McpPresetCategoryPanel — Assets 页 MCP Preset 类目（PR4 + PR5）。
 *
 * PR4（已 commit）：列表 + 只读预览 + 装载内置
 * PR5（本次）：新建 / 编辑 / 复制为 user / 删除 / builtin 只读查看
 *
 * 交互：
 * - 左列：builtin 优先展示、按 name 排序的卡片网格
 * - 右侧：同一区域叠放 detail 面板（表单 or 只读查看），仿 Workflow 的 inline modal 风格
 * - 表单：复用 `features/mcp-shared/McpServerDeclEditor`，字段与 agent-preset-editor 完全对齐
 *
 * 关键约束：
 * - description tombstone：表单追踪"原值 vs 当前值"，未改 → patch 不包含；
 *   用户清空 → 显式 null；修改到非空字符串 → 传字符串（对齐后端 Option<Option<String>>）
 * - Builtin Preset：
 *   - 编辑按钮 → 打开 disabled 只读表单
 *   - 删除按钮隐藏；tooltip 提示改用"复制为 user"
 *   - 复制按钮始终可用
 * - Transport 切换：由 `McpServerDeclEditor` 内部处理字段保留策略
 * - 客户端校验：name / server_decl.name 必填，http/sse 要求 url 非空（提交时拦截）
 */

import { useCallback, useEffect, useMemo, useState } from "react";

import { useProjectStore } from "../../../stores/projectStore";
import {
  bootstrapMcpPresets,
  cloneMcpPreset,
  createMcpPreset,
  deleteMcpPreset,
  fetchProjectMcpPresets,
  updateMcpPreset,
} from "../../../services/mcpPreset";
import type {
  CreateMcpPresetRequest,
  McpPresetDto,
  McpServerDecl,
  UpdateMcpPresetRequest,
} from "../../../types";
import { McpServerDeclEditor, createDefaultMcpServerDecl } from "../../mcp-shared";

/* ─── 表单状态 ─── */
//
// 表单保持"原始装载值"副本，用于在 update 时与当前值做 diff，
// 构造 patch（含 description tombstone 三态）。

interface FormBaseline {
  /** 编辑模式下为装载时的 Preset（用于 diff）；新建模式为 null */
  original: McpPresetDto | null;
}

interface FormState {
  name: string;
  /** 直接映射到 <textarea>；空串在 update 时表示"清空"（tombstone）*/
  description: string;
  server_decl: McpServerDecl;
}

function buildInitialForm(preset?: McpPresetDto | null): FormState {
  if (!preset) {
    return {
      name: "",
      description: "",
      server_decl: createDefaultMcpServerDecl(),
    };
  }
  return {
    name: preset.name,
    description: preset.description ?? "",
    server_decl: preset.server_decl,
  };
}

/** 客户端校验；返回错误信息或 null。 */
function validateForm(form: FormState): string | null {
  const trimmedName = form.name.trim();
  if (!trimmedName) return "Preset 名称不能为空";
  if (!form.server_decl.name.trim()) return "MCP Server 名称不能为空";
  if (form.server_decl.type === "http" || form.server_decl.type === "sse") {
    if (!form.server_decl.url.trim()) return "URL 不能为空";
    try {
      // 容错：允许 http(s) 以及项目习惯的相对 / ws(s)：只做"非空 + URL 可 parse"
      new URL(form.server_decl.url.trim());
    } catch {
      return "URL 格式非法";
    }
  }
  if (form.server_decl.type === "stdio" && !form.server_decl.command.trim()) {
    return "Command 不能为空";
  }
  return null;
}

/** 构造 update patch：仅把发生变化的字段放入；description 支持 null tombstone。 */
function buildUpdatePatch(current: FormState, original: McpPresetDto): UpdateMcpPresetRequest {
  const patch: UpdateMcpPresetRequest = {};
  const trimmedName = current.name.trim();
  if (trimmedName !== original.name) {
    patch.name = trimmedName;
  }
  const currentDesc = current.description.trim();
  const originalDesc = (original.description ?? "").trim();
  if (currentDesc !== originalDesc) {
    // 空串 → null（tombstone 清空）；非空 → 字符串
    patch.description = currentDesc ? currentDesc : null;
  }
  // server_decl：结构化比较用 JSON 序列化，字段顺序受 TS 序列化影响，
  // 但在受控表单里字段形态稳定；用 JSON.stringify 作 cheap deep equal
  if (JSON.stringify(current.server_decl) !== JSON.stringify(original.server_decl)) {
    patch.server_decl = current.server_decl;
  }
  return patch;
}

/* ─── 主面板 ─── */

type DetailMode =
  | { kind: "closed" }
  | { kind: "create" }
  | { kind: "edit"; presetId: string }
  | { kind: "view"; presetId: string };

export function McpPresetCategoryPanel() {
  const currentProjectId = useProjectStore((s) => s.currentProjectId);

  const [presets, setPresets] = useState<McpPresetDto[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [isBootstrapping, setIsBootstrapping] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [detail, setDetail] = useState<DetailMode>({ kind: "closed" });
  const [isSaving, setIsSaving] = useState(false);
  const [busyRowId, setBusyRowId] = useState<string | null>(null);
  const [confirmDelete, setConfirmDelete] = useState<McpPresetDto | null>(null);

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

  const handleClone = useCallback(
    async (preset: McpPresetDto) => {
      if (!currentProjectId) return;
      setBusyRowId(preset.id);
      setError(null);
      try {
        const cloned = await cloneMcpPreset(currentProjectId, preset.id, {});
        setMessage(`已复制为 user Preset：${cloned.name}`);
        await loadPresets(currentProjectId);
      } catch (e) {
        setError(friendlyError(e, `复制「${preset.name}」失败`));
      } finally {
        setBusyRowId(null);
      }
    },
    [currentProjectId, loadPresets],
  );

  const handleConfirmDelete = useCallback(async () => {
    if (!currentProjectId || !confirmDelete) return;
    setBusyRowId(confirmDelete.id);
    setError(null);
    try {
      await deleteMcpPreset(currentProjectId, confirmDelete.id);
      setMessage(`已删除：${confirmDelete.name}`);
      setConfirmDelete(null);
      // 如果详情面板正在查看被删的 Preset，关闭它
      if (
        (detail.kind === "edit" || detail.kind === "view") &&
        detail.presetId === confirmDelete.id
      ) {
        setDetail({ kind: "closed" });
      }
      await loadPresets(currentProjectId);
    } catch (e) {
      setError(friendlyError(e, `删除「${confirmDelete.name}」失败`));
    } finally {
      setBusyRowId(null);
    }
  }, [currentProjectId, confirmDelete, detail, loadPresets]);

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
            onClick={() => setDetail({ kind: "create" })}
            className="h-9 rounded-[10px] border border-primary bg-primary px-3.5 text-sm text-primary-foreground transition-colors hover:opacity-95"
          >
            + Preset
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
        <div className="flex items-center justify-between rounded-[10px] border border-destructive/30 bg-destructive/5 px-3 py-2">
          <p className="text-xs text-destructive">{error}</p>
          <button
            type="button"
            onClick={() => setError(null)}
            className="ml-2 text-xs text-destructive/60 hover:text-destructive"
          >
            ×
          </button>
        </div>
      )}

      {/* 列表 */}
      {isLoading && presets.length === 0 ? (
        <div className="rounded-[12px] border border-dashed border-border bg-secondary/20 px-6 py-10 text-center">
          <p className="text-sm text-muted-foreground">正在加载 MCP Preset…</p>
        </div>
      ) : presets.length === 0 ? (
        <div className="flex flex-col items-center rounded-[12px] border border-dashed border-border bg-secondary/20 px-6 py-10 text-center">
          <p className="text-sm text-foreground">当前项目还没有任何 MCP Preset</p>
          <p className="mt-1 text-xs text-muted-foreground">
            可点击「装载内置 Preset」装载常用模板，或点击下方按钮新建用户 Preset。
          </p>
          <button
            type="button"
            onClick={() => setDetail({ kind: "create" })}
            className="mt-4 rounded-[10px] border border-primary bg-primary px-3.5 py-1.5 text-sm text-primary-foreground transition-colors hover:opacity-95"
          >
            + 新建 MCP Preset
          </button>
        </div>
      ) : (
        <McpPresetGrid
          presets={presets}
          busyRowId={busyRowId}
          onEdit={(preset) =>
            setDetail({
              kind: preset.source === "builtin" ? "view" : "edit",
              presetId: preset.id,
            })
          }
          onClone={(preset) => void handleClone(preset)}
          onDelete={(preset) => setConfirmDelete(preset)}
        />
      )}

      {/* 详情 / 编辑 / 新建面板：通过 key 保证切换编辑目标时组件重挂载（表单自动重置，避免用 useEffect 同步 props 到 state） */}
      {detail.kind !== "closed" && (
        <McpPresetDetailDialog
          key={detail.kind === "create" ? "create" : `${detail.kind}:${detail.presetId}`}
          detail={detail}
          presets={presets}
          isSaving={isSaving}
          onClose={() => setDetail({ kind: "closed" })}
          onCreate={async (input) => {
            if (!currentProjectId) return;
            setIsSaving(true);
            setError(null);
            try {
              const created = await createMcpPreset(currentProjectId, input);
              setMessage(`已创建 Preset：${created.name}`);
              setDetail({ kind: "closed" });
              await loadPresets(currentProjectId);
            } catch (e) {
              setError(friendlyError(e, "创建 Preset 失败"));
            } finally {
              setIsSaving(false);
            }
          }}
          onUpdate={async (presetId, patch) => {
            if (!currentProjectId) return;
            setIsSaving(true);
            setError(null);
            try {
              const updated = await updateMcpPreset(currentProjectId, presetId, patch);
              setMessage(`已更新 Preset：${updated.name}`);
              setDetail({ kind: "closed" });
              await loadPresets(currentProjectId);
            } catch (e) {
              setError(friendlyError(e, "更新 Preset 失败"));
            } finally {
              setIsSaving(false);
            }
          }}
        />
      )}

      {/* 删除确认 */}
      {confirmDelete && (
        <ConfirmDeleteDialog
          preset={confirmDelete}
          isDeleting={busyRowId === confirmDelete.id}
          onCancel={() => setConfirmDelete(null)}
          onConfirm={() => void handleConfirmDelete()}
        />
      )}
    </div>
  );
}

export default McpPresetCategoryPanel;

/* ─── 错误翻译 ─── */
//
// 409 冲突 → "名字已存在…"；其他错误直接透传 message。
// 后端 ApiError::Conflict 返回 body.error 的字符串本身就带上下文，
// 这里只是对最常见的 409 补一个中文前缀。

function friendlyError(err: unknown, fallback: string): string {
  if (err instanceof Error) {
    type WithStatus = Error & { status?: number };
    const status = (err as WithStatus).status;
    if (status === 409) {
      return `${err.message}（建议换个名字）`;
    }
    return err.message || fallback;
  }
  return fallback;
}

/* ─── 列表 ─── */

interface GridCallbacks {
  onEdit: (preset: McpPresetDto) => void;
  onClone: (preset: McpPresetDto) => void;
  onDelete: (preset: McpPresetDto) => void;
  busyRowId: string | null;
}

function McpPresetGrid({
  presets,
  onEdit,
  onClone,
  onDelete,
  busyRowId,
}: { presets: McpPresetDto[] } & GridCallbacks) {
  const sorted = useMemo(() => {
    return presets.slice().sort((a, b) => {
      if (a.source !== b.source) {
        return a.source === "builtin" ? -1 : 1;
      }
      return a.name.localeCompare(b.name, "zh-CN");
    });
  }, [presets]);

  return (
    <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
      {sorted.map((preset) => (
        <McpPresetCard
          key={preset.id}
          preset={preset}
          onEdit={() => onEdit(preset)}
          onClone={() => onClone(preset)}
          onDelete={() => onDelete(preset)}
          isBusy={busyRowId === preset.id}
        />
      ))}
    </div>
  );
}

function McpPresetCard({
  preset,
  onEdit,
  onClone,
  onDelete,
  isBusy,
}: {
  preset: McpPresetDto;
  onEdit: () => void;
  onClone: () => void;
  onDelete: () => void;
  isBusy: boolean;
}) {
  const decl = preset.server_decl;
  const isBuiltin = preset.source === "builtin";

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
            onClick={onEdit}
            className="rounded-[6px] px-1.5 py-0.5 text-[11px] text-foreground/80 transition-colors hover:bg-secondary hover:text-foreground"
          >
            {isBuiltin ? "查看" : "编辑"}
          </button>
          <button
            type="button"
            onClick={onClone}
            disabled={isBusy}
            className="rounded-[6px] px-1.5 py-0.5 text-[11px] text-sky-600 transition-colors hover:bg-sky-500/10 disabled:opacity-50 dark:text-sky-300"
            title={isBuiltin ? "基于此 builtin Preset 生成可编辑的 user 副本" : "复制一份可独立修改的 user 副本"}
          >
            {isBusy ? "处理中…" : "复制为 user"}
          </button>
          {isBuiltin ? (
            <span
              className="cursor-not-allowed rounded-[6px] px-1.5 py-0.5 text-[11px] text-muted-foreground opacity-50"
              title="内置 Preset 不可删除，请使用“复制为 user”生成可编辑副本"
            >
              删除
            </span>
          ) : (
            <button
              type="button"
              onClick={onDelete}
              disabled={isBusy}
              className="rounded-[6px] px-1.5 py-0.5 text-[11px] text-destructive transition-colors hover:bg-destructive/10 disabled:opacity-50"
            >
              删除
            </button>
          )}
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

/* ─── Detail Dialog（新建 / 编辑 / 查看） ─── */

function McpPresetDetailDialog({
  detail,
  presets,
  isSaving,
  onClose,
  onCreate,
  onUpdate,
}: {
  detail: DetailMode;
  presets: McpPresetDto[];
  isSaving: boolean;
  onClose: () => void;
  onCreate: (input: CreateMcpPresetRequest) => Promise<void>;
  onUpdate: (presetId: string, patch: UpdateMcpPresetRequest) => Promise<void>;
}) {
  const target = useMemo(() => {
    if (detail.kind === "edit" || detail.kind === "view") {
      return presets.find((p) => p.id === detail.presetId) ?? null;
    }
    return null;
  }, [detail, presets]);

  const baseline = useMemo<FormBaseline>(() => ({ original: target }), [target]);

  const [form, setForm] = useState<FormState>(() => buildInitialForm(target));
  const [validationError, setValidationError] = useState<string | null>(null);

  // 切换 detail（编辑不同 Preset / 切到新建）由外层 `<McpPresetDetailDialog key=... />`
  // 触发组件重挂载，表单初始值通过 useState 懒初始化；此处无需 useEffect 同步 props。

  const isCreating = detail.kind === "create";
  const isViewOnly = detail.kind === "view";
  const isEditing = detail.kind === "edit";

  const patchForm = (patch: Partial<FormState>) => {
    setForm((prev) => ({ ...prev, ...patch }));
    setValidationError(null);
  };

  const handleSave = async () => {
    const err = validateForm(form);
    if (err) {
      setValidationError(err);
      return;
    }
    if (isCreating) {
      const input: CreateMcpPresetRequest = {
        name: form.name.trim(),
        server_decl: form.server_decl,
      };
      const trimmedDesc = form.description.trim();
      if (trimmedDesc) input.description = trimmedDesc;
      await onCreate(input);
      return;
    }
    if (isEditing && baseline.original) {
      const patch = buildUpdatePatch(form, baseline.original);
      if (Object.keys(patch).length === 0) {
        setValidationError("未检测到变更，无需保存");
        return;
      }
      await onUpdate(baseline.original.id, patch);
    }
  };

  const headerTitle = isCreating
    ? "新建 MCP Preset"
    : isViewOnly
      ? `查看 MCP Preset：${target?.name ?? ""}`
      : `编辑 MCP Preset：${target?.name ?? ""}`;

  return (
    <>
      <div
        className="fixed inset-0 z-[90] bg-foreground/18 backdrop-blur-[2px]"
        onClick={onClose}
      />
      <div className="fixed inset-0 z-[91] flex items-center justify-center p-4">
        <div className="w-full max-w-2xl rounded-[16px] border border-border bg-background shadow-2xl">
          <div className="border-b border-border px-5 py-4">
            <span className="inline-flex rounded-[6px] border border-border bg-secondary/70 px-1.5 py-0.5 text-[10px] font-medium uppercase tracking-[0.12em] text-muted-foreground">
              MCP Preset
            </span>
            <h4 className="mt-1 text-base font-semibold text-foreground">{headerTitle}</h4>
            {isViewOnly && (
              <p className="mt-1 text-xs text-muted-foreground">
                内置 Preset 为只读。如需修改，请关闭并使用「复制为 user」生成可编辑副本。
              </p>
            )}
            {isCreating && (
              <p className="mt-1 text-xs text-muted-foreground">
                创建后即可在 agent 配置面板中引用（活引用接入由后续子任务交付）。
              </p>
            )}
          </div>

          <div className="max-h-[70vh] space-y-3 overflow-y-auto p-5">
            <div className="grid grid-cols-1 gap-2.5 sm:grid-cols-2">
              <div>
                <label className="agentdash-form-label">Preset 名称</label>
                <input
                  value={form.name}
                  onChange={(e) => patchForm({ name: e.target.value })}
                  placeholder="唯一标识，例如 filesystem-read"
                  disabled={isViewOnly}
                  className="agentdash-form-input"
                />
                <p className="mt-0.5 text-[10px] text-muted-foreground/60">
                  项目内唯一；用于在 agent 配置处选择
                </p>
              </div>
              <div>
                <label className="agentdash-form-label">来源</label>
                <div className="agentdash-form-input flex items-center text-xs text-muted-foreground">
                  {isCreating
                    ? "user（新建）"
                    : target?.source === "builtin"
                      ? `builtin${target?.builtin_key ? ` · ${target.builtin_key}` : ""}`
                      : "user"}
                </div>
              </div>
            </div>

            <div>
              <label className="agentdash-form-label">描述</label>
              <textarea
                value={form.description}
                onChange={(e) => patchForm({ description: e.target.value })}
                rows={2}
                placeholder="可选，说明该 Preset 的用途"
                disabled={isViewOnly}
                className="agentdash-form-textarea"
              />
              {isEditing && baseline.original && (
                <p className="mt-0.5 text-[10px] text-muted-foreground/60">
                  留空保存会清空当前描述
                </p>
              )}
            </div>

            <div>
              <label className="agentdash-form-label">MCP Server 定义</label>
              <McpServerDeclEditor
                value={form.server_decl}
                onChange={(server_decl) => patchForm({ server_decl })}
                disabled={isViewOnly}
              />
            </div>

            {validationError && (
              <p className="text-xs text-destructive">{validationError}</p>
            )}
          </div>

          <div className="flex items-center justify-end gap-2 border-t border-border px-5 py-4">
            <button
              type="button"
              onClick={onClose}
              className="agentdash-button-secondary"
              disabled={isSaving}
            >
              {isViewOnly ? "关闭" : "取消"}
            </button>
            {!isViewOnly && (
              <button
                type="button"
                onClick={() => void handleSave()}
                className="agentdash-button-primary"
                disabled={isSaving}
              >
                {isSaving
                  ? "保存中…"
                  : isCreating
                    ? "创建 Preset"
                    : "保存修改"}
              </button>
            )}
          </div>
        </div>
      </div>
    </>
  );
}

/* ─── 删除确认 Dialog ─── */

function ConfirmDeleteDialog({
  preset,
  isDeleting,
  onCancel,
  onConfirm,
}: {
  preset: McpPresetDto;
  isDeleting: boolean;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  return (
    <div
      className="fixed inset-0 z-[92] flex items-center justify-center bg-black/40"
      onClick={onCancel}
    >
      <div
        className="w-[380px] rounded-[14px] border border-border bg-background p-5 shadow-xl"
        onClick={(e) => e.stopPropagation()}
      >
        <h3 className="text-sm font-semibold text-foreground">确认删除 MCP Preset</h3>
        <p className="mt-2 text-xs leading-5 text-muted-foreground">
          确定要删除{" "}
          <span className="font-medium text-foreground">{preset.name}</span> 吗？此操作不可撤销；
          已引用该 Preset 的 agent 配置在运行时会提示缺失（本任务尚未接入活引用）。
        </p>
        <div className="mt-4 flex justify-end gap-2">
          <button
            type="button"
            onClick={onCancel}
            className="rounded-[8px] border border-border px-3 py-1.5 text-xs text-muted-foreground transition-colors hover:bg-secondary"
          >
            取消
          </button>
          <button
            type="button"
            onClick={onConfirm}
            disabled={isDeleting}
            className="rounded-[8px] border border-destructive/30 bg-destructive px-3 py-1.5 text-xs text-destructive-foreground transition-colors hover:opacity-90 disabled:opacity-50"
          >
            {isDeleting ? "删除中…" : "删除"}
          </button>
        </div>
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
