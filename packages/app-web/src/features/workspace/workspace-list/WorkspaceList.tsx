import { useCallback, useEffect, useMemo, useRef, useState, type MouseEvent } from "react";
import type {
  ProjectBackendAccess,
  Workspace,
  WorkspaceInventoryCandidate,
} from "../../../types";
import { findWorkspaceBinding } from "../../../stores/workspaceStore";
import { useCoordinatorStore } from "../../../stores/coordinatorStore";
import {
  listProjectBackendAccess,
  listWorkspaceInventoryCandidates,
} from "../../../services/backendAccess";
import {
  identityKindLabels,
  identitySummary,
  summarizeAvailability,
  summarizeResolution,
} from "../model/workspaceRouting";
import {
  BindingStatusBadge,
  ResolutionBadge,
  WorkspaceStatusBadge,
} from "./badges";
import { WorkspaceEditorDrawer } from "./WorkspaceListEditor";
interface WorkspaceListProps {
  projectId: string;
  workspaces: Workspace[];
  defaultWorkspaceId?: string | null;
  canManageBindings?: boolean;
  onSetDefault?: (workspaceId: string | null) => void | Promise<void>;
  onInventoryChanged?: () => void | Promise<void>;
}

export function WorkspaceList({
  projectId,
  workspaces,
  defaultWorkspaceId,
  canManageBindings = false,
  onSetDefault,
  onInventoryChanged,
}: WorkspaceListProps) {
  const backends = useCoordinatorStore((state) => state.backends);
  const fetchBackends = useCoordinatorStore((state) => state.fetchBackends);
  const [isCreateOpen, setIsCreateOpen] = useState(false);
  const [selectedWorkspaceId, setSelectedWorkspaceId] = useState<string | null>(null);
  const [accesses, setAccesses] = useState<ProjectBackendAccess[]>([]);
  const [candidates, setCandidates] = useState<WorkspaceInventoryCandidate[]>([]);
  const [loadError, setLoadError] = useState<string | null>(null);
  const loadRoutingInputs = useCallback(async () => {
    setLoadError(null);
    try {
      const [nextAccesses, nextCandidates] = await Promise.all([
        listProjectBackendAccess(projectId),
        listWorkspaceInventoryCandidates(projectId),
      ]);
      setAccesses(nextAccesses);
      setCandidates(nextCandidates);
    } catch (loadErrorValue) {
      setLoadError((loadErrorValue as Error).message);
    }
  }, [projectId]);

  useEffect(() => {
    void fetchBackends();
    const timer = window.setTimeout(() => {
      void loadRoutingInputs();
    }, 0);
    return () => window.clearTimeout(timer);
  }, [fetchBackends, loadRoutingInputs]);

  // 跟随 backend 上下线 / 健康变化重载运行位置输入，与 BackendAccessPanel 保持一致刷新。
  const backendRuntimeSignature = useMemo(
    () => backends
      .map((backend) => [
        backend.id,
        backend.online ? "online" : "offline",
        backend.runtime_health?.status ?? "",
        backend.runtime_health?.updated_at ?? "",
      ].join(":"))
      .join("|"),
    [backends],
  );
  const hasObservedBackendRuntimeRef = useRef(false);
  useEffect(() => {
    if (!backendRuntimeSignature) return;
    if (!hasObservedBackendRuntimeRef.current) {
      hasObservedBackendRuntimeRef.current = true;
      return;
    }
    const timer = window.setTimeout(() => {
      void loadRoutingInputs();
    }, 0);
    return () => window.clearTimeout(timer);
  }, [backendRuntimeSignature, loadRoutingInputs]);

  // 从 workspaces 派生当前详情对象，保证保存 + store upsert 后详情抽屉拿到最新数据。
  const selectedWorkspace = useMemo(
    () => workspaces.find((item) => item.id === selectedWorkspaceId) ?? null,
    [workspaces, selectedWorkspaceId],
  );

  const handleToggleDefault = (workspaceId: string, event: MouseEvent) => {
    event.stopPropagation();
    if (!onSetDefault) return;
    onSetDefault(defaultWorkspaceId === workspaceId ? null : workspaceId);
  };

  const handleOpenCreate = async () => {
    await loadRoutingInputs();
    setIsCreateOpen(true);
  };

  const handleOpenDetail = async (workspaceId: string) => {
    await loadRoutingInputs();
    setSelectedWorkspaceId(workspaceId);
  };

  return (
    <>
      <div className="space-y-3">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div>
            <p className="text-[11px] uppercase tracking-[0.14em] text-muted-foreground">Workspace</p>
            <p className="text-xs text-muted-foreground">
              每个 Workspace 描述一处代码来源，运行位置来自已授权 Backend 的可选目录。
            </p>
          </div>
          <button
            type="button"
            onClick={() => void handleOpenCreate()}
            className="rounded-[8px] border border-border bg-background px-3 py-2 text-xs text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
          >
            + 新建 Workspace
          </button>
        </div>

        {loadError && (
          <p className="rounded-[8px] border border-destructive/35 bg-destructive/10 px-3 py-2 text-xs text-destructive">
            {loadError}
          </p>
        )}

        {workspaces.length === 0 && (
          <div className="rounded-[12px] border border-dashed border-border px-4 py-4 text-sm text-muted-foreground">
            <p>当前还没有 Workspace。</p>
            <p className="mt-1 text-xs">
              可以从可选目录直接创建，或浏览本机目录快速添加。
            </p>
          </div>
        )}

        {workspaces.map((workspace) => {
          const availability = summarizeAvailability(workspace, backends, accesses);
          const resolution = summarizeResolution(workspace, backends, accesses);
          const primaryBinding = resolution.binding ?? findWorkspaceBinding(workspace);
          const isDefault = defaultWorkspaceId === workspace.id;
          return (
            <div
              key={workspace.id}
              className={`w-full rounded-[12px] border px-4 py-4 transition-colors ${
                isDefault
                  ? "border-primary/30 bg-primary/[0.03]"
                  : "border-border bg-background"
              }`}
            >
              <div className="flex items-start justify-between gap-3">
                <div className="min-w-0 flex-1">
                  <div className="flex flex-wrap items-center gap-2">
                    <p className="truncate text-sm font-medium text-foreground">{workspace.name}</p>
                    {isDefault && (
                      <span className="inline-flex items-center rounded-[8px] border border-primary/25 bg-primary/10 px-2.5 py-0.5 text-[10px] font-medium text-primary">
                        Project 默认
                      </span>
                    )}
                    <WorkspaceStatusBadge status={workspace.status} />
                    <span className="rounded-[8px] border border-border bg-background px-2 py-0.5 text-[10px] text-muted-foreground">
                      {identityKindLabels[workspace.identity_kind]}
                    </span>
                    <ResolutionBadge state={resolution.state} />
                  </div>
                  <p className="mt-1 truncate text-xs text-muted-foreground">
                    代码来源：{identitySummary(workspace.identity_kind, workspace.identity_payload)}
                  </p>
                  <p className="mt-1 truncate text-xs text-muted-foreground">
                    运行解析：{resolution.label} · {resolution.description}
                  </p>
                  {resolution.warnings.length > 0 && (
                    <p className="mt-1 truncate text-xs text-warning">
                      {resolution.warnings[0]}
                    </p>
                  )}
                </div>

                <div className="flex shrink-0 flex-col items-end gap-2">
                  <div className="text-right">
                    <p className="text-[11px] uppercase tracking-[0.14em] text-muted-foreground">
                      运行位置
                    </p>
                    <p className="text-sm font-medium text-foreground">
                      {availability.online}/{availability.total}
                    </p>
                    <p className="text-[10px] text-muted-foreground">
                      可用 {availability.ready} · 已授权 {availability.authorized}
                    </p>
                    {primaryBinding && <BindingStatusBadge status={primaryBinding.status} />}
                  </div>
                  <div className="flex flex-wrap justify-end gap-2">
                    {onSetDefault && (
                      <button
                        type="button"
                        onClick={(event) => handleToggleDefault(workspace.id, event)}
                        className={`rounded-[8px] border px-2.5 py-1.5 text-[11px] transition-colors ${
                          isDefault
                            ? "border-primary/25 bg-primary/10 text-primary hover:bg-primary/15"
                            : "border-border bg-background text-muted-foreground hover:border-primary/25 hover:bg-primary/5 hover:text-primary"
                        }`}
                      >
                        {isDefault ? "取消 Project 默认" : "设为 Project 默认"}
                      </button>
                    )}
                    <button
                      type="button"
                      onClick={() => void handleOpenDetail(workspace.id)}
                      className="rounded-[8px] border border-border bg-background px-2.5 py-1.5 text-[11px] text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
                    >
                      详情
                    </button>
                  </div>
                </div>
              </div>
            </div>
          );
        })}
      </div>

      <WorkspaceEditorDrawer
        key={`workspace-create-${projectId}-${isCreateOpen ? "open" : "closed"}`}
        open={isCreateOpen}
        projectId={projectId}
        mode="create"
        workspace={null}
        defaultWorkspaceId={defaultWorkspaceId}
        candidates={candidates}
        accesses={accesses}
        canManageBindings={canManageBindings}
        onClose={() => setIsCreateOpen(false)}
        onSetDefault={onSetDefault}
        onCandidatesChanged={loadRoutingInputs}
        onInventoryChanged={onInventoryChanged}
      />

      <WorkspaceEditorDrawer
        key={`workspace-detail-${selectedWorkspaceId ?? "none"}`}
        open={Boolean(selectedWorkspace)}
        projectId={projectId}
        mode="detail"
        workspace={selectedWorkspace}
        candidates={candidates}
        accesses={accesses}
        canManageBindings={canManageBindings}
        onClose={() => setSelectedWorkspaceId(null)}
        onSetDefault={onSetDefault}
        onCandidatesChanged={loadRoutingInputs}
        onInventoryChanged={onInventoryChanged}
      />
    </>
  );
}
