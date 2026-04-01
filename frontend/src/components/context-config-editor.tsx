import { useEffect, useMemo, useState } from "react";
import type {
  ContextContainerCapability,
  ContextContainerDefinition,
  MountDerivationPolicy,
  SessionComposition,
  SessionRequiredContextBlock,
} from "../types";
import { api } from "../api/client";
import { CONTEXT_CAPABILITY_OPTIONS } from "./context-config-defaults";

interface ConfigurableProviderInfo {
  service_id: string;
  display_name: string;
  root_ref_hint: string;
  supported_capabilities: string[];
}

function useMountProviders() {
  const [providers, setProviders] = useState<ConfigurableProviderInfo[]>([]);
  useEffect(() => {
    api.get<ConfigurableProviderInfo[]>("/mount-providers").then(setProviders).catch(() => {});
  }, []);
  return providers;
}

function cloneContainer(
  container: ContextContainerDefinition,
): ContextContainerDefinition {
  return {
    ...container,
    provider:
      container.provider.kind === "inline_files"
        ? {
            kind: "inline_files",
            files: container.provider.files.map((file) => ({ ...file })),
          }
        : {
            kind: "external_service",
            service_id: container.provider.service_id,
            root_ref: container.provider.root_ref,
          },
    capabilities: [...container.capabilities],
    exposure: {
      include_in_project_sessions: container.exposure.include_in_project_sessions ?? true,
      include_in_task_sessions: container.exposure.include_in_task_sessions ?? true,
      include_in_story_sessions: container.exposure.include_in_story_sessions ?? true,
      allowed_agent_types: [...container.exposure.allowed_agent_types],
    },
  };
}

function cloneMountPolicy(policy: MountDerivationPolicy): MountDerivationPolicy {
  return {
    include_local_workspace: policy.include_local_workspace,
    local_workspace_capabilities: [...policy.local_workspace_capabilities],
  };
}

function cloneRequiredBlock(
  block: SessionRequiredContextBlock,
): SessionRequiredContextBlock {
  return {
    title: block.title,
    content: block.content,
  };
}

function cloneSessionComposition(
  composition: SessionComposition,
): SessionComposition {
  return {
    persona_label: composition.persona_label ?? null,
    persona_prompt: composition.persona_prompt ?? null,
    workflow_steps: [...composition.workflow_steps],
    required_context_blocks: composition.required_context_blocks.map(cloneRequiredBlock),
  };
}

function generateNextId(
  domain: string,
  existing: ContextContainerDefinition[],
): string {
  const prefix = `${domain}-ctx-`;
  let max = 0;
  for (const c of existing) {
    if (c.id.startsWith(prefix)) {
      const num = parseInt(c.id.slice(prefix.length), 10);
      if (!isNaN(num) && num > max) max = num;
    }
  }
  return `${prefix}${max + 1}`;
}

function createDefaultContainer(
  domain: string,
  existing: ContextContainerDefinition[],
): ContextContainerDefinition {
  const id = generateNextId(domain, existing);
  return {
    id,
    mount_id: id,
    display_name: "",
    provider: {
      kind: "inline_files",
      files: [{ path: "context.md", content: "" }],
    },
    capabilities: ["read", "write", "list", "search"],
    default_write: false,
    exposure: {
      include_in_project_sessions: true,
      include_in_task_sessions: true,
      include_in_story_sessions: true,
      allowed_agent_types: [],
    },
  };
}

function parseAgentTypeList(value: string): string[] {
  const seen = new Set<string>();
  const parsed: string[] = [];
  for (const item of value.split(/[\n,]/)) {
    const trimmed = item.trim();
    if (!trimmed || seen.has(trimmed)) continue;
    seen.add(trimmed);
    parsed.push(trimmed);
  }
  return parsed;
}

function joinAgentTypeList(value: string[]): string {
  return value.join(", ");
}

