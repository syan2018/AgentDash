import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type {
  DiscoveredWorkspaceBindingCandidate,
  DiscoverLocalWorkspaceBindingsResponse,
} from "../../../generated/workspace-contracts";
import {
  bindDiscoveredWorkspaceBindings,
  discoverLocalWorkspaceBindings,
  listProjectBackendAccess,
} from "../../../services/backendAccess";
import { useCoordinatorStore } from "../../../stores/coordinatorStore";
import type { ProjectBackendAccess, Workspace } from "../../../types";
import { IDENTITY_KIND_LABELS } from "../model/workspaceTerms";

interface LocalWorkspaceDiscoveryPanelProps {
  projectId: string;
  workspaces: Workspace[];
  canEdit: boolean;
  refreshKey?: number;
  onBound?: () => void | Promise<void>;
}

interface DiscoverableBackend {
  access: ProjectBackendAccess;
  id: string;
  name: string;
  online: boolean;
}

export function LocalWorkspaceDiscoveryPanel({
  projectId,
  workspaces,
  canEdit,
  refreshKey = 0,
  onBound,
}: LocalWorkspaceDiscoveryPanelProps) {
  const backends = useCoordinatorStore((state) => state.backends);
  const fetchBackends = useCoordinatorStore((state) => state.fetchBackends);
  const [accesses, setAccesses] = useState<ProjectBackendAccess[]>([]);
  const [selectedBackendId, setSelectedBackendId] = useState("");
  const [result, setResult] = useState<DiscoverLocalWorkspaceBindingsResponse | null>(null);
  const [selectedCandidateKeys, setSelectedCandidateKeys] = useState<Record<string, string>>({});
  const [bindingCandidateKeys, setBindingCandidateKeys] = useState<Record<string, boolean>>({});
  const [isLoadingAccesses, setIsLoadingAccesses] = useState(false);
  const [isDiscovering, setIsDiscovering] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const hasObservedBackendRuntimeRef = useRef(false);

  const loadAccesses = useCallback(async () => {
    setIsLoadingAccesses(true);
    setError(null);
    try {
      const [nextAccesses] = await Promise.all([
        listProjectBackendAccess(projectId),
        fetchBackends(),
      ]);
      setAccesses(nextAccesses);
    } catch (loadError) {
      setError((loadError as Error).message);
    } finally {
      setIsLoadingAccesses(false);
    }
  }, [fetchBackends, projectId]);

  useEffect(() => {
    void loadAccesses();
  }, [loadAccesses]);

  useEffect(() => {
    if (refreshKey === 0) return;
    void loadAccesses();
  }, [loadAccesses, refreshKey]);

  const backendRuntimeSignature = useMemo(
    () =>
      backends
        .map((backend) =>
          [
            backend.id,
            backend.online ? "online" : "offline",
            backend.runtime_health?.status ?? "",
            backend.runtime_health?.updated_at ?? "",
          ].join(":"),
        )
        .join("|"),
    [backends],
  );

  useEffect(() => {
    if (!backendRuntimeSignature) return;
    if (!hasObservedBackendRuntimeRef.current) {
      hasObservedBackendRuntimeRef.current = true;
      return;
    }
    void loadAccesses();
  }, [backendRuntimeSignature, loadAccesses]);

  const discoverableBackends = useMemo<DiscoverableBackend[]>(() => {
    return accesses
      .filter((access) => access.status === "active")
      .map((access) => {
        const backend = backends.find((item) => item.id === access.backend_id);
        if (!backend || backend.backend_type !== "local") return null;
        return {
          access,
          id: access.backend_id,
          name: backend.name,
          online: backend.online,
        };
      })
      .filter((item): item is DiscoverableBackend => item !== null)
      .sort((a, b) => Number(b.online) - Number(a.online) || a.name.localeCompare(b.name));
  }, [accesses, backends]);

  const onlineDiscoverableBackends = useMemo(
    () => discoverableBackends.filter((backend) => backend.online),
    [discoverableBackends],
  );

  useEffect(() => {
    if (
      selectedBackendId &&
      onlineDiscoverableBackends.some((backend) => backend.id === selectedBackendId)
    ) {
      return;
    }
    setSelectedBackendId(onlineDiscoverableBackends[0]?.id ?? "");
  }, [onlineDiscoverableBackends, selectedBackendId]);

  const discoverableWorkspaceCount = useMemo(
    () => workspaces.filter((workspace) => workspace.identity_kind === "p4_workspace").length,
    [workspaces],
  );

  const groupedCandidates = useMemo(() => {
    if (!result) return [];
    const groups = new Map<string, DiscoveredWorkspaceBindingCandidate[]>();
    for (const candidate of result.candidates) {
      const current = groups.get(candidate.workspace_id) ?? [];
      current.push(candidate);
      groups.set(candidate.workspace_id, current);
    }
    return [...groups.entries()].map(([workspaceId, candidates]) => ({
      workspaceId,
      workspaceName: candidates[0]?.workspace_name ?? workspaceId,
      candidates,
    }));
  }, [result]);

  const boundCandidateKeys = useMemo(() => {
    const keys = new Set<string>();
    for (const workspace of workspaces) {
      for (const binding of workspace.bindings) {
        keys.add(candidateBindingKey(workspace.id, binding.backend_id, binding.root_ref));
      }
    }
    return keys;
  }, [workspaces]);

  const runDiscover = useCallback(
    async (backendId: string) => {
      if (!backendId) {
        setError("请选择可用的本机 backend");
        return;
      }
      setIsDiscovering(true);
      setMessage(null);
      setError(null);
      try {
        const nextResult = await discoverLocalWorkspaceBindings(projectId, {
          backend_id: backendId,
        });
        setResult(nextResult);
        setSelectedCandidateKeys({});
      } catch (discoverError) {
        setError((discoverError as Error).message);
      } finally {
        setIsDiscovering(false);
      }
    },
    [projectId],
  );

  const bindCandidates = async (candidates: DiscoveredWorkspaceBindingCandidate[]) => {
    if (!result || candidates.length === 0) return;
    const keys = candidates.map((candidate) =>
      candidateBindingKey(candidate.workspace_id, result.backend_id, candidate.root_ref),
    );
    setBindingCandidateKeys((current) => ({
      ...current,
      ...Object.fromEntries(keys.map((key) => [key, true])),
    }));
    setError(null);
    setMessage(null);
    try {
      const response = await bindDiscoveredWorkspaceBindings(projectId, {
        bindings: candidates.map((candidate) => ({
          workspace_id: candidate.workspace_id,
          backend_id: result.backend_id,
          root_ref: candidate.root_ref,
        })),
      });
      setMessage(
        `已绑定 ${response.bound_workspace_ids.length} 个 Workspace，新增 ${response.created_bindings} 条，更新 ${response.updated_bindings} 条。`,
      );
      await onBound?.();
      await runDiscover(result.backend_id);
    } catch (bindError) {
      setError((bindError as Error).message);
    } finally {
      setBindingCandidateKeys((current) => {
        const next = { ...current };
        for (const key of keys) {
          delete next[key];
        }
        return next;
      });
    }
  };

  return (
    <div className="space-y-5">
      <div className="grid gap-3 md:grid-cols-[minmax(0,1fr)_auto]">
        <select
          value={selectedBackendId}
          onChange={(event) => {
            setSelectedBackendId(event.target.value);
            setResult(null);
            setMessage(null);
            setError(null);
          }}
          disabled={!canEdit || isLoadingAccesses || onlineDiscoverableBackends.length === 0}
          className="agentdash-form-select disabled:cursor-not-allowed disabled:opacity-60"
        >
          <option value="">选择本机 backend</option>
          {discoverableBackends.map((backend) => (
            <option key={backend.id} value={backend.id} disabled={!backend.online}>
              {backend.name} {backend.online ? "(online)" : "(offline)"}
            </option>
          ))}
        </select>
        <button
          type="button"
          onClick={() => void runDiscover(selectedBackendId)}
          disabled={!canEdit || !selectedBackendId || isDiscovering}
          className="agentdash-button-secondary disabled:cursor-not-allowed disabled:opacity-60"
        >
          {isDiscovering ? "发现中..." : "发现本机 Workspace"}
        </button>
      </div>

      <div className="flex flex-wrap gap-2 text-xs">
        <span className="rounded-[8px] border border-border bg-secondary/35 px-3 py-1 text-muted-foreground">
          可发现 identity: {discoverableWorkspaceCount}
        </span>
        <span className="rounded-[8px] border border-border bg-secondary/35 px-3 py-1 text-muted-foreground">
          本机 backend: {onlineDiscoverableBackends.length}
        </span>
      </div>

      {!isLoadingAccesses && discoverableBackends.length === 0 && (
        <p className="rounded-[12px] border border-dashed border-border px-4 py-4 text-sm text-muted-foreground">
          当前 Project 还没有可用于本机发现的 local backend access。
        </p>
      )}

      {message && (
        <p className="rounded-[12px] border border-success/30 bg-success/10 px-4 py-3 text-sm text-success">
          {message}
        </p>
      )}
      {error && (
        <p className="rounded-[12px] border border-destructive/40 bg-destructive/10 px-4 py-3 text-sm text-destructive">
          {error}
        </p>
      )}

      {result && (
        <div className="space-y-4">
          {result.warnings.length > 0 && (
            <div className="rounded-[12px] border border-warning/30 bg-warning/10 px-4 py-3 text-xs text-warning">
              {result.warnings.slice(0, 3).map((warning) => (
                <p key={warning}>{warning}</p>
              ))}
            </div>
          )}

          {groupedCandidates.length === 0 ? (
            <p className="rounded-[12px] border border-dashed border-border px-4 py-4 text-sm text-muted-foreground">
              没有发现可绑定候选。
            </p>
          ) : (
            groupedCandidates.map((group) => (
              <CandidateGroup
                key={group.workspaceId}
                backendId={result.backend_id}
                workspaceName={group.workspaceName}
                candidates={group.candidates}
                boundCandidateKeys={boundCandidateKeys}
                bindingCandidateKeys={bindingCandidateKeys}
                selectedCandidateKey={selectedCandidateKeys[group.workspaceId] ?? ""}
                onSelect={(candidateKey) =>
                  setSelectedCandidateKeys((current) => ({
                    ...current,
                    [group.workspaceId]: candidateKey,
                  }))
                }
                onBind={(candidates) => void bindCandidates(candidates)}
              />
            ))
          )}

          {result.skipped.length > 0 && (
            <div className="space-y-2 rounded-[12px] border border-border bg-muted/20 px-4 py-4">
              <p className="text-xs font-medium text-foreground">暂未支持的 Workspace identity</p>
              <div className="space-y-1">
                {result.skipped.slice(0, 6).map((item) => (
                  <div
                    key={`${item.workspace_id}:${item.reason}`}
                    className="grid gap-2 text-xs md:grid-cols-[minmax(0,1fr)_120px_minmax(0,1.4fr)]"
                  >
                    <span className="truncate text-foreground">{item.workspace_name}</span>
                    <span className="text-muted-foreground">
                      {IDENTITY_KIND_LABELS[item.identity_kind]}
                    </span>
                    <span className="truncate text-muted-foreground">{item.message}</span>
                  </div>
                ))}
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

function CandidateGroup({
  backendId,
  workspaceName,
  candidates,
  boundCandidateKeys,
  bindingCandidateKeys,
  selectedCandidateKey,
  onSelect,
  onBind,
}: {
  backendId: string;
  workspaceName: string;
  candidates: DiscoveredWorkspaceBindingCandidate[];
  boundCandidateKeys: Set<string>;
  bindingCandidateKeys: Record<string, boolean>;
  selectedCandidateKey: string;
  onSelect: (candidateKey: string) => void;
  onBind: (candidates: DiscoveredWorkspaceBindingCandidate[]) => void;
}) {
  const singleCandidate = candidates.length === 1 ? candidates[0] : null;
  const selectedCandidate = candidates.find(
    (candidate) =>
      candidateBindingKey(candidate.workspace_id, backendId, candidate.root_ref) ===
      selectedCandidateKey,
  );

  return (
    <div className="rounded-[12px] border border-border bg-background px-4 py-4">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0">
          <p className="truncate text-sm font-medium text-foreground">{workspaceName}</p>
          <p className="mt-1 text-xs text-muted-foreground">
            {candidates.length} 个候选 · {IDENTITY_KIND_LABELS[candidates[0].identity_kind]}
          </p>
        </div>
        {singleCandidate ? (
          <BindCandidateButton
            backendId={backendId}
            candidate={singleCandidate}
            boundCandidateKeys={boundCandidateKeys}
            bindingCandidateKeys={bindingCandidateKeys}
            onBind={() => onBind([singleCandidate])}
          />
        ) : (
          <button
            type="button"
            onClick={() => selectedCandidate && onBind([selectedCandidate])}
            disabled={!selectedCandidate}
            className="agentdash-button-secondary text-xs disabled:cursor-not-allowed disabled:opacity-50"
          >
            绑定选中候选
          </button>
        )}
      </div>

      <div className="mt-3 space-y-2">
        {candidates.map((candidate) => {
          const key = candidateBindingKey(candidate.workspace_id, backendId, candidate.root_ref);
          const isBound = boundCandidateKeys.has(key);
          const isBinding = bindingCandidateKeys[key] === true;
          return (
            <label
              key={key}
              className="grid cursor-pointer gap-2 rounded-[8px] border border-border bg-muted/20 px-3 py-3 text-xs md:grid-cols-[auto_minmax(0,1fr)_auto]"
            >
              {candidates.length > 1 ? (
                <input
                  type="radio"
                  name={`candidate-${candidate.workspace_id}`}
                  checked={selectedCandidateKey === key}
                  onChange={() => onSelect(key)}
                  disabled={isBound || isBinding}
                  className="mt-0.5"
                />
              ) : (
                <span className="mt-1 h-2 w-2 rounded-[4px] bg-primary" />
              )}
              <span className="min-w-0">
                <span className="block truncate font-mono text-foreground">
                  {candidate.root_ref}
                </span>
                <span className="mt-1 flex flex-wrap gap-2 text-muted-foreground">
                  {candidate.client_name && <span>client: {candidate.client_name}</span>}
                  {candidate.stream && <span>stream: {candidate.stream}</span>}
                  {candidate.confidence && <span>confidence: {candidate.confidence}</span>}
                </span>
                {candidate.warnings.length > 0 && (
                  <span className="mt-1 block truncate text-warning">
                    {candidate.warnings[0]}
                  </span>
                )}
              </span>
              <span className="text-muted-foreground">
                {isBound ? "已绑定" : isBinding ? "绑定中" : candidate.display_name ?? "可绑定"}
              </span>
            </label>
          );
        })}
      </div>
    </div>
  );
}

function BindCandidateButton({
  backendId,
  candidate,
  boundCandidateKeys,
  bindingCandidateKeys,
  onBind,
}: {
  backendId: string;
  candidate: DiscoveredWorkspaceBindingCandidate;
  boundCandidateKeys: Set<string>;
  bindingCandidateKeys: Record<string, boolean>;
  onBind: () => void;
}) {
  const key = candidateBindingKey(candidate.workspace_id, backendId, candidate.root_ref);
  const isBound = boundCandidateKeys.has(key);
  const isBinding = bindingCandidateKeys[key] === true;
  return (
    <button
      type="button"
      onClick={onBind}
      disabled={isBound || isBinding}
      className="agentdash-button-secondary text-xs disabled:cursor-not-allowed disabled:opacity-50"
    >
      {isBound ? "已绑定" : isBinding ? "绑定中..." : "一键绑定"}
    </button>
  );
}

function candidateBindingKey(workspaceId: string, backendId: string, rootRef: string): string {
  return `${workspaceId}:${backendId}:${normalizeRootRef(rootRef)}`;
}

function normalizeRootRef(rootRef: string): string {
  return rootRef.trim().replaceAll("\\", "/").replace(/\/+$/, "");
}
