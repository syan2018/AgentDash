import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Button, CardMenu, DialogFrame, type CardMenuItem } from "@agentdash/ui";
import type {
  BackendWorkspaceInventory,
  ProjectBackendAccess,
  Workspace,
} from "../../../types";
import { useCoordinatorStore } from "../../../stores/coordinatorStore";
import {
  classifyMachine,
  machineKindLabel,
  type MachineKind,
} from "../../workspace/model/machinePresentation";
import {
  BACKEND_ACCESS_STATUS_LABELS,
  IDENTITY_KIND_LABELS,
  INVENTORY_STATUS_LABELS,
} from "../../workspace/model/workspaceTerms";
import { backendStatusSignature } from "../../../utils/backendStatusSignature";
import {
  createProjectBackendAccess,
  listBackendWorkspaceInventory,
  listProjectBackendAccess,
  revokeProjectBackendAccess,
} from "../../../services/backendAccess";
import { RunnerTokensPanel } from "../../workspace/runner-tokens/RunnerTokensPanel";
import { LocalWorkspaceDiscoveryPanel } from "../../workspace/workspace-list/LocalWorkspaceDiscoveryPanel";
import { SectionCard } from "./settings-ui";

const MACHINE_KIND_TONE: Record<MachineKind, string> = {
  local_device: "border-primary/30 bg-primary/10 text-primary",
  server_runner: "border-info/30 bg-info/10 text-info",
  other: "border-border bg-secondary/40 text-muted-foreground",
};

function MachineKindBadge({ kind }: { kind: MachineKind }) {
  return (
    <span className={`shrink-0 rounded-[8px] border px-2 py-0.5 text-[10px] font-medium ${MACHINE_KIND_TONE[kind]}`}>
      {machineKindLabel(kind)}
    </span>
  );
}

// 把「在线」与「授权状态」融合成用户一眼可懂的运行态：能不能跑活。
type MachineRunState = "runnable" | "paused" | "offline" | "revoked";

function machineRunState(status: ProjectBackendAccess["status"], online: boolean): MachineRunState {
  if (status === "revoked") return "revoked";
  if (status === "paused") return "paused";
  return online ? "runnable" : "offline";
}

const RUN_STATE_LABELS: Record<MachineRunState, string> = {
  runnable: "可运行",
  paused: "已暂停",
  offline: "离线",
  revoked: "已撤销",
};

const RUN_STATE_TONE: Record<MachineRunState, string> = {
  runnable: "border-success/30 bg-success/10 text-success",
  paused: "border-warning/30 bg-warning/10 text-warning",
  offline: "border-border bg-secondary/40 text-muted-foreground",
  revoked: "border-border bg-secondary/40 text-muted-foreground",
};

const RUN_STATE_DOT: Record<MachineRunState, string> = {
  runnable: "bg-success",
  paused: "bg-warning",
  offline: "bg-muted-foreground/30",
  revoked: "bg-muted-foreground/30",
};

function RunStatePill({ state }: { state: MachineRunState }) {
  return (
    <span className={`inline-flex shrink-0 items-center gap-1.5 rounded-[8px] border px-2 py-0.5 text-[10px] font-medium ${RUN_STATE_TONE[state]}`}>
      <span className={`h-1.5 w-1.5 rounded-full ${RUN_STATE_DOT[state]}`} />
      {RUN_STATE_LABELS[state]}
    </span>
  );
}

function AddMachineMenu({
  disabled,
  onAddKnown,
  onEnrollServer,
}: {
  disabled: boolean;
  onAddKnown: () => void;
  onEnrollServer: () => void;
}) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const onPointerDown = (event: MouseEvent) => {
      if (ref.current && !ref.current.contains(event.target as Node)) setOpen(false);
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    document.addEventListener("mousedown", onPointerDown);
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("mousedown", onPointerDown);
      document.removeEventListener("keydown", onKeyDown);
    };
  }, [open]);

  return (
    <div ref={ref} className="relative">
      <button
        type="button"
        disabled={disabled}
        onClick={() => setOpen((value) => !value)}
        className="agentdash-button-secondary disabled:cursor-not-allowed disabled:opacity-60"
      >
        + 添加机器
      </button>
      {open && (
        <div className="absolute right-0 top-full z-[80] mt-1 min-w-[12rem] rounded-[8px] border border-border bg-background p-1 shadow-xl">
          <button
            type="button"
            onClick={() => { setOpen(false); onAddKnown(); }}
            className="flex w-full items-center rounded-[6px] px-2.5 py-1.5 text-left text-xs text-foreground transition-colors hover:bg-secondary"
          >
            从已知机器添加
          </button>
          <button
            type="button"
            onClick={() => { setOpen(false); onEnrollServer(); }}
            className="flex w-full items-center rounded-[6px] px-2.5 py-1.5 text-left text-xs text-foreground transition-colors hover:bg-secondary"
          >
            接入新服务器…
          </button>
        </div>
      )}
    </div>
  );
}