function joinWorkflowSteps(value: string[]): string {
  return value.join("\n");
}

function parseWorkflowSteps(value: string): string[] {
  return value
    .split("\n")
    .map((item) => item.trim())
    .filter(Boolean);
}

function getExternalServiceId(
  provider: ContextContainerDefinition["provider"],
): string | null {
  return provider.kind === "external_service" ? provider.service_id : null;
}

function containerSupportsCapability(
  container: ContextContainerDefinition,
  capability: ContextContainerCapability,
  mountProviders: ConfigurableProviderInfo[],
): boolean {
  if (capability === "exec") return false;
  const serviceId = getExternalServiceId(container.provider);
  if (serviceId) {
    const matched = mountProviders.find(
      (p) => p.service_id === serviceId,
    );
    if (matched) {
      return matched.supported_capabilities.includes(capability);
    }
  }
  return true;
}

function updateCapabilityList(
  current: ContextContainerCapability[],
  capability: ContextContainerCapability,
  checked: boolean,
): ContextContainerCapability[] {
  if (checked) {
    return current.includes(capability) ? current : [...current, capability];
  }
  return current.filter((item) => item !== capability);
}

function saveLabel(isSaving: boolean, label: string): string {
  return isSaving ? "保存中…" : label;
}

export interface ContextContainersEditorProps {
  value: ContextContainerDefinition[];
  domain: string;
  isSaving?: boolean;
  addLabel?: string;
  emptyText?: string;
  onSave: (next: ContextContainerDefinition[]) => Promise<unknown>;
}

export function ContextContainersEditor({
  value,
  domain,
  isSaving = false,
  addLabel = "添加容器",
  emptyText = "暂无容器",
  onSave,
}: ContextContainersEditorProps) {
  return (
    <ContextContainersEditorForm
      key={JSON.stringify(value)}
      value={value}
      domain={domain}
      isSaving={isSaving}
      addLabel={addLabel}
      emptyText={emptyText}
      onSave={onSave}
    />
  );
}

function ContextContainersEditorForm({
  value,
  domain,
  isSaving = false,
  addLabel = "添加容器",
  emptyText = "暂无容器",
  onSave,
}: ContextContainersEditorProps) {
  const [draft, setDraft] = useState<ContextContainerDefinition[]>(() => value.map(cloneContainer));
  const mountProviders = useMountProviders();

  const isDirty = useMemo(
    () => JSON.stringify(draft) !== JSON.stringify(value),
    [draft, value],
  );

  const updateContainerAt = (
    index: number,
    updater: (container: ContextContainerDefinition) => ContextContainerDefinition,
  ) => {
    setDraft((current) =>
      current.map((container, currentIndex) =>
        currentIndex === index ? updater(cloneContainer(container)) : container,
      ),
    );
  };

  return (
    <div className="space-y-3">
      {draft.length === 0 && (
        <p className="text-xs text-muted-foreground">{emptyText}</p>
      )}

      {draft.map((container, index) => (
        <ContainerEditorItem
          key={`container-editor-${index}`}
          container={container}
          index={index}
          isSaving={isSaving}
          mountProviders={mountProviders}
          onUpdate={(updater) => updateContainerAt(index, updater)}
          onRemove={() => setDraft((current) => current.filter((_, i) => i !== index))}
        />
      ))}

      <div className="flex flex-wrap items-center gap-2">
        <button
          type="button"
          onClick={() => setDraft((current) => [...current, createDefaultContainer(domain, current)])}
          disabled={isSaving}
          className="rounded-[8px] border border-dashed border-border px-3 py-2 text-xs text-muted-foreground transition-colors hover:border-primary/30 hover:text-foreground disabled:opacity-40"
        >
          + {addLabel}
        </button>

        {isDirty && (
          <>
            <button
              type="button"
              onClick={() => void onSave(draft.map(cloneContainer))}
              disabled={isSaving}
              className="agentdash-button-primary"
            >
              {saveLabel(isSaving, "保存容器配置")}
            </button>
            <button
              type="button"
              onClick={() => setDraft(value.map(cloneContainer))}
              disabled={isSaving}
              className="agentdash-button-secondary"
            >
              还原
            </button>
          </>
        )}
      </div>
    </div>
  );
}

