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

import { useCallback, useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";

import { useProjectStore } from "../../../stores/projectStore";
import { useWorkflowStore } from "../../../stores/workflowStore";
import { useCurrentUserStore } from "../../../stores/currentUserStore";
import type {
  ActivityLifecycleDefinition,
  LibraryAssetDto,
  WorkflowDefinitionSource,
} from "../../../types";
import { formatTargetKinds } from "../../workflow/shared-labels";
import {
  AssetCard,
  CardMenu,
  CreateButton,
  DangerConfirmDialog,
  DismissibleNotice,
  type DismissibleNoticeData,
  MetaTagList,
  OriginBadge,
} from "@agentdash/ui";
import { buildAssetMenuItems } from "../_shared/assetMenu";
import { resolveOriginBadge } from "../_shared/origin-badge-tone";
import { PublishedBadge } from "../_shared/PublishedBadge";
import { SelectProjectEmpty } from "../_shared/SelectProjectEmpty";
import { useLibraryPublishedAssets } from "../_shared/useLibraryPublishedAssets";
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

  const [notice, setNotice] = useState<DismissibleNoticeData | null>(null);
  const [busyKey, setBusyKey] = useState<string | null>(null);
  const [confirmDelete, setConfirmDelete] = useState<DeleteTarget | null>(null);
  const [publishTarget, setPublishTarget] = useState<ActivityLifecycleDefinition | null>(null);
  const { publishedByKey, reloadPublished } = useLibraryPublishedAssets("workflow_template");

  useEffect(() => {
    if (!currentProjectId) return;
    void fetchDefinitions({ projectId: currentProjectId });
    void fetchLifecycles({ projectId: currentProjectId });
  }, [currentProjectId, fetchDefinitions, fetchLifecycles]);

  const handleDelete = useCallback(async () => {
    if (!confirmDelete) return;
    setBusyKey(`delete:${confirmDelete.id}`);
    const ok = await removeLifecycle(confirmDelete.id);
    if (ok) setNotice({ tone: "success", message: `已删除：${confirmDelete.name}` });
    setConfirmDelete(null);
    setBusyKey(null);
  }, [confirmDelete, removeLifecycle]);

  if (!currentProjectId) {
    return <SelectProjectEmpty assetLabel="Workflow 资产" />;
  }

  return (
    <div className="flex h-full flex-col gap-4 p-6">
      <header className="flex flex-wrap items-center justify-between gap-3">
        <div className="space-y-1">
          <h2 className="text-base font-semibold tracking-tight text-foreground">Workflow 资产</h2>
          <p className="text-xs text-muted-foreground">
            {lifecycles.length} 个 Workflow
          </p>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <CreateButton entity="Workflow" onClick={() => navigate("/workflow/new")} />
        </div>
      </header>

      <DismissibleNotice notice={notice} onDismiss={() => setNotice(null)} />
      {error && (
        <DismissibleNotice
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
      <DangerConfirmDialog
        open={confirmDelete != null}
        title="确认删除"
        description={
          confirmDelete
            ? `确定要删除 Workflow ${confirmDelete.name} 吗？此操作不可撤销。`
            : ""
        }
        confirmLabel={busyKey != null ? "删除中…" : "删除"}
        onClose={() => setConfirmDelete(null)}
        onConfirm={() => void handleDelete()}
        isConfirming={busyKey != null}
      />

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
  items: ActivityLifecycleDefinition[];
  publishedByKey: Map<string, LibraryAssetDto>;
  onEdit: (lc: ActivityLifecycleDefinition) => void;
  onPublish: (lc: ActivityLifecycleDefinition) => void;
  onDelete: (lc: ActivityLifecycleDefinition) => void;
  busyKey: string | null;
}) {
  if (items.length === 0) {
    return (
      <div className="rounded-[12px] border border-dashed border-border bg-secondary/20 px-6 py-10 text-center">
        <p className="text-sm text-foreground">暂无 Workflow 资产</p>
        <p className="mt-1 text-xs text-muted-foreground">
          可从资源市场安装公共模板，或点击右上角"+ Workflow"新建。
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
  item: ActivityLifecycleDefinition;
  published: LibraryAssetDto | null;
  onEdit: (lc: ActivityLifecycleDefinition) => void;
  onPublish: (lc: ActivityLifecycleDefinition) => void;
  onDelete: (lc: ActivityLifecycleDefinition) => void;
  isDeleting: boolean;
}) {
  const stepCount = item.activities.length;
  const edgeCount = (item.transitions ?? []).length;
  const isInstalled = Boolean(item.installed_source);
  const isBuiltin = item.source === "builtin_seed";
  // 已经从市场安装回来的资产或 builtin 不允许走"发布"路径，避免循环发布
  const canPublish = !isInstalled && !isBuiltin;
  const sourceOrigin = resolveOriginBadge(item.source, isInstalled);

  const menuItems = buildAssetMenuItems({
    primary: { label: isBuiltin ? "查看" : "编辑", onSelect: () => onEdit(item) },
    publish: canPublish
      ? { published: Boolean(published), onSelect: () => onPublish(item) }
      : null,
    danger: {
      label: "删除",
      busy: isDeleting,
      busyLabel: "删除中…",
      onSelect: () => onDelete(item),
    },
  });

  return (
    <AssetCard
      onOpen={() => onEdit(item)}
      openTitle={isBuiltin ? "查看" : "编辑"}
      title={item.name}
      subtitle={item.key}
      description={item.description}
      headerRight={
        <>
          {published && <PublishedBadge version={published.version} />}
          <OriginBadge tone={sourceOrigin.tone} label={sourceOrigin.label} />
          <CardMenu items={menuItems} />
        </>
      }
      footer={<>更新于 {formatDateTime(item.updated_at)}</>}
    >
      <MetaTagList
        items={[
          { key: "activity", label: `${stepCount} activity` },
          { key: "transition", label: `${edgeCount} transition` },
          { key: "target", label: `target: ${formatTargetKinds(item.target_kinds)}` },
        ]}
      />
    </AssetCard>
  );
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
