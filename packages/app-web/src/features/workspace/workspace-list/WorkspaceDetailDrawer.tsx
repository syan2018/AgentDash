import { useMemo, useState } from "react";
import type {
  ContextContainerCapability,
  ProjectBackendAccess,
  Workspace,
  WorkspaceIdentityKind,
  WorkspaceInventoryCandidate,
  WorkspaceResolutionPolicy,
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
  authorizedBackends,
  backendDisplayName,
  bindingToInput,
  candidateToBindingInput,
  detectedFactsSummary,
  identitySummary,
  localAuthorizedBackends,
  summarizeResolution,
} from "../model/workspaceRouting";
import { IDENTITY_KIND_LABELS, TERMS } from "../model/workspaceTerms";
import { BindingStatusBadge, ResolutionBadge } from "./badges";
import { CandidateList } from "./CandidateList";
import { DirectoryDetector } from "./DirectoryDetector";
import { IdentityFields } from "./IdentityFields";
import {
  type Feedback,
  bindingDraftKey,
  candidateDraftKey,
  dedupeBindings,
  emptyPayload,
  normalizePayloadText,
} from "./editorHelpers";

interface WorkspaceDetailDrawerProps {
  open: boolean;
  projectId: string;
  workspace: Workspace;
  candidates: WorkspaceInventoryCandidate[];
  accesses: ProjectBackendAccess[];
  canManageBindings: boolean;
  onClose: () => void;
  onCandidatesChanged: () => void | Promise<void>;
  onInventoryChanged?: () => void | Promise<void>;
}

const RESOLUTION_POLICY: WorkspaceResolutionPolicy = "prefer_online";

