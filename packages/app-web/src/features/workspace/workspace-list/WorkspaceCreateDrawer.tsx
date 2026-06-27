import { useMemo, useState } from "react";
import type {
  ContextContainerCapability,
  ProjectBackendAccess,
  WorkspaceDetectionResult,
  WorkspaceIdentityKind,
  WorkspaceInventoryCandidate,
  WorkspaceResolutionPolicy,
} from "../../../types";
import { ALL_CAPABILITIES, CONTEXT_CAPABILITY_OPTIONS } from "../../../components/context-config-defaults";
import { type WorkspaceBindingInput, useWorkspaceStore } from "../../../stores/workspaceStore";
import { useCoordinatorStore } from "../../../stores/coordinatorStore";
import { DetailPanel, DetailSection } from "@agentdash/ui";
import {
  authorizedBackends,
  backendDisplayName,
  bindingToInput,
  buildDefaultWorkspaceName,
  candidateToDraft,
  identitySummary,
  localAuthorizedBackends,
} from "../model/workspaceRouting";
import { IDENTITY_KIND_LABELS, TERMS } from "../model/workspaceTerms";
import { BindingStatusBadge } from "./badges";
import { CandidateList } from "./CandidateList";
import { DirectoryDetector } from "./DirectoryDetector";
import { IdentityFields } from "./IdentityFields";
import {
  CREATE_MODE_LABELS,
  type CreateMode,
  type Feedback,
  candidateKey,
  dedupeBindings,
  emptyPayload,
  normalizePayloadText,
} from "./editorHelpers";

interface WorkspaceCreateDrawerProps {
  open: boolean;
  projectId: string;
  defaultWorkspaceId?: string | null;
  candidates: WorkspaceInventoryCandidate[];
  accesses: ProjectBackendAccess[];
  canManageBindings: boolean;
  onClose: () => void;
  onSetDefault?: (workspaceId: string | null) => void | Promise<void>;
  onCandidatesChanged: () => void | Promise<void>;
  onInventoryChanged?: () => void | Promise<void>;
}

const RESOLUTION_POLICY: WorkspaceResolutionPolicy = "prefer_online";

