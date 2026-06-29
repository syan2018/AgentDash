import { useCallback, useEffect, useMemo, useState } from "react";
import type { ProjectVfsMount, ProjectVfsMountExposureGrant } from "../../../types";
import { listProjectVfsMounts } from "../../../services/projectVfsMounts";
import { useProjectStore } from "../../../stores/projectStore";
import { CapabilityPicker } from "./capability-picker";

const VFS_CAPS = [
  { key: "read", label: "Read" },
  { key: "list", label: "List" },
  { key: "search", label: "Search" },
  { key: "write", label: "Write" },
] as const;

type VfsCap = typeof VFS_CAPS[number]["key"];

export function ProjectVfsMountExposurePicker({
  projectId,
  grants,
  onChange,
}: {
  projectId?: string;
  grants: ProjectVfsMountExposureGrant[];
  onChange: (next: ProjectVfsMountExposureGrant[]) => void;
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
    const map = new Map<string, ProjectVfsMountExposureGrant>();
    for (const grant of grants) map.set(grant.mount_id, grant);
    return map;
  }, [grants]);

  const setGrantCaps = useCallback(
    (mountId: string, capabilities: VfsCap[]) => {
      const next = grants.filter((grant) => grant.mount_id !== mountId);
      if (capabilities.length > 0) next.push({ mount_id: mountId, capabilities });
      onChange(next);
    },
    [grants, onChange],
  );

  const sortedItems = useMemo(
    () => items.slice().sort((a, b) => a.display_name.localeCompare(b.display_name, "zh-CN")),
    [items],
  );

  const selectedKeys = useMemo(
    () => sortedItems.filter((it) => (grantByMountId.get(it.mount_id)?.capabilities.length ?? 0) > 0).map((it) => it.mount_id),
    [sortedItems, grantByMountId],
  );

  const toggleMount = (mountId: string) => {
    const item = sortedItems.find((it) => it.mount_id === mountId);
    if (!item) return;
    const current = grantByMountId.get(mountId)?.capabilities ?? [];
    if (current.length > 0) {
      setGrantCaps(mountId, []);
      return;
    }
    const allowed = VFS_CAPS.filter((cap) => item.capabilities.includes(cap.key)).map((cap) => cap.key);
    setGrantCaps(mountId, allowed);
  };

  if (!projectId) {
    return <p className="text-xs text-muted-foreground/70">保存到项目后即可配置 Project VFS mount exposure。</p>;
  }

  return (
    <CapabilityPicker
      hint="选择该 Agent preset 暴露的 Project Filespace mount，下方按钮可裁剪每个 mount 的 provider capability。"
      isLoading={isLoading}
      error={error}
      items={sortedItems}
      selectedKeys={selectedKeys}
      itemKey={(it) => it.mount_id}
      itemToCardProps={(it) => {
        const selected = grantByMountId.get(it.mount_id)?.capabilities ?? [];
        const allowed = VFS_CAPS.filter((cap) => it.capabilities.includes(cap.key));
        return {
          reactKey: it.mount_id,
          title: it.display_name,
          subtitle: it.mount_id,
          footer: (
            <div className="flex flex-wrap gap-1.5">
              {allowed.length === 0 ? (
                <span className="text-[10px] text-muted-foreground/70">该 mount 未声明任何 cap</span>
              ) : (
                allowed.map((cap) => {
                  const selectedCaps: VfsCap[] = selected;
                  const active = selected.includes(cap.key);
                  return (
                    <button
                      key={cap.key}
                      type="button"
                      onClick={() => {
                        const next = active
                          ? selectedCaps.filter((c) => c !== cap.key)
                          : [...selectedCaps, cap.key];
                        setGrantCaps(it.mount_id, next);
                      }}
                      className={`rounded-[6px] border px-2 py-0.5 text-[10px] font-medium transition-colors ${
                        active
                          ? "border-primary/30 bg-primary/10 text-foreground"
                          : "border-border bg-background text-muted-foreground hover:text-foreground"
                      }`}
                    >
                      {cap.label}
                    </button>
                  );
                })
              )}
            </div>
          ),
        };
      }}
      onToggle={toggleMount}
      loadingText="正在加载 VFS Mount…"
      emptyAllText="当前 Project 尚未创建 Filespace。"
      enabledEmptyText="尚未启用任何 Mount，从下方选取。"
      availableEmptyText="所有 Mount 都已启用。"
    />
  );
}
