/**
 * McpPresetCategoryPanel — Assets 页 MCP Preset 类目（PR4 + PR5）。
 *
 * PR4（已 commit）：列表 + 只读预览 + 装载内置
 * PR5（本次）：新建 / 编辑 / 复制为 user / 删除 / builtin 只读查看
 *
 * 交互：
 * - 左列：builtin 优先展示、按 display_name 排序的卡片网格
 * - 右侧：同一区域叠放 detail 面板（表单 or 只读查看），仿 Workflow 的 inline modal 风格
 * - 表单：复用 `features/mcp-shared/McpTransportConfigEditor`
 *
 * 关键约束：
 * - description tombstone：表单追踪"原值 vs 当前值"，未改 → patch 不包含；
 *   用户清空 → 显式 null；修改到非空字符串 → 传字符串（对齐后端 Option<Option<String>>）
 * - Builtin Preset：
 *   - 编辑按钮 → 打开 disabled 只读表单
 *   - 删除按钮隐藏；tooltip 提示改用"复制为 user"
 *   - 复制按钮始终可用
 * - Transport 切换：由 `McpTransportConfigEditor` 内部处理字段保留策略
 * - 客户端校验：key / display_name 必填，http/sse 要求 url 非空（提交时拦截）
 */

import { useCallback, useEffect, useMemo, useState } from "react";

import { formatDateTime } from "../../../lib/format";
import { useProjectStore } from "../../../stores/projectStore";
import { useMcpProbeStore } from "../../../stores/mcpProbeStore";
import { useCurrentUserStore } from "../../../stores/currentUserStore";
import {
  cloneMcpPreset,
  createMcpPreset,
  deleteMcpPreset,
  fetchProjectMcpPresets,
  updateMcpPreset,
} from "../../../services/mcpPreset";
import type {
  CreateMcpPresetRequest,
  LibraryAssetDto,
  McpPresetDto,
  McpRoutePolicy,
  McpRuntimeBindingConfig,
  McpRuntimeBindingRule,
  McpRuntimeBindingSource,
  McpRuntimeBindingTarget,
  McpTransportConfig,
  ProbeMcpPresetResponse,
  UpdateMcpPresetRequest,
} from "../../../types";
import { McpTransportConfigEditor } from "../../mcp-shared";
import {
  MCP_ROUTE_POLICY_OPTIONS,
  buildCreateMcpPresetRequest,
  buildMcpPresetFormState,
  buildUpdateMcpPresetPatch,
  createDefaultMcpRuntimeBindingRule,
  mcpRuntimeBindingRuleCount,
  readMcpRoutePolicy,
  validateMcpPresetForm,
  type McpPresetFormState,
} from "../../mcp-shared/helpers";
import {
  buildMcpProbeViewModel,
  describeMcpProbeTransport,
  type McpProbeTone,
  type McpProbeViewModel,
  type McpProbeViewStatus,
} from "../../mcp-shared/probeViewModel";
import {
  AssetCard,
  CardMenu,
  CreateButton,
  DangerConfirmDialog,
  type DismissibleNoticeData,
  OriginBadge,
} from "@agentdash/ui";
import { buildAssetMenuItems } from "../_shared/assetMenu";
import { CategoryPageShell } from "../_shared/CategoryPageShell";
import { resolveOriginBadge } from "../_shared/origin-badge-tone";
import { PublishedBadge } from "../_shared/PublishedBadge";
import { SelectProjectEmpty } from "../_shared/SelectProjectEmpty";
import { useLibraryPublishedAssets } from "../_shared/useLibraryPublishedAssets";
import { PublishLibraryAssetDialog } from "../publish/PublishLibraryAssetDialog";

/* ─── 表单状态 ─── */
//
// 表单保持"原始装载值"副本，用于在 update 时与当前值做 diff，
// 构造 patch（含 description tombstone 三态）。

interface FormBaseline {
  /** 编辑模式下为装载时的 Preset（用于 diff）；新建模式为 null */
  original: McpPresetDto | null;
}

interface RuntimeBindingSourceOption {
  value: string;
  label: string;
  source: McpRuntimeBindingSource;
}

interface RuntimeBindingTargetOption {
  kind: McpRuntimeBindingTarget["kind"];
  label: string;
}

