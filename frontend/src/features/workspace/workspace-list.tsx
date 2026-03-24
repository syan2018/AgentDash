import { useState } from "react";
import type {
  Workspace,
  WorkspaceBinding,
  WorkspaceBindingStatus,
  WorkspaceDetectionResult,
  WorkspaceIdentityKind,
  WorkspaceResolutionPolicy,
  WorkspaceStatus,
} from "../../types";
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

const identityKindLabels: Record<WorkspaceIdentityKind, string> = {
  git_repo: "Git 仓库",
  p4_workspace: "P4 工作空间",
  local_dir: "本地目录",
};

const resolutionPolicyLabels: Record<WorkspaceResolutionPolicy, string> = {
  prefer_default_binding: "优先默认 binding",
  prefer_online: "优先在线 backend",
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

function rootHintFromPayload(workspace: Workspace): string {
  const value = workspace.identity_payload.root_hint;
  return typeof value === "string" && value.trim() ? value : "-";
}

function gitSummary(binding: WorkspaceBinding | null): string | null {
  if (!binding) return null;
  const git = binding.detected_facts.git;
  if (!git || typeof git !== "object") return null;
  const record = git as Record<string, unknown>;
  if (record.is_repo !== true) return null;
  const branch = typeof record.branch === "string" && record.branch.trim()
    ? record.branch
    : "HEAD";
  const sourceRepo = typeof record.source_repo === "string" && record.source_repo.trim()
    ? record.source_repo
    : binding.root_ref;
  return `${branch} · ${sourceRepo}`;
}

function buildDefaultWorkspaceName(identityKind: WorkspaceIdentityKind, rootRef: string): string {
  const segments = rootRef.replaceAll("\\", "/").split("/").filter(Boolean);
  const tail = segments.at(-1) ?? "workspace";
  if (identityKind === "git_repo") return tail;
  if (identityKind === "p4_workspace") return `${tail}-p4`;
  return tail;
}

interface WorkspaceBindingEditorProps {
  bindings: WorkspaceBindingInput[];
  defaultBindingId: string | null;
  onChange: (bindings: WorkspaceBindingInput[]) => void;
  onDefaultBindingChange: (bindingId: string | null) => void;
}

function WorkspaceBindingEditor({
  bindings,
  defaultBindingId,
  onChange,
  onDefaultBindingChange,
}: WorkspaceBindingEditorProps) {
  const { backends } = useCoordinatorStore();

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

  return (
    <div className="space-y-3">
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
              {backends.map((backend) => (
                <option key={backend.id} value={backend.id}>
                  {backend.name}
                </option>
              ))}
            </select>

            <input
              value={binding.root_ref}
              onChange={(event) => updateBinding(index, { root_ref: event.target.value })}
              placeholder="backend 上的目录根路径"
              className="agentdash-form-input"
            />

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
            backend_id: "",
            root_ref: "",
            status: "pending",
            detected_facts: {},
            priority: 0,
          },
        ])}
        className="rounded-[8px] border border-border bg-background px-3 py-2 text-xs text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
      >
        + 添加 binding
      </button>
    </div>
  );
}

interface WorkspaceEditorDrawerProps {
  open: boolean;
  projectId: string;
  mode: "create" | "detail";
  workspace: Workspace | null;
  onClose: () => void;
}

