/**
 * WorkflowCategoryPanel — Assets 页 Workflow 类目。
 *
 * 职责：
 * - 从 `useWorkflowStore` 拉取 Lifecycle 定义（= Workflow 资产）
 * - 每行展示：name、key、description、来源 chip、更新时间、step/edge 计数
 * - 只读预览：用 step/edge 计数文字代替 DAG 缩略（避免重造渲染器）
 * - 行动作：
 *   - `编辑` / `查看` → `navigate("/workflow/:id")`（统一编辑器，按 step 规模自适应 Form / DAG）
 *   - 删除：走 removeLifecycle；Marketplace 安装包的级联清理由后端负责。
 */

import { useCallback, useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";

import { useProjectStore } from "../../../stores/projectStore";
import { useWorkflowStore } from "../../../stores/workflowStore";
import { useCurrentUserStore } from "../../../stores/currentUserStore";
import { fetchLibraryAssets } from "../../../services/sharedLibrary";
import type {
  LibraryAssetDto,
  LifecycleDefinition,
  WorkflowDefinitionSource,
} from "../../../types";
import { formatTargetKinds } from "../../workflow/shared-labels";
import { Notice, type NoticeData } from "../_shared/Notice";
import { CardMenu } from "../_shared/CardMenu";
import { SourceBadge, type AssetSourceVariant } from "../_shared/SourceBadge";
import { PublishedBadge } from "../_shared/PublishedBadge";
import { PublishLibraryAssetDialog } from "../publish/PublishLibraryAssetDialog";

type DeleteTarget = { id: string; name: string; source: WorkflowDefinitionSource };

export function WorkflowCategoryPanel() {
  const navigate = useNavigate();
  const currentProjectId = useProjectStore((s) => s.currentProjectId);

  const lifecycles = useWorkflowStore((s) => s.lifecycleDefinitions);
  const error = useWorkflowStore((s) => s.error);

  const fetchDefinitions = useWorkflowStore((s) => s.fetchDefinitions);
  const fetchLifecycles = useWorkflowStore((s) => s.fetchLifecycles);
  const removeLifecycle = useWorkflowStore((s) => s.removeLifecycle);

  const currentUserId = useCurrentUserStore((s) => s.currentUser?.user_id ?? null);

  const [notice, setNotice] = useState<NoticeData | null>(null);
  const [busyKey, setBusyKey] = useState<string | null>(null);
  const [confirmDelete, setConfirmDelete] = useState<DeleteTarget | null>(null);
  const [publishTarget, setPublishTarget] = useState<LifecycleDefinition | null>(null);
  // 当前用户已发布的 workflow 模板列表，用于在卡片上展示"已发布"徽章
  const [publishedAssets, setPublishedAssets] = useState<LibraryAssetDto[]>([]);
  const [publishedReloadTick, setPublishedReloadTick] = useState(0);

  useEffect(() => {
    if (!currentProjectId) return;
    void fetchDefinitions({ projectId: currentProjectId });
    void fetchLifecycles({ projectId: currentProjectId });
  }, [currentProjectId, fetchDefinitions, fetchLifecycles]);

  useEffect(() => {
    if (!currentUserId) return;
    let cancelled = false;
    fetchLibraryAssets({ asset_type: "workflow_template", owner_id: currentUserId })
      .then((list) => {
        if (!cancelled) setPublishedAssets(list);
      })
      .catch(() => {
        if (!cancelled) setPublishedAssets([]);
      });
    return () => {
      cancelled = true;
    };
  }, [currentUserId, publishedReloadTick]);

  const publishedByKey = useMemo(() => {
    if (!currentUserId) return new Map<string, LibraryAssetDto>();
    const map = new Map<string, LibraryAssetDto>();
    for (const a of publishedAssets) {
      if (a.source === "user_authored") map.set(a.key, a);
    }
    return map;
  }, [publishedAssets, currentUserId]);

  const reloadPublished = useCallback(() => {
    setPublishedReloadTick((tick) => tick + 1);
  }, []);

  const handleDelete = useCallback(async () => {
    if (!confirmDelete) return;
    setBusyKey(`delete:${confirmDelete.id}`);
    const ok = await removeLifecycle(confirmDelete.id);
    if (ok) setNotice({ tone: "success", message: `已删除：${confirmDelete.name}` });
    setConfirmDelete(null);
    setBusyKey(null);
  }, [confirmDelete, removeLifecycle]);

  if (!currentProjectId) {
    return (
      <div className="flex h-full items-center justify-center p-6">
        <div className="text-center text-sm text-muted-foreground">
          请选择项目后查看 Workflow 资产
        </div>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col gap-4 p-6">
      <header className="flex flex-wrap items-center justify-between gap-3">
        <div className="space-y-1">
          <h2 className="text-base font-semibold tracking-tight text-foreground">Workflow 资产</h2>
          <p className="text-xs text-muted-foreground">
            {lifecycles.length} 个 Workflow 资产 · 支持 Marketplace 安装来源追踪
          </p>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <button
            type="button"
            onClick={() => navigate("/workflow/new")}
            className="h-9 rounded-[10px] border border-primary bg-primary px-3.5 text-sm text-primary-foreground transition-colors hover:opacity-95"
          >
            + Workflow
          </button>
        </div>
      </header>

      {/* 反馈消息 */}
      <Notice notice={notice} onDismiss={() => setNotice(null)} />
      {error && (
        <Notice
          notice={{ tone: "danger", message: error }}
          onDismiss={() => {
            /* store 错误清理由 store 自身负责，这里不做 */
          }}
          autoHideMs={0}
        />
      )}

      {/* 统一列表 */}
      <LifecycleAssetGrid
        items={lifecycles}
        publishedByKey={publishedByKey}
        onEdit={(lc) => navigate(`/workflow/${lc.id}`)}
        onPublish={setPublishTarget}
        onDelete={(lc) =>
          setConfirmDelete({ id: lc.id, name: lc.name, source: lc.source })
        }
        busyKey={busyKey}
      />

      {/* 删除确认 */}
      {confirmDelete && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/40"
          onClick={() => setConfirmDelete(null)}
        >
          <div
            className="w-[380px] rounded-[14px] border border-border bg-background p-5 shadow-xl"
            onClick={(e) => e.stopPropagation()}
          >
            <h3 className="text-sm font-semibold text-foreground">确认删除</h3>
            <p className="mt-2 text-xs leading-5 text-muted-foreground">
              确定要删除 Workflow{" "}
              <span className="font-medium text-foreground">{confirmDelete.name}</span> 吗？
              <span className="mt-1 block">此操作不可撤销。</span>
            </p>
            <div className="mt-4 flex justify-end gap-2">
              <button
                type="button"
                onClick={() => setConfirmDelete(null)}
                className="rounded-[8px] border border-border px-3 py-1.5 text-xs text-muted-foreground transition-colors hover:bg-secondary"
              >
                取消
              </button>
              <button
                type="button"
                onClick={() => void handleDelete()}
                disabled={busyKey != null}
                className="rounded-[8px] border border-destructive/30 bg-destructive px-3 py-1.5 text-xs text-destructive-foreground transition-colors hover:opacity-90 disabled:opacity-50"
              >
                {busyKey != null ? "删除中…" : "删除"}
              </button>
            </div>
          </div>
        </div>
      )}

      {publishTarget && (
        <PublishLibraryAssetDialog
          projectId={currentProjectId}
          assetKind="workflow_bundle"
          projectAssetId={publishTarget.id}
          defaults={{
            key: publishTarget.key,
            display_name: publishTarget.name,
            description: publishTarget.description,
          }}
          currentUserId={currentUserId}
          onClose={() => setPublishTarget(null)}
          onPublished={(message) => {
            setNotice({ tone: "success", message });
            void fetchDefinitions({ projectId: currentProjectId });
            void fetchLifecycles({ projectId: currentProjectId });
            reloadPublished();
          }}
        />
      )}
    </div>
  );
}

export default WorkflowCategoryPanel;

/* ─── 资产列表：Lifecycle ─── */

function LifecycleAssetGrid({
  items,
  publishedByKey,
  onEdit,
  onPublish,
  onDelete,
  busyKey,
}: {
  items: LifecycleDefinition[];
  publishedByKey: Map<string, LibraryAssetDto>;
  onEdit: (lc: LifecycleDefinition) => void;
  onPublish: (lc: LifecycleDefinition) => void;
  onDelete: (lc: LifecycleDefinition) => void;
  busyKey: string | null;
}) {
  if (items.length === 0) {
    return (
      <div className="rounded-[12px] border border-dashed border-border bg-secondary/20 px-6 py-10 text-center">
        <p className="text-sm text-foreground">暂无 Lifecycle 定义</p>
        <p className="mt-1 text-xs text-muted-foreground">
          可从资源市场安装公共模板，或"+ Lifecycle"新建用户定义。
        </p>
      </div>
    );
  }

  const sorted = items.slice().sort((a, b) => a.name.localeCompare(b.name, "zh-CN"));

  return (
    <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
      {sorted.map((lc) => (
        <LifecycleAssetCard
          key={lc.id}
          item={lc}
          published={publishedByKey.get(lc.key) ?? null}
          onEdit={onEdit}
          onPublish={onPublish}
          onDelete={onDelete}
          isDeleting={busyKey === `delete:${lc.id}`}
        />
      ))}
    </div>
  );
}

function LifecycleAssetCard({
  item,
  published,
  onEdit,
  onPublish,
  onDelete,
  isDeleting,
}: {
  item: LifecycleDefinition;
  published: LibraryAssetDto | null;
  onEdit: (lc: LifecycleDefinition) => void;
  onPublish: (lc: LifecycleDefinition) => void;
  onDelete: (lc: LifecycleDefinition) => void;
  isDeleting: boolean;
}) {
  const stepCount = item.steps.length;
  const edgeCount = (item.edges ?? []).length;
  const isInstalled = Boolean(item.installed_source);
  const isBuiltin = item.source === "builtin_seed";
  // 已经从市场安装回来的资产或 builtin 不允许走"发布"路径，避免循环发布
  const canPublish = !isInstalled && !isBuiltin;
  const sourceVariant = workflowSourceVariant(item.source, isInstalled);

  const menuItems = [
    {
      key: "edit",
      label: isBuiltin ? "查看" : "编辑",
      onSelect: () => onEdit(item),
    },
    ...(canPublish
      ? [
          {
            key: "publish",
            label: published ? "更新发布" : "发布到资源市场",
            onSelect: () => onPublish(item),
          },
        ]
      : []),
    { key: "---", label: "", onSelect: () => {} },
    {
      key: "delete",
      label: isDeleting ? "删除中…" : "删除",
      danger: true,
      onSelect: () => onDelete(item),
    },
  ];

  return (
    <article
      role="button"
      tabIndex={0}
      onClick={() => onEdit(item)}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onEdit(item);
        }
      }}
      title={isBuiltin ? "查看" : "编辑"}
      className="flex cursor-pointer flex-col rounded-[12px] border border-border bg-background p-3.5 text-left transition-colors hover:border-primary/25 hover:bg-secondary/30 focus:outline-none focus-visible:ring-2 focus-visible:ring-primary/40"
    >
      <header className="flex items-start justify-between gap-2">
        <div className="min-w-0">
          <p className="truncate text-sm font-medium leading-6 text-foreground">{item.name}</p>
          <p className="mt-0.5 truncate text-xs text-muted-foreground">{item.key}</p>
        </div>
        <div className="flex shrink-0 items-center gap-1">
          {published && <PublishedBadge version={published.version} />}
          <SourceBadge variant={sourceVariant} />
          <CardMenu items={menuItems} />
        </div>
      </header>

      {item.description && (
        <p className="mt-1.5 line-clamp-2 text-xs leading-5 text-muted-foreground">
          {item.description}
        </p>
      )}

      <div className="mt-3 flex flex-wrap gap-1.5 text-[11px] text-muted-foreground">
        <span className="rounded-[6px] border border-border bg-secondary/40 px-1.5 py-0.5">
          {stepCount} step
        </span>
        <span className="rounded-[6px] border border-border bg-secondary/40 px-1.5 py-0.5">
          {edgeCount} edge
        </span>
        <span className="rounded-[6px] border border-border bg-secondary/40 px-1.5 py-0.5">
          target: {formatTargetKinds(item.target_kinds)}
        </span>
      </div>

      <footer className="mt-3 border-t border-border/70 pt-2.5 text-[11px] text-muted-foreground">
        更新于 {formatDateTime(item.updated_at)}
      </footer>
    </article>
  );
}

function workflowSourceVariant(source: WorkflowDefinitionSource, installed: boolean): AssetSourceVariant {
  if (installed) return "marketplace";
  if (source === "builtin_seed") return "builtin";
  if (source === "cloned") return "cloned";
  return "user";
}

/* ─── 公共：时间格式化 ─── */

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
