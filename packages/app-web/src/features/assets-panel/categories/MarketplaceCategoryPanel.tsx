/**
 * MarketplaceCategoryPanel — 资源市场。
 *
 * 设计要点（见 .trellis/tasks/05-18-marketplace-ux-overhaul/design.md）：
 * - flatten + group：把后端 5 个数组合并成 installSummaryByAssetId（以 library_asset_id 为键）
 * - 卡片右上角同时展示 type chip + install-status chip；不再有底部独立"项目安装来源"区
 * - 类型 segmented control + 前端搜索过滤
 * - 详情抽屉按 asset_type 自适应 manifest 展示
 * - 更新覆写走 ConfirmOverwriteDialog
 */

import { useCallback, useEffect, useMemo, useState } from "react";
import { useSearchParams } from "react-router-dom";
import { useProjectStore } from "../../../stores/projectStore";
import { useCurrentUserStore } from "../../../stores/currentUserStore";
import {
  fetchLibraryAssets,
  fetchProjectAssetSourceStatus,
  installLibraryAsset,
} from "../../../services/sharedLibrary";
import type {
  LibraryAssetDto,
  LibraryAssetType,
  ProjectAssetSourceStatusDto,
  PublishLibraryAssetKind,
  SharedLibrarySourceStatus,
} from "../../../types";
import { Button } from "@agentdash/ui";
import { Notice, type NoticeData } from "../_shared/Notice";
import {
  AssetPickerDrawer,
  type AssetPickerSelection,
} from "../publish/AssetPickerDrawer";
import { PublishLibraryAssetDialog } from "../publish/PublishLibraryAssetDialog";
import {
  ConfirmOverwriteDialog,
  InstallStatusChip,
  MarketplaceAssetDrawer,
  type InstallSummary,
} from "./MarketplaceAssetDrawer";

const ASSET_TYPE_OPTIONS: Array<{ value: LibraryAssetType | "all"; label: string }> = [
  { value: "all", label: "全部" },
  { value: "agent_template", label: "Agent" },
  { value: "mcp_server_template", label: "MCP" },
  { value: "workflow_template", label: "Workflow" },
  { value: "skill_template", label: "Skill" },
  { value: "extension_template", label: "Extension" },
];

const ASSET_TYPE_LABELS: Record<LibraryAssetType, string> = {
  agent_template: "Agent",
  mcp_server_template: "MCP",
  workflow_template: "Workflow",
  skill_template: "Skill",
  extension_template: "Extension",
};

type DrawerState = { kind: "closed" } | { kind: "open"; assetId: string };
type OverwriteState =
  | { kind: "closed" }
  | { kind: "open"; asset: LibraryAssetDto; installedVersion: string };
type ViewMode = "all" | "published";
type PublishFlow =
  | { kind: "closed" }
  | { kind: "picker" }
  | {
      kind: "dialog";
      assetKind: PublishLibraryAssetKind;
      projectAssetId: string;
      defaults: { key: string; display_name: string; description?: string | null };
    };