function ContainerEditorItem({
  container,
  index,
  isSaving,
  mountProviders,
  onUpdate,
  onRemove,
}: {
  container: ContextContainerDefinition;
  index: number;
  isSaving: boolean;
  mountProviders: ConfigurableProviderInfo[];
  onUpdate: (updater: (c: ContextContainerDefinition) => ContextContainerDefinition) => void;
  onRemove: () => void;
}) {
  const [showAdvanced, setShowAdvanced] = useState(false);

  return (
    <div className="space-y-3 rounded-[12px] border border-border bg-background/70 p-3">
      <div className="flex items-center justify-between gap-3">
        <p className="text-xs font-medium text-foreground">
          {container.display_name.trim() || `容器 ${index + 1}`}
          <span className="ml-2 rounded-[4px] bg-muted/60 px-1.5 py-0.5 font-mono text-[10px] text-muted-foreground">
            {container.id}
          </span>
        </p>
        <button
          type="button"
          onClick={onRemove}
          disabled={isSaving}
          className="rounded-[8px] border border-destructive/20 bg-destructive/5 px-2 py-1 text-[11px] text-destructive transition-colors hover:bg-destructive/10 disabled:opacity-40"
        >
          删除
        </button>
      </div>

      <input
        value={container.display_name}
        onChange={(e) => onUpdate((c) => ({ ...c, display_name: e.target.value }))}
        placeholder="备注名称（仅作展示标记，不影响容器 ID）"
        className="agentdash-form-input"
      />

      {/* 高级选项折叠区 */}
      <details
        open={showAdvanced}
        onToggle={(e) => setShowAdvanced((e.target as HTMLDetailsElement).open)}
      >
        <summary className="cursor-pointer text-[11px] text-muted-foreground hover:text-foreground">
          高级选项（ID / 能力 / 可见性 / Agent 过滤）
        </summary>
        <div className="mt-2 space-y-3">
          {/* ID / mount_id */}
          <div className="grid gap-2 md:grid-cols-2">
            <div>
              <label className="text-[10px] text-muted-foreground">容器 ID</label>
              <input
                value={container.id}
                onChange={(e) => onUpdate((c) => ({ ...c, id: e.target.value }))}
                placeholder="容器 ID"
                className="agentdash-form-input"
              />
            </div>
            <div>
              <label className="text-[10px] text-muted-foreground">挂载 ID</label>
              <input
                value={container.mount_id}
                onChange={(e) => onUpdate((c) => ({ ...c, mount_id: e.target.value }))}
                placeholder="挂载 ID"
                className="agentdash-form-input"
              />
            </div>
          </div>

          {/* Provider 类型 */}
          <div className="space-y-2">
            <label className="text-[10px] text-muted-foreground">Provider</label>
            <select
              value={
                container.provider.kind === "inline_files"
                  ? "inline_files"
                  : container.provider.service_id || "__external_select__"
              }
              onChange={(e) => {
                const val = e.target.value;
                if (val === "inline_files") {
                  if (container.provider.kind === "inline_files") return;
                  onUpdate((c) => ({
                    ...c,
                    provider: { kind: "inline_files", files: [{ path: "context.md", content: "" }] },
                  }));
                  return;
                }
                if (val === "__external_select__") return;
                const matched = mountProviders.find((p) => p.service_id === val);
                onUpdate((c) => ({
                  ...c,
                  provider: {
                    kind: "external_service",
                    service_id: val,
                    root_ref: c.provider.kind === "external_service" ? c.provider.root_ref : "",
                  },
                  capabilities: matched
                    ? (matched.supported_capabilities as ContextContainerCapability[])
                    : c.capabilities,
                }));
              }}
              disabled={isSaving}
              className="agentdash-form-input text-[11px]"
            >
              <option value="inline_files">inline_files</option>
              {mountProviders.map((p) => (
                <option key={p.service_id} value={p.service_id}>
                  {p.display_name}
                </option>
              ))}
              {/* 已保存的 external_service 但 provider 列表中不存在时，保留原值防止竞态覆盖 */}
              {(() => {
                const serviceId = getExternalServiceId(container.provider);
                if (
                  !serviceId ||
                  mountProviders.some((p) => p.service_id === serviceId)
                ) {
                  return null;
                }
                return (
                  <option value={serviceId}>
                    {serviceId}
                    {mountProviders.length === 0 ? " (加载中…)" : ""}
                  </option>
                );
              })()}
              {mountProviders.length === 0 &&
                container.provider.kind !== "external_service" && (
                  <option value="__external_select__" disabled>
                    （未检测到可用的外部服务）
                  </option>
                )}
            </select>

            {container.provider.kind === "external_service" && (() => {
              const serviceId = getExternalServiceId(container.provider);
              const selectedProvider = mountProviders.find(
                (p) => p.service_id === serviceId,
              );
              return (
                <div>
                  <label className="text-[10px] text-muted-foreground">Root Ref</label>
                  <input
                    value={container.provider.root_ref}
                    onChange={(e) =>
                      onUpdate((c) => ({
                        ...c,
                        provider: {
                          kind: "external_service" as const,
                          service_id: getExternalServiceId(c.provider) ?? "",
                          root_ref: e.target.value,
                        },
                      }))
                    }
                    placeholder={selectedProvider?.root_ref_hint || "请填写引用路径"}
                    className="agentdash-form-input text-[11px]"
                  />
                </div>
              );
            })()}

            {container.provider.kind === "inline_files" && (
              <p className="mt-0.5 rounded-[6px] bg-muted/40 px-2 py-1 font-mono text-[11px] text-foreground">
                {container.provider.files.length} 个文件
              </p>
            )}
          </div>

          {/* 能力 */}
          <div className="flex flex-wrap gap-2">
            {CONTEXT_CAPABILITY_OPTIONS.map((option) => {
              const supported = containerSupportsCapability(container, option.value, mountProviders);
              return (
                <label
                  key={option.value}
                  className={`inline-flex items-center gap-1.5 rounded-[6px] border px-2 py-1 text-[11px] ${
                    supported ? "border-border text-foreground" : "border-border/50 text-muted-foreground/50"
                  }`}
                >
                  <input
                    type="checkbox"
                    checked={container.capabilities.includes(option.value)}
                    disabled={!supported || isSaving}
                    onChange={(e) =>
                      onUpdate((c) => ({
                        ...c,
                        capabilities: updateCapabilityList(c.capabilities, option.value, e.target.checked),
                      }))
                    }
                    className="h-3.5 w-3.5 rounded border-border"
                  />
                  {option.label}
                </label>
              );
            })}
          </div>

          {/* Exposure */}
          <div className="grid gap-2 md:grid-cols-2">
            <div className="space-y-1.5">
              <p className="text-[10px] font-medium text-muted-foreground">可见性</p>
              {(["include_in_project_sessions", "include_in_story_sessions", "include_in_task_sessions"] as const).map((key) => (
                <label key={key} className="flex items-center gap-1.5 text-[11px] text-foreground">
                  <input
                    type="checkbox"
                    checked={container.exposure[key]}
                    onChange={(e) =>
                      onUpdate((c) => ({ ...c, exposure: { ...c.exposure, [key]: e.target.checked } }))
                    }
                    disabled={isSaving}
                    className="h-3.5 w-3.5 rounded border-border"
                  />
                  {key.replace("include_in_", "").replace("_sessions", "")}
                </label>
              ))}
            </div>
            <div className="space-y-1.5">
              <p className="text-[10px] font-medium text-muted-foreground">Agent 过滤</p>
              <input
                value={joinAgentTypeList(container.exposure.allowed_agent_types)}
                onChange={(e) =>
                  onUpdate((c) => ({
                    ...c,
                    exposure: { ...c.exposure, allowed_agent_types: parseAgentTypeList(e.target.value) },
                  }))
                }
                placeholder="留空 = 全部 Agent"
                className="agentdash-form-input text-[11px]"
              />
            </div>
          </div>
        </div>
      </details>
    </div>
  );
}