const RUNTIME_BINDING_SOURCE_OPTIONS = [
  {
    value: "workspace_detected_fact:p4.client_name",
    label: "P4 client",
    source: { kind: "workspace_detected_fact", path: ["p4", "client_name"] },
  },
  {
    value: "workspace_detected_fact:p4.workspace_root",
    label: "P4 workspace root",
    source: { kind: "workspace_detected_fact", path: ["p4", "workspace_root"] },
  },
  {
    value: "workspace_detected_fact:p4.server_address",
    label: "P4 server",
    source: { kind: "workspace_detected_fact", path: ["p4", "server_address"] },
  },
  {
    value: "workspace_detected_fact:p4.stream",
    label: "P4 stream",
    source: { kind: "workspace_detected_fact", path: ["p4", "stream"] },
  },
  {
    value: "workspace_detected_fact:p4.user_name",
    label: "P4 user",
    source: { kind: "workspace_detected_fact", path: ["p4", "user_name"] },
  },
  {
    value: "workspace_id",
    label: "Workspace id",
    source: { kind: "workspace_id" },
  },
  {
    value: "workspace_binding_id",
    label: "Workspace binding id",
    source: { kind: "workspace_binding_id" },
  },
  {
    value: "vfs_root_ref",
    label: "VFS root ref",
    source: { kind: "vfs_root_ref" },
  },
  {
    value: "runtime_backend_anchor_backend_id",
    label: "Runtime backend anchor",
    source: { kind: "runtime_backend_anchor_backend_id" },
  },
  {
    value: "workspace_detected_fact:custom",
    label: "Detected fact path",
    source: { kind: "workspace_detected_fact", path: [] },
  },
  {
    value: "workspace_identity:custom",
    label: "Workspace identity path",
    source: { kind: "workspace_identity", path: [] },
  },
] satisfies ReadonlyArray<RuntimeBindingSourceOption>;

const RUNTIME_BINDING_TARGET_OPTIONS = [
  { kind: "http_query", label: "HTTP query" },
  { kind: "http_header", label: "HTTP header" },
  { kind: "stdio_env", label: "Stdio env" },
  { kind: "stdio_cwd", label: "Stdio cwd" },
] satisfies ReadonlyArray<RuntimeBindingTargetOption>;

/* ─── 主面板 ─── */

type DetailMode =
  | { kind: "closed" }
  | { kind: "create" }
  | { kind: "edit"; presetId: string }
  | { kind: "view"; presetId: string };

