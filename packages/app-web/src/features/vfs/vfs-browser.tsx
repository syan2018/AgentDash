/**
 * 统一 VFS 浏览器入口
 *
 * 组件负责解析 runtime / preview surface，并把具体文件树与 CodeMirror
 * 编辑体验委托给 VfsBrowserPanel，避免 runtime preview 维护另一套
 * textarea 文件预览/编辑实现。
 */

import { useEffect, useMemo, useState } from "react";
import type { ReactNode } from "react";
import { StatusDot } from "@agentdash/ui";
import type {
  ExecutionVfs,
  ResolvedMountSummary,
  ResolvedVfsSurface,
  ResolvedVfsSurfaceSource,
} from "../../types";
import { resolveVfsSurface } from "../../services/vfs";
import { VfsBrowserPanel } from "./vfs-browser-panel";
import type { VfsBrowserPanelInspectorContext } from "./vfs-browser-panel";

export interface VfsBrowserProps {
  /** 已解析好的 runtime / preview surface（优先使用） */
  surface?: ResolvedVfsSurface | null;
  /** 在组件内部先 resolve surface（Project/Story/Agent Knowledge 预览入口） */
  source?: ResolvedVfsSurfaceSource;
  /** 仅展示 mount 摘要；若未提供 surface/source，则无法进行文件浏览 */
  vfs?: ExecutionVfs | null;
  /** 限制当前入口可见的 mount，适用于 Agent 知识库等专用入口 */
  visibleMountIds?: string[];
  /** 初始选中的 mount id */
  initialMountId?: string;
  /** 初始选中文件 */
  initialFilePath?: string;
  /** 裁切到 mount 内的指定子目录 */
  rootPath?: string;
  protectedFilePaths?: string[];
  renderInspector?: (context: VfsBrowserPanelInspectorContext) => ReactNode;
  browserHeightClassName?: string;
  className?: string;
}

const PROVIDER_LABELS: Record<string, string> = {
  relay_fs: "工作区文件",
  inline_fs: "内联文件",
  lifecycle_vfs: "Lifecycle 记录",
  canvas_fs: "Canvas",
  skill_asset_fs: "Skill 资产",
  external_service: "外部服务",
};

const CAPABILITY_LABELS: Record<string, string> = {
  read: "读",
  write: "写",
  list: "列",
  search: "搜",
  exec: "执行",
};

