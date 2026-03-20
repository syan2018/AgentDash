import { useEffect, useMemo, useState } from "react";
import type {
  ContextContainerCapability,
  ContextContainerDefinition,
  MountDerivationPolicy,
  SessionComposition,
  SessionRequiredContextBlock,
} from "../types";

export const CONTEXT_CAPABILITY_OPTIONS: Array<{
  value: ContextContainerCapability;
  label: string;
}> = [
  { value: "read", label: "读" },
  { value: "write", label: "写" },
  { value: "list", label: "列" },
  { value: "search", label: "搜" },
  { value: "exec", label: "执行" },
];

const READONLY_PROVIDER_HINT = "当前 provider 首轮仅支持只读能力，write / exec / default_write 会被禁用。";

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

function createDefaultContainer(): ContextContainerDefinition {
  return {
    id: "",
    mount_id: "",
    display_name: "",
    provider: {
      kind: "inline_files",
      files: [{ path: "README.md", content: "请填写容器内容" }],
    },
    capabilities: ["read", "list"],
    default_write: false,
    exposure: {
      include_in_project_sessions: true,
      include_in_task_sessions: true,
      include_in_story_sessions: true,
      allowed_agent_types: [],
    },
  };
}

export function createDefaultMountPolicy(): MountDerivationPolicy {
  return {
    include_local_workspace: true,
    local_workspace_capabilities: [],
  };
}

