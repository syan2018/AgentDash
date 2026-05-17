import { useEffect, useMemo, useState } from "react";
import { useProjectStore } from "../../../stores/projectStore";
import {
  fetchLibraryAssets,
  fetchProjectAssetSourceStatus,
  installLibraryAsset,
  seedBuiltinLibraryAssets,
} from "../../../services/sharedLibrary";
import type {
  LibraryAssetDto,
  LibraryAssetType,
  ProjectAssetSourceStatusDto,
} from "../../../types";

const ASSET_TYPE_OPTIONS: Array<{ value: LibraryAssetType | "all"; label: string }> = [
  { value: "all", label: "全部" },
  { value: "agent_template", label: "Agent Template" },
  { value: "mcp_server_template", label: "MCP Server Template" },
  { value: "workflow_template", label: "Workflow Template" },
  { value: "skill_template", label: "Skill Template" },
];

const ASSET_TYPE_LABELS: Record<LibraryAssetType, string> = {
  agent_template: "Agent Template",
  mcp_server_template: "MCP Server Template",
  workflow_template: "Workflow Template",
  skill_template: "Skill Template",
};

export function MarketplaceCategoryPanel() {
  const currentProjectId = useProjectStore((state) => state.currentProjectId);
  const [assetType, setAssetType] = useState<LibraryAssetType | "all">("all");
  const [assets, setAssets] = useState<LibraryAssetDto[]>([]);
  const [sourceStatus, setSourceStatus] = useState<ProjectAssetSourceStatusDto | null>(null);
  const [loading, setLoading] = useState(false);
  const [busyAssetId, setBusyAssetId] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const statusItems = useMemo(
    () => [...(sourceStatus?.mcp_presets ?? []), ...(sourceStatus?.skill_assets ?? [])],
    [sourceStatus],
  );

  const load = async () => {
    if (!currentProjectId) return;
    setLoading(true);
    setError(null);
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
      setError(err instanceof Error ? err.message : "加载公共资源库失败");
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    void load();
    // load 依赖 UI 状态，直接展开依赖避免 useCallback 带来的额外噪音。
  }, [currentProjectId, assetType]);

  const seedBuiltins = async () => {
    setBusyAssetId("__seed__");
    setError(null);
    setMessage(null);
    try {
      const seeded = await seedBuiltinLibraryAssets(
        assetType === "all" ? {} : { asset_type: assetType },
      );
      setMessage(`已同步 ${seeded.length} 个内置资源`);
      await load();
    } catch (err) {
      setError(err instanceof Error ? err.message : "同步内置资源失败");
    } finally {
      setBusyAssetId(null);
    }
  };

  const install = async (asset: LibraryAssetDto) => {
    if (!currentProjectId) return;
    setBusyAssetId(asset.id);
    setError(null);
    setMessage(null);
    try {
      await installLibraryAsset(currentProjectId, {
        library_asset_id: asset.id,
        overwrite: true,
      });
      setMessage(`已安装 ${asset.display_name}`);
      await load();
    } catch (err) {
      setError(err instanceof Error ? err.message : "安装资源失败");
    } finally {
      setBusyAssetId(null);
    }
  };

  if (!currentProjectId) {
    return (
      <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
        请选择项目
      </div>
    );
  }

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
          <select
            value={assetType}
            onChange={(event) => setAssetType(event.target.value as LibraryAssetType | "all")}
            className="h-9 rounded-[8px] border border-border bg-background px-3 text-sm text-foreground"
          >
            {ASSET_TYPE_OPTIONS.map((option) => (
              <option key={option.value} value={option.value}>
                {option.label}
              </option>
            ))}
          </select>
          <button
            type="button"
            onClick={() => void seedBuiltins()}
            disabled={busyAssetId === "__seed__"}
            className="h-9 rounded-[8px] border border-border bg-secondary px-3 text-sm font-medium text-foreground hover:bg-secondary/80 disabled:opacity-60"
          >
            {busyAssetId === "__seed__" ? "同步中..." : "同步内置资源"}
          </button>
          <button
            type="button"
            onClick={() => void load()}
            className="h-9 rounded-[8px] border border-border bg-background px-3 text-sm text-foreground hover:bg-secondary/60"
          >
            刷新
          </button>
        </div>
      </header>

      {error && (
        <div className="rounded-[8px] border border-destructive/30 bg-destructive/5 px-3 py-2 text-sm text-destructive">
          {error}
        </div>
      )}
      {message && (
        <div className="rounded-[8px] border border-emerald-500/30 bg-emerald-500/5 px-3 py-2 text-sm text-emerald-700 dark:text-emerald-300">
          {message}
        </div>
      )}

      <section className="grid gap-3 md:grid-cols-2 xl:grid-cols-3">
        {loading ? (
          <div className="col-span-full rounded-[8px] border border-border p-6 text-sm text-muted-foreground">
            正在加载公共资源...
          </div>
        ) : assets.length === 0 ? (
          <div className="col-span-full rounded-[8px] border border-border p-6 text-sm text-muted-foreground">
            暂无资源
          </div>
        ) : (
          assets.map((asset) => (
            <LibraryAssetCard
              key={asset.id}
              asset={asset}
              busy={busyAssetId === asset.id}
              onInstall={() => void install(asset)}
            />
          ))
        )}
      </section>

      <section className="border-t border-border pt-4">
        <div className="mb-3 flex items-center justify-between">
          <h3 className="text-sm font-semibold text-foreground">项目安装来源</h3>
          <span className="text-xs text-muted-foreground">{statusItems.length} 个已追踪资源</span>
        </div>
        <div className="grid gap-2 md:grid-cols-2">
          {statusItems.length === 0 ? (
            <div className="rounded-[8px] border border-border p-4 text-sm text-muted-foreground">
              当前项目还没有来自资源市场的 MCP Preset 或 Skill Asset
            </div>
          ) : (
            statusItems.map((item) => (
              <div key={`${item.asset_kind}:${item.project_asset_id}`} className="rounded-[8px] border border-border p-3">
                <div className="flex items-center justify-between gap-2">
                  <p className="text-sm font-medium text-foreground">{item.project_asset_key}</p>
                  <SourceStatusBadge status={item.source_status} />
                </div>
                <p className="mt-1 text-xs text-muted-foreground">
                  {item.asset_kind} · {item.installed_source.source_version}
                  {item.current_source_version ? ` → ${item.current_source_version}` : ""}
                </p>
              </div>
            ))
          )}
        </div>
      </section>
    </div>
  );
}