export function MarketplaceCategoryPanel() {
  const currentProjectId = useProjectStore((s) => s.currentProjectId);
  const currentUserId = useCurrentUserStore((s) => s.currentUser?.user_id ?? null);
  const [searchParams, setSearchParams] = useSearchParams();
  const viewMode: ViewMode = searchParams.get("view") === "published" ? "published" : "all";
  const setViewMode = useCallback(
    (next: ViewMode) => {
      const params = new URLSearchParams(searchParams);
      if (next === "published") params.set("view", "published");
      else params.delete("view");
      setSearchParams(params, { replace: true });
    },
    [searchParams, setSearchParams],
  );

  const [assetType, setAssetType] = useState<LibraryAssetType | "all">("all");
  const [searchTerm, setSearchTerm] = useState("");
  const [assets, setAssets] = useState<LibraryAssetDto[]>([]);
  const [sourceStatus, setSourceStatus] = useState<ProjectAssetSourceStatusDto | null>(null);
  const [loading, setLoading] = useState(false);
  const [busyAssetId, setBusyAssetId] = useState<string | null>(null);
  const [notice, setNotice] = useState<NoticeData | null>(null);
  const [drawer, setDrawer] = useState<DrawerState>({ kind: "closed" });
  const [overwrite, setOverwrite] = useState<OverwriteState>({ kind: "closed" });
  const [publishFlow, setPublishFlow] = useState<PublishFlow>({ kind: "closed" });

  const showSuccess = useCallback((m: string) => setNotice({ tone: "success", message: m }), []);
  const showError = useCallback((m: string) => setNotice({ tone: "danger", message: m }), []);
  const clearNotice = useCallback(() => setNotice(null), []);

  // installSummaryByAssetId: library_asset_id → InstallSummary（status 取最坏）
  const installSummaryByAssetId = useMemo(() => {
    const map = new Map<string, InstallSummary>();
    if (!sourceStatus) return map;
    const allItems = [
      ...sourceStatus.project_agents,
      ...sourceStatus.mcp_presets,
      ...sourceStatus.skill_assets,
      ...sourceStatus.workflow_definitions,
      ...sourceStatus.lifecycle_definitions,
      ...sourceStatus.extension_installations,
    ];
    for (const item of allItems) {
      const key = item.installed_source.library_asset_id;
      const existing = map.get(key);
      const installation = {
        asset_kind: item.asset_kind,
        project_asset_key: item.project_asset_key,
        installed_version: item.installed_source.source_version,
        current_source_version: item.current_source_version ?? null,
        item_status: item.source_status,
      };
      if (!existing) {
        map.set(key, { status: item.source_status, installations: [installation] });
        continue;
      }
      existing.installations.push(installation);
      if (sourceStatusPriority(item.source_status) > sourceStatusPriority(existing.status)) {
        existing.status = item.source_status;
      }
    }
    return map;
  }, [sourceStatus]);

  const visibleAssets = useMemo(() => {
    let pool = assets;
    if (viewMode === "published") {
      pool = pool.filter(
        (a) =>
          a.source === "user_authored" &&
          (currentUserId ? a.owner_id === currentUserId : true),
      );
    }
    const term = searchTerm.trim().toLowerCase();
    if (!term) return pool;
    return pool.filter((a) =>
      a.display_name.toLowerCase().includes(term) ||
      (a.description ?? "").toLowerCase().includes(term) ||
      a.key.toLowerCase().includes(term),
    );
  }, [assets, viewMode, currentUserId, searchTerm]);

  const load = useCallback(async () => {
    if (!currentProjectId) return;
    setLoading(true);
    clearNotice();
    try {
      const [nextAssets, nextStatus] = await Promise.all([
        fetchLibraryAssets({
          asset_type: assetType === "all" ? undefined : assetType,
          include_deprecated: true,
        }),
        fetchProjectAssetSourceStatus(currentProjectId),
      ]);
      setAssets(nextAssets);
      setSourceStatus(nextStatus);
    } catch (err) {
      showError(err instanceof Error ? err.message : "加载公共资源库失败");
    } finally {
      setLoading(false);
    }
  }, [currentProjectId, assetType, clearNotice, showError]);

  useEffect(() => {
    void load();
  }, [load]);

  const performInstall = useCallback(
    async (asset: LibraryAssetDto, doOverwrite: boolean) => {
      if (!currentProjectId) return;
      setBusyAssetId(asset.id);
      clearNotice();
      try {
        await installLibraryAsset(currentProjectId, {
          library_asset_id: asset.id,
          overwrite: doOverwrite,
        });
        showSuccess(doOverwrite ? `已更新 ${asset.display_name}` : `已安装 ${asset.display_name}`);
        setOverwrite({ kind: "closed" });
        setDrawer({ kind: "closed" });
        await load();
      } catch (err) {
        showError(err instanceof Error ? err.message : "安装资源失败");
      } finally {
        setBusyAssetId(null);
      }
    },
    [currentProjectId, load, clearNotice, showSuccess, showError],
  );

  // 卡片/抽屉统一入口：update_available 弹 confirm，首装直连
  const tryInstall = useCallback(
    (asset: LibraryAssetDto, summary: InstallSummary | undefined) => {
      if (summary?.status === "update_available" && summary.installations.length > 0) {
        const installedVersion = summary.installations[0].installed_version;
        setOverwrite({ kind: "open", asset, installedVersion });
        return;
      }
      void performInstall(asset, false);
    },
    [performInstall],
  );

  if (!currentProjectId) {
    return (
      <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
        请选择项目
      </div>
    );
  }

  const drawerAsset =
    drawer.kind === "open" ? assets.find((a) => a.id === drawer.assetId) ?? null : null;

  return (
    <div className="flex h-full flex-col gap-4 p-6">
      <header className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <p className="text-[11px] uppercase tracking-[0.14em] text-muted-foreground">
            Shared Library
          </p>
          <h2 className="text-lg font-semibold text-foreground">资源市场</h2>
        </div>
        <div className="flex items-center gap-2">
          <Button
            variant="secondary"
            size="sm"
            onClick={() => void load()}
            disabled={loading}
          >
            {loading ? "刷新中…" : "刷新"}
          </Button>
          <Button
            variant="primary"
            size="sm"
            onClick={() => setPublishFlow({ kind: "picker" })}
          >
            发布资产 →
          </Button>
        </div>
      </header>

      {/* 视图 segmented：浏览全部 / 我发布的 */}
      <div className="flex items-center gap-1 self-start rounded-[8px] border border-border bg-secondary/20 p-1">
        <ViewModeButton
          active={viewMode === "all"}
          onClick={() => setViewMode("all")}
          label="浏览全部"
        />
        <ViewModeButton
          active={viewMode === "published"}
          onClick={() => setViewMode("published")}
          label="我发布的"
          disabled={!currentUserId}
          title={currentUserId ? undefined : "请先登录后查看自己发布的资产"}
        />
      </div>

      {/* Toolbar：类型 segmented + 搜索 */}
      <div className="flex flex-wrap items-center gap-2">
        <div className="flex items-center gap-1 rounded-[8px] border border-border bg-secondary/20 p-1">
          {ASSET_TYPE_OPTIONS.map((option) => {
            const active = assetType === option.value;
            return (
              <button
                key={option.value}
                type="button"
                onClick={() => setAssetType(option.value)}
                className={`h-7 rounded-[7px] px-2.5 text-xs transition-colors ${
                  active
                    ? "bg-background font-medium text-foreground shadow-sm"
                    : "text-muted-foreground hover:text-foreground"
                }`}
              >
                {option.label}
              </button>
            );
          })}
        </div>
        <input
          type="search"
          value={searchTerm}
          onChange={(e) => setSearchTerm(e.target.value)}
          placeholder="按名称 / 描述 / key 搜索"
          className="agentdash-form-input h-9 max-w-xs flex-1"
        />
      </div>

      <Notice notice={notice} onDismiss={clearNotice} />

      {/* Grid */}
      <section className="grid gap-3 md:grid-cols-2 xl:grid-cols-3">
        {loading ? (
          <div className="col-span-full rounded-[8px] border border-border p-6 text-sm text-muted-foreground">
            正在加载公共资源…
          </div>
        ) : assets.length === 0 && viewMode === "all" ? (
          <EmptyState assetType={assetType} />
        ) : visibleAssets.length === 0 ? (
          <PublishedOrSearchEmpty
            viewMode={viewMode}
            searchTerm={searchTerm}
            onPublish={() => setPublishFlow({ kind: "picker" })}
          />
        ) : (
          visibleAssets.map((asset) => (
            <MarketplaceAssetCard
              key={asset.id}
              asset={asset}
              installSummary={installSummaryByAssetId.get(asset.id)}
              busy={busyAssetId === asset.id}
              onOpenDetail={() => setDrawer({ kind: "open", assetId: asset.id })}
              onInstall={() => tryInstall(asset, installSummaryByAssetId.get(asset.id))}
            />
          ))
        )}
      </section>

      {/* 详情抽屉 */}
      <MarketplaceAssetDrawer
        asset={drawerAsset}
        installSummary={drawerAsset ? installSummaryByAssetId.get(drawerAsset.id) : undefined}
        busy={drawerAsset ? busyAssetId === drawerAsset.id : false}
        onClose={() => setDrawer({ kind: "closed" })}
        onInstall={() => {
          if (!drawerAsset) return;
          tryInstall(drawerAsset, installSummaryByAssetId.get(drawerAsset.id));
        }}
      />

      {/* 覆写确认 */}
      {overwrite.kind === "open" && (
        <ConfirmOverwriteDialog
          asset={overwrite.asset}
          installedVersion={overwrite.installedVersion}
          busy={busyAssetId === overwrite.asset.id}
          onCancel={() => setOverwrite({ kind: "closed" })}
          onConfirm={() => void performInstall(overwrite.asset, true)}
        />
      )}

      {/* 发布主流程：picker → dialog */}
      {publishFlow.kind === "picker" && (
        <AssetPickerDrawer
          projectId={currentProjectId}
          onClose={() => setPublishFlow({ kind: "closed" })}
          onPick={(selection: AssetPickerSelection) => {
            setPublishFlow({
              kind: "dialog",
              assetKind: selection.assetKind,
              projectAssetId: selection.projectAssetId,
              defaults: selection.defaults,
            });
          }}
        />
      )}
      {publishFlow.kind === "dialog" && (
        <PublishLibraryAssetDialog
          projectId={currentProjectId}
          assetKind={publishFlow.assetKind}
          projectAssetId={publishFlow.projectAssetId}
          defaults={publishFlow.defaults}
          currentUserId={currentUserId}
          onClose={() => setPublishFlow({ kind: "closed" })}
          onPublished={(message) => {
            setPublishFlow({ kind: "closed" });
            showSuccess(message);
            setViewMode("published");
            void load();
          }}
        />
      )}
    </div>
  );
}

