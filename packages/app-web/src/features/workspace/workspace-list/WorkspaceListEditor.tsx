import { useCallback, useEffect, useMemo, useState, type MouseEvent } from "react";
import type {
  BackendConfig,
  ContextContainerCapability,
  ProjectBackendAccess,
  Workspace,
  WorkspaceBindingStatus,
  WorkspaceDetectionResult,
  WorkspaceIdentityKind,
  WorkspaceInventoryCandidate,
  WorkspaceResolutionPolicy,
  WorkspaceStatus,
} from "../../../types";
import { ALL_CAPABILITIES, CONTEXT_CAPABILITY_OPTIONS } from "../../../components/context-config-defaults";
import {
  type WorkspaceBindingInput,
  findWorkspaceBinding,
  useWorkspaceStore,
} from "../../../stores/workspaceStore";
import { useCoordinatorStore } from "../../../stores/coordinatorStore";
import {
  DangerConfirmDialog,
  DetailMenu,
  DetailPanel,
  DetailSection,
} from "@agentdash/ui";
import {
  listProjectBackendAccess,
  listWorkspaceInventoryCandidates,
  registerBackendWorkspaceInventory,
} from "../../../services/backendAccess";
import { DirectoryBrowserDialog } from "../directory-browser-dialog";
import {
  authorizedBackends,
  backendDisplayName,
  bindingToInput,
  buildDefaultWorkspaceName,
  candidateToBindingInput,
  candidateToDraft,
  detectedFactsSummary,
  identityKindLabels,
  identitySummary,
  localAuthorizedBackends,
  summarizeAvailability,
  summarizeResolution,
} from "../model/workspaceRouting";

type CreateMode = "candidate" | "logical" | "local_detect";

const statusConfig: Record<WorkspaceStatus, { label: string; cls: string }> = {
  pending: { label: "待完善", cls: "border-border bg-secondary text-muted-foreground" },
  preparing: { label: "准备中", cls: "border-info/20 bg-info/10 text-info" },
  ready: { label: "可用", cls: "border-success/20 bg-success/10 text-success" },
  active: { label: "使用中", cls: "border-primary/20 bg-primary/10 text-primary" },
  archived: { label: "已归档", cls: "border-border bg-secondary text-muted-foreground" },
  error: { label: "异常", cls: "border-destructive/20 bg-destructive/10 text-destructive" },
};

const bindingStatusLabels: Record<WorkspaceBindingStatus, string> = {
  pending: "未确认",
  ready: "可用",
  offline: "离线",
  error: "异常",
};

const createModeLabels: Record<CreateMode, string> = {
  candidate: "从发现项创建",
  logical: "创建逻辑 Workspace",
  local_detect: "本机目录识别",
};

export function WorkspaceStatusBadge({ status }: { status: WorkspaceStatus }) {
  const config = statusConfig[status];
  return (
    <span className={`inline-flex items-center rounded-full border px-2.5 py-1 text-[10px] font-medium ${config.cls}`}>
      {config.label}
    </span>
  );
}

export function BindingStatusBadge({ status }: { status: WorkspaceBindingStatus }) {
  const tone = status === "ready"
    ? "bg-success"
    : status === "offline"
      ? "bg-muted-foreground/45"
      : "bg-destructive";
  return (
    <span className="inline-flex w-fit items-center gap-1.5 self-start whitespace-nowrap text-[11px] text-muted-foreground">
      <span className={`h-1.5 w-1.5 rounded-full ${tone}`} />
      {bindingStatusLabels[status]}
    </span>
  );
}

function bindingDraftKey(binding: Pick<WorkspaceBindingInput, "backend_id" | "root_ref">): string {
  const backendId = binding.backend_id.trim();
  const rootRef = binding.root_ref.trim().replaceAll("\\", "/").replace(/\/+$/, "");
  return `${backendId}:${rootRef}`;
}

function candidateDraftKey(candidate: WorkspaceInventoryCandidate): string {
  return bindingDraftKey({
    backend_id: candidate.backend_id,
    root_ref: candidate.root_ref,
  });
}