function LibraryAssetCard({
  asset,
  busy,
  onInstall,
}: {
  asset: LibraryAssetDto;
  busy: boolean;
  onInstall: () => void;
}) {
  return (
    <article className="rounded-[8px] border border-border bg-background p-4">
      <div className="flex items-start justify-between gap-3">
        <div>
          <p className="text-[11px] uppercase tracking-[0.12em] text-muted-foreground">
            {ASSET_TYPE_LABELS[asset.asset_type]}
          </p>
          <h3 className="mt-1 text-sm font-semibold text-foreground">{asset.display_name}</h3>
        </div>
        <span className="rounded-[6px] border border-border px-2 py-1 text-[11px] text-muted-foreground">
          {asset.scope}
        </span>
      </div>
      <p className="mt-2 line-clamp-2 min-h-[2.5rem] text-sm text-muted-foreground">
        {asset.description || asset.key}
      </p>
      <div className="mt-3 flex items-center justify-between gap-2">
        <span className="text-xs text-muted-foreground">v{asset.version}</span>
        <button
          type="button"
          onClick={onInstall}
          disabled={busy || asset.deprecated}
          className="h-8 rounded-[8px] bg-primary px-3 text-xs font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-60"
        >
          {busy ? "安装中..." : asset.deprecated ? "已废弃" : "安装到项目"}
        </button>
      </div>
    </article>
  );
}

function SourceStatusBadge({ status }: { status: string }) {
  const label =
    status === "update_available" ? "有新版" : status === "source_missing" ? "来源不可用" : "已是最新";
  return (
    <span className="rounded-[6px] border border-border px-2 py-1 text-[11px] text-muted-foreground">
      {label}
    </span>
  );
}

export default MarketplaceCategoryPanel;