export function McpPresetCategoryPanel() {
  const currentProjectId = useProjectStore((s) => s.currentProjectId);
  const currentUserId = useCurrentUserStore((s) => s.currentUser?.user_id ?? null);

  const [presets, setPresets] = useState<McpPresetDto[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [notice, setNotice] = useState<DismissibleNoticeData | null>(null);
  const showSuccess = useCallback((msg: string) => setNotice({ tone: "success", message: msg }), []);
  const showError = useCallback((msg: string) => setNotice({ tone: "danger", message: msg }), []);
  const clearNotice = useCallback(() => setNotice(null), []);
  const [detail, setDetail] = useState<DetailMode>({ kind: "closed" });
  const [isSaving, setIsSaving] = useState(false);
  const [busyRowId, setBusyRowId] = useState<string | null>(null);
  const [confirmDelete, setConfirmDelete] = useState<McpPresetDto | null>(null);
  const [publishTarget, setPublishTarget] = useState<McpPresetDto | null>(null);
  const { publishedByKey, reloadPublished } = useLibraryPublishedAssets("mcp_server_template");

  const loadPresets = useCallback(
    async (projectId: string) => {
      setIsLoading(true);
      clearNotice();
      try {
        const next = await fetchProjectMcpPresets(projectId);
        setPresets(next);
      } catch (e) {
        showError(e instanceof Error ? e.message : "加载 MCP Preset 失败");
      } finally {
        setIsLoading(false);
      }
    },
    [clearNotice, showError],
  );

  useEffect(() => {
    if (!currentProjectId) return;
    void loadPresets(currentProjectId);
  }, [currentProjectId, loadPresets]);

  const handleClone = useCallback(
    async (preset: McpPresetDto) => {
      if (!currentProjectId) return;
      setBusyRowId(preset.id);
      clearNotice();
      try {
        const cloned = await cloneMcpPreset(currentProjectId, preset.id, {});
        showSuccess(`已复制为 user Preset：${cloned.display_name}`);
        await loadPresets(currentProjectId);
      } catch (e) {
        showError(friendlyError(e, `复制「${preset.display_name}」失败`));
      } finally {
        setBusyRowId(null);
      }
    },
    [currentProjectId, loadPresets, clearNotice, showSuccess, showError],
  );

  const handleConfirmDelete = useCallback(async () => {
    if (!currentProjectId || !confirmDelete) return;
    setBusyRowId(confirmDelete.id);
    clearNotice();
    try {
      await deleteMcpPreset(currentProjectId, confirmDelete.id);
      showSuccess(`已删除：${confirmDelete.display_name}`);
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
      showError(friendlyError(e, `删除「${confirmDelete.display_name}」失败`));
    } finally {
      setBusyRowId(null);
    }
  }, [currentProjectId, confirmDelete, detail, loadPresets, clearNotice, showSuccess, showError]);

  if (!currentProjectId) {
    return <SelectProjectEmpty assetLabel="MCP Preset 资产" />;
  }

  const builtinCount = presets.filter((p) => p.source === "builtin").length;
  const userCount = presets.length - builtinCount;

  return (
    <CategoryPageShell
      title="MCP Preset 资产"
      stats={`${builtinCount} 个内置 · ${userCount} 个自定义 · 供 Agent 装配的 MCP Server 模板`}
      actions={<CreateButton entity="MCP" onClick={() => setDetail({ kind: "create" })} />}
      notice={notice}
      onDismissNotice={clearNotice}
    >
      {isLoading && presets.length === 0 ? (
        <div className="rounded-[12px] border border-dashed border-border bg-secondary/20 px-6 py-10 text-center">
          <p className="text-sm text-muted-foreground">正在加载 MCP Preset…</p>
        </div>
      ) : presets.length === 0 ? (
        <div className="flex flex-col items-center rounded-[12px] border border-dashed border-border bg-secondary/20 px-6 py-10 text-center">
          <p className="text-sm text-foreground">当前项目还没有任何 MCP Preset</p>
          <p className="mt-1 text-xs text-muted-foreground">
            可从资源市场安装公共模板，或点击下方"+ MCP"新建。
          </p>
          <CreateButton
            entity="MCP"
            className="mt-4"
            onClick={() => setDetail({ kind: "create" })}
          />
        </div>
      ) : (
        <McpPresetGrid
          presets={presets}
          publishedByKey={publishedByKey}
          busyRowId={busyRowId}
          onEdit={(preset) =>
            setDetail({
              kind: preset.source === "builtin" ? "view" : "edit",
              presetId: preset.id,
            })
          }
          onPublish={setPublishTarget}
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
            clearNotice();
            try {
              const created = await createMcpPreset(currentProjectId, input);
              showSuccess(`已创建 Preset：${created.display_name}`);
              setDetail({ kind: "closed" });
              await loadPresets(currentProjectId);
            } catch (e) {
              showError(friendlyError(e, "创建 Preset 失败"));
            } finally {
              setIsSaving(false);
            }
          }}
          onUpdate={async (presetId, patch) => {
            if (!currentProjectId) return;
            setIsSaving(true);
            clearNotice();
            try {
              const updated = await updateMcpPreset(currentProjectId, presetId, patch);
              showSuccess(`已更新 Preset：${updated.display_name}`);
              setDetail({ kind: "closed" });
              await loadPresets(currentProjectId);
            } catch (e) {
              showError(friendlyError(e, "更新 Preset 失败"));
            } finally {
              setIsSaving(false);
            }
          }}
        />
      )}

      <DangerConfirmDialog
        open={confirmDelete != null}
        title="确认删除 MCP Preset"
        description={
          confirmDelete
            ? `确定要删除 ${confirmDelete.display_name} 吗？此操作不可撤销，已引用该 Preset 的 Agent 配置在运行时会提示缺失。`
            : ""
        }
        confirmLabel={
          confirmDelete && busyRowId === confirmDelete.id ? "删除中…" : "删除"
        }
        isConfirming={confirmDelete != null && busyRowId === confirmDelete.id}
        onClose={() => setConfirmDelete(null)}
        onConfirm={() => void handleConfirmDelete()}
      />

      {publishTarget && (
        <PublishLibraryAssetDialog
          projectId={currentProjectId}
          assetKind="mcp_preset"
          projectAssetId={publishTarget.id}
          defaults={{
            key: publishTarget.key,
            display_name: publishTarget.display_name,
            description: publishTarget.description,
          }}
          currentUserId={currentUserId}
          onClose={() => setPublishTarget(null)}
          onPublished={(message) => {
            showSuccess(message);
            void loadPresets(currentProjectId);
            reloadPublished();
          }}
        />
      )}
    </CategoryPageShell>
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
  onPublish: (preset: McpPresetDto) => void;
  onClone: (preset: McpPresetDto) => void;
  onDelete: (preset: McpPresetDto) => void;
  busyRowId: string | null;
}