function dedupeBindings(bindings: WorkspaceBindingInput[]): WorkspaceBindingInput[] {
  const seen = new Set<string>();
  return bindings.filter((binding) => {
    const key = bindingDraftKey(binding);
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

export function ResolutionBadge({ state }: { state: "resolved" | "warning" | "blocked" }) {
  const cls = state === "resolved"
    ? "border-success/20 bg-success/10 text-success"
    : state === "warning"
      ? "border-warning/25 bg-warning/10 text-warning"
      : "border-destructive/25 bg-destructive/10 text-destructive";
  const label = state === "resolved" ? "可路由" : state === "warning" ? "需注意" : "不可路由";
  return (
    <span className={`inline-flex rounded-full border px-2 py-0.5 text-[10px] font-medium ${cls}`}>
      {label}
    </span>
  );
}

function normalizePayloadText(payload: Record<string, unknown>): string {
  return JSON.stringify(payload, null, 2);
}

function emptyPayload(kind: WorkspaceIdentityKind): Record<string, unknown> {
  if (kind === "git_repo") return { repo_key: "", branch: "" };
  if (kind === "p4_workspace") return { server_address: "", stream: "", client_name: "", path_key: "" };
  return { path_key: "" };
}

function updatePayloadField(
  payload: Record<string, unknown>,
  key: string,
  value: string,
): Record<string, unknown> {
  return { ...payload, [key]: value };
}

function stringField(payload: Record<string, unknown>, key: string): string {
  const value = payload[key];
  return typeof value === "string" ? value.trim() : "";
}

function detectionPrimaryText(result: WorkspaceDetectionResult): string {
  if (result.identity_kind === "git_repo") {
    return stringField(result.binding.detected_facts, "remote_url")
      || stringField(result.identity_payload, "remote_url")
      || stringField(result.identity_payload, "repo_url")
      || stringField(result.identity_payload, "repo_key")
      || result.binding.root_ref;
  }

  if (result.identity_kind === "p4_workspace") {
    const server = stringField(result.identity_payload, "server_address");
    const stream = stringField(result.identity_payload, "stream");
    const client = stringField(result.identity_payload, "client_name");
    return [server, stream || client].filter(Boolean).join(" · ") || result.binding.root_ref;
  }

  return stringField(result.identity_payload, "path_key") || result.binding.root_ref;
}

interface IdentityFieldsProps {
  identityKind: WorkspaceIdentityKind;
  identityPayload: Record<string, unknown>;
  onPayloadChange: (payload: Record<string, unknown>) => void;
}

function IdentityFields({
  identityKind,
  identityPayload,
  onPayloadChange,
}: IdentityFieldsProps) {
  const fieldValue = (key: string) => {
    const value = identityPayload[key];
    return typeof value === "string" ? value : "";
  };

  if (identityKind === "git_repo") {
    return (
      <div className="grid gap-3 md:grid-cols-2">
        <input
          value={fieldValue("repo_key")}
          onChange={(event) => onPayloadChange(updatePayloadField(identityPayload, "repo_key", event.target.value))}
          placeholder="repo_key 或 remote_url"
          className="agentdash-form-input"
        />
        <input
          value={fieldValue("branch")}
          onChange={(event) => onPayloadChange(updatePayloadField(identityPayload, "branch", event.target.value))}
          placeholder="branch，可选"
          className="agentdash-form-input"
        />
      </div>
    );
  }

  if (identityKind === "p4_workspace") {
    return (
      <div className="grid gap-3 md:grid-cols-2">
        <input
          value={fieldValue("server_address")}
          onChange={(event) => onPayloadChange(updatePayloadField(identityPayload, "server_address", event.target.value))}
          placeholder="P4 server_address"
          className="agentdash-form-input"
        />
        <input
          value={fieldValue("stream")}
          onChange={(event) => onPayloadChange(updatePayloadField(identityPayload, "stream", event.target.value))}
          placeholder="stream"
          className="agentdash-form-input"
        />
        <input
          value={fieldValue("client_name")}
          onChange={(event) => onPayloadChange(updatePayloadField(identityPayload, "client_name", event.target.value))}
          placeholder="client_name"
          className="agentdash-form-input"
        />
        <input
          value={fieldValue("path_key")}
          onChange={(event) => onPayloadChange(updatePayloadField(identityPayload, "path_key", event.target.value))}
          placeholder="path_key"
          className="agentdash-form-input"
        />
      </div>
    );
  }

  return (
    <input
      value={fieldValue("path_key")}
      onChange={(event) => onPayloadChange(updatePayloadField(identityPayload, "path_key", event.target.value))}
      placeholder="path_key，例如 d:/workspaces/app"
      className="agentdash-form-input"
    />
  );
}

interface CandidateListProps {
  candidates: WorkspaceInventoryCandidate[];
  backends: BackendConfig[];
  selectedKey?: string | null;
  emptyText?: string;
  onSelect?: (candidate: WorkspaceInventoryCandidate) => void;
  onAddBinding?: (candidate: WorkspaceInventoryCandidate) => void;
}

function candidateKey(candidate: WorkspaceInventoryCandidate): string {
  return `${candidate.backend_id}:${candidate.root_ref}`;
}

function CandidateList({
  candidates,
  backends,
  selectedKey,
  emptyText = "暂无未匹配的可用目录发现项。可以使用本机目录识别登记新的可用目录。",
  onSelect,
  onAddBinding,
}: CandidateListProps) {
  if (candidates.length === 0) {
    return (
      <p className="rounded-[8px] border border-dashed border-border px-3 py-3 text-xs text-muted-foreground">
        {emptyText}
      </p>
    );
  }

  return (
    <div className="space-y-2">
      {candidates.map((candidate) => {
        const key = candidateKey(candidate);
        const active = selectedKey === key;
        return (
          <div
            key={key}
            className={`rounded-[8px] border px-3 py-3 ${
              active ? "border-primary/30 bg-primary/[0.04]" : "border-border bg-background"
            }`}
          >
            <div className="flex flex-wrap items-start justify-between gap-3">
              <div className="min-w-0">
                <div className="flex flex-wrap items-center gap-2">
                  <span className="rounded-[8px] border border-border bg-secondary/40 px-2 py-0.5 text-[10px] text-muted-foreground">
                    {identityKindLabels[candidate.identity_kind]}
                  </span>
                  <span className="truncate font-mono text-xs text-foreground">{candidate.root_ref}</span>
                </div>
                <p className="mt-1 text-xs text-muted-foreground">
                  {backendDisplayName(backends, candidate.backend_id)} · {candidate.reason}
                </p>
                <p className="mt-1 truncate text-xs text-muted-foreground">
                  {identitySummary(candidate.identity_kind, candidate.identity_payload)}
                </p>
              </div>
              <div className="flex shrink-0 gap-2">
                {onSelect && (
                  <button
                    type="button"
                    onClick={() => onSelect(candidate)}
                    className="rounded-[8px] border border-border bg-background px-3 py-1.5 text-xs text-muted-foreground hover:bg-secondary"
                  >
                    {active ? "已选择" : "选择"}
                  </button>
                )}
                {onAddBinding && (
                  <button
                    type="button"
                    onClick={() => onAddBinding(candidate)}
                    className="agentdash-button-secondary text-xs"
                  >
                    添加落点
                  </button>
                )}
              </div>
            </div>
          </div>
        );
      })}
    </div>
  );
}

interface WorkspaceEditorDrawerProps {
  open: boolean;
  projectId: string;
  mode: "create" | "detail";
  workspace: Workspace | null;
  candidates: WorkspaceInventoryCandidate[];
  accesses: ProjectBackendAccess[];
  canManageBindings: boolean;
  onClose: () => void;
  onSetDefault?: (workspaceId: string | null) => void;
  onCandidatesChanged: () => void | Promise<void>;
  onInventoryChanged?: () => void | Promise<void>;
}

export function WorkspaceEditorDrawer({
  open,
  projectId,
  mode,
  workspace,
  candidates,
  accesses,
  canManageBindings,
  onClose,
  onSetDefault,
  onCandidatesChanged,
  onInventoryChanged,
}: WorkspaceEditorDrawerProps) {
  const {
    createWorkspace,
    deleteWorkspace,
    detectWorkspace,
    error,
    updateWorkspace,
  } = useWorkspaceStore();
  const backends = useCoordinatorStore((state) => state.backends);
  const selectableBackends = useMemo(() => authorizedBackends(backends, accesses), [accesses, backends]);
  const localBackends = useMemo(() => localAuthorizedBackends(backends, accesses), [accesses, backends]);
  const fallbackDetectBackends = localBackends.length > 0 ? localBackends : selectableBackends;
  const visibleCreateModes = useMemo<CreateMode[]>(
    () => canManageBindings ? ["candidate", "logical", "local_detect"] : ["candidate", "logical"],
    [canManageBindings],
  );
  const initialBinding = workspace ? findWorkspaceBinding(workspace) : null;
  const [createMode, setCreateMode] = useState<CreateMode>("candidate");
  const [selectedCandidateKey, setSelectedCandidateKey] = useState<string | null>(null);
  const [name, setName] = useState(workspace?.name ?? "");
  const [identityKind, setIdentityKind] = useState<WorkspaceIdentityKind>(
    workspace?.identity_kind ?? "git_repo",
  );
  const [identityPayload, setIdentityPayload] = useState<Record<string, unknown>>(
    workspace?.identity_payload ?? emptyPayload("git_repo"),
  );
  const [payloadText, setPayloadText] = useState(normalizePayloadText(workspace?.identity_payload ?? emptyPayload("git_repo")));
  const resolutionPolicy: WorkspaceResolutionPolicy = "prefer_online";
  const [defaultBindingId, setDefaultBindingId] = useState<string | null>(
    workspace?.default_binding_id ?? workspace?.bindings[0]?.id ?? null,
  );
  const [bindings, setBindings] = useState<WorkspaceBindingInput[]>(
    workspace?.bindings.map(bindingToInput) ?? [],
  );
  const [detectBackendId, setDetectBackendId] = useState(initialBinding?.backend_id ?? fallbackDetectBackends[0]?.id ?? "");
  const [detectRootRef, setDetectRootRef] = useState(initialBinding?.root_ref ?? "");
  const [detectionResult, setDetectionResult] = useState<WorkspaceDetectionResult | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [isDetectingDirectory, setIsDetectingDirectory] = useState(false);
  const [isRegisteringInventory, setIsRegisteringInventory] = useState(false);
  const [mountCapabilities, setMountCapabilities] = useState<ContextContainerCapability[]>(
    workspace?.mount_capabilities ?? [...ALL_CAPABILITIES],
  );
  const [isDeleteConfirmOpen, setIsDeleteConfirmOpen] = useState(false);
  const [deleteConfirmValue, setDeleteConfirmValue] = useState("");
  const [isDetectBrowseOpen, setIsDetectBrowseOpen] = useState(false);
  const [setAsDefault, setSetAsDefault] = useState(false);
  const effectiveDetectBackendId = detectBackendId || fallbackDetectBackends[0]?.id || "";
  const existingBindingKeys = useMemo(
    () => new Set(workspace?.bindings.map((binding) => bindingDraftKey(binding)) ?? []),
    [workspace],
  );
  const candidateBindingKeys = useMemo(
    () => new Set(candidates.map(candidateDraftKey)),
    [candidates],
  );

  const bindingsAllowedForCurrentUser = () => {
    if (canManageBindings) return true;
    return bindings.every((binding) => {
      const key = bindingDraftKey(binding);
      return existingBindingKeys.has(key) || candidateBindingKeys.has(key);
    });
  };

  const syncPayload = (payload: Record<string, unknown>) => {
    setIdentityPayload(payload);
    setPayloadText(normalizePayloadText(payload));
  };

  const handleIdentityKindChange = (nextKind: WorkspaceIdentityKind) => {
    setIdentityKind(nextKind);
    syncPayload(emptyPayload(nextKind));
  };

  const handleSelectCandidate = (candidate: WorkspaceInventoryCandidate) => {
    const draft = candidateToDraft(candidate);
    setSelectedCandidateKey(candidateKey(candidate));
    setName(draft.name);
    setIdentityKind(draft.identity_kind);
    syncPayload(draft.identity_payload);
    setBindings([draft.binding]);
    setDefaultBindingId(draft.binding.id ?? null);
    setMessage(null);
  };

  const handleAddCandidateBinding = (candidate: WorkspaceInventoryCandidate) => {
    const binding = candidateToBindingInput(candidate);
    if (bindings.some((item) => bindingDraftKey(item) === bindingDraftKey(binding))) {
      setMessage("这个 backend/root 已经在当前落点中");
      return;
    }
    setBindings((current) => [...current, binding]);
    if (!defaultBindingId) {
      setDefaultBindingId(binding.id ?? null);
    }
    setMessage("已加入落点，保存后生效");
  };

  const handleDetectLocalDirectory = async (rootRefOverride?: string) => {
    const backendId = effectiveDetectBackendId.trim();
    const rootRef = (rootRefOverride ?? detectRootRef).trim();
    if (!backendId || !rootRef) {
      setMessage("请先选择已授权 backend 并填写目录根路径");
      return;
    }
    setIsDetectingDirectory(true);
    setMessage(null);
    try {
      const detected = await detectWorkspace(projectId, backendId, rootRef);
      if (!detected) return;
      setDetectionResult(detected);
      setIdentityKind(detected.identity_kind);
      syncPayload(detected.identity_payload);
      const detectedBinding = bindingToInput(detected.binding);
      setBindings([detectedBinding]);
      setDefaultBindingId(detectedBinding.id ?? null);
      if (!name.trim()) {
        setName(buildDefaultWorkspaceName(detected.identity_kind, detected.binding.root_ref));
      }
      setMessage(null);
    } finally {
      setIsDetectingDirectory(false);
    }
  };

  const handleDetectInventoryRegistration = async (rootRefOverride?: string) => {
    const backendId = effectiveDetectBackendId.trim();
    const rootRef = (rootRefOverride ?? detectRootRef).trim();
    if (!backendId || !rootRef) {
      setMessage("请先选择已授权 backend 并填写目录根路径");
      return;
    }
    setIsDetectingDirectory(true);
    setMessage(null);
    try {
      const detected = await detectWorkspace(projectId, backendId, rootRef);
      if (!detected) return;
      setDetectionResult(detected);
      setMessage(null);
    } finally {
      setIsDetectingDirectory(false);
    }
  };

  const handleDetectPathCommitted = (path: string) => {
    const normalizedPath = path.trim();
    setDetectRootRef(normalizedPath);
    if (!normalizedPath) return;
    if (mode === "create" && createMode === "local_detect") {
      void handleDetectLocalDirectory(normalizedPath);
      return;
    }
    void handleDetectInventoryRegistration(normalizedPath);
  };

  const handleDetectRootBlur = () => {
    if (!detectRootRef.trim() || detectionResult?.binding.root_ref === detectRootRef.trim()) return;
    handleDetectPathCommitted(detectRootRef);
  };

  const handleRegisterInventory = async () => {
    const backendId = effectiveDetectBackendId.trim();
    const rootRef = (detectionResult?.binding.root_ref ?? detectRootRef).trim();
    if (!backendId || !rootRef) {
      setMessage("请先选择已授权 backend 并识别或填写目录");
      return;
    }
    const access = accesses.find((item) => item.backend_id === backendId && item.status === "active");
    if (!access) {
      setMessage("当前 Project 尚未授权这个 backend，无法登记 inventory");
      return;
    }

    setIsRegisteringInventory(true);
    setMessage(null);
    try {
      await registerBackendWorkspaceInventory(projectId, access.id, { root_ref: rootRef });
      await onCandidatesChanged();
      await onInventoryChanged?.();
      setMessage("已登记为可用目录，可从候选项确认落点");
    } catch (registerError) {
      setMessage((registerError as Error).message);
    } finally {
      setIsRegisteringInventory(false);
    }
  };

  const handleSave = async () => {
    const trimmedName = name.trim();
    if (!trimmedName) {
      setMessage("请填写工作空间名称");
      return;
    }
    if (mode === "create" && createMode === "candidate" && bindings.length === 0) {
      setMessage("请先选择一个发现项");
      return;
    }
    if (!bindingsAllowedForCurrentUser()) {
      setMessage("当前权限只能从既有候选项确认落点，不能登记新的可用目录");
      return;
    }

    const normalizedBindings = dedupeBindings(bindings);
    if (normalizedBindings.length !== bindings.length) {
      setBindings(normalizedBindings);
    }

    setMessage(null);

    if (mode === "create") {
      const created = await createWorkspace(projectId, trimmedName, {
        identity_kind: identityKind,
        identity_payload: identityPayload,
        resolution_policy: resolutionPolicy,
        bindings: normalizedBindings,
        mount_capabilities: mountCapabilities,
      });
      if (!created) return;
      if (setAsDefault && onSetDefault) {
        onSetDefault(created.id);
      }
      onCandidatesChanged();
      onClose();
      return;
    }

    if (!workspace) return;
    const updated = await updateWorkspace(workspace.id, projectId, {
      name: trimmedName,
      identity_kind: identityKind,
      identity_payload: identityPayload,
      resolution_policy: resolutionPolicy,
      default_binding_id: defaultBindingId,
      bindings: normalizedBindings,
      mount_capabilities: mountCapabilities,
    });
    if (!updated) return;

    onCandidatesChanged();
    onClose();
  };

  const handleDelete = async () => {
    if (!workspace) return;
    if (deleteConfirmValue.trim() !== workspace.name) {
      setMessage("请输入完整工作空间名称后再删除");
      return;
    }
    await deleteWorkspace(workspace.id, projectId);
    setIsDeleteConfirmOpen(false);
    onClose();
  };

  const resolutionSummary = workspace
    ? summarizeResolution(workspace, backends, accesses)
    : null;
  const selectedBindingSummary = detectedFactsSummary(findWorkspaceBinding({
    id: workspace?.id ?? "draft",
    project_id: projectId,
    name,
    identity_kind: identityKind,
    identity_payload: identityPayload,
    resolution_policy: resolutionPolicy,
    default_binding_id: defaultBindingId,
    status: workspace?.status ?? "pending",
    bindings: bindings.map((binding) => ({
      id: binding.id ?? `${binding.backend_id}:${binding.root_ref}`,
      workspace_id: workspace?.id ?? "draft",
      backend_id: binding.backend_id,
      root_ref: binding.root_ref,
      status: binding.status ?? "pending",
      detected_facts: binding.detected_facts ?? {},
      priority: binding.priority ?? 0,
      created_at: "",
      updated_at: "",
    })),
    mount_capabilities: mountCapabilities,
    created_at: "",
    updated_at: "",
  }));

  return (
    <>
      <DetailPanel
        open={open}
        title={mode === "create" ? "新建 Workspace" : "Workspace 详情"}
        subtitle={mode === "create"
          ? "选择创建路径，再确认 Workspace 身份与运行落点"
          : workspace
            ? `ID: ${workspace.id}`
            : undefined}
        onClose={onClose}
        widthClassName="max-w-5xl"
        headerExtra={mode === "detail" && workspace ? (
          <DetailMenu
            items={[{
              key: "delete",
              label: "删除 Workspace",
              danger: true,
              onSelect: () => setIsDeleteConfirmOpen(true),
            }]}
          />
        ) : undefined}
      >
        <div className="space-y-4 p-5">
          {mode === "create" && (
            <DetailSection
              title="创建入口"
              description={canManageBindings
                ? "默认使用已登记目录的发现项；管理员可识别本机目录并登记为可用目录。"
                : "默认使用已登记目录的发现项；当前权限只能从既有候选项创建带落点的 Workspace。"}
            >
              <div className="grid gap-2 md:grid-cols-3">
                {visibleCreateModes.map((value) => (
                  <button
                    key={value}
                    type="button"
                    onClick={() => setCreateMode(value)}
                    className={`rounded-[8px] border px-3 py-2 text-left text-xs transition-colors ${
                      createMode === value
                        ? "border-primary/35 bg-primary/10 text-primary"
                        : "border-border bg-background text-muted-foreground hover:bg-secondary"
                    }`}
                  >
                    {createModeLabels[value]}
                  </button>
                ))}
              </div>

              {createMode === "candidate" && (
                <p className="mt-3 rounded-[8px] border border-border bg-background px-3 py-3 text-xs text-muted-foreground">
                  在下方 Backend 路由中选择一个发现项，确认后会生成 Workspace 身份和初始落点。
                </p>
              )}

              {createMode === "logical" && (
                <p className="mt-3 rounded-[8px] border border-border bg-background px-3 py-3 text-xs text-muted-foreground">
                  先创建 logical identity，后续由已授权 backend 的可用目录自动匹配 binding。适合先建 Project contract，再让设备补齐可用目录。
                </p>
              )}

              {canManageBindings && createMode === "local_detect" && (
                <div className="mt-3 space-y-3">
                  <div className="grid gap-3 md:grid-cols-[200px_minmax(0,1fr)]">
                    <select
                      value={effectiveDetectBackendId}
                      onChange={(event) => setDetectBackendId(event.target.value)}
                      className="agentdash-form-select"
                    >
                      <option value="">选择已授权 backend</option>
                      {fallbackDetectBackends.map((backend) => (
                        <option key={backend.id} value={backend.id}>
                          {backend.name} {backend.backend_type === "local" ? "(本机)" : "(远程)"}
                        </option>
                      ))}
                    </select>

                    <div className="flex gap-1.5">
                      <input
                        value={detectRootRef}
                        onChange={(event) => setDetectRootRef(event.target.value)}
                        onBlur={handleDetectRootBlur}
                        onKeyDown={(event) => {
                          if (event.key === "Enter") {
                            event.preventDefault();
                            handleDetectPathCommitted(detectRootRef);
                          }
                        }}
                        placeholder="选择或填写 backend 上的目录"
                        className="agentdash-form-input min-w-0 flex-1"
                      />
                      <button
                        type="button"
                        onClick={() => setIsDetectBrowseOpen(true)}
                        disabled={!effectiveDetectBackendId}
                        className="shrink-0 rounded-[8px] border border-border bg-background px-2.5 py-2 text-xs text-muted-foreground hover:bg-secondary disabled:cursor-not-allowed disabled:opacity-40"
                      >
                        浏览
                      </button>
                    </div>
                  </div>
                  <p className="text-[11px] text-muted-foreground">
                    {isDetectingDirectory ? "正在识别目录..." : "选择目录后会自动识别；手动填写时按 Enter 或离开输入框确认。"}
                  </p>

                  {fallbackDetectBackends.length === 0 && (
                    <p className="rounded-[8px] border border-dashed border-border px-3 py-3 text-xs text-muted-foreground">
                      当前没有已授权且在线的 backend。请先在 Backend Access 中授权本机 backend。
                    </p>
                  )}

                  {detectionResult && (
                    <div className="flex flex-wrap items-start justify-between gap-3 rounded-[8px] border border-border bg-background px-3 py-3 text-xs text-muted-foreground">
                      <div className="min-w-0 flex-1">
                        <p>
                          识别结果：{identityKindLabels[detectionResult.identity_kind]}
                          <span className="text-muted-foreground/60"> · </span>
                          <span className="font-mono text-foreground">
                            {detectionPrimaryText(detectionResult)}
                          </span>
                        </p>
                        <p className="mt-1">
                          解析目录：<span className="font-mono text-foreground">{detectionResult.binding.root_ref}</span>
                        </p>
                        {detectionResult.matched_workspace_ids.length > 0 && (
                          <p className="mt-1 text-warning">
                            检测到可能匹配的已有 Workspace：{detectionResult.matched_workspace_ids.join(", ")}
                          </p>
                        )}
                        {detectionResult.warnings.map((warning) => (
                          <p key={warning} className="mt-1 text-warning">{warning}</p>
                        ))}
                      </div>
                      <div className="shrink-0">
                        <button
                          type="button"
                          onClick={() => void handleRegisterInventory()}
                          disabled={isRegisteringInventory}
                          className="agentdash-button-secondary disabled:cursor-not-allowed disabled:opacity-50"
                        >
                          {isRegisteringInventory ? "登记中..." : "登记为可用目录"}
                        </button>
                      </div>
                    </div>
                  )}
                </div>
              )}
            </DetailSection>
          )}

          <DetailSection
            title="Workspace 身份"
            description="描述这个工作空间代表哪个仓库、P4 workspace 或本地目录。"
          >
            <div className="grid gap-3 md:grid-cols-2">
              <input
                value={name}
                onChange={(event) => setName(event.target.value)}
                placeholder="Workspace 名称"
                className="agentdash-form-input"
              />

              <select
                value={identityKind}
                onChange={(event) => handleIdentityKindChange(event.target.value as WorkspaceIdentityKind)}
                className="agentdash-form-select"
              >
                {Object.entries(identityKindLabels).map(([value, label]) => (
                  <option key={value} value={value}>{label}</option>
                ))}
              </select>
            </div>

            <IdentityFields
              identityKind={identityKind}
              identityPayload={identityPayload}
              onPayloadChange={syncPayload}
            />
            <p className="text-xs text-muted-foreground">
              当前摘要：{identitySummary(identityKind, identityPayload)}
            </p>

            <details className="rounded-[8px] border border-border bg-background px-3 py-3">
              <summary className="cursor-pointer text-xs font-medium text-muted-foreground">
                高级 identity JSON
              </summary>
              <textarea
                value={payloadText}
                onChange={(event) => {
                  setPayloadText(event.target.value);
                  try {
                    const parsed = JSON.parse(event.target.value);
                    if (typeof parsed === "object" && parsed !== null && !Array.isArray(parsed)) {
                      setIdentityPayload(parsed as Record<string, unknown>);
                      setMessage(null);
                    } else {
                      setMessage("identity_payload 需要是 JSON object");
                    }
                  } catch {
                    setMessage("identity_payload 需要是合法 JSON");
                  }
                }}
                className="mt-3 min-h-[132px] w-full rounded-[8px] border border-border bg-background px-3 py-2 font-mono text-xs text-foreground"
              />
            </details>
          </DetailSection>

          {mode === "detail" && resolutionSummary && (
            <DetailSection
              title="当前路由预览"
              description="展示保存后的配置会落到哪个 backend/root。"
            >
              <div className="rounded-[8px] border border-border bg-background px-3 py-3">
                <div className="flex flex-wrap items-center gap-2">
                  <ResolutionBadge state={resolutionSummary.state} />
                  <span className="text-sm font-medium text-foreground">{resolutionSummary.label}</span>
                </div>
                <p className="mt-1 text-xs text-muted-foreground">{resolutionSummary.description}</p>
                {resolutionSummary.warnings.length > 0 && (
                  <div className="mt-2 space-y-1">
                    {resolutionSummary.warnings.slice(0, 3).map((warning) => (
                      <p key={warning} className="text-xs text-warning">{warning}</p>
                    ))}
                  </div>
                )}
              </div>
            </DetailSection>
          )}

          <DetailSection
            title="Backend 路由"
            description={canManageBindings
              ? "选择这个 Workspace 可以落到哪些 backend/root；运行时会优先使用在线 backend。"
              : "当前权限只能从既有候选项确认落点。"}
          >
            <div className="space-y-4">
              <div className="space-y-2">
                <p className="text-xs font-medium text-foreground">当前落点</p>
                {dedupeBindings(bindings).length === 0 ? (
                  <p className="rounded-[8px] border border-dashed border-border px-3 py-3 text-xs text-muted-foreground">
                    当前还没有可用落点。请从下方候选项确认一个 backend/root。
                  </p>
                ) : (
                  dedupeBindings(bindings).map((binding) => (
                    <div
                      key={binding.id ?? `${binding.backend_id}:${binding.root_ref}`}
                      className="flex flex-wrap items-center justify-between gap-3 rounded-[8px] border border-border bg-background px-3 py-3 text-xs"
                    >
                      <div className="min-w-0 flex-1">
                        <p className="truncate text-sm font-medium text-foreground">
                          {backendDisplayName(backends, binding.backend_id)}
                        </p>
                        <p className="mt-1 truncate font-mono text-xs text-muted-foreground">
                          {binding.root_ref || "未填写 root"}
                        </p>
                      </div>
                      <div className="flex shrink-0 items-center gap-3">
                        <BindingStatusBadge status={binding.status ?? "pending"} />
                      </div>
                    </div>
                  ))
                )}
                {selectedBindingSummary && (
                  <p className="text-xs text-muted-foreground">{selectedBindingSummary}</p>
                )}
              </div>

              <div className="space-y-2">
                <p className="text-xs font-medium text-foreground">可确认候选项</p>
                <CandidateList
                  candidates={candidates}
                  backends={backends}
                  emptyText={canManageBindings
                    ? "暂无未匹配的可用目录发现项。可以使用本机目录识别登记新的可用目录。"
                    : "暂无可确认的目录候选项。请让管理员先登记可用目录。"}
                  onAddBinding={mode === "detail" ? handleAddCandidateBinding : undefined}
                  selectedKey={selectedCandidateKey}
                  onSelect={mode === "create" ? handleSelectCandidate : undefined}
                />
              </div>

              {canManageBindings ? (
                <details className="rounded-[8px] border border-border bg-background px-3 py-3">
                  <summary className="cursor-pointer text-xs font-medium text-foreground">
                    登记新的可用目录
                  </summary>
                  <p className="mt-2 text-xs text-muted-foreground">
                    仅管理员可使用。选择目录后自动识别，状态、identity 和 detected_facts 都来自 backend detect 结果。
                  </p>
                  <div className="mt-3 space-y-3">
                    <div className="grid gap-3 md:grid-cols-[200px_minmax(0,1fr)]">
                      <select
                        value={effectiveDetectBackendId}
                        onChange={(event) => setDetectBackendId(event.target.value)}
                        className="agentdash-form-select"
                      >
                        <option value="">选择已授权 backend</option>
                        {fallbackDetectBackends.map((backend) => (
                          <option key={backend.id} value={backend.id}>
                            {backend.name} {backend.backend_type === "local" ? "(本机)" : "(远程)"}
                          </option>
                        ))}
                      </select>

                      <div className="flex gap-1.5">
                        <input
                          value={detectRootRef}
                          onChange={(event) => setDetectRootRef(event.target.value)}
                          onBlur={handleDetectRootBlur}
                          onKeyDown={(event) => {
                            if (event.key === "Enter") {
                              event.preventDefault();
                              handleDetectPathCommitted(detectRootRef);
                            }
                          }}
                          placeholder="选择或填写 backend 上的目录"
                          className="agentdash-form-input min-w-0 flex-1"
                        />
                        <button
                          type="button"
                          onClick={() => setIsDetectBrowseOpen(true)}
                          disabled={!effectiveDetectBackendId}
                          className="shrink-0 rounded-[8px] border border-border bg-background px-2.5 py-2 text-xs text-muted-foreground hover:bg-secondary disabled:cursor-not-allowed disabled:opacity-40"
                        >
                          浏览
                        </button>
                      </div>
                    </div>
                    <p className="text-[11px] text-muted-foreground">
                      {isDetectingDirectory ? "正在识别目录..." : "选择目录后会自动识别；手动填写时按 Enter 或离开输入框确认。"}
                    </p>

                    {detectionResult && (
                      <div className="flex flex-wrap items-start justify-between gap-3 rounded-[8px] border border-border bg-muted/20 px-3 py-3 text-xs text-muted-foreground">
                        <div className="min-w-0 flex-1">
                          <p>
                            识别结果：{identityKindLabels[detectionResult.identity_kind]}
                            <span className="text-muted-foreground/60"> · </span>
                            <span className="font-mono text-foreground">
                              {detectionPrimaryText(detectionResult)}
                            </span>
                          </p>
                          <p className="mt-1">
                            解析目录：<span className="font-mono text-foreground">{detectionResult.binding.root_ref}</span>
                          </p>
                        </div>
                        <div className="shrink-0">
                          <button
                            type="button"
                            onClick={() => void handleRegisterInventory()}
                            disabled={isRegisteringInventory}
                            className="agentdash-button-secondary disabled:cursor-not-allowed disabled:opacity-50"
                          >
                            {isRegisteringInventory ? "登记中..." : "登记为可用目录"}
                          </button>
                        </div>
                      </div>
                    )}
                  </div>
                </details>
              ) : (
                <p className="rounded-[8px] border border-border bg-muted/25 px-3 py-3 text-xs text-muted-foreground">
                  无管理员权限时不开放新的可用目录登记入口。
                </p>
              )}
            </div>
          </DetailSection>

          <DetailSection title="挂载能力">
            <div className="flex flex-wrap gap-1.5">
              {CONTEXT_CAPABILITY_OPTIONS.map((opt) => {
                const active = mountCapabilities.includes(opt.value);
                return (
                  <button
                    key={opt.value}
                    type="button"
                    onClick={() => {
                      setMountCapabilities((prev) =>
                        active
                          ? prev.filter((c) => c !== opt.value)
                          : [...prev, opt.value],
                      );
                    }}
                    className={`rounded-[8px] border px-2.5 py-1 text-[11px] font-medium transition-colors ${
                      active
                        ? "border-primary/30 bg-primary/10 text-primary"
                        : "border-border bg-background text-muted-foreground hover:border-primary/25 hover:bg-primary/5"
                    }`}
                  >
                    {opt.label}
                  </button>
                );
              })}
            </div>
          </DetailSection>

          {(message || error) && (
            <p className="text-xs text-destructive">{message || error}</p>
          )}

          <div className="flex items-center justify-between border-t border-border pt-3">
            {mode === "create" && onSetDefault ? (
              <label className="flex items-center gap-2 text-xs text-muted-foreground">
                <input
                  type="checkbox"
                  checked={setAsDefault}
                  onChange={(event) => setSetAsDefault(event.target.checked)}
                  className="rounded border-border"
                />
                创建后设为 Project 默认 Workspace
              </label>
            ) : (
              <div />
            )}
            <button
              type="button"
              onClick={() => void handleSave()}
              className="agentdash-button-primary"
            >
              {mode === "create" ? "创建 Workspace" : "保存变更"}
            </button>
          </div>
        </div>

        <DirectoryBrowserDialog
          open={isDetectBrowseOpen}
          backendId={effectiveDetectBackendId}
          initialPath={detectRootRef || undefined}
          onSelect={handleDetectPathCommitted}
          onClose={() => setIsDetectBrowseOpen(false)}
        />
      </DetailPanel>

      {workspace && (
        <DangerConfirmDialog
          open={isDeleteConfirmOpen}
          title="删除 Workspace"
          description="删除后将无法恢复。"
          expectedValue={workspace.name}
          inputValue={deleteConfirmValue}
          onInputValueChange={setDeleteConfirmValue}
          confirmLabel="确认删除"
          onClose={() => {
            setIsDeleteConfirmOpen(false);
            setDeleteConfirmValue("");
          }}
          onConfirm={() => void handleDelete()}
        />
      )}
    </>
  );
}