function WorkspaceEditorDrawer({
  open,
  projectId,
  mode,
  workspace,
  onClose,
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

  const initialBinding = workspace ? findWorkspaceBinding(workspace) : null;
  const [name, setName] = useState(workspace?.name ?? "");
  const [identityKind, setIdentityKind] = useState<WorkspaceIdentityKind>(
    workspace?.identity_kind ?? "git_repo",
  );
  const [identityPayload, setIdentityPayload] = useState<Record<string, unknown>>(
    workspace?.identity_payload ?? {},
  );
  const [resolutionPolicy, setResolutionPolicy] = useState<WorkspaceResolutionPolicy>(
    workspace?.resolution_policy ?? "prefer_online",
  );
  const [defaultBindingId, setDefaultBindingId] = useState<string | null>(
    workspace?.default_binding_id ?? workspace?.bindings[0]?.id ?? null,
  );
  const [bindings, setBindings] = useState<WorkspaceBindingInput[]>(
    workspace?.bindings.map((binding) => ({
      id: binding.id,
      backend_id: binding.backend_id,
      root_ref: binding.root_ref,
      status: binding.status,
      detected_facts: binding.detected_facts,
      priority: binding.priority,
    })) ?? [],
  );
  const [workspaceStatus, setWorkspaceStatus] = useState<WorkspaceStatus>(
    workspace?.status ?? "pending",
  );
  const [shortcutBackendId, setShortcutBackendId] = useState(initialBinding?.backend_id ?? backends[0]?.id ?? "");
  const [shortcutRootRef, setShortcutRootRef] = useState(initialBinding?.root_ref ?? "");
  const [detectionResult, setDetectionResult] = useState<WorkspaceDetectionResult | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [isDeleteConfirmOpen, setIsDeleteConfirmOpen] = useState(false);
  const [deleteConfirmValue, setDeleteConfirmValue] = useState("");

  const handleDetectShortcut = async () => {
    const backendId = shortcutBackendId.trim();
    const rootRef = shortcutRootRef.trim();
    if (!backendId || !rootRef) {
      setMessage("请先选择 backend 并填写目录根路径");
      return;
    }
    const detected = await detectWorkspace(projectId, backendId, rootRef);
    if (!detected) return;
    setDetectionResult(detected);
    setIdentityKind(detected.identity_kind);
    setIdentityPayload(detected.identity_payload);
    setBindings([{
      id: detected.binding.id,
      backend_id: detected.binding.backend_id,
      root_ref: detected.binding.root_ref,
      status: detected.binding.status,
      detected_facts: detected.binding.detected_facts,
      priority: detected.binding.priority,
    }]);
    setDefaultBindingId(detected.binding.id);
    if (!name.trim()) {
      setName(buildDefaultWorkspaceName(detected.identity_kind, detected.binding.root_ref));
    }
    setMessage(null);
  };

  const handleSave = async () => {
    const trimmedName = name.trim();
    if (!trimmedName) {
      setMessage("请填写工作空间名称");
      return;
    }
    if (bindings.length === 0) {
      setMessage("至少需要一个 binding");
      return;
    }

    setMessage(null);

    if (mode === "create") {
      const created = await createWorkspace(projectId, trimmedName, {
        identity_kind: identityKind,
        identity_payload: identityPayload,
        resolution_policy: resolutionPolicy,
        bindings,
      });
      if (!created) return;
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
    });
    if (!updated) return;

    if (workspace.status !== workspaceStatus) {
      await updateStatus(workspace.id, workspaceStatus);
    }
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

  return (
    <>
      <DetailPanel
        open={open}
        title={mode === "create" ? "新建工作空间" : "工作空间详情"}
        subtitle={mode === "create"
          ? "逻辑 Workspace + 物理 Binding"
          : workspace
            ? `ID: ${workspace.id}`
            : undefined}
        onClose={onClose}
        widthClassName="max-w-4xl"
        headerExtra={mode === "detail" && workspace ? (
          <DetailMenu
            items={[{
              key: "delete",
              label: "删除工作空间",
              danger: true,
              onSelect: () => setIsDeleteConfirmOpen(true),
            }]}
          />
        ) : undefined}
      >
        <div className="space-y-4 p-5">
          <DetailSection
            title="快捷入口"
            description="先选 backend 和目录，系统自动识别 logical workspace 身份并生成 binding。"
          >
            <div className="grid gap-3 md:grid-cols-[180px_minmax(0,1fr)_auto]">
              <select
                value={shortcutBackendId}
                onChange={(event) => setShortcutBackendId(event.target.value)}
                className="agentdash-form-select"
              >
                <option value="">选择 backend</option>
                {backends.map((backend) => (
                  <option key={backend.id} value={backend.id}>
                    {backend.name}
                  </option>
                ))}
              </select>

              <input
                value={shortcutRootRef}
                onChange={(event) => setShortcutRootRef(event.target.value)}
                placeholder="backend 上的目录根路径"
                className="agentdash-form-input"
              />

              <button
                type="button"
                onClick={() => void handleDetectShortcut()}
                className="agentdash-button-secondary"
              >
                自动识别
              </button>
            </div>

            {detectionResult && (
              <div className="rounded-[10px] border border-border bg-background px-3 py-3 text-xs text-muted-foreground">
                <p>
                  识别结果：{identityKindLabels[detectionResult.identity_kind]} · confidence: {detectionResult.confidence}
                </p>
                <p className="mt-1">
                  解析目录：<span className="font-mono text-foreground">{detectionResult.binding.root_ref}</span>
                </p>
                {detectionResult.matched_workspace_ids.length > 0 && (
                  <p className="mt-1 text-amber-700">
                    检测到可能匹配的已有 Workspace：{detectionResult.matched_workspace_ids.join(", ")}
                  </p>
                )}
                {detectionResult.warnings.map((warning) => (
                  <p key={warning} className="mt-1 text-amber-700">{warning}</p>
                ))}
              </div>
            )}
          </DetailSection>

          <DetailSection title="逻辑 Workspace">
            <div className="grid gap-3 md:grid-cols-2">
              <input
                value={name}
                onChange={(event) => setName(event.target.value)}
                placeholder="工作空间名称"
                className="agentdash-form-input"
              />

              <select
                value={identityKind}
                onChange={(event) => setIdentityKind(event.target.value as WorkspaceIdentityKind)}
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
                  <option value="preparing">准备中</option>
                  <option value="ready">可解析</option>
                  <option value="active">使用中</option>
                  <option value="archived">已归档</option>
                  <option value="error">异常</option>
                </select>
              )}
            </div>

            <textarea
              value={JSON.stringify(identityPayload, null, 2)}
              onChange={(event) => {
                try {
                  setIdentityPayload(JSON.parse(event.target.value) as Record<string, unknown>);
                  setMessage(null);
                } catch {
                  setMessage("identity_payload 需要是合法 JSON");
                }
              }}
              className="min-h-[132px] w-full rounded-[10px] border border-border bg-background px-3 py-2 font-mono text-xs text-foreground"
            />
          </DetailSection>

          <DetailSection title="Bindings">
            <WorkspaceBindingEditor
              bindings={bindings}
              defaultBindingId={defaultBindingId}
              onChange={setBindings}
              onDefaultBindingChange={setDefaultBindingId}
            />
          </DetailSection>

          {(message || error) && (
            <p className="text-xs text-destructive">{message || error}</p>
          )}

          <div className="flex items-center justify-end border-t border-border pt-3">
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
          title="删除工作空间"
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
}

