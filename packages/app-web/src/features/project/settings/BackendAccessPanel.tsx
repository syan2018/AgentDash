import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type {
  BackendWorkspaceInventory,
  ProjectBackendAccess,
} from "../../../types";
import { useCoordinatorStore } from "../../../stores/coordinatorStore";
import {
  classifyMachine,
  machineKindLabel,
  type MachineKind,
} from "../../workspace/model/machinePresentation";
import { backendStatusSignature } from "../../../utils/backendStatusSignature";
import {
  createProjectBackendAccess,
  listBackendWorkspaceInventory,
  listProjectBackendAccess,
  revokeProjectBackendAccess,
} from "../../../services/backendAccess";
import { ContentGroup } from "./settings-ui";

const ACCESS_STATUS_LABELS: Record<ProjectBackendAccess["status"], string> = {
  active: "已启用",
  paused: "已暂停",
  revoked: "已撤销",
};

const INVENTORY_STATUS_LABELS: Record<BackendWorkspaceInventory["status"], string> = {
  available: "可用",
  stale: "过期",
  offline: "离线",
  error: "异常",
};

const MACHINE_KIND_TONE: Record<MachineKind, string> = {
  local_device: "border-primary/30 bg-primary/10 text-primary",
  server_runner: "border-info/30 bg-info/10 text-info",
  other: "border-border bg-secondary/40 text-muted-foreground",
};

function MachineKindBadge({ kind }: { kind: MachineKind }) {
  return (
    <span className={`rounded-[8px] border px-2 py-0.5 text-[10px] font-medium ${MACHINE_KIND_TONE[kind]}`}>
      {machineKindLabel(kind)}
    </span>
  );
}

