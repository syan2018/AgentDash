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
} from "../../types";
import { ALL_CAPABILITIES, CONTEXT_CAPABILITY_OPTIONS } from "../../components/context-config-defaults";
import {
  type WorkspaceBindingInput,
  findWorkspaceBinding,
  useWorkspaceStore,
} from "../../stores/workspaceStore";
import { useCoordinatorStore } from "../../stores/coordinatorStore";
import {
  DangerConfirmDialog,
  DetailMenu,
  DetailPanel,
  DetailSection,
} from "../../components/ui/detail-panel";
import {
  listProjectBackendAccess,
  listWorkspaceInventoryCandidates,
  registerBackendWorkspaceInventory,
} from "../../services/backendAccess";
import { DirectoryBrowserDialog } from "./directory-browser-dialog";
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
} from "./model/workspaceRouting";

type CreateMode = "candidate" | "logical" | "local_detect";

const statusConfig: Record<WorkspaceStatus, { label: string; cls: string }> = {
  pending: { label: "待完善", cls: "border-border bg-secondary text-muted-foreground" },
  preparing: { label: "准备中", cls: "border-info/20 bg-info/10 text-info" },
  ready: { label: "可解析", cls: "border-success/20 bg-success/10 text-success" },
  active: { label: "使用中", cls: "border-primary/20 bg-primary/10 text-primary" },
  archived: { label: "已归档", cls: "border-border bg-secondary text-muted-foreground" },
  error: { label: "异常", cls: "border-destructive/20 bg-destructive/10 text-destructive" },
};

const bindingStatusLabels: Record<WorkspaceBindingStatus, string> = {
  pending: "待校验",
  ready: "可用",
  offline: "离线",
  error: "异常",
};

const resolutionPolicyLabels: Record<WorkspaceResolutionPolicy, string> = {
  prefer_default_binding: "优先默认 binding",
  prefer_online: "优先在线 backend",
};

const createModeLabels: Record<CreateMode, string> = {
  candidate: "从发现项创建",
  logical: "创建逻辑 Workspace",
  local_detect: "本机目录识别",
};

function WorkspaceStatusBadge({ status }: { status: WorkspaceStatus }) {
  const config = statusConfig[status];
  return (
    <span className={`inline-flex items-center rounded-full border px-2.5 py-1 text-[10px] font-medium ${config.cls}`}>
      {config.label}
    </span>
  );
}

function BindingStatusBadge({ status }: { status: WorkspaceBindingStatus }) {
  return (
    <span className="rounded-full border border-border bg-background px-2 py-0.5 text-[10px] text-muted-foreground">
      {bindingStatusLabels[status]}
    </span>
  );
}

