import { useCallback, useEffect, useMemo, useState } from "react";
import type { AgentVfsAccessGrant, ProjectVfsMount } from "../../../types";
import { listProjectVfsMounts } from "../../../services/projectVfsMounts";
import { useProjectStore } from "../../../stores/projectStore";

const VFS_CAPS = [
  { key: "read", label: "Read" },
  { key: "list", label: "List" },
  { key: "search", label: "Search" },
  { key: "write", label: "Write" },
] as const;

type VfsCap = typeof VFS_CAPS[number]["key"];

export function VfsAccessPicker({
  projectId,
  grants,
  onChange,
}: {
  projectId?: string;
  grants: AgentVfsAccessGrant[];
  onChange: (next: AgentVfsAccessGrant[]) => void;
}) {
  const [items, setItems] = useState<ProjectVfsMount[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const mountsRevision = useProjectStore(
    (s) => (projectId ? s.vfsMountsRevision[projectId] ?? 0 : 0),
  );

  const load = useCallback(async () => {
    if (!projectId) return;
    setIsLoading(true);
    setError(null);
    try {
      setItems(await listProjectVfsMounts(projectId));
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setIsLoading(false);
    }
  }, [projectId]);

  useEffect(() => {
    void load();
  }, [load, mountsRevision]);

  const grantByMountId = useMemo(() => {
    const map = new Map<string, AgentVfsAccessGrant>();
    for (const grant of grants) map.set(grant.mount_id, grant);
    return map;
  }, [grants]);

  const setGrantCaps = (mountId: string, capabilities: VfsCap[]) => {
    const next = grants.filter((grant) => grant.mount_id !== mountId);
    if (capabilities.length > 0) {
      next.push({ mount_id: mountId, capabilities });
    }
    onChange(next);
  };

  if (!projectId) {
    return <p className="text-xs text-muted-foreground/70">保存到项目后即可分配 Project VFS。</p>;
  }

  return (
    <div className="space-y-2">
      {isLoading && <p className="text-xs text-muted-foreground/70">正在加载 VFS Mount...</p>}
      {error && <p className="text-xs text-destructive">{error}</p>}
      {!isLoading && items.length === 0 && (
        <p className="text-xs text-muted-foreground/70">当前 Project 尚未创建 Filespace。</p>
      )}
      <div className="space-y-2">
        {items.map((item) => {
          const selected = grantByMountId.get(item.mount_id)?.capabilities ?? [];
          const allowed = VFS_CAPS.filter((cap) => item.capabilities.includes(cap.key));
          return (
            <div key={item.mount_id} className="rounded-[8px] border border-border bg-card/40 p-3">
              <div className="flex items-start justify-between gap-3">
                <div className="min-w-0">
                  <div className="truncate text-sm font-medium text-foreground">{item.display_name}</div>
                  <div className="truncate text-xs text-muted-foreground">{item.mount_id}</div>
                </div>
                <label className="flex shrink-0 items-center gap-1.5 text-xs text-muted-foreground">
                  <input
                    type="checkbox"
                    checked={selected.length > 0}
                    onChange={(event) => {
                      setGrantCaps(
                        item.mount_id,
                        event.currentTarget.checked ? allowed.map((cap) => cap.key) : [],
                      );
                    }}
                  />
                  启用
                </label>
              </div>
              <div className="mt-3 flex flex-wrap gap-1.5">
                {allowed.map((cap) => {
                  const active = selected.includes(cap.key);
                  return (
                    <button
                      key={cap.key}
                      type="button"
                      onClick={() => {
                        const next = active
                          ? selected.filter((item) => item !== cap.key)
                          : [...selected, cap.key];
                        setGrantCaps(item.mount_id, next as VfsCap[]);
                      }}
                      className={`rounded-[8px] border px-2 py-1 text-[11px] font-medium transition-colors ${
                        active
                          ? "border-primary/30 bg-primary/10 text-foreground"
                          : "border-border bg-background text-muted-foreground hover:text-foreground"
                      }`}
                    >
                      {cap.label}
                    </button>
                  );
                })}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