export interface DisabledContainerIdsEditorProps {
  value: string[];
  availableContainers: ContextContainerDefinition[];
  isSaving?: boolean;
  onSave: (next: string[]) => Promise<unknown>;
}

export function DisabledContainerIdsEditor({
  value,
  availableContainers,
  isSaving = false,
  onSave,
}: DisabledContainerIdsEditorProps) {
  return (
    <DisabledContainerIdsEditorForm
      key={JSON.stringify({
        value,
        available_ids: availableContainers.map((container) => container.id),
      })}
      value={value}
      availableContainers={availableContainers}
      isSaving={isSaving}
      onSave={onSave}
    />
  );
}

function DisabledContainerIdsEditorForm({
  value,
  availableContainers,
  isSaving = false,
  onSave,
}: DisabledContainerIdsEditorProps) {
  const [draft, setDraft] = useState<string[]>(() => [...value]);

  const isDirty = useMemo(
    () => JSON.stringify(draft) !== JSON.stringify(value),
    [draft, value],
  );

  if (availableContainers.length === 0) {
    return (
      <p className="text-xs text-muted-foreground">
        当前 Project 没有可继承容器，因此没有可禁用项。
      </p>
    );
  }

  return (
    <div className="space-y-3">
      <div className="space-y-2">
        {availableContainers.map((container) => {
          const checked = draft.includes(container.id);
          return (
            <label
              key={container.id}
              className={`flex items-start gap-3 rounded-[10px] border px-3 py-2 text-xs ${
                checked
                  ? "border-destructive/25 bg-destructive/5"
                  : "border-border bg-background/70"
              }`}
            >
              <input
                type="checkbox"
                checked={checked}
                disabled={isSaving}
                onChange={(event) =>
                  setDraft((current) =>
                    event.target.checked
                      ? [...current, container.id]
                      : current.filter((item) => item !== container.id),
                  )
                }
                className="mt-0.5 h-4 w-4 rounded border-border"
              />
              <div className="min-w-0 flex-1">
                <div className="flex flex-wrap items-center gap-2">
                  <span className="font-medium text-foreground">{container.display_name}</span>
                  <span className="rounded-[4px] bg-muted px-1.5 py-0.5 text-[10px] text-muted-foreground">
                    {container.id}
                  </span>
                  <span className="rounded-[4px] bg-secondary px-1.5 py-0.5 text-[10px] text-muted-foreground">
                    {container.mount_id}
                  </span>
                </div>
                <p className="mt-1 text-[11px] text-muted-foreground">
                  勾选后会从 Project 默认容器集合中移除该容器。
                </p>
              </div>
            </label>
          );
        })}
      </div>

      {isDirty && (
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={() => void onSave([...draft])}
            disabled={isSaving}
            className="agentdash-button-primary"
          >
            {saveLabel(isSaving, "保存禁用列表")}
          </button>
          <button
            type="button"
            onClick={() => setDraft([...value])}
            disabled={isSaving}
            className="agentdash-button-secondary"
          >
            还原
          </button>
        </div>
      )}
    </div>
  );
}