function ViewModeButton({
  active,
  onClick,
  label,
  disabled,
  title,
}: {
  active: boolean;
  onClick: () => void;
  label: string;
  disabled?: boolean;
  title?: string;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      title={title}
      className={`h-7 rounded-[7px] px-3 text-xs transition-colors ${
        active
          ? "bg-background font-medium text-foreground shadow-sm"
          : "text-muted-foreground hover:text-foreground disabled:opacity-50"
      }`}
    >
      {label}
    </button>
  );
}

function PublishedOrSearchEmpty({
  viewMode,
  searchTerm,
  onPublish,
}: {
  viewMode: ViewMode;
  searchTerm: string;
  onPublish: () => void;
}) {
  if (searchTerm) {
    return (
      <div className="col-span-full rounded-[8px] border border-dashed border-border p-6 text-center text-sm text-muted-foreground">
        没有匹配「{searchTerm}」的资源
      </div>
    );
  }
  if (viewMode === "published") {
    return (
      <div className="col-span-full flex flex-col items-center rounded-[8px] border border-dashed border-border bg-secondary/20 px-6 py-12 text-center">
        <p className="text-sm text-foreground">还没有发布过资产</p>
        <p className="mt-1 text-xs text-muted-foreground">
          可以从项目里挑一个 Agent / MCP / Workflow / Skill 发布到资源市场。
        </p>
        <button type="button" onClick={onPublish} className="agentdash-button-primary mt-4">
          发布资产 →
        </button>
      </div>
    );
  }
  return (
    <div className="col-span-full rounded-[8px] border border-dashed border-border p-6 text-center text-sm text-muted-foreground">
      暂无可见资源
    </div>
  );
}