export function createDefaultSessionComposition(): SessionComposition {
  return {
    persona_label: null,
    persona_prompt: null,
    workflow_steps: [],
    required_context_blocks: [],
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

function containerSupportsCapability(
  _container: ContextContainerDefinition,
  capability: ContextContainerCapability,
): boolean {
  if (capability === "write" || capability === "exec") {
    return false;
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
  isSaving?: boolean;
  addLabel?: string;
  emptyText?: string;
  onSave: (next: ContextContainerDefinition[]) => Promise<unknown>;
}

export function ContextContainersEditor({
  value,
  isSaving = false,
  addLabel = "添加容器",
  emptyText = "暂无容器",
  onSave,
}: ContextContainersEditorProps) {
  const [draft, setDraft] = useState<ContextContainerDefinition[]>(() => value.map(cloneContainer));

  useEffect(() => {
    setDraft(value.map(cloneContainer));
  }, [value]);

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
        <div
          key={`container-editor-${index}`}
          className="space-y-3 rounded-[12px] border border-border bg-background/70 p-3"
        >
          <div className="flex items-center justify-between gap-3">
            <div>
              <p className="text-xs font-medium text-foreground">
                {container.display_name.trim() || `容器 ${index + 1}`}
              </p>
              <p className="text-[11px] text-muted-foreground">
                provider / capabilities / exposure 会一起保存
              </p>
            </div>
            <button
              type="button"
              onClick={() => setDraft((current) => current.filter((_, currentIndex) => currentIndex !== index))}
              disabled={isSaving}
              className="rounded-[8px] border border-destructive/20 bg-destructive/5 px-2 py-1 text-[11px] text-destructive transition-colors hover:bg-destructive/10 disabled:opacity-40"
            >
              删除
            </button>
          </div>

          <div className="grid gap-2 md:grid-cols-3">
            <input
              value={container.id}
              onChange={(event) => updateContainerAt(index, (current) => ({
                ...current,
                id: event.target.value,
              }))}
              placeholder="容器 ID"
              className="agentdash-form-input"
            />
            <input
              value={container.mount_id}
              onChange={(event) => updateContainerAt(index, (current) => ({
                ...current,
                mount_id: event.target.value,
              }))}
              placeholder="挂载 ID"
              className="agentdash-form-input"
            />
            <input
              value={container.display_name}
              onChange={(event) => updateContainerAt(index, (current) => ({
                ...current,
                display_name: event.target.value,
              }))}
              placeholder="显示名称"
              className="agentdash-form-input"
            />
          </div>

          <div className="space-y-2 rounded-[10px] border border-border/70 bg-secondary/20 p-3">
            <div className="grid gap-2 md:grid-cols-[180px_minmax(0,1fr)]">
              <select
                value={container.provider.kind}
                onChange={(event) =>
                  updateContainerAt(index, (current) => ({
                    ...current,
                    provider:
                      event.target.value === "external_service"
                        ? {
                            kind: "external_service",
                            service_id: "",
                            root_ref: "",
                          }
                        : {
                            kind: "inline_files",
                            files:
                              current.provider.kind === "inline_files"
                                ? current.provider.files.map((file) => ({ ...file }))
                                : [{ path: "README.md", content: "请填写容器内容" }],
                          },
                    capabilities: current.capabilities.filter((capability) =>
                      containerSupportsCapability(current, capability),
                    ),
                    default_write: false,
                  }))
                }
                className="agentdash-form-select"
              >
                <option value="inline_files">inline_files</option>
                <option value="external_service">external_service</option>
              </select>

              <div className="rounded-[8px] border border-border bg-background/80 px-3 py-2 text-[11px] text-muted-foreground">
                {container.provider.kind === "inline_files"
                  ? "内联文件容器会把文件内容直接注入虚拟 mount。"
                  : "外部服务容器通过 service_id + root_ref 映射到远端 provider。"}
              </div>
            </div>

            {container.provider.kind === "inline_files" ? (
              <div className="space-y-2">
                {container.provider.files.map((file, fileIndex) => (
                  <div
                    key={`inline-file-${fileIndex}`}
                    className="space-y-2 rounded-[10px] border border-border bg-background/80 p-3"
                  >
                    <div className="flex items-center justify-between gap-2">
                      <input
                        value={file.path}
                        onChange={(event) =>
                          updateContainerAt(index, (current) => ({
                            ...current,
                            provider: {
                              kind: "inline_files",
                              files: current.provider.kind === "inline_files"
                                ? current.provider.files.map((entry, currentFileIndex) =>
                                    currentFileIndex === fileIndex
                                      ? { ...entry, path: event.target.value }
                                      : entry,
                                  )
                                : [{ path: event.target.value, content: "" }],
                            },
                          }))
                        }
                        placeholder="文件路径，例如 docs/spec.md"
                        className="agentdash-form-input"
                      />
                      <button
                        type="button"
                        onClick={() =>
                          updateContainerAt(index, (current) => ({
                            ...current,
                            provider: {
                              kind: "inline_files",
                              files:
                                current.provider.kind === "inline_files"
                                  ? current.provider.files.filter((_, currentFileIndex) => currentFileIndex !== fileIndex)
                                  : [],
                            },
                          }))
                        }
                        disabled={
                          isSaving
                          || container.provider.kind !== "inline_files"
                          || container.provider.files.length <= 1
                        }
                        className="rounded-[8px] border border-border px-2 py-1 text-[11px] text-muted-foreground transition-colors hover:text-foreground disabled:opacity-40"
                      >
                        删除文件
                      </button>
                    </div>
                    <textarea
                      value={file.content}
                      onChange={(event) =>
                        updateContainerAt(index, (current) => ({
                          ...current,
                          provider: {
                            kind: "inline_files",
                            files: current.provider.kind === "inline_files"
                              ? current.provider.files.map((entry, currentFileIndex) =>
                                  currentFileIndex === fileIndex
                                    ? { ...entry, content: event.target.value }
                                    : entry,
                                )
                              : [{ path: file.path, content: event.target.value }],
                          },
                        }))
                      }
                      rows={4}
                      placeholder="文件内容"
                      className="agentdash-form-textarea font-mono text-xs"
                    />
                  </div>
                ))}
                <button
                  type="button"
                  onClick={() =>
                    updateContainerAt(index, (current) => ({
                      ...current,
                      provider: {
                        kind: "inline_files",
                        files:
                          current.provider.kind === "inline_files"
                            ? [...current.provider.files, { path: "", content: "" }]
                            : [{ path: "", content: "" }],
                      },
                    }))
                  }
                  disabled={isSaving}
                  className="rounded-[8px] border border-dashed border-border px-3 py-2 text-xs text-muted-foreground transition-colors hover:border-primary/30 hover:text-foreground disabled:opacity-40"
                >
                  + 添加内联文件
                </button>
              </div>
            ) : (
              <div className="grid gap-2 md:grid-cols-2">
                <input
                  value={container.provider.service_id}
                  onChange={(event) =>
                    updateContainerAt(index, (current) => ({
                      ...current,
                      provider:
                        current.provider.kind === "external_service"
                          ? { ...current.provider, service_id: event.target.value }
                          : { kind: "external_service", service_id: event.target.value, root_ref: "" },
                    }))
                  }
                  placeholder="service_id"
                  className="agentdash-form-input"
                />
                <input
                  value={container.provider.root_ref}
                  onChange={(event) =>
                    updateContainerAt(index, (current) => ({
                      ...current,
                      provider:
                        current.provider.kind === "external_service"
                          ? { ...current.provider, root_ref: event.target.value }
                          : { kind: "external_service", service_id: "", root_ref: event.target.value },
                    }))
                  }
                  placeholder="root_ref"
                  className="agentdash-form-input"
                />
              </div>
            )}
          </div>

          <div className="space-y-2">
            <div className="flex flex-wrap gap-2">
              {CONTEXT_CAPABILITY_OPTIONS.map((option) => {
                const supported = containerSupportsCapability(container, option.value);
                const checked = container.capabilities.includes(option.value);
                return (
                  <label
                    key={option.value}
                    className={`inline-flex items-center gap-2 rounded-[8px] border px-2.5 py-1.5 text-xs ${
                      supported
                        ? "border-border bg-background text-foreground"
                        : "border-border/70 bg-muted/40 text-muted-foreground"
                    }`}
                  >
                    <input
                      type="checkbox"
                      checked={checked}
                      disabled={!supported || isSaving}
                      onChange={(event) =>
                        updateContainerAt(index, (current) => ({
                          ...current,
                          capabilities: updateCapabilityList(
                            current.capabilities,
                            option.value,
                            event.target.checked,
                          ),
                          default_write:
                            option.value === "write" && !event.target.checked
                              ? false
                              : current.default_write,
                        }))
                      }
                      className="h-4 w-4 rounded border-border"
                    />
                    {option.label}
                  </label>
                );
              })}
            </div>
            <p className="text-[11px] text-muted-foreground">{READONLY_PROVIDER_HINT}</p>
          </div>

          <div className="grid gap-3 rounded-[10px] border border-border bg-secondary/20 p-3 md:grid-cols-2">
            <div className="space-y-2">
              <p className="text-[11px] font-medium uppercase tracking-wide text-muted-foreground/70">
                Exposure
              </p>
              <label className="flex items-center gap-2 text-xs text-foreground">
                <input
                  type="checkbox"
                  checked={container.exposure.include_in_project_sessions}
                  onChange={(event) =>
                    updateContainerAt(index, (current) => ({
                      ...current,
                      exposure: {
                        ...current.exposure,
                        include_in_project_sessions: event.target.checked,
                      },
                    }))
                  }
                  disabled={isSaving}
                  className="h-4 w-4 rounded border-border"
                />
                包含在 Project Sessions
              </label>
              <label className="flex items-center gap-2 text-xs text-foreground">
                <input
                  type="checkbox"
                  checked={container.exposure.include_in_story_sessions}
                  onChange={(event) =>
                    updateContainerAt(index, (current) => ({
                      ...current,
                      exposure: {
                        ...current.exposure,
                        include_in_story_sessions: event.target.checked,
                      },
                    }))
                  }
                  disabled={isSaving}
                  className="h-4 w-4 rounded border-border"
                />
                包含在 Story Sessions
              </label>
              <label className="flex items-center gap-2 text-xs text-foreground">
                <input
                  type="checkbox"
                  checked={container.exposure.include_in_task_sessions}
                  onChange={(event) =>
                    updateContainerAt(index, (current) => ({
                      ...current,
                      exposure: {
                        ...current.exposure,
                        include_in_task_sessions: event.target.checked,
                      },
                    }))
                  }
                  disabled={isSaving}
                  className="h-4 w-4 rounded border-border"
                />
                包含在 Task Sessions
              </label>
              <label className="flex items-center gap-2 text-xs text-muted-foreground">
                <input
                  type="checkbox"
                  checked={container.default_write}
                  disabled
                  className="h-4 w-4 rounded border-border"
                />
                default_write（当前 provider 不支持）
              </label>
            </div>

            <div className="space-y-2">
              <p className="text-[11px] font-medium uppercase tracking-wide text-muted-foreground/70">
                Agent Filter
              </p>
              <input
                value={joinAgentTypeList(container.exposure.allowed_agent_types)}
                onChange={(event) =>
                  updateContainerAt(index, (current) => ({
                    ...current,
                    exposure: {
                      ...current.exposure,
                      allowed_agent_types: parseAgentTypeList(event.target.value),
                    },
                  }))
                }
                placeholder="allowed_agent_types，逗号分隔；留空表示全部 Agent"
                className="agentdash-form-input"
              />
              <p className="text-[11px] text-muted-foreground">
                例如：`codex`, `claude-code`。留空表示不做 agent 类型过滤。
              </p>
            </div>
          </div>
        </div>
      ))}

      <div className="flex flex-wrap items-center gap-2">
        <button
          type="button"
          onClick={() => setDraft((current) => [...current, createDefaultContainer()])}
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
  const [draft, setDraft] = useState<string[]>(() => [...value]);

  useEffect(() => {
    setDraft([...value]);
  }, [value]);

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
  const [draft, setDraft] = useState<MountDerivationPolicy>(() => cloneMountPolicy(value));

  useEffect(() => {
    setDraft(cloneMountPolicy(value));
  }, [value]);

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
  const [draft, setDraft] = useState<SessionComposition>(() => cloneSessionComposition(value));
  const [workflowDraft, setWorkflowDraft] = useState(() => joinWorkflowSteps(value.workflow_steps));

  useEffect(() => {
    setDraft(cloneSessionComposition(value));
    setWorkflowDraft(joinWorkflowSteps(value.workflow_steps));
  }, [value]);

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