function McpPresetGrid({
  presets,
  publishedByKey,
  onEdit,
  onPublish,
  onClone,
  onDelete,
  busyRowId,
}: {
  presets: McpPresetDto[];
  publishedByKey: Map<string, LibraryAssetDto>;
} & GridCallbacks) {
  const sorted = useMemo(() => {
    return presets.slice().sort((a, b) => {
      if (a.source !== b.source) {
        return a.source === "builtin" ? -1 : 1;
      }
      return a.display_name.localeCompare(b.display_name, "zh-CN");
    });
  }, [presets]);

  return (
    <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
      {sorted.map((preset) => (
        <McpPresetCard
          key={preset.id}
          preset={preset}
          published={publishedByKey.get(preset.key) ?? null}
          onEdit={() => onEdit(preset)}
          onPublish={() => onPublish(preset)}
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
  published,
  onEdit,
  onPublish,
  onClone,
  onDelete,
  isBusy,
}: {
  preset: McpPresetDto;
  published: LibraryAssetDto | null;
  onEdit: () => void;
  onPublish: () => void;
  onClone: () => void;
  onDelete: () => void;
  isBusy: boolean;
}) {
  const isBuiltin = preset.source === "builtin";
  const isInstalled = Boolean(preset.installed_source);
  const canPublish = !isBuiltin && !isInstalled;
  const sourceOrigin = resolveOriginBadge(preset.source, isInstalled);

  // probe 改为按需：缓存命中直接展示，无缓存只显示"尚未探测"，
  // 仅在用户点击"重新检测"时才真正发请求（避免每次切到 MCP Preset 页就并发 N 个 rmcp client）。
  const probeResult = useMcpProbeStore((state) =>
    state.getCached(preset.project_id, preset.transport, preset.runtime_binding ?? null),
  );
  const refreshProbe = useMcpProbeStore((state) => state.refresh);
  const [probing, setProbing] = useState(false);

  const handleRecheck = useCallback(() => {
    setProbing(true);
    void refreshProbe(preset.project_id, preset.transport, preset.runtime_binding ?? null).finally(
      () => setProbing(false),
    );
  }, [refreshProbe, preset.project_id, preset.transport, preset.runtime_binding]);

  const menuItems = buildAssetMenuItems({
    primary: { label: isBuiltin ? "查看" : "编辑", onSelect: onEdit },
    publish: canPublish
      ? { published: Boolean(published), onSelect: onPublish }
      : null,
    extras: [
      {
        key: "clone",
        label: isBusy ? "处理中…" : "复制为 user",
        onSelect: onClone,
      },
    ],
    danger: isBuiltin ? null : { label: "删除", onSelect: onDelete },
  });

  return (
    <AssetCard
      onOpen={onEdit}
      openTitle={isBuiltin ? "查看" : "编辑"}
      title={preset.display_name}
      subtitle={`key: ${preset.key}`}
      description={preset.description}
      headerRight={
        <>
          {published && <PublishedBadge version={published.version} />}
          <RuntimeBindingBadge runtimeBinding={preset.runtime_binding} />
          <RoutePolicyBadge policy={preset.route_policy} />
          <OriginBadge tone={sourceOrigin.tone} label={sourceOrigin.label} />
          <CardMenu items={menuItems} />
        </>
      }
      footer={<>更新于 {formatDateTime(preset.updated_at)}</>}
    >
      <ToolCapsules
        probing={probing}
        result={probeResult}
        onRecheck={handleRecheck}
      />
    </AssetCard>
  );
}

/* ─── 工具 capsule 预览：按需 probe 后展示工具列表（带手动重测）─── */

function ToolCapsules({
  probing,
  result,
  onRecheck,
}: {
  probing: boolean;
  result: ProbeMcpPresetResponse | null;
  onRecheck: () => void;
}) {
  const probeView = buildMcpProbeViewModel(result);
  return (
    <div className="mt-3 space-y-1.5">
      <div className="flex items-center justify-between gap-2 text-[10px] text-muted-foreground/70">
        <span>{probing ? "探测中…" : probeView.headerLabel}</span>
        <button
          type="button"
          onClick={(e) => {
            e.stopPropagation();
            onRecheck();
          }}
          disabled={probing}
          className="rounded-[6px] px-1.5 py-0.5 text-[10px] text-foreground/70 transition-colors hover:bg-secondary hover:text-foreground disabled:opacity-50"
          title="重新检测连通性并刷新工具列表"
        >
          {probing ? "…" : "重新检测"}
        </button>
      </div>
      <ToolCapsulesBody probing={probing} probeView={probeView} />
    </div>
  );
}

function ToolCapsulesBody({
  probing,
  probeView,
}: {
  probing: boolean;
  probeView: McpProbeViewModel;
}) {
  const box =
    "flex min-h-[44px] flex-wrap items-center gap-1.5 rounded-[10px] border border-border/70 bg-secondary/20 p-2.5 text-[11px]";

  if (probing && probeView.status === "idle") {
    return (
      <div className={box}>
        <span className="text-muted-foreground">探测中…</span>
      </div>
    );
  }

  if (!probeView.showToolGrid) {
    return (
      <div className={box}>
        <span
          className={probeBodyClassName(probeView.bodyTone, probeView.status)}
          title={probeView.bodyTitle ?? undefined}
        >
          {probeView.bodyMessage}
        </span>
      </div>
    );
  }
  return <ToolCapsuleGrid tools={probeView.tools} />;
}

function probeBodyClassName(tone: McpProbeTone, status: McpProbeViewStatus): string {
  if (tone === "danger") return "text-destructive";
  if (status === "idle") return "text-muted-foreground/60";
  return "text-muted-foreground";
}

function probeToneClassName(tone: McpProbeTone): string {
  if (tone === "success") return "text-success";
  if (tone === "danger") return "text-destructive";
  return "text-muted-foreground";
}

/** 通用 capsule 网格：展示全部工具，hover 显示描述。 */
function ToolCapsuleGrid({
  tools,
}: {
  tools: ReadonlyArray<{ name: string; description: string }>;
}) {
  return (
    <div className="flex flex-wrap items-center gap-1.5 rounded-[8px] border border-border/70 bg-secondary/20 p-2.5 text-[11px]">
      {tools.map((tool) => (
        <span
          key={tool.name}
          className="max-w-full truncate rounded-[8px] border border-border bg-background px-2 py-0.5 font-mono text-[10.5px] text-foreground/80"
          title={tool.description ? `${tool.name} — ${tool.description}` : tool.name}
        >
          {tool.name}
        </span>
      ))}
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
  const currentProjectId = useProjectStore((s) => s.currentProjectId);
  const target = useMemo(() => {
    if (detail.kind === "edit" || detail.kind === "view") {
      return presets.find((p) => p.id === detail.presetId) ?? null;
    }
    return null;
  }, [detail, presets]);

  const baseline = useMemo<FormBaseline>(() => ({ original: target }), [target]);

  const [form, setForm] = useState<McpPresetFormState>(() => buildMcpPresetFormState(target));
  const [validationError, setValidationError] = useState<string | null>(null);

  // Probe 状态：使用当前表单里的 transport（所见即所测），
  // 不依赖 preset id，因此新建模式也可以预先验证。共享 mcpProbeStore 缓存：
  // 同一 transport 在卡片上点过"重新检测"，进入详情就能直接看到结果。
  const cachedProbeResult = useMcpProbeStore((state) =>
    currentProjectId ? state.getCached(currentProjectId, form.transport, form.runtime_binding) : null,
  );
  const refreshProbe = useMcpProbeStore((state) => state.refresh);
  const [probing, setProbing] = useState(false);
  // 本地覆盖：用户在 dialog 内点 Test Connection 后的最新结果。
  // null 时回退到 cachedProbeResult（包括 transport 改动后的缓存命中）。
  const [localProbeResult, setLocalProbeResult] = useState<ProbeMcpPresetResponse | null>(null);
  const probeResult = localProbeResult ?? cachedProbeResult;

  const runProbe = async () => {
    if (!currentProjectId) return;
    setProbing(true);
    setLocalProbeResult(null);
    try {
      const result = await refreshProbe(currentProjectId, form.transport, form.runtime_binding);
      setLocalProbeResult(result);
    } finally {
      setProbing(false);
    }
  };

  // 切换 detail（编辑不同 Preset / 切到新建）由外层 `<McpPresetDetailDialog key=... />`
  // 触发组件重挂载，表单初始值通过 useState 懒初始化；此处无需 useEffect 同步 props。

  const isCreating = detail.kind === "create";
  const isViewOnly = detail.kind === "view";
  const isEditing = detail.kind === "edit";

  const patchForm = (patch: Partial<McpPresetFormState>) => {
    setForm((prev) => ({ ...prev, ...patch }));
    setValidationError(null);
  };

  const handleSave = async () => {
    const err = validateMcpPresetForm(form);
    if (err) {
      setValidationError(err);
      return;
    }
    if (isCreating) {
      await onCreate(buildCreateMcpPresetRequest(form));
      return;
    }
    if (isEditing && baseline.original) {
      const patch = buildUpdateMcpPresetPatch(form, baseline.original);
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
      ? `查看 MCP Preset：${target?.display_name ?? ""}`
      : `编辑 MCP Preset：${target?.display_name ?? ""}`;

  return (
    <>
      <div
        className="fixed inset-0 z-[90] bg-foreground/18 backdrop-blur-[2px]"
        onClick={onClose}
      />
      <div className="fixed inset-0 z-[91] flex items-center justify-center p-4">
        <div className="w-full max-w-2xl rounded-[12px] border border-border bg-background shadow-2xl">
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
                <label className="agentdash-form-label">工具标识</label>
                <input
                  value={form.key}
                  onChange={(e) => patchForm({ key: e.target.value })}
                  placeholder="唯一标识，例如 filesystem-read"
                  disabled={isViewOnly}
                  className="agentdash-form-input"
                />
                <p className="mt-0.5 text-[10px] text-muted-foreground/60">
                  项目内唯一；同时作为 agent-facing server name
                </p>
              </div>
              <div>
                <label className="agentdash-form-label">显示名称</label>
                <input
                  value={form.display_name}
                  onChange={(e) => patchForm({ display_name: e.target.value })}
                  placeholder="例如 Filesystem"
                  disabled={isViewOnly}
                  className="agentdash-form-input"
                />
              </div>
            </div>

            <div className="grid grid-cols-1 gap-2.5 sm:grid-cols-2">
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
              <div>
                <label className="agentdash-form-label">路由策略</label>
                <select
                  value={form.route_policy}
                  onChange={(e) => patchForm({ route_policy: readMcpRoutePolicy(e.target.value) })}
                  disabled={isViewOnly}
                  className="agentdash-form-select"
                >
                  {MCP_ROUTE_POLICY_OPTIONS.map((option) => (
                    <option key={option.value} value={option.value}>
                      {option.label}
                    </option>
                  ))}
                </select>
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
              <label className="agentdash-form-label">Transport 定义</label>
              <McpTransportConfigEditor
                value={form.transport}
                onChange={(transport) => patchForm({ transport })}
                disabled={isViewOnly}
              />
            </div>

            <RuntimeBindingEditor
              value={form.runtime_binding}
              transportType={form.transport.type}
              onChange={(runtime_binding) => patchForm({ runtime_binding })}
              disabled={isViewOnly}
            />

            {!isCreating && target && (
              <ProbePanel
                probing={probing}
                result={probeResult}
                transportType={form.transport.type}
                onProbe={() => void runProbe()}
              />
            )}

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

/* ─── Runtime Binding 编辑器 ─── */

function RuntimeBindingEditor({
  value,
  transportType,
  onChange,
  disabled,
}: {
  value: McpRuntimeBindingConfig | null;
  transportType: McpTransportConfig["type"];
  onChange: (value: McpRuntimeBindingConfig | null) => void;
  disabled?: boolean;
}) {
  const config = value ?? { mount_id: "main", bindings: [] };
  const bindings = config.bindings ?? [];
  const ruleCount = bindings.length;

  const commit = (next: McpRuntimeBindingConfig) => {
    const nextBindings = next.bindings ?? [];
    onChange(nextBindings.length > 0 ? { ...next, bindings: nextBindings } : null);
  };

  const addRule = () => {
    commit({
      mount_id: config.mount_id?.trim() || "main",
      bindings: [...bindings, createDefaultMcpRuntimeBindingRule(transportType)],
    });
  };

  const updateRule = (index: number, nextRule: McpRuntimeBindingRule) => {
    commit({
      ...config,
      bindings: bindings.map((rule, ruleIndex) => (ruleIndex === index ? nextRule : rule)),
    });
  };

  const removeRule = (index: number) => {
    commit({
      ...config,
      bindings: bindings.filter((_, ruleIndex) => ruleIndex !== index),
    });
  };

  return (
    <div className="rounded-[8px] border border-border bg-secondary/20 p-3">
      <div className="flex items-center justify-between gap-3">
        <div>
          <p className="text-xs font-medium text-foreground">运行时绑定</p>
          <p className="mt-0.5 text-[10px] text-muted-foreground/70">
            从当前运行上下文的 workspace/VFS 读取变量，并写入 MCP transport。
          </p>
        </div>
        <div className="flex items-center gap-2">
          <RuntimeBindingBadge runtimeBinding={value} />
          {!disabled && (
            <button
              type="button"
              onClick={addRule}
              className="agentdash-button-secondary shrink-0"
            >
              添加绑定
            </button>
          )}
        </div>
      </div>

      {ruleCount === 0 ? (
        <p className="mt-2 text-[11px] text-muted-foreground/70">未配置运行时绑定。</p>
      ) : (
        <div className="mt-3 space-y-3">
          <div>
            <label className="agentdash-form-label">Mount ID</label>
            <input
              value={config.mount_id ?? "main"}
              onChange={(e) => commit({ ...config, mount_id: e.target.value, bindings })}
              placeholder="main"
              disabled={disabled}
              className="agentdash-form-input"
            />
          </div>

          <div className="space-y-2">
            {bindings.map((rule, index) => (
              <RuntimeBindingRuleEditor
                key={index}
                rule={rule}
                index={index}
                disabled={disabled}
                onChange={(nextRule) => updateRule(index, nextRule)}
                onRemove={() => removeRule(index)}
              />
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

function RuntimeBindingRuleEditor({
  rule,
  index,
  disabled,
  onChange,
  onRemove,
}: {
  rule: McpRuntimeBindingRule;
  index: number;
  disabled?: boolean;
  onChange: (rule: McpRuntimeBindingRule) => void;
  onRemove: () => void;
}) {
  const sourceValue = runtimeBindingSourceValue(rule.source);
  const targetKind = rule.target.kind;
  const sourceNeedsPath = runtimeBindingSourceNeedsPath(rule.source);
  const targetNeedsName = runtimeBindingTargetNeedsName(rule.target);

  return (
    <div className="rounded-[8px] border border-border bg-background p-2.5">
      <div className="mb-2 flex items-center justify-between gap-2">
        <span className="text-[10px] font-medium text-muted-foreground">Binding {index + 1}</span>
        {!disabled && (
          <button
            type="button"
            onClick={onRemove}
            className="rounded-[6px] border border-destructive/30 px-2 py-0.5 text-[10px] text-destructive hover:bg-destructive/10"
          >
            删除
          </button>
        )}
      </div>

      <div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
        <div>
          <label className="agentdash-form-label">Source</label>
          <select
            value={sourceValue}
            onChange={(e) => {
              onChange({
                ...rule,
                source: createRuntimeBindingSource(e.target.value, rule.source),
              });
            }}
            disabled={disabled}
            className="agentdash-form-select"
          >
            {RUNTIME_BINDING_SOURCE_OPTIONS.map((option) => (
              <option key={option.value} value={option.value}>
                {option.label}
              </option>
            ))}
          </select>
        </div>
        <div>
          <label className="agentdash-form-label">Target</label>
          <select
            value={targetKind}
            onChange={(e) => {
              onChange({
                ...rule,
                target: createRuntimeBindingTarget(e.target.value, rule.target),
              });
            }}
            disabled={disabled}
            className="agentdash-form-select"
          >
            {RUNTIME_BINDING_TARGET_OPTIONS.map((option) => (
              <option key={option.kind} value={option.kind}>
                {option.label}
              </option>
            ))}
          </select>
        </div>
      </div>

      {(sourceNeedsPath || targetNeedsName) && (
        <div className="mt-2 grid grid-cols-1 gap-2 sm:grid-cols-2">
          {sourceNeedsPath && (
            <div>
              <label className="agentdash-form-label">Source path</label>
              <input
                value={runtimeBindingSourcePathInput(rule.source)}
                onChange={(e) =>
                  onChange({
                    ...rule,
                    source: updateRuntimeBindingSourcePath(rule.source, e.target.value),
                  })
                }
                placeholder="p4.client_name"
                disabled={disabled}
                className="agentdash-form-input"
              />
            </div>
          )}
          {targetNeedsName && (
            <div>
              <label className="agentdash-form-label">Target name</label>
              <input
                value={runtimeBindingTargetName(rule.target)}
                onChange={(e) =>
                  onChange({
                    ...rule,
                    target: updateRuntimeBindingTargetName(rule.target, e.target.value),
                  })
                }
                placeholder={runtimeBindingTargetPlaceholder(rule.target)}
                disabled={disabled}
                className="agentdash-form-input"
              />
            </div>
          )}
        </div>
      )}

      <label className="mt-2 flex items-center gap-2 text-[11px] text-muted-foreground">
        <input
          type="checkbox"
          checked={rule.required}
          onChange={(e) => onChange({ ...rule, required: e.target.checked })}
          disabled={disabled}
          className="h-3.5 w-3.5 rounded-[4px] border-border"
        />
        required
      </label>
    </div>
  );
}

function RuntimeBindingBadge({
  runtimeBinding,
}: {
  runtimeBinding: McpRuntimeBindingConfig | null | undefined;
}) {
  const count = mcpRuntimeBindingRuleCount(runtimeBinding);
  if (count === 0) {
    return null;
  }
  return (
    <span className="shrink-0 rounded-[6px] border border-border bg-background px-1.5 py-0.5 text-[10px] font-medium text-muted-foreground">
      运行时绑定 {count}
    </span>
  );
}

function runtimeBindingSourceValue(source: McpRuntimeBindingSource): string {
  const matched = RUNTIME_BINDING_SOURCE_OPTIONS.find((option) =>
    runtimeBindingSourceEquals(option.source, source),
  );
  if (matched) return matched.value;
  if (source.kind === "workspace_identity") return "workspace_identity:custom";
  if (source.kind === "workspace_detected_fact") return "workspace_detected_fact:custom";
  return source.kind;
}

function runtimeBindingSourceEquals(
  left: McpRuntimeBindingSource,
  right: McpRuntimeBindingSource,
): boolean {
  return JSON.stringify(left) === JSON.stringify(right);
}

function createRuntimeBindingSource(
  value: string,
  current: McpRuntimeBindingSource,
): McpRuntimeBindingSource {
  const option = RUNTIME_BINDING_SOURCE_OPTIONS.find((item) => item.value === value);
  if (!option) return current;
  if (
    option.value === "workspace_identity:custom" &&
    current.kind === "workspace_identity"
  ) {
    return current;
  }
  if (
    option.value === "workspace_detected_fact:custom" &&
    current.kind === "workspace_detected_fact"
  ) {
    return current;
  }
  return option.source;
}

function runtimeBindingSourceNeedsPath(
  source: McpRuntimeBindingSource,
): source is Extract<
  McpRuntimeBindingSource,
  { kind: "workspace_identity" | "workspace_detected_fact" }
> {
  return source.kind === "workspace_identity" || source.kind === "workspace_detected_fact";
}

function runtimeBindingSourcePathInput(source: McpRuntimeBindingSource): string {
  if (!runtimeBindingSourceNeedsPath(source)) return "";
  return source.path.join(".");
}

function updateRuntimeBindingSourcePath(
  source: McpRuntimeBindingSource,
  value: string,
): McpRuntimeBindingSource {
  if (!runtimeBindingSourceNeedsPath(source)) return source;
  return {
    ...source,
    path: value.split(".").map((segment) => segment.trim()).filter(Boolean),
  };
}

function createRuntimeBindingTarget(
  value: string,
  current: McpRuntimeBindingTarget,
): McpRuntimeBindingTarget {
  switch (value) {
    case "http_query":
      return {
        kind: "http_query",
        name: current.kind === "http_query" ? current.name : "p4_client",
      };
    case "http_header":
      return {
        kind: "http_header",
        name: current.kind === "http_header" ? current.name : "X-P4-Client",
      };
    case "stdio_env":
      return {
        kind: "stdio_env",
        name: current.kind === "stdio_env" ? current.name : "P4CLIENT",
      };
    case "stdio_cwd":
      return { kind: "stdio_cwd" };
    default:
      return current;
  }
}

function runtimeBindingTargetNeedsName(
  target: McpRuntimeBindingTarget,
): target is Extract<
  McpRuntimeBindingTarget,
  { kind: "http_query" | "http_header" | "stdio_env" }
> {
  return (
    target.kind === "http_query" ||
    target.kind === "http_header" ||
    target.kind === "stdio_env"
  );
}

function runtimeBindingTargetName(target: McpRuntimeBindingTarget): string {
  if (!runtimeBindingTargetNeedsName(target)) return "";
  return target.name;
}

function updateRuntimeBindingTargetName(
  target: McpRuntimeBindingTarget,
  value: string,
): McpRuntimeBindingTarget {
  if (!runtimeBindingTargetNeedsName(target)) return target;
  return { ...target, name: value };
}

function runtimeBindingTargetPlaceholder(target: McpRuntimeBindingTarget): string {
  if (target.kind === "http_query") return "p4_client";
  if (target.kind === "http_header") return "X-P4-Client";
  if (target.kind === "stdio_env") return "P4CLIENT";
  return "";
}

/* ─── Probe 面板（Test Connection + 工具列表）─── */

function ProbePanel({
  probing,
  result,
  transportType,
  onProbe,
}: {
  probing: boolean;
  result: ProbeMcpPresetResponse | null;
  transportType: McpTransportConfig["type"];
  onProbe: () => void;
}) {
  const subtitle = describeMcpProbeTransport(transportType);
  const probeView = buildMcpProbeViewModel(result);

  return (
    <div className="rounded-[8px] border border-dashed border-border bg-secondary/30 px-3 py-2.5">
      <div className="flex items-center justify-between gap-3">
        <div>
          <p className="text-xs font-medium text-foreground">连通性 & 工具发现</p>
          <p className="mt-0.5 text-[10px] text-muted-foreground/70">{subtitle}</p>
        </div>
        <button
          type="button"
          onClick={onProbe}
          disabled={probing}
          className="agentdash-button-secondary shrink-0"
        >
          {probing ? "探测中…" : "Test Connection"}
        </button>
      </div>

      {probeView.detailMessage && (
        <div className="mt-2.5">
          <p className={`text-xs ${probeToneClassName(probeView.detailTone)}`}>
            {probeView.detailMessage}
          </p>
          {probeView.showToolGrid && (
            <div className="mt-1.5 max-h-48 overflow-y-auto">
              <ToolCapsuleGrid tools={probeView.tools} />
            </div>
          )}
        </div>
      )}
    </div>
  );
}

/* ─── Badges ─── */

function RoutePolicyBadge({ policy }: { policy: McpRoutePolicy }) {
  const style =
    policy === "relay"
      ? "border-sky-500/30 bg-sky-500/10 text-sky-700 dark:text-sky-300"
      : policy === "direct"
        ? "border-emerald-500/30 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300"
        : "border-border bg-background text-muted-foreground";
  return (
    <span className={`shrink-0 rounded-[6px] border px-1.5 py-0.5 text-[10px] font-medium ${style}`}>
      {policy}
    </span>
  );
}

/* ─── Utils ─── */