export function WorkspaceDetailDrawer({
  open,
  projectId,
  workspace,
  candidates,
  accesses,
  canManageBindings,
  onClose,
  onCandidatesChanged,
  onInventoryChanged,
}: WorkspaceDetailDrawerProps) {
  const updateWorkspace = useWorkspaceStore((state) => state.updateWorkspace);
  const deleteWorkspace = useWorkspaceStore((state) => state.deleteWorkspace);
  const error = useWorkspaceStore((state) => state.error);
  const backends = useCoordinatorStore((state) => state.backends);

  const selectableBackends = useMemo(() => authorizedBackends(backends, accesses), [accesses, backends]);
  const localBackends = useMemo(() => localAuthorizedBackends(backends, accesses), [accesses, backends]);
  const detectBackends = localBackends.length > 0 ? localBackends : selectableBackends;

  const [name, setName] = useState(workspace.name);
  const [identityKind, setIdentityKind] = useState<WorkspaceIdentityKind>(workspace.identity_kind);
  const [identityPayload, setIdentityPayload] = useState<Record<string, unknown>>(
    (workspace.identity_payload as Record<string, unknown> | null) ?? emptyPayload(workspace.identity_kind),
  );
  const [payloadText, setPayloadText] = useState(
    normalizePayloadText((workspace.identity_payload as Record<string, unknown> | null) ?? emptyPayload(workspace.identity_kind)),
  );
  const [defaultBindingId, setDefaultBindingId] = useState<string | null>(
    workspace.default_binding_id ?? workspace.bindings[0]?.id ?? null,
  );
  const [bindings, setBindings] = useState<WorkspaceBindingInput[]>(
    workspace.bindings.map(bindingToInput),
  );
  const [mountCapabilities, setMountCapabilities] = useState<ContextContainerCapability[]>(
    workspace.mount_capabilities ?? [...ALL_CAPABILITIES],
  );
  const [feedback, setFeedback] = useState<Feedback | null>(null);
  const [isDeleteConfirmOpen, setIsDeleteConfirmOpen] = useState(false);
  const [deleteConfirmValue, setDeleteConfirmValue] = useState("");

  // 显式同步目标 workspace 的表单状态（含切换 workspace 或保存后 store 刷新带来的新数据）。
  // 采用 React 官方「渲染期间根据 props 变化重置 state」模式：以 id + updated_at 作为签名，
  // 变化时在渲染中重置各字段，再触发一次重渲染。不再依赖外层 key remount 作为唯一重置手段。
  const workspaceSignature = `${workspace.id}:${workspace.updated_at}`;
  const [syncedSignature, setSyncedSignature] = useState(workspaceSignature);
  if (workspaceSignature !== syncedSignature) {
    const payload = (workspace.identity_payload as Record<string, unknown> | null)
      ?? emptyPayload(workspace.identity_kind);
    setSyncedSignature(workspaceSignature);
    setName(workspace.name);
    setIdentityKind(workspace.identity_kind);
    setIdentityPayload(payload);
    setPayloadText(normalizePayloadText(payload));
    setDefaultBindingId(workspace.default_binding_id ?? workspace.bindings[0]?.id ?? null);
    setBindings(workspace.bindings.map(bindingToInput));
    setMountCapabilities(workspace.mount_capabilities ?? [...ALL_CAPABILITIES]);
    setFeedback(null);
  }

  const existingBindingKeys = useMemo(
    () => new Set(workspace.bindings.map((binding) => bindingDraftKey(binding))),
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

  const handleAddCandidateBinding = (candidate: WorkspaceInventoryCandidate) => {
    const binding = candidateToBindingInput(candidate);
    if (bindings.some((item) => bindingDraftKey(item) === bindingDraftKey(binding))) {
      setFeedback({ tone: "info", text: "这个目录已在当前运行位置中" });
      return;
    }
    setBindings((current) => [...current, binding]);
    if (!defaultBindingId) {
      setDefaultBindingId(binding.id ?? null);
    }
    setFeedback({ tone: "success", text: "已加入运行位置，保存后生效" });
  };

  const handleSave = async () => {
    const trimmedName = name.trim();
    if (!trimmedName) {
      setFeedback({ tone: "error", text: "请填写 Workspace 名称" });
      return;
    }
    if (!bindingsAllowedForCurrentUser()) {
      setFeedback({ tone: "error", text: "当前权限只能从已有可选目录确认运行位置，不能添加新的可选目录" });
      return;
    }

    const normalizedBindings = dedupeBindings(bindings);
    if (normalizedBindings.length !== bindings.length) {
      setBindings(normalizedBindings);
    }
    setFeedback(null);

    const updated = await updateWorkspace(workspace.id, projectId, {
      name: trimmedName,
      identity_kind: identityKind,
      identity_payload: identityPayload,
      resolution_policy: RESOLUTION_POLICY,
      default_binding_id: defaultBindingId,
      bindings: normalizedBindings,
      mount_capabilities: mountCapabilities,
    });
    if (!updated) return;

    // 先 await 刷新（store 已 upsert），再驻留并刷新「运行解析预览」。
    // 把签名对齐到刚保存的结果，避免渲染期同步逻辑清掉这条成功反馈、并保留用户已编辑的字段。
    setSyncedSignature(`${updated.id}:${updated.updated_at}`);
    await onCandidatesChanged();
    setFeedback({ tone: "success", text: "已保存变更，下方运行解析预览已更新。" });
  };

  const handleDelete = async () => {
    if (deleteConfirmValue.trim() !== workspace.name) {
      setFeedback({ tone: "error", text: "请输入完整 Workspace 名称后再删除" });
      return;
    }
    await deleteWorkspace(workspace.id, projectId);
    setIsDeleteConfirmOpen(false);
    onClose();
  };

  const resolutionSummary = summarizeResolution(workspace, backends, accesses);
  const selectedBindingSummary = detectedFactsSummary(findWorkspaceBinding({
    ...workspace,
    name,
    identity_kind: identityKind,
    identity_payload: identityPayload as Workspace["identity_payload"],
    resolution_policy: RESOLUTION_POLICY,
    default_binding_id: defaultBindingId,
    bindings: bindings.map((binding) => ({
      id: binding.id ?? `${binding.backend_id}:${binding.root_ref}`,
      workspace_id: workspace.id,
      backend_id: binding.backend_id,
      root_ref: binding.root_ref,
      status: binding.status ?? "pending",
      detected_facts: (binding.detected_facts ?? {}) as Workspace["identity_payload"],
      last_verified_at: null,
      priority: binding.priority ?? 0,
      created_at: "",
      updated_at: "",
    })),
    mount_capabilities: mountCapabilities,
  }));

  const shownFeedback: Feedback | null = error ? { tone: "error", text: error } : feedback;

  return (
    <>
      <DetailPanel
        open={open}
        title="Workspace 详情"
        subtitle={identitySummary(workspace.identity_kind, workspace.identity_payload)}
        onClose={onClose}
        widthClassName="max-w-5xl"
        headerExtra={(
          <DetailMenu
            items={[{
              key: "delete",
              label: "删除 Workspace",
              danger: true,
              onSelect: () => setIsDeleteConfirmOpen(true),
            }]}
          />
        )}
      >
        <div className="space-y-4 p-5">
          <DetailSection
            title="代码来源"
            description="描述这个 Workspace 代表哪个仓库、P4 工作空间或本地目录。"
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
                {Object.entries(IDENTITY_KIND_LABELS).map(([value, label]) => (
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
              当前代码来源：{identitySummary(identityKind, identityPayload)}
            </p>

            <details className="rounded-[8px] border border-border bg-background px-3 py-3">
              <summary className="cursor-pointer text-xs font-medium text-muted-foreground">
                高级（开发者）：identity JSON
              </summary>
              <textarea
                value={payloadText}
                onChange={(event) => {
                  setPayloadText(event.target.value);
                  try {
                    const parsed = JSON.parse(event.target.value);
                    if (typeof parsed === "object" && parsed !== null && !Array.isArray(parsed)) {
                      setIdentityPayload(parsed as Record<string, unknown>);
                      setFeedback(null);
                    } else {
                      setFeedback({ tone: "error", text: "identity JSON 需要是一个 JSON 对象" });
                    }
                  } catch {
                    setFeedback({ tone: "error", text: "identity JSON 需要是合法 JSON" });
                  }
                }}
                className="mt-3 min-h-[132px] w-full rounded-[8px] border border-border bg-background px-3 py-2 font-mono text-xs text-foreground"
              />
            </details>
          </DetailSection>

          {resolutionSummary && (
            <DetailSection
              title="运行解析预览"
              description="展示保存后这个 Workspace 会运行在哪个 Backend / 目录。"
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
            title="运行位置"
            description={canManageBindings
              ? "选择这个 Workspace 可以运行在哪些 Backend / 目录；运行时会优先使用在线的 Backend。"
              : "当前权限只能从已有可选目录确认运行位置。"}
          >
            <div className="space-y-4">
              <div className="space-y-2">
                <p className="text-xs font-medium text-foreground">当前运行位置</p>
                {dedupeBindings(bindings).length === 0 ? (
                  <p className="rounded-[8px] border border-dashed border-border px-3 py-3 text-xs text-muted-foreground">
                    当前还没有运行位置。请从下方可选目录确认一个 Backend / 目录。
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
                          {binding.root_ref || "未填写目录"}
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
                <p className="text-xs font-medium text-foreground">可选目录</p>
                <CandidateList
                  candidates={candidates}
                  backends={backends}
                  emptyText={canManageBindings
                    ? "暂无可选目录。可以浏览本机目录添加新的可选目录。"
                    : "暂无可选目录。请让管理员先添加可选目录。"}
                  onAddBinding={handleAddCandidateBinding}
                />
              </div>

              {canManageBindings ? (
                <details className="rounded-[8px] border border-border bg-background px-3 py-3">
                  <summary className="cursor-pointer text-xs font-medium text-foreground">
                    浏览本机目录添加
                  </summary>
                  <p className="mt-2 text-xs text-muted-foreground">
                    仅管理员可使用。选择目录后会自动识别{TERMS.identity}，识别结果可登记为{TERMS.inventory}。
                  </p>
                  <div className="mt-3">
                    <DirectoryDetector
                      projectId={projectId}
                      detectBackends={detectBackends}
                      accesses={accesses}
                      mode="register-inventory"
                      onFeedback={setFeedback}
                      onInventoryRegistered={async () => {
                        await onCandidatesChanged();
                        await onInventoryChanged?.();
                      }}
                    />
                  </div>
                </details>
              ) : (
                <p className="rounded-[8px] border border-border bg-muted/25 px-3 py-3 text-xs text-muted-foreground">
                  无管理员权限时不开放添加新的可选目录。
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

          {shownFeedback && (
            <p className={`text-xs ${
              shownFeedback.tone === "success"
                ? "text-success"
                : shownFeedback.tone === "info"
                  ? "text-muted-foreground"
                  : "text-destructive"
            }`}>
              {shownFeedback.text}
            </p>
          )}

          <div className="flex items-center justify-end border-t border-border pt-3">
            <button
              type="button"
              onClick={() => void handleSave()}
              className="agentdash-button-primary"
            >
              保存变更
            </button>
          </div>
        </div>
      </DetailPanel>

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
    </>
  );
}