export interface MountPolicyEditorProps {
  value: MountDerivationPolicy;
  isSaving?: boolean;
  onSave: (next: MountDerivationPolicy) => Promise<unknown>;
}

export function MountPolicyEditor({
  value,
  isSaving = false,
  onSave,
}: MountPolicyEditorProps) {
  return (
    <MountPolicyEditorForm
      key={JSON.stringify(value)}
      value={value}
      isSaving={isSaving}
      onSave={onSave}
    />
  );
}

function MountPolicyEditorForm({
  value,
  isSaving = false,
  onSave,
}: MountPolicyEditorProps) {
  const [draft, setDraft] = useState<MountDerivationPolicy>(() => cloneMountPolicy(value));

  const isDirty = useMemo(
    () => JSON.stringify(draft) !== JSON.stringify(value),
    [draft, value],
  );

  return (
    <div className="space-y-3 rounded-[12px] border border-border bg-background/70 p-3">
      <label className="flex items-center gap-2 text-sm text-foreground">
        <input
          type="checkbox"
          checked={draft.include_local_workspace}
          onChange={(event) =>
            setDraft((current) => ({
              ...current,
              include_local_workspace: event.target.checked,
            }))
          }
          disabled={isSaving}
          className="h-4 w-4 rounded border-border"
        />
        包含本地工作空间
      </label>

      <div className="space-y-2">
        <p className="text-[11px] font-medium uppercase tracking-wide text-muted-foreground/70">
          Local Workspace Capabilities
        </p>
        <div className="flex flex-wrap gap-2">
          {CONTEXT_CAPABILITY_OPTIONS.map((option) => (
            <label
              key={option.value}
              className="inline-flex items-center gap-2 rounded-[8px] border border-border bg-background px-2.5 py-1.5 text-xs text-foreground"
            >
              <input
                type="checkbox"
                checked={draft.local_workspace_capabilities.includes(option.value)}
                disabled={isSaving}
                onChange={(event) =>
                  setDraft((current) => ({
                    ...current,
                    local_workspace_capabilities: updateCapabilityList(
                      current.local_workspace_capabilities,
                      option.value,
                      event.target.checked,
                    ),
                  }))
                }
                className="h-4 w-4 rounded border-border"
              />
              {option.label}
            </label>
          ))}
        </div>
      </div>

      {isDirty && (
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={() => void onSave(cloneMountPolicy(draft))}
            disabled={isSaving}
            className="agentdash-button-primary"
          >
            {saveLabel(isSaving, "保存挂载策略")}
          </button>
          <button
            type="button"
            onClick={() => setDraft(cloneMountPolicy(value))}
            disabled={isSaving}
            className="agentdash-button-secondary"
          >
            还原
          </button>
        </div>
      )}
    </div>
  );
}