export function BackendAccessPanel({
  projectId,
  canEdit,
  workspaces,
  inventoryRefreshKey = 0,
  onWorkspacesChanged,
}: {
  projectId: string;
  canEdit: boolean;
  workspaces: Workspace[];
  inventoryRefreshKey?: number;
  onWorkspacesChanged?: () => void | Promise<void>;
}) {
  const backends = useCoordinatorStore((state) => state.backends);
  const fetchBackends = useCoordinatorStore((state) => state.fetchBackends);
  const [accesses, setAccesses] = useState<ProjectBackendAccess[]>([]);
  const [inventoriesByAccessId, setInventoriesByAccessId] = useState<Record<string, BackendWorkspaceInventory[]>>({});
  const [expandedInventoryAccessIds, setExpandedInventoryAccessIds] = useState<Record<string, boolean>>({});
  const [loadingInventoryAccessIds, setLoadingInventoryAccessIds] = useState<Record<string, boolean>>({});
  const [selectedBackendId, setSelectedBackendId] = useState("");
  const [showKnownPicker, setShowKnownPicker] = useState(false);
  const [serverDialogOpen, setServerDialogOpen] = useState(false);
  const [scanBackendId, setScanBackendId] = useState<string | null>(null);
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

  // 本机相关的两个高频快速操作：加入本机、扫描本机目录。放在本节底部，样式克制。
  const localQuickBindBackendId =
    accesses.find(
      (access) =>
        access.status === "active" &&
        backendMachineKind(access.backend_id) === "local_device" &&
        backendIsOnline(access.backend_id),
    )?.backend_id ?? null;
  const localSelectableBackendId =
    selectableBackends.find((backend) => classifyMachine(backend) === "local_device")?.id ?? null;

  const addAccess = async (backendId: string) => {
    setError(null);
    try {
      const access = await createProjectBackendAccess(projectId, { backend_id: backendId });
      setAccesses((current) => {
        const next = current.filter((item) => item.id !== access.id);
        return [...next, access].sort((a, b) => b.priority - a.priority);
      });
      return true;
    } catch (addError) {
      setError((addError as Error).message);
      return false;
    }
  };

  const handleAddAccess = async () => {
    if (!selectedBackendId) {
      setError("请选择 backend");
      return;
    }
    if (await addAccess(selectedBackendId)) setShowKnownPicker(false);
  };

  const handleToggleInventory = async (access: ProjectBackendAccess) => {
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
    <>
      <SectionCard
        title="运行机器"
        description="项目可以在这些机器上运行；本机来自桌面 App，服务器 runner 来自接入令牌。"
        action={
          <AddMachineMenu
            disabled={!canEdit}
            onAddKnown={() => setShowKnownPicker((value) => !value)}
            onEnrollServer={() => setServerDialogOpen(true)}
          />
        }
      >
        {showKnownPicker && (
          <div className="grid gap-3 rounded-[12px] border border-border bg-muted/20 p-3 md:grid-cols-[minmax(0,1fr)_auto_auto]">
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
              添加
            </button>
            <button
              type="button"
              onClick={() => setShowKnownPicker(false)}
              className="text-xs text-muted-foreground hover:text-foreground"
            >
              取消
            </button>
          </div>
        )}

        {isLoading && <p className="text-xs text-muted-foreground">正在加载可用机器...</p>}
        {accesses.length === 0 && !isLoading && (
          <p className="rounded-[12px] border border-dashed border-border px-4 py-4 text-sm text-muted-foreground">
            还没有可用机器。
          </p>
        )}

        <div className="space-y-3">
          {accesses.map((access) => {
            const inventory = inventoriesByAccessId[access.id] ?? [];
            const inventoryExpanded = expandedInventoryAccessIds[access.id] === true;
            const inventoryLoading = loadingInventoryAccessIds[access.id] === true;
            const online = backendIsOnline(access.backend_id);
            const kind = backendMachineKind(access.backend_id);
            const runState = machineRunState(access.status, online);

            const menuItems: CardMenuItem[] = [];
            if (canEdit && kind === "local_device" && online && access.status === "active") {
              menuItems.push({
                key: "scan",
                label: "扫描可绑定目录",
                onSelect: () => setScanBackendId(access.backend_id),
              });
            }
            if (canEdit) {
              menuItems.push({
                key: "remove",
                label: "移除",
                danger: true,
                onSelect: () => void handleRevoke(access),
              });
            }

            return (
              <div key={access.id} className="rounded-[12px] border border-border bg-background px-4 py-3">
                <div className="flex items-center justify-between gap-3">
                  <button
                    type="button"
                    onClick={() => void handleToggleInventory(access)}
                    aria-expanded={inventoryExpanded}
                    className="flex min-w-0 flex-1 items-center gap-2 text-left"
                  >
                    <span className="text-[10px] text-muted-foreground">{inventoryExpanded ? "▾" : "▸"}</span>
                    <RunStatePill state={runState} />
                    <span className="truncate text-sm font-medium text-foreground">{backendName(access.backend_id)}</span>
                    <MachineKindBadge kind={kind} />
                  </button>
                  {menuItems.length > 0 && <CardMenu items={menuItems} />}
                </div>

                {inventoryExpanded && (
                  <div className="mt-3 space-y-2 border-t border-border/70 pt-3">
                    <div className="flex flex-wrap items-center gap-2">
                      <span className="rounded-[8px] border border-border bg-secondary/40 px-2 py-0.5 text-[10px] text-muted-foreground">
                        授权状态：{BACKEND_ACCESS_STATUS_LABELS[access.status]}
                      </span>
                      <span className="rounded-[8px] border border-border bg-background px-2 py-0.5 text-[10px] text-muted-foreground">
                        优先级 {access.priority}
                      </span>
                      <span className="truncate rounded-[8px] border border-border bg-background px-2 py-0.5 font-mono text-[10px] text-muted-foreground">
                        {access.backend_id}
                      </span>
                    </div>
                    <p className="text-[10px] font-medium text-muted-foreground">这台机器上可用的目录</p>
                    {inventoryLoading ? (
                      <p className="rounded-[8px] border border-border bg-muted/25 px-3 py-3 text-xs text-muted-foreground">
                        正在加载目录...
                      </p>
                    ) : inventory.length === 0 ? (
                      <p className="rounded-[8px] border border-dashed border-border px-3 py-3 text-xs text-muted-foreground">
                        还没有可用目录。等待机器上报，或用「扫描可绑定目录」登记。
                      </p>
                    ) : (
                      inventory.map((item) => (
                        <div key={item.id} className="grid gap-2 rounded-[8px] bg-muted/25 px-3 py-2 text-xs md:grid-cols-[100px_minmax(0,1fr)_72px]">
                          <span className="text-muted-foreground">
                            {IDENTITY_KIND_LABELS[item.identity_kind] ?? item.identity_kind}
                          </span>
                          <span className="truncate font-mono text-foreground" title={item.root_ref}>{item.root_ref}</span>
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

        {canEdit && (localSelectableBackendId || localQuickBindBackendId) && (
          <div className="flex flex-wrap items-center justify-between gap-3 rounded-[12px] border border-dashed border-border px-4 py-3">
            <span className="text-xs text-muted-foreground">
              {localQuickBindBackendId
                ? "本机：快速绑定本机上的工作空间目录"
                : "本机：加入本机即可使用本地工作空间"}
            </span>
            <div className="flex flex-wrap gap-2">
              {localQuickBindBackendId && (
                <button
                  type="button"
                  onClick={() => setScanBackendId(localQuickBindBackendId)}
                  className="agentdash-button-secondary text-xs"
                >
                  扫描目录
                </button>
              )}
              {localSelectableBackendId && (
                <button
                  type="button"
                  onClick={() => void addAccess(localSelectableBackendId)}
                  className="agentdash-button-secondary text-xs"
                >
                  添加本机
                </button>
              )}
            </div>
          </div>
        )}

        {error && (
          <div className="rounded-[12px] border border-destructive/40 bg-destructive/10 px-4 py-3 text-sm text-destructive">
            {error}
          </div>
        )}
      </SectionCard>

      <DialogFrame
        open={serverDialogOpen}
        title="接入新服务器"
        onClose={() => setServerDialogOpen(false)}
        footer={<Button variant="secondary" onClick={() => setServerDialogOpen(false)}>关闭</Button>}
      >
        <RunnerTokensPanel projectId={projectId} canEdit={canEdit} />
      </DialogFrame>

      <DialogFrame
        open={scanBackendId != null}
        title={`扫描可绑定目录${scanBackendId ? ` · ${backendName(scanBackendId)}` : ""}`}
        onClose={() => setScanBackendId(null)}
        footer={<Button variant="secondary" onClick={() => setScanBackendId(null)}>关闭</Button>}
      >
        {scanBackendId && (
          <LocalWorkspaceDiscoveryPanel
            projectId={projectId}
            workspaces={workspaces}
            canEdit={canEdit}
            backendId={scanBackendId}
            onBound={onWorkspacesChanged}
          />
        )}
      </DialogFrame>
    </>
  );
}