export function BackendAccessPanel({
  projectId,
  canEdit,
  inventoryRefreshKey = 0,
}: {
  projectId: string;
  canEdit: boolean;
  inventoryRefreshKey?: number;
}) {
  const backends = useCoordinatorStore((state) => state.backends);
  const fetchBackends = useCoordinatorStore((state) => state.fetchBackends);
  const [accesses, setAccesses] = useState<ProjectBackendAccess[]>([]);
  const [inventoriesByAccessId, setInventoriesByAccessId] = useState<Record<string, BackendWorkspaceInventory[]>>({});
  const [expandedInventoryAccessIds, setExpandedInventoryAccessIds] = useState<Record<string, boolean>>({});
  const [loadingInventoryAccessIds, setLoadingInventoryAccessIds] = useState<Record<string, boolean>>({});
  const [selectedBackendId, setSelectedBackendId] = useState("");
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const hasObservedBackendRuntimeRef = useRef(false);

  const load = useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      const nextAccesses = await listProjectBackendAccess(projectId);
      setAccesses(nextAccesses);
    } catch (loadError) {
      setError((loadError as Error).message);
    } finally {
      setIsLoading(false);
    }
  }, [projectId]);

  useEffect(() => {
    void fetchBackends();
    void load();
  }, [fetchBackends, load]);

  const reloadExpandedInventories = useCallback(async () => {
    const expandedAccessIds = Object.entries(expandedInventoryAccessIds)
      .filter(([, expanded]) => expanded)
      .map(([accessId]) => accessId);
    if (expandedAccessIds.length === 0) return;

    setError(null);
    for (const accessId of expandedAccessIds) {
      setLoadingInventoryAccessIds((current) => ({ ...current, [accessId]: true }));
    }
    try {
      const inventoryEntries = await Promise.all(
        expandedAccessIds.map(async (accessId) => [
          accessId,
          await listBackendWorkspaceInventory(projectId, accessId),
        ] as const),
      );
      setInventoriesByAccessId((current) => ({
        ...current,
        ...Object.fromEntries(inventoryEntries),
      }));
    } catch (inventoryError) {
      setError((inventoryError as Error).message);
    } finally {
      for (const accessId of expandedAccessIds) {
        setLoadingInventoryAccessIds((current) => ({ ...current, [accessId]: false }));
      }
    }
  }, [expandedInventoryAccessIds, projectId]);

  useEffect(() => {
    if (inventoryRefreshKey === 0) return;
    void load();
    void reloadExpandedInventories();
  }, [inventoryRefreshKey, load, reloadExpandedInventories]);

  const authorizedBackendIds = useMemo(
    () => new Set(accesses.map((access) => access.backend_id)),
    [accesses],
  );
  const selectableBackends = useMemo(
    () => backends.filter((backend) => !authorizedBackendIds.has(backend.id)),
    [authorizedBackendIds, backends],
  );
  const backendRuntimeSignature = useMemo(
    () => backendStatusSignature(backends),
    [backends],
  );

  useEffect(() => {
    if (!backendRuntimeSignature) return;
    if (!hasObservedBackendRuntimeRef.current) {
      hasObservedBackendRuntimeRef.current = true;
      return;
    }
    void load();
    void reloadExpandedInventories();
  }, [backendRuntimeSignature, load, reloadExpandedInventories]);

  useEffect(() => {
    if (selectedBackendId && selectableBackends.some((backend) => backend.id === selectedBackendId)) return;
    setSelectedBackendId(selectableBackends[0]?.id ?? "");
  }, [selectableBackends, selectedBackendId]);

  const backendName = (backendId: string) => backends.find((backend) => backend.id === backendId)?.name ?? backendId;
  const backendMachineKind = (backendId: string): MachineKind => {
    const backend = backends.find((item) => item.id === backendId);
    return backend ? classifyMachine(backend) : "other";
  };
  const backendIsOnline = (backendId: string): boolean =>
    backends.find((item) => item.id === backendId)?.online === true;

  const handleAddAccess = async () => {
    if (!selectedBackendId) {
      setError("请选择 backend");
      return;
    }
    setError(null);
    try {
      const access = await createProjectBackendAccess(projectId, {
        backend_id: selectedBackendId,
      });
      setAccesses((current) => {
        const next = current.filter((item) => item.id !== access.id);
        return [...next, access].sort((a, b) => b.priority - a.priority);
      });
    } catch (addError) {
      setError((addError as Error).message);
    }
  };

  const handleLoadInventory = async (access: ProjectBackendAccess) => {
    const isExpanded = expandedInventoryAccessIds[access.id] === true;
    if (isExpanded) {
      setExpandedInventoryAccessIds((current) => ({ ...current, [access.id]: false }));
      return;
    }
    setExpandedInventoryAccessIds((current) => ({ ...current, [access.id]: true }));
    setError(null);
    setLoadingInventoryAccessIds((current) => ({ ...current, [access.id]: true }));
    try {
      const items = await listBackendWorkspaceInventory(projectId, access.id);
      setInventoriesByAccessId((current) => ({ ...current, [access.id]: items }));
    } catch (inventoryError) {
      setError((inventoryError as Error).message);
    } finally {
      setLoadingInventoryAccessIds((current) => ({ ...current, [access.id]: false }));
    }
  };

  const handleRevoke = async (access: ProjectBackendAccess) => {
    setError(null);
    try {
      await revokeProjectBackendAccess(projectId, access.id);
      setAccesses((current) => current.filter((item) => item.id !== access.id));
    } catch (revokeError) {
      setError((revokeError as Error).message);
    }
  };

  return (
    <div className="space-y-6">
      <ContentGroup
        title="可用机器"
        description="这个项目可以在下面这些机器上运行。本机（这台设备）来自桌面 App，服务器 runner 来自接入令牌。"
      >
        <div className="grid gap-3 md:grid-cols-[minmax(0,1fr)_auto]">
          <select
            value={selectedBackendId}
            onChange={(event) => setSelectedBackendId(event.target.value)}
            disabled={!canEdit}
            className="agentdash-form-select disabled:cursor-not-allowed disabled:opacity-60"
          >
            <option value="">选择一台已知机器</option>
            {selectableBackends.map((backend) => (
              <option key={backend.id} value={backend.id}>
                {backend.name} · {machineKindLabel(classifyMachine(backend))} {backend.online ? "(在线)" : "(离线)"}
              </option>
            ))}
          </select>
          <button
            type="button"
            onClick={() => void handleAddAccess()}
            disabled={!canEdit || !selectedBackendId}
            className="agentdash-button-secondary disabled:cursor-not-allowed disabled:opacity-60"
          >
            添加机器
          </button>
        </div>

        {isLoading && <p className="text-xs text-muted-foreground">正在加载可用机器...</p>}
        {accesses.length === 0 && !isLoading && (
          <p className="rounded-[12px] border border-dashed border-border px-4 py-4 text-sm text-muted-foreground">
            这个项目还没有可用机器。把本机加进来，或在下方「接入新服务器」签发令牌后接入服务器 runner。
          </p>
        )}

        <div className="space-y-3">
          {accesses.map((access) => {
            const inventory = inventoriesByAccessId[access.id] ?? [];
            const inventoryExpanded = expandedInventoryAccessIds[access.id] === true;
            const inventoryLoading = loadingInventoryAccessIds[access.id] === true;
            return (
              <div key={access.id} className="rounded-[12px] border border-border bg-background px-4 py-4">
                <div className="flex flex-wrap items-start justify-between gap-3">
                  <div className="min-w-0">
                    <div className="flex flex-wrap items-center gap-2">
                      <span
                        className={`inline-block h-2 w-2 shrink-0 rounded-full ${
                          backendIsOnline(access.backend_id) ? "bg-success" : "bg-muted-foreground/30"
                        }`}
                        title={backendIsOnline(access.backend_id) ? "在线" : "离线"}
                      />
                      <p className="truncate text-sm font-medium text-foreground">{backendName(access.backend_id)}</p>
                      <MachineKindBadge kind={backendMachineKind(access.backend_id)} />
                    </div>
                    <p className="mt-1 truncate font-mono text-xs text-muted-foreground">{access.backend_id}</p>
                  </div>
                  <div className="flex flex-wrap gap-2">
                    <button
                      type="button"
                      onClick={() => void handleLoadInventory(access)}
                      className="inline-flex items-center gap-1.5 rounded-[8px] border border-border bg-background px-3 py-1.5 text-xs text-muted-foreground hover:bg-secondary"
                    >
                      <span className="text-[10px]">{inventoryExpanded ? "▾" : "▸"}</span>
                      <span>{inventoryExpanded ? "收起详情" : "运行落点详情"}</span>
                    </button>
                    <button
                      type="button"
                      onClick={() => void handleRevoke(access)}
                      disabled={!canEdit}
                      className="rounded-[8px] border border-destructive/25 bg-destructive/5 px-3 py-1.5 text-xs text-destructive hover:bg-destructive/10 disabled:cursor-not-allowed disabled:opacity-50"
                    >
                      移除
                    </button>
                  </div>
                </div>

                {inventoryExpanded && (
                  <div className="mt-3 space-y-2 border-t border-border/70 pt-3">
                    <div className="flex flex-wrap items-center gap-2">
                      <span className="rounded-[8px] border border-border bg-secondary/40 px-2 py-0.5 text-[10px] text-muted-foreground">
                        授权状态：{ACCESS_STATUS_LABELS[access.status]}
                      </span>
                      <span className="rounded-[8px] border border-border bg-background px-2 py-0.5 text-[10px] text-muted-foreground">
                        优先级 {access.priority}
                      </span>
                    </div>
                    {inventoryLoading ? (
                      <p className="rounded-[8px] border border-border bg-muted/25 px-3 py-3 text-xs text-muted-foreground">
                        正在加载 inventory...
                      </p>
                    ) : inventory.length === 0 ? (
                      <p className="rounded-[8px] border border-dashed border-border px-3 py-3 text-xs text-muted-foreground">
                        当前还没有可用目录快照。等待 backend 上报，或使用本机 Workspace 发现登记新的可用目录。
                      </p>
                    ) : (
                      inventory.map((item) => (
                        <div key={item.id} className="grid gap-2 rounded-[8px] bg-muted/25 px-3 py-2 text-xs md:grid-cols-[120px_minmax(0,1fr)_100px]">
                          <span className="text-muted-foreground">{item.identity_kind}</span>
                          <span className="truncate font-mono text-foreground">{item.root_ref}</span>
                          <span className="text-muted-foreground">{INVENTORY_STATUS_LABELS[item.status]}</span>
                        </div>
                      ))
                    )}
                  </div>
                )}
              </div>
            );
          })}
        </div>
      </ContentGroup>

      {error && (
        <div className="rounded-[12px] border border-destructive/40 bg-destructive/10 px-4 py-3 text-sm text-destructive">
          {error}
        </div>
      )}
    </div>
  );
}