export interface SessionCompositionEditorProps {
  value: SessionComposition;
  isSaving?: boolean;
  onSave: (next: SessionComposition) => Promise<unknown>;
}

export function SessionCompositionEditor({
  value,
  isSaving = false,
  onSave,
}: SessionCompositionEditorProps) {
  return (
    <SessionCompositionEditorForm
      key={JSON.stringify(value)}
      value={value}
      isSaving={isSaving}
      onSave={onSave}
    />
  );
}

function SessionCompositionEditorForm({
  value,
  isSaving = false,
  onSave,
}: SessionCompositionEditorProps) {
  const [draft, setDraft] = useState<SessionComposition>(() => cloneSessionComposition(value));
  const [workflowDraft, setWorkflowDraft] = useState(() => joinWorkflowSteps(value.workflow_steps));

  const isDirty = useMemo(
    () =>
      JSON.stringify({
        ...draft,
        workflow_steps: parseWorkflowSteps(workflowDraft),
      }) !== JSON.stringify(value),
    [draft, value, workflowDraft],
  );

  const setRequiredBlocks = (
    updater: (blocks: SessionRequiredContextBlock[]) => SessionRequiredContextBlock[],
  ) => {
    setDraft((current) => ({
      ...current,
      required_context_blocks: updater(current.required_context_blocks.map(cloneRequiredBlock)),
    }));
  };

  return (
    <div className="space-y-3 rounded-[12px] border border-border bg-background/70 p-3">
      <input
        value={draft.persona_label ?? ""}
        onChange={(event) =>
          setDraft((current) => ({
            ...current,
            persona_label: event.target.value,
          }))
        }
        placeholder="Persona 标签，例如：资深前端工程师"
        className="agentdash-form-input"
      />

      <textarea
        value={draft.persona_prompt ?? ""}
        onChange={(event) =>
          setDraft((current) => ({
            ...current,
            persona_prompt: event.target.value,
          }))
        }
        rows={4}
        placeholder="Persona Prompt"
        className="agentdash-form-textarea"
      />

      <textarea
        value={workflowDraft}
        onChange={(event) => setWorkflowDraft(event.target.value)}
        rows={5}
        placeholder="Workflow steps，每行一步"
        className="agentdash-form-textarea font-mono text-xs"
      />

      <div className="space-y-2">
        <div className="flex items-center justify-between gap-3">
          <p className="text-[11px] font-medium uppercase tracking-wide text-muted-foreground/70">
            Required Context Blocks
          </p>
          <button
            type="button"
            onClick={() =>
              setRequiredBlocks((blocks) => [...blocks, { title: "", content: "" }])
            }
            disabled={isSaving}
            className="rounded-[8px] border border-dashed border-border px-2.5 py-1.5 text-[11px] text-muted-foreground transition-colors hover:border-primary/30 hover:text-foreground disabled:opacity-40"
          >
            + 上下文块
          </button>
        </div>

        {draft.required_context_blocks.length === 0 && (
          <p className="text-xs text-muted-foreground">
            没有显式 required_context_blocks 时，运行时只依赖 persona / workflow / mounts。
          </p>
        )}

        {draft.required_context_blocks.map((block, index) => (
          <div
            key={`required-context-block-${index}`}
            className="space-y-2 rounded-[10px] border border-border bg-secondary/20 p-3"
          >
            <div className="flex items-center justify-between gap-2">
              <input
                value={block.title}
                onChange={(event) =>
                  setRequiredBlocks((blocks) =>
                    blocks.map((entry, currentIndex) =>
                      currentIndex === index
                        ? { ...entry, title: event.target.value }
                        : entry,
                    ),
                  )
                }
                placeholder="块标题"
                className="agentdash-form-input"
              />
              <button
                type="button"
                onClick={() =>
                  setRequiredBlocks((blocks) => blocks.filter((_, currentIndex) => currentIndex !== index))
                }
                disabled={isSaving}
                className="rounded-[8px] border border-border px-2 py-1 text-[11px] text-muted-foreground transition-colors hover:text-foreground disabled:opacity-40"
              >
                删除
              </button>
            </div>
            <textarea
              value={block.content}
              onChange={(event) =>
                setRequiredBlocks((blocks) =>
                  blocks.map((entry, currentIndex) =>
                    currentIndex === index
                      ? { ...entry, content: event.target.value }
                      : entry,
                  ),
                )
              }
              rows={4}
              placeholder="块内容"
              className="agentdash-form-textarea"
            />
          </div>
        ))}
      </div>

      {isDirty && (
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={() =>
              void onSave({
                ...cloneSessionComposition(draft),
                workflow_steps: parseWorkflowSteps(workflowDraft),
              })
            }
            disabled={isSaving}
            className="agentdash-button-primary"
          >
            {saveLabel(isSaving, "保存会话编排")}
          </button>
          <button
            type="button"
            onClick={() => {
              setDraft(cloneSessionComposition(value));
              setWorkflowDraft(joinWorkflowSteps(value.workflow_steps));
            }}
            disabled={isSaving}
            className="agentdash-button-secondary"
          >
            还原
          </button>
        </div>
      )}
    </div>
  );
}