export function WorkspaceCreateDrawer({
  open,
  projectId,
  defaultWorkspaceId,
  candidates,
  accesses,
  canManageBindings,
  onClose,
  onSetDefault,
  onCandidatesChanged,
  onInventoryChanged,
}: WorkspaceCreateDrawerProps) {
  const createWorkspace = useWorkspaceStore((state) => state.createWorkspace);
  const error = useWorkspaceStore((state) => state.error);
  const backends = useCoordinatorStore((state) => state.backends);

  const selectableBackends = useMemo(() => authorizedBackends(backends, accesses), [accesses, backends]);
  const localBackends = useMemo(() => localAuthorizedBackends(backends, accesses), [accesses, backends]);
  const detectBackends = localBackends.length > 0 ? localBackends : selectableBackends;

  const visibleCreateModes = useMemo<CreateMode[]>(() => ["from_directory", "logical"], []);

  const [createMode, setCreateMode] = useState<CreateMode>("from_directory");
  const [isLocalDetectOpen, setIsLocalDetectOpen] = useState(false);
  const [selectedCandidateKey, setSelectedCandidateKey] = useState<string | null>(null);
  const [name, setName] = useState("");
  const [identityKind, setIdentityKind] = useState<WorkspaceIdentityKind>("git_repo");
  const [identityPayload, setIdentityPayload] = useState<Record<string, unknown>>(emptyPayload("git_repo"));
  const [payloadText, setPayloadText] = useState(normalizePayloadText(emptyPayload("git_repo")));
  const [bindings, setBindings] = useState<WorkspaceBindingInput[]>([]);
  const [detectionResult, setDetectionResult] = useState<WorkspaceDetectionResult | null>(null);
  const [feedback, setFeedback] = useState<Feedback | null>(null);
  const [mountCapabilities, setMountCapabilities] = useState<ContextContainerCapability[]>([...ALL_CAPABILITIES]);
  const [setAsDefault, setSetAsDefault] = useState(false);
  const shouldSetCreatedAsDefault = (setAsDefault || !defaultWorkspaceId) && Boolean(onSetDefault);

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
    setFeedback(null);
  };

  // 识别成功后填充创建表单，使「用这个目录创建」一步可达。
  const handleDetected = (detected: WorkspaceDetectionResult) => {
    setDetectionResult(detected);
    setIdentityKind(detected.identity_kind);
    syncPayload(detected.identity_payload);
    const detectedBinding = bindingToInput(detected.binding);
    setBindings([detectedBinding]);
    setName((current) => current.trim()
      || buildDefaultWorkspaceName(detected.identity_kind, detected.binding.root_ref));
  };

  // 一步创建：用识别结果直接 createWorkspace，免去「登记 → 回候选区 → 选中 → 保存」多步。
  const handleCreateFromDetection = async () => {
    if (!detectionResult) return;
    const detectedBinding = bindingToInput(detectionResult.binding);
    const draftName = name.trim()
      || buildDefaultWorkspaceName(detectionResult.identity_kind, detectionResult.binding.root_ref);
    setFeedback(null);
    const created = await createWorkspace(projectId, draftName, {
      identity_kind: detectionResult.identity_kind,
      identity_payload: detectionResult.identity_payload as Record<string, unknown>,
      resolution_policy: RESOLUTION_POLICY,
      bindings: [detectedBinding],
      mount_capabilities: mountCapabilities,
    });
    if (!created) return;
    if (shouldSetCreatedAsDefault && onSetDefault) {
      await onSetDefault(created.id);
    }
    await onCandidatesChanged();
    await onInventoryChanged?.();
    onClose();
  };

  const handleSave = async () => {
    const trimmedName = name.trim();
    if (!trimmedName) {
      setFeedback({ tone: "error", text: "请填写 Workspace 名称" });
      return;
    }
    if (createMode === "from_directory" && bindings.length === 0) {
      setFeedback({ tone: "error", text: `请先选择一个${TERMS.inventory}` });
      return;
    }

    const normalizedBindings = dedupeBindings(bindings);
    if (normalizedBindings.length !== bindings.length) {
      setBindings(normalizedBindings);
    }
    setFeedback(null);

    const created = await createWorkspace(projectId, trimmedName, {
      identity_kind: identityKind,
      identity_payload: identityPayload,
      resolution_policy: RESOLUTION_POLICY,
      bindings: normalizedBindings,
      mount_capabilities: mountCapabilities,
    });
    if (!created) return;
    if (shouldSetCreatedAsDefault && onSetDefault) {
      await onSetDefault(created.id);
    }
    await onCandidatesChanged();
    onClose();
  };

  const shownFeedback: Feedback | null = error ? { tone: "error", text: error } : feedback;

  return (
    <DetailPanel
      open={open}
      title="新建 Workspace"
      subtitle="选择创建方式，再确认代码来源与目录绑定"
      onClose={onClose}
      widthClassName="max-w-5xl"
    >
      <div className="space-y-4 p-5">
        <DetailSection
          title="创建方式"
          description={canManageBindings
            ? "默认从可选目录创建；管理员还可以浏览本机目录添加新的可选目录。"
            : "默认从可选目录创建；当前权限只能从已有可选目录创建带目录绑定的 Workspace。"}
        >
          <div className="grid gap-2 md:grid-cols-2">
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
                {CREATE_MODE_LABELS[value]}
              </button>
            ))}
          </div>

          {createMode === "from_directory" && (
            <div className="mt-3 space-y-3">
              <p className="rounded-[8px] border border-border bg-background px-3 py-3 text-xs text-muted-foreground">
                在下方「{TERMS.binding}」中选择一个{TERMS.inventory}，确认后会自动生成{TERMS.identity}和初始{TERMS.binding}。
              </p>

              {canManageBindings && (
                <details
                  open={isLocalDetectOpen}
                  onToggle={(event) => setIsLocalDetectOpen((event.target as HTMLDetailsElement).open)}
                  className="rounded-[8px] border border-border bg-background px-3 py-3"
                >
                  <summary className="cursor-pointer text-xs font-medium text-foreground">
                    找不到？浏览本机目录添加
                  </summary>
                  <p className="mt-2 text-xs text-muted-foreground">
                    仅管理员可使用。选择目录后会自动识别{TERMS.identity}，可直接用识别结果创建 Workspace。
                  </p>
                  <div className="mt-3">
                    <DirectoryDetector
                      projectId={projectId}
                      detectBackends={detectBackends}
                      accesses={accesses}
                      mode="fill-binding"
                      onDetected={handleDetected}
                      onFeedback={setFeedback}
                      onInventoryRegistered={async () => {
                        await onCandidatesChanged();
                        await onInventoryChanged?.();
                      }}
                      renderPrimaryAction={() => (
                        <button
                          type="button"
                          onClick={() => void handleCreateFromDetection()}
                          className="agentdash-button-primary"
                        >
                          用这个目录创建 Workspace
                        </button>
                      )}
                    />
                  </div>
                </details>
              )}
            </div>
          )}

          {createMode === "logical" && (
            <p className="mt-3 rounded-[8px] border border-border bg-background px-3 py-3 text-xs text-muted-foreground">
              先只填写{TERMS.identity}，{TERMS.binding}稍后由已授权 Backend 的{TERMS.inventory}自动匹配。适合先约定好 Project，再让设备补齐{TERMS.binding}。
            </p>
          )}
        </DetailSection>

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

        {createMode === "from_directory" && (
          <DetailSection
            title="目录绑定"
            description="选择这个 Workspace 已确认的 Backend / 目录。"
          >
            <div className="space-y-4">
              <div className="space-y-2">
                <p className="text-xs font-medium text-foreground">当前目录绑定</p>
                {dedupeBindings(bindings).length === 0 ? (
                  <p className="rounded-[8px] border border-dashed border-border px-3 py-3 text-xs text-muted-foreground">
                    当前还没有目录绑定。请从下方可选目录确认一个 Backend / 目录。
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
              </div>

              <div className="space-y-2">
                <p className="text-xs font-medium text-foreground">可选目录</p>
                <CandidateList
                  candidates={candidates}
                  backends={backends}
                  emptyText={canManageBindings
                    ? "暂无可选目录。可以浏览本机目录添加新的可选目录。"
                    : "暂无可选目录。请让管理员先添加可选目录。"}
                  selectedKey={selectedCandidateKey}
                  onSelect={handleSelectCandidate}
                />
              </div>
            </div>
          </DetailSection>
        )}

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

        <div className="flex items-center justify-between border-t border-border pt-3">
          {onSetDefault ? (
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
            创建 Workspace
          </button>
        </div>
      </div>
    </DetailPanel>
  );
}
