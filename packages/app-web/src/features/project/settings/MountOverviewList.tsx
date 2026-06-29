import { useEffect, useState } from "react";
import { resolveVfsSurface } from "../../../services/vfs";
import type { ResolvedMountSummary } from "../../../types";

const PROVIDER_LABELS: Record<string, string> = {
  relay_fs: "工作区文件",
  inline_fs: "内联文件",
  lifecycle_vfs: "Lifecycle 记录",
  canvas_fs: "Canvas",
  external_service: "外部服务",
};

const CAPABILITY_LABELS: Record<string, string> = {
  read: "读",
  write: "写",
  list: "列",
  search: "搜",
  exec: "执行",
};

export function MountOverviewList({ projectId }: { projectId: string }) {
  const [mounts, setMounts] = useState<ResolvedMountSummary[]>([]);
  const [defaultMountId, setDefaultMountId] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    void (async () => {
      try {
        const result = await resolveVfsSurface({
          source_type: "project_preview",
          project_id: projectId,
        });
        if (cancelled) return;
        setMounts(result.mounts);
        setDefaultMountId(result.default_mount_id ?? null);
      } catch (err) {
        if (!cancelled) setError(err instanceof Error ? err.message : String(err));
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();
    return () => { cancelled = true; };
  }, [projectId]);

  if (loading) {
    return (
      <p className="py-6 text-center text-xs text-muted-foreground">
        正在加载 Mount 概览…
      </p>
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
      <p className="rounded-[8px] border border-dashed border-border px-4 py-4 text-center text-sm text-muted-foreground">
        当前配置下没有可用的 VFS Mount。请先配置工作空间或项目级 VFS Mount。
      </p>
    );
  }

  return (
    <div className="space-y-2">
      {mounts.map((mount) => {
        const isDefault = mount.id === defaultMountId;
        const providerLabel = PROVIDER_LABELS[mount.provider] ?? mount.provider;
        const online = mount.backend_online;

        return (
          <div
            key={mount.id}
            className={`rounded-[12px] border px-4 py-3 ${
              isDefault
                ? "border-primary/25 bg-primary/[0.03]"
                : "border-border bg-background"
            }`}
          >
            <div className="flex items-start justify-between gap-3">
              <div className="min-w-0 flex-1">
                <div className="flex flex-wrap items-center gap-2">
                  {/* 状态指示点 */}
                  {mount.provider === "relay_fs" ? (
                    <span
                      className={`inline-block h-2 w-2 shrink-0 rounded-full ${
                        online === true
                          ? "bg-success"
                          : online === false
                            ? "bg-destructive"
                            : "bg-muted-foreground/30"
                      }`}
                      title={online === true ? "Backend 在线" : online === false ? "Backend 离线" : "状态未知"}
                    />
                  ) : (
                    // eslint-disable-next-line no-restricted-syntax -- 状态指示圆点
                    <span className="inline-block h-2 w-2 shrink-0 rounded-full bg-info" />
                  )}

                  <p className="truncate text-sm font-medium text-foreground">
                    {mount.display_name}
                  </p>

                  {isDefault && (
                    <span className="inline-flex items-center rounded-[8px] border border-primary/25 bg-primary/10 px-2 py-0.5 text-[10px] font-medium text-primary">
                      默认
                    </span>
                  )}
                  {mount.default_write && (
                    <span className="inline-flex items-center rounded-[8px] border border-warning/25 bg-warning/10 px-2 py-0.5 text-[10px] font-medium text-warning">
                      默认写入
                    </span>
                  )}
                  <span className="rounded-[8px] border border-border bg-muted/40 px-2 py-0.5 text-[10px] text-muted-foreground">
                    {providerLabel}
                  </span>
                </div>

                <p className="mt-1 truncate font-mono text-xs text-muted-foreground">
                  {mount.id}
                </p>
              </div>

              {/* 能力标签 */}
              <div className="flex shrink-0 flex-wrap justify-end gap-1">
                {mount.capabilities.map((cap) => (
                  <span
                    key={cap}
                    className="rounded-[8px] border border-border bg-background px-2 py-0.5 text-[10px] text-muted-foreground"
                  >
                    {CAPABILITY_LABELS[cap] ?? cap}
                  </span>
                ))}
              </div>
            </div>

            {mount.file_count != null && (
              <p className="mt-1 text-[10px] text-muted-foreground">
                {mount.file_count} 个文件
              </p>
            )}
          </div>
        );
      })}
    </div>
  );
}