export function VfsBrowser({
  surface,
  source,
  vfs,
  visibleMountIds,
  initialMountId,
  initialFilePath,
  rootPath,
  protectedFilePaths,
  renderInspector,
  browserHeightClassName = "h-[520px] min-h-[360px] max-h-[70vh]",
  className = "",
}: VfsBrowserProps) {
  const [resolvedSurface, setResolvedSurface] = useState<ResolvedVfsSurface | null>(surface ?? null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setResolvedSurface(surface ?? null);
  }, [surface]);

  useEffect(() => {
    if (surface || !source) return;
    let cancelled = false;
    setLoading(true);
    setError(null);
    void (async () => {
      try {
        const nextSurface = await resolveVfsSurface(source);
        if (!cancelled) setResolvedSurface(nextSurface);
      } catch (err) {
        if (!cancelled) setError(err instanceof Error ? err.message : String(err));
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [surface, source]);

  const visibleSet = useMemo(
    () => (visibleMountIds ? new Set(visibleMountIds) : null),
    [visibleMountIds],
  );

  const filteredSurface = useMemo(
    () => filterSurface(resolvedSurface, visibleSet),
    [resolvedSurface, visibleSet],
  );

  const filteredVfs = useMemo(
    () => filterVfs(vfs ?? null, visibleSet),
    [vfs, visibleSet],
  );

  const mounts = filteredSurface?.mounts ?? filteredVfs?.mounts ?? [];
  const hasBrowsableSurface = Boolean(filteredSurface?.surface_ref);

  if (loading) {
    return (
      <div className="flex items-center justify-center py-8 text-xs text-muted-foreground">
        正在加载 VFS…
      </div>
    );
  }

  if (error) {
    return (
      <div className="rounded-[8px] border border-destructive/20 bg-destructive/5 px-3 py-2 text-xs text-destructive">
        {error}
      </div>
    );
  }

  if (mounts.length === 0) {
    return (
      <div className="rounded-[8px] border border-dashed border-border px-3 py-4 text-center text-xs text-muted-foreground">
        当前配置下没有可用的 Mount。
      </div>
    );
  }

  if (!hasBrowsableSurface) {
    return (
      <div className="space-y-2">
        <MountSummaryList mounts={mounts} defaultMountId={filteredVfs?.default_mount_id ?? null} />
        <div className="rounded-[8px] border border-border bg-secondary/20 px-3 py-2 text-[11px] text-muted-foreground">
          当前入口只提供 mount 摘要，未附带可浏览的 resolved surface。
        </div>
      </div>
    );
  }

  return (
    <div className={`overflow-hidden rounded-[8px] border border-border bg-background ${className}`}>
      <div className="border-b border-border bg-secondary/20 px-3 py-2">
        <MountSummaryList
          mounts={mounts}
          defaultMountId={filteredSurface?.default_mount_id ?? filteredVfs?.default_mount_id ?? null}
          compact
        />
      </div>
      <div className={browserHeightClassName}>
        <VfsBrowserPanel
          surface={filteredSurface}
          vfs={filteredVfs}
          initialMountId={initialMountId}
          initialFilePath={initialFilePath}
          rootPath={rootPath}
          protectedFilePaths={protectedFilePaths}
          renderInspector={renderInspector}
        />
      </div>
    </div>
  );
}

function filterSurface(
  surface: ResolvedVfsSurface | null,
  visibleSet: Set<string> | null,
): ResolvedVfsSurface | null {
  if (!surface || !visibleSet) return surface;
  const mounts = surface.mounts.filter((mount) => visibleSet.has(mount.id));
  return {
    ...surface,
    mounts,
    default_mount_id: normalizeDefaultMount(surface.default_mount_id, mounts),
  };
}

function filterVfs(vfs: ExecutionVfs | null, visibleSet: Set<string> | null): ExecutionVfs | null {
  if (!vfs || !visibleSet) return vfs;
  const mounts = vfs.mounts.filter((mount) => visibleSet.has(mount.id));
  return {
    ...vfs,
    mounts,
    default_mount_id: normalizeDefaultMount(vfs.default_mount_id, mounts),
  };
}

function normalizeDefaultMount<T extends { id: string }>(
  defaultMountId: string | null | undefined,
  mounts: T[],
): string | null {
  if (defaultMountId && mounts.some((mount) => mount.id === defaultMountId)) {
    return defaultMountId;
  }
  return mounts[0]?.id ?? null;
}

function MountSummaryList({
  mounts,
  defaultMountId,
  compact = false,
}: {
  mounts: Array<ResolvedMountSummary | ExecutionVfs["mounts"][number]>;
  defaultMountId: string | null;
  compact?: boolean;
}) {
  return (
    <div className={compact ? "flex flex-wrap gap-1.5" : "space-y-1.5"}>
      {mounts.map((mount) => (
        <MountSummaryItem
          key={mount.id}
          mount={mount}
          isDefault={mount.id === defaultMountId}
          compact={compact}
        />
      ))}
    </div>
  );
}

function MountSummaryItem({
  mount,
  isDefault,
  compact,
}: {
  mount: ResolvedMountSummary | ExecutionVfs["mounts"][number];
  isDefault: boolean;
  compact: boolean;
}) {
  const providerLabel = PROVIDER_LABELS[mount.provider] ?? mount.provider;
  const capabilities = mount.capabilities.map((capability) => CAPABILITY_LABELS[capability] ?? capability);

  if (compact) {
    return (
      <span className="inline-flex min-w-0 items-center gap-1.5 rounded-[6px] border border-border bg-background px-2 py-1 text-[11px] text-muted-foreground">
        <MountStatusDot mount={mount} />
        <span className="max-w-[180px] truncate font-medium text-foreground/85">
          {mount.display_name || mount.id}
        </span>
        <span className="font-mono text-[10px]">{providerLabel}</span>
        {isDefault && <span className="text-[10px] text-primary">默认</span>}
      </span>
    );
  }

  return (
    <div className="rounded-[8px] border border-border bg-secondary/20 px-3 py-2 text-xs">
      <div className="flex flex-wrap items-center gap-2">
        <MountStatusDot mount={mount} />
        <span className="font-medium text-foreground">{mount.display_name || mount.id}</span>
        <span className="rounded-[4px] bg-background px-1.5 py-0.5 font-mono text-[10px] text-muted-foreground">
          {providerLabel}
        </span>
        {isDefault && (
          <span className="rounded-[4px] bg-primary/12 px-1.5 py-0.5 text-[10px] text-primary">
            默认
          </span>
        )}
      </div>
      <div className="mt-1 flex flex-wrap items-center gap-x-3 gap-y-1 text-[10px] text-muted-foreground">
        {"root_ref" in mount && mount.root_ref && (
          <span className="min-w-0 truncate font-mono">{mount.root_ref}</span>
        )}
        <span>{capabilities.join(" / ")}</span>
      </div>
    </div>
  );
}

function MountStatusDot({ mount }: { mount: ResolvedMountSummary | ExecutionVfs["mounts"][number] }) {
  if (mount.provider === "relay_fs" && "backend_online" in mount) {
    if (mount.backend_online === true) {
      return <StatusDot tone="success" title="Backend 在线" />;
    }
    if (mount.backend_online === false) {
      return <StatusDot tone="danger" title="Backend 离线" />;
    }
  }
  if (mount.provider === "inline_fs") {
    return <StatusDot tone="info" title="内联文件" />;
  }
  return <StatusDot tone="muted" />;
}