export default MarketplaceCategoryPanel;

/* ─── EmptyState ─── */

function EmptyState({
  assetType,
}: {
  assetType: LibraryAssetType | "all";
}) {
  const typeLabel = assetType === "all" ? "类目" : `${ASSET_TYPE_LABELS[assetType]} 类目`;
  return (
    <div className="col-span-full flex flex-col items-center rounded-[8px] border border-dashed border-border bg-secondary/20 px-6 py-12 text-center">
      <p className="text-sm text-foreground">当前 {typeLabel} 暂无资源</p>
      <p className="mt-1 text-xs text-muted-foreground">
        系统内置资产会在服务启动时自动同步。若这里为空，请刷新或检查后端启动日志。
      </p>
    </div>
  );
}

/* ─── Card ─── */

function MarketplaceAssetCard({
  asset,
  installSummary,
  busy,
  onOpenDetail,
  onInstall,
}: {
  asset: LibraryAssetDto;
  installSummary?: InstallSummary;
  busy: boolean;
  onOpenDetail: () => void;
  onInstall: () => void;
}) {
  const status = installSummary?.status;
  const isInstalled = status === "up_to_date";
  const hasUpdate = status === "update_available";
  const sourceMissing = status === "source_missing";
  const installDisabled = busy || asset.deprecated || isInstalled || sourceMissing;

  return (
    <article className="flex flex-col rounded-[8px] border border-border bg-background p-4 transition-colors hover:border-primary/25">
      <header className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <p className="text-[11px] uppercase tracking-[0.12em] text-muted-foreground">
            {ASSET_TYPE_LABELS[asset.asset_type]} · {sourceLabel(asset.source)}
          </p>
          <h3 className="mt-1 truncate text-sm font-semibold text-foreground">
            {asset.display_name}
          </h3>
        </div>
        <div className="flex shrink-0 flex-wrap items-center gap-1">
          <InstallStatusChip summary={installSummary} />
          {asset.deprecated && (
            <span className="rounded-[6px] border border-warning/30 bg-warning/10 px-1.5 py-0.5 text-[10px] font-medium text-warning">
              已废弃
            </span>
          )}
        </div>
      </header>
      <p className="mt-2 line-clamp-2 min-h-[2.5rem] text-sm text-muted-foreground">
        {asset.description || asset.key}
      </p>
      <div className="mt-3 flex items-center justify-between gap-2">
        <span className="text-xs text-muted-foreground">v{asset.version}</span>
        <div className="flex gap-1.5">
          <button
            type="button"
            onClick={onOpenDetail}
            className="rounded-[6px] border border-border px-2.5 py-1 text-xs text-foreground/80 transition-colors hover:bg-secondary"
          >
            详情
          </button>
          <button
            type="button"
            onClick={onInstall}
            disabled={installDisabled}
            className="agentdash-button-primary h-7 px-3 text-xs"
            title={
              sourceMissing
                ? "市场来源已废弃或不可用"
                : isInstalled
                  ? "项目已是最新版本"
                  : undefined
            }
          >
            {busy
              ? "处理中…"
              : asset.deprecated
                ? "已废弃"
                : sourceMissing
                  ? "来源缺失"
                  : isInstalled
                    ? "已安装"
                    : hasUpdate
                      ? "更新"
                      : "安装"}
          </button>
        </div>
      </div>
    </article>
  );
}

/* ─── Helpers ─── */

function sourceStatusPriority(status: SharedLibrarySourceStatus): number {
  if (status === "source_missing") return 3;
  if (status === "update_available") return 2;
  return 1;
}

function sourceLabel(source: LibraryAssetDto["source"]): string {
  if (source === "plugin_embedded") return "Plugin";
  if (source === "user_authored") return "User";
  if (source === "remote_imported") return "Remote";
  return "Builtin";
}