function ResolutionBadge({ state }: { state: "resolved" | "warning" | "blocked" }) {
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

interface WorkspaceBindingEditorProps {
  bindings: WorkspaceBindingInput[];
  defaultBindingId: string | null;
  selectableBackends: BackendConfig[];
  onChange: (bindings: WorkspaceBindingInput[]) => void;
  onDefaultBindingChange: (bindingId: string | null) => void;
}

function WorkspaceBindingEditor({
  bindings,
  defaultBindingId,
  selectableBackends,
  onChange,
  onDefaultBindingChange,
}: WorkspaceBindingEditorProps) {
  const [browseIndex, setBrowseIndex] = useState<number | null>(null);

  const updateBinding = (index: number, patch: Partial<WorkspaceBindingInput>) => {
    onChange(bindings.map((binding, itemIndex) => (
      itemIndex === index ? { ...binding, ...patch } : binding
    )));
  };

  const removeBinding = (index: number) => {
    const next = bindings.filter((_, itemIndex) => itemIndex !== index);
    onChange(next);
    const removed = bindings[index];
    if (removed?.id && removed.id === defaultBindingId) {
      onDefaultBindingChange(next[0]?.id ?? null);
    }
  };

  const browseBinding = browseIndex !== null ? bindings[browseIndex] : null;

  return (
    <div className="space-y-3">
      {bindings.length === 0 && (
        <p className="rounded-[10px] border border-dashed border-border px-3 py-3 text-xs text-muted-foreground">
          当前还没有 binding。可以等待 backend inventory 自动匹配，或通过本机目录识别/发现项补充。
        </p>
      )}

      {bindings.map((binding, index) => (
        <div
          key={binding.id ?? `${binding.backend_id}-${binding.root_ref}-${index}`}
          className="rounded-[10px] border border-border bg-background px-3 py-3"
        >
          <div className="grid gap-3 md:grid-cols-[1fr_1.4fr_120px_96px_auto]">
            <select
              value={binding.backend_id}
              onChange={(event) => updateBinding(index, { backend_id: event.target.value })}
              className="agentdash-form-select"
            >
              <option value="">选择 backend</option>
              {selectableBackends.map((backend) => (
                <option key={backend.id} value={backend.id}>
                  {backend.name} {backend.online ? "(online)" : "(offline)"}
                </option>
              ))}
            </select>

            <div className="flex gap-1.5">
              <input
                value={binding.root_ref}
                onChange={(event) => updateBinding(index, { root_ref: event.target.value })}
                placeholder="backend 上的目录根路径"
                className="agentdash-form-input min-w-0 flex-1"
              />
              <button
                type="button"
                onClick={() => setBrowseIndex(index)}
                disabled={!binding.backend_id}
                title={binding.backend_id ? "浏览目录" : "请先选择 backend"}
                className="shrink-0 rounded-[8px] border border-border bg-background px-2.5 py-2 text-xs text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground disabled:cursor-not-allowed disabled:opacity-40"
              >
                浏览
              </button>
            </div>

            <select
              value={binding.status ?? "pending"}
              onChange={(event) => updateBinding(index, {
                status: event.target.value as WorkspaceBindingStatus,
              })}
              className="agentdash-form-select"
            >
              <option value="pending">待校验</option>
              <option value="ready">可用</option>
              <option value="offline">离线</option>
              <option value="error">异常</option>
            </select>

            <label className="flex items-center gap-2 rounded-[8px] border border-border px-3 py-2 text-xs text-foreground">
              <input
                type="radio"
                checked={defaultBindingId === (binding.id ?? null)}
                onChange={() => onDefaultBindingChange(binding.id ?? null)}
              />
              默认
            </label>

            <button
              type="button"
              onClick={() => removeBinding(index)}
              className="rounded-[8px] border border-destructive/25 bg-destructive/5 px-3 py-2 text-xs text-destructive transition-colors hover:bg-destructive/10"
            >
              删除
            </button>
          </div>
        </div>
      ))}

      <button
        type="button"
        onClick={() => onChange([
          ...bindings,
          {
            id: crypto.randomUUID(),
            backend_id: selectableBackends[0]?.id ?? "",
            root_ref: "",
            status: "pending",
            detected_facts: {},
            priority: 0,
          },
        ])}
        disabled={selectableBackends.length === 0}
        className="rounded-[8px] border border-border bg-background px-3 py-2 text-xs text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground disabled:cursor-not-allowed disabled:opacity-50"
      >
        + 手工添加 binding
      </button>

      {browseBinding && (
        <DirectoryBrowserDialog
          open={browseIndex !== null}
          backendId={browseBinding.backend_id}
          initialPath={browseBinding.root_ref || undefined}
          onSelect={(path) => {
            if (browseIndex !== null) {
              updateBinding(browseIndex, { root_ref: path });
            }
          }}
          onClose={() => setBrowseIndex(null)}
        />
      )}
    </div>
  );
}

interface CandidateListProps {
  candidates: WorkspaceInventoryCandidate[];
  backends: BackendConfig[];
  selectedKey?: string | null;
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
  onSelect,
  onAddBinding,
}: CandidateListProps) {
  if (candidates.length === 0) {
    return (
      <p className="rounded-[10px] border border-dashed border-border px-3 py-3 text-xs text-muted-foreground">
        暂无未匹配的 backend inventory 发现项。可以刷新 Inventory 或使用本机目录识别。
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
            className={`rounded-[10px] border px-3 py-3 ${
              active ? "border-primary/30 bg-primary/[0.04]" : "border-border bg-background"
            }`}
          >
            <div className="flex flex-wrap items-start justify-between gap-3">
              <div className="min-w-0">
                <div className="flex flex-wrap items-center gap-2">
                  <span className="rounded-full border border-border bg-secondary/40 px-2 py-0.5 text-[10px] text-muted-foreground">
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
                    添加 binding
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
  onClose: () => void;
  onSetDefault?: (workspaceId: string | null) => void;
  onCandidatesChanged: () => void | Promise<void>;
  onInventoryChanged?: () => void | Promise<void>;
}

function WorkspaceEditorDrawer({
  open,
  projectId,
  mode,
  workspace,
  candidates,
  accesses,
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
    updateStatus,
    updateWorkspace,
  } = useWorkspaceStore();
  const { backends } = useCoordinatorStore();
  const selectableBackends = useMemo(() => authorizedBackends(backends, accesses), [accesses, backends]);
  const localBackends = useMemo(() => localAuthorizedBackends(backends, accesses), [accesses, backends]);
  const fallbackDetectBackends = localBackends.length > 0 ? localBackends : selectableBackends;
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
  const [resolutionPolicy, setResolutionPolicy] = useState<WorkspaceResolutionPolicy>(
    workspace?.resolution_policy ?? "prefer_online",
  );
  const [defaultBindingId, setDefaultBindingId] = useState<string | null>(
    workspace?.default_binding_id ?? workspace?.bindings[0]?.id ?? null,
  );
  const [bindings, setBindings] = useState<WorkspaceBindingInput[]>(
    workspace?.bindings.map(bindingToInput) ?? [],
  );
  const [workspaceStatus, setWorkspaceStatus] = useState<WorkspaceStatus>(
    workspace?.status ?? "pending",
  );
  const [detectBackendId, setDetectBackendId] = useState(initialBinding?.backend_id ?? fallbackDetectBackends[0]?.id ?? "");
  const [detectRootRef, setDetectRootRef] = useState(initialBinding?.root_ref ?? "");
  const [detectionResult, setDetectionResult] = useState<WorkspaceDetectionResult | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [isRegisteringInventory, setIsRegisteringInventory] = useState(false);
  const [mountCapabilities, setMountCapabilities] = useState<ContextContainerCapability[]>(
    workspace?.mount_capabilities ?? [...ALL_CAPABILITIES],
  );
  const [isDeleteConfirmOpen, setIsDeleteConfirmOpen] = useState(false);
  const [deleteConfirmValue, setDeleteConfirmValue] = useState("");
  const [isDetectBrowseOpen, setIsDetectBrowseOpen] = useState(false);
  const [setAsDefault, setSetAsDefault] = useState(false);
  const effectiveDetectBackendId = detectBackendId || fallbackDetectBackends[0]?.id || "";

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
    setBindings((current) => [...current, binding]);
    if (!defaultBindingId) {
      setDefaultBindingId(binding.id ?? null);
    }
    setMessage("已加入 binding，保存后生效");
  };

  const handleDetectLocalDirectory = async () => {
    const backendId = effectiveDetectBackendId.trim();
    const rootRef = detectRootRef.trim();
    if (!backendId || !rootRef) {
      setMessage("请先选择已授权 backend 并填写目录根路径");
      return;
    }
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
      setMessage("已登记到 Backend Inventory，可从发现项创建或同步 binding");
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

    setMessage(null);

    if (mode === "create") {
      const created = await createWorkspace(projectId, trimmedName, {
        identity_kind: identityKind,
        identity_payload: identityPayload,
        resolution_policy: resolutionPolicy,
        bindings,
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
      bindings,
      mount_capabilities: mountCapabilities,
    });
    if (!updated) return;

    if (workspace.status !== workspaceStatus) {
      await updateStatus(workspace.id, workspaceStatus);
    }
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
    status: workspaceStatus,
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
          ? "选择创建路径，再确认 logical identity 与 binding"
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
              description="默认使用 backend inventory 的发现项；个人本机用户可以直接使用本机目录识别。"
            >
              <div className="grid gap-2 md:grid-cols-3">
                {Object.entries(createModeLabels).map(([value, label]) => (
                  <button
                    key={value}
                    type="button"
                    onClick={() => setCreateMode(value as CreateMode)}
                    className={`rounded-[10px] border px-3 py-2 text-left text-xs transition-colors ${
                      createMode === value
                        ? "border-primary/35 bg-primary/10 text-primary"
                        : "border-border bg-background text-muted-foreground hover:bg-secondary"
                    }`}
                  >
                    {label}
                  </button>
                ))}
              </div>

              {createMode === "candidate" && (
                <div className="mt-3">
                  <CandidateList
                    candidates={candidates}
                    backends={backends}
                    selectedKey={selectedCandidateKey}
                    onSelect={handleSelectCandidate}
                  />
                </div>
              )}

              {createMode === "logical" && (
                <p className="mt-3 rounded-[10px] border border-border bg-background px-3 py-3 text-xs text-muted-foreground">
                  先创建 logical identity，后续由已授权 backend inventory 自动匹配 binding。适合先建 Project contract，再让设备补齐可用目录。
                </p>
              )}

              {createMode === "local_detect" && (
                <div className="mt-3 space-y-3">
                  <div className="grid gap-3 md:grid-cols-[200px_minmax(0,1fr)_auto]">
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

                    <button
                      type="button"
                      onClick={() => void handleDetectLocalDirectory()}
                      disabled={!effectiveDetectBackendId || !detectRootRef.trim()}
                      className="agentdash-button-secondary disabled:cursor-not-allowed disabled:opacity-50"
                    >
                      识别目录
                    </button>
                  </div>

                  {fallbackDetectBackends.length === 0 && (
                    <p className="rounded-[10px] border border-dashed border-border px-3 py-3 text-xs text-muted-foreground">
                      当前没有已授权且在线的 backend。请先在 Backend Access 中授权本机 backend。
                    </p>
                  )}

                  <DirectoryBrowserDialog
                    open={isDetectBrowseOpen}
                    backendId={effectiveDetectBackendId}
                    initialPath={detectRootRef || undefined}
                    onSelect={(path) => setDetectRootRef(path)}
                    onClose={() => setIsDetectBrowseOpen(false)}
                  />

                  {detectionResult && (
                    <div className="rounded-[10px] border border-border bg-background px-3 py-3 text-xs text-muted-foreground">
                      <p>
                        识别结果：{identityKindLabels[detectionResult.identity_kind]} · confidence: {detectionResult.confidence}
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
                      <div className="mt-3 flex flex-wrap gap-2">
                        <button
                          type="button"
                          onClick={() => void handleRegisterInventory()}
                          disabled={isRegisteringInventory}
                          className="agentdash-button-secondary disabled:cursor-not-allowed disabled:opacity-50"
                        >
                          {isRegisteringInventory ? "登记中..." : "登记到 Backend Inventory"}
                        </button>
                        <span className="self-center text-[11px] text-muted-foreground">
                          登记后会进入发现项和自动 sync 流程。
                        </span>
                      </div>
                    </div>
                  )}
                </div>
              )}
            </DetailSection>
          )}

          <DetailSection
            title="Identity"
            description="维护 logical workspace contract；它描述 Workspace 是谁，而不是固定某台 backend。"
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

              <select
                value={resolutionPolicy}
                onChange={(event) => setResolutionPolicy(event.target.value as WorkspaceResolutionPolicy)}
                className="agentdash-form-select"
              >
                {Object.entries(resolutionPolicyLabels).map(([value, label]) => (
                  <option key={value} value={value}>{label}</option>
                ))}
              </select>

              {mode === "detail" && (
                <select
                  value={workspaceStatus}
                  onChange={(event) => setWorkspaceStatus(event.target.value as WorkspaceStatus)}
                  className="agentdash-form-select"
                >
                  <option value="pending">待完善</option>
                  <option value="ready">可解析</option>
                  <option value="active">使用中</option>
                  <option value="archived">已归档</option>
                  <option value="error">异常</option>
                </select>
              )}
            </div>

            <IdentityFields
              identityKind={identityKind}
              identityPayload={identityPayload}
              onPayloadChange={syncPayload}
            />
            <p className="text-xs text-muted-foreground">
              当前摘要：{identitySummary(identityKind, identityPayload)}
            </p>

            <details className="rounded-[10px] border border-border bg-background px-3 py-3">
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
                className="mt-3 min-h-[132px] w-full rounded-[10px] border border-border bg-background px-3 py-2 font-mono text-xs text-foreground"
              />
            </details>
          </DetailSection>

          {mode === "detail" && resolutionSummary && (
            <DetailSection
              title="Resolution"
              description="展示当前 Project 会如何解析这个 logical Workspace。"
            >
              <div className="rounded-[10px] border border-border bg-background px-3 py-3">
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
            title="Bindings"
            description="已确认的 backend/root 落点。默认 binding 是 Workspace 内部默认，不等于 Project 默认 Workspace。"
          >
            <div className="space-y-2">
              {bindings.length > 0 && bindings.map((binding) => (
                <div
                  key={binding.id ?? `${binding.backend_id}:${binding.root_ref}`}
                  className="grid gap-2 rounded-[10px] border border-border bg-background px-3 py-3 text-xs md:grid-cols-[minmax(0,1fr)_120px_96px]"
                >
                  <div className="min-w-0">
                    <p className="truncate font-medium text-foreground">
                      {backendDisplayName(backends, binding.backend_id)} @ {binding.root_ref || "未填写 root"}
                    </p>
                    <p className="mt-1 truncate text-muted-foreground">
                      priority {binding.priority ?? 0}
                    </p>
                  </div>
                  <BindingStatusBadge status={binding.status ?? "pending"} />
                  <span className="text-muted-foreground">
                    {defaultBindingId === (binding.id ?? null) ? "Workspace 默认" : ""}
                  </span>
                </div>
              ))}
              {selectedBindingSummary && (
                <p className="text-xs text-muted-foreground">{selectedBindingSummary}</p>
              )}
            </div>
          </DetailSection>

          <DetailSection
            title="Candidates"
            description="来自 backend inventory 的未匹配发现项，可用于创建 Workspace 或补充 binding。"
          >
            <CandidateList
              candidates={candidates}
              backends={backends}
              onAddBinding={mode === "detail" ? handleAddCandidateBinding : undefined}
              selectedKey={selectedCandidateKey}
              onSelect={mode === "create" ? handleSelectCandidate : undefined}
            />
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

          <DetailSection
            title="Advanced Maintenance"
            description="仅用于维护当前 Workspace 的 binding，不会登记或修改 backend inventory。"
          >
            <WorkspaceBindingEditor
              bindings={bindings}
              defaultBindingId={defaultBindingId}
              selectableBackends={selectableBackends}
              onChange={setBindings}
              onDefaultBindingChange={setDefaultBindingId}
            />
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

interface WorkspaceListProps {
  projectId: string;
  workspaces: Workspace[];
  defaultWorkspaceId?: string | null;
  onSetDefault?: (workspaceId: string | null) => void;
  onInventoryChanged?: () => void | Promise<void>;
}

export function WorkspaceList({
  projectId,
  workspaces,
  defaultWorkspaceId,
  onSetDefault,
  onInventoryChanged,
}: WorkspaceListProps) {
  const { backends, fetchBackends } = useCoordinatorStore();
  const [isCreateOpen, setIsCreateOpen] = useState(false);
  const [selectedWorkspace, setSelectedWorkspace] = useState<Workspace | null>(null);
  const [accesses, setAccesses] = useState<ProjectBackendAccess[]>([]);
  const [candidates, setCandidates] = useState<WorkspaceInventoryCandidate[]>([]);
  const [loadError, setLoadError] = useState<string | null>(null);
  const workspaceRefreshSignature = useMemo(
    () => workspaces.map((workspace) => `${workspace.id}:${workspace.updated_at}`).join("|"),
    [workspaces],
  );

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
  }, [fetchBackends, loadRoutingInputs, workspaceRefreshSignature]);

  const handleToggleDefault = (workspaceId: string, event: MouseEvent) => {
    event.stopPropagation();
    if (!onSetDefault) return;
    onSetDefault(defaultWorkspaceId === workspaceId ? null : workspaceId);
  };

  return (
    <>
      <div className="space-y-3">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div>
            <p className="text-[11px] uppercase tracking-[0.14em] text-muted-foreground">逻辑 Workspace</p>
            <p className="text-xs text-muted-foreground">
              Workspace 表达 identity，binding 表达已授权 backend/root 的运行时落点。
            </p>
          </div>
          <button
            type="button"
            onClick={() => setIsCreateOpen(true)}
            className="rounded-[8px] border border-border bg-background px-3 py-2 text-xs text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
          >
            + 新建 Workspace
          </button>
        </div>

        {loadError && (
          <p className="rounded-[10px] border border-destructive/35 bg-destructive/10 px-3 py-2 text-xs text-destructive">
            {loadError}
          </p>
        )}

        {workspaces.length === 0 && (
          <div className="rounded-[12px] border border-dashed border-border px-4 py-4 text-sm text-muted-foreground">
            <p>当前还没有 logical Workspace。</p>
            <p className="mt-1 text-xs">
              可以从 backend inventory 发现项创建，或使用本机目录识别快速从本机 backend 添加。
            </p>
          </div>
        )}

        {workspaces.map((workspace) => {
          const availability = summarizeAvailability(workspace, backends, accesses);
          const resolution = summarizeResolution(workspace, backends, accesses);
          const primaryBinding = resolution.binding ?? findWorkspaceBinding(workspace);
          const factSummary = detectedFactsSummary(primaryBinding);
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
                      <span className="inline-flex items-center rounded-full border border-primary/25 bg-primary/10 px-2.5 py-0.5 text-[10px] font-medium text-primary">
                        Project 默认
                      </span>
                    )}
                    <WorkspaceStatusBadge status={workspace.status} />
                    <span className="rounded-full border border-border bg-background px-2 py-0.5 text-[10px] text-muted-foreground">
                      {identityKindLabels[workspace.identity_kind]}
                    </span>
                    <ResolutionBadge state={resolution.state} />
                  </div>
                  <p className="mt-1 truncate text-xs text-muted-foreground">
                    Identity: {identitySummary(workspace.identity_kind, workspace.identity_payload)}
                  </p>
                  <p className="mt-1 truncate text-xs text-muted-foreground">
                    Resolution: {resolution.label} · {resolution.description}
                  </p>
                  {factSummary && (
                    <p className="mt-1 truncate text-xs text-muted-foreground">
                      Facts: {factSummary}
                    </p>
                  )}
                  {resolution.warnings.length > 0 && (
                    <p className="mt-1 truncate text-xs text-warning">
                      {resolution.warnings[0]}
                    </p>
                  )}
                </div>

                <div className="flex shrink-0 flex-col items-end gap-2">
                  <div className="text-right">
                    <p className="text-[11px] uppercase tracking-[0.14em] text-muted-foreground">
                      bindings
                    </p>
                    <p className="text-sm font-medium text-foreground">
                      {availability.online}/{availability.total}
                    </p>
                    <p className="text-[10px] text-muted-foreground">
                      ready {availability.ready} · authorized {availability.authorized}
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
                      onClick={() => setSelectedWorkspace(workspace)}
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
        candidates={candidates}
        accesses={accesses}
        onClose={() => setIsCreateOpen(false)}
        onSetDefault={onSetDefault}
        onCandidatesChanged={loadRoutingInputs}
        onInventoryChanged={onInventoryChanged}
      />

      <WorkspaceEditorDrawer
        key={`workspace-detail-${selectedWorkspace?.id ?? "none"}`}
        open={Boolean(selectedWorkspace)}
        projectId={projectId}
        mode="detail"
        workspace={selectedWorkspace}
        candidates={candidates}
        accesses={accesses}
        onClose={() => setSelectedWorkspace(null)}
        onSetDefault={onSetDefault}
        onCandidatesChanged={loadRoutingInputs}
        onInventoryChanged={onInventoryChanged}
      />
    </>
  );
}