export function WorkspaceList({ projectId, workspaces }: WorkspaceListProps) {
  const [isCreateOpen, setIsCreateOpen] = useState(false);
  const [selectedWorkspace, setSelectedWorkspace] = useState<Workspace | null>(null);

  return (
    <>
      <div className="space-y-3">
        <div className="flex items-center justify-between">
          <div>
            <p className="text-[11px] uppercase tracking-[0.14em] text-muted-foreground">逻辑 Workspace</p>
            <p className="text-xs text-muted-foreground">
              每个 Workspace 表达“它是谁”，binding 负责表达“它在 backend 上落在哪”。
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

        {workspaces.length === 0 && (
          <p className="rounded-[12px] border border-dashed border-border px-4 py-4 text-sm text-muted-foreground">
            当前还没有逻辑 Workspace。建议先通过 backend + 目录快捷入口创建一个。
          </p>
        )}

        {workspaces.map((workspace) => {
          const primaryBinding = findWorkspaceBinding(workspace);
          return (
            <button
              key={workspace.id}
              type="button"
              onClick={() => setSelectedWorkspace(workspace)}
              className="w-full rounded-[12px] border border-border bg-background px-4 py-4 text-left transition-colors hover:bg-secondary/35"
            >
              <div className="flex items-start justify-between gap-3">
                <div className="min-w-0 flex-1">
                  <div className="flex flex-wrap items-center gap-2">
                    <p className="truncate text-sm font-medium text-foreground">{workspace.name}</p>
                    <WorkspaceStatusBadge status={workspace.status} />
                    <span className="rounded-full border border-border bg-background px-2 py-0.5 text-[10px] text-muted-foreground">
                      {identityKindLabels[workspace.identity_kind]}
                    </span>
                  </div>
                  <p className="mt-1 truncate text-xs text-muted-foreground">
                    identify: {rootHintFromPayload(workspace)}
                  </p>
                  <p className="mt-1 truncate text-xs text-muted-foreground">
                    policy: {resolutionPolicyLabels[workspace.resolution_policy]}
                  </p>
                  {primaryBinding && (
                    <p className="mt-1 truncate text-xs text-muted-foreground">
                      当前主 binding: {primaryBinding.backend_id} @ {primaryBinding.root_ref}
                    </p>
                  )}
                  {gitSummary(primaryBinding) && (
                    <p className="mt-1 truncate text-xs text-muted-foreground">
                      Git: {gitSummary(primaryBinding)}
                    </p>
                  )}
                </div>

                <div className="shrink-0 text-right">
                  <p className="text-[11px] uppercase tracking-[0.14em] text-muted-foreground">
                    bindings
                  </p>
                  <p className="text-sm font-medium text-foreground">{workspace.bindings.length}</p>
                  {primaryBinding && <BindingStatusBadge status={primaryBinding.status} />}
                </div>
              </div>
            </button>
          );
        })}
      </div>

      <WorkspaceEditorDrawer
        key={`workspace-create-${projectId}-${isCreateOpen ? "open" : "closed"}`}
        open={isCreateOpen}
        projectId={projectId}
        mode="create"
        workspace={null}
        onClose={() => setIsCreateOpen(false)}
      />

      <WorkspaceEditorDrawer
        key={`workspace-detail-${selectedWorkspace?.id ?? "none"}`}
        open={Boolean(selectedWorkspace)}
        projectId={projectId}
        mode="detail"
        workspace={selectedWorkspace}
        onClose={() => setSelectedWorkspace(null)}
      />
    </>
  );
}
