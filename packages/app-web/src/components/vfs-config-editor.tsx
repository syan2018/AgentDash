import { useEffect, useMemo, useState } from "react";
import type {
  ContextContainerCapability,
  ContextContainerDefinition,
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
    if (c.mount_id.startsWith(prefix)) {
      const num = parseInt(c.mount_id.slice(prefix.length), 10);
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
    mount_id: id,
    display_name: "",
    provider: {
      kind: "inline_files",
      files: [{ path: "context.md", content: "" }],
    },
    capabilities: ["read", "write", "list", "search"],
    default_write: false,
  };
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
  readOnly?: boolean;
  addLabel?: string;
  emptyText?: string;
  onSave: (next: ContextContainerDefinition[]) => Promise<unknown>;
}

export function ContextContainersEditor({
  value,
  domain,
  isSaving = false,
  readOnly = false,
  addLabel = "添加 VFS Mount",
  emptyText = "暂无 VFS Mount",
  onSave,
}: ContextContainersEditorProps) {
  return (
    <ContextContainersEditorForm
      key={JSON.stringify(value)}
      value={value}
      domain={domain}
      isSaving={isSaving}
      readOnly={readOnly}
      addLabel={addLabel}
      emptyText={emptyText}
      onSave={onSave}
    />
  );
}

function ContainerSummaryCard({
  container,
  readOnly,
  menuOpen,
  onEdit,
  onToggleMenu,
  onDelete,
}: {
  container: ContextContainerDefinition;
  readOnly?: boolean;
  menuOpen?: boolean;
  onEdit?: () => void;
  onToggleMenu?: () => void;
  onDelete?: () => void;
}) {
  const providerLabel =
    container.provider.kind === "inline_files"
      ? `内联 · ${container.provider.files.length} 个文件`
      : container.provider.service_id;

  return (
    <div className="relative rounded-[10px] border border-border bg-background px-4 py-3">
      <div className="flex items-start justify-between gap-4">
        <div className="min-w-0 flex-1 space-y-2">
          <div className="flex min-w-0 flex-wrap items-center gap-2">
            <p className="min-w-0 truncate text-sm font-medium text-foreground">
              {container.display_name.trim() || container.mount_id}
            </p>
            <span className="rounded-[4px] bg-muted/60 px-1.5 py-0.5 font-mono text-[10px] text-muted-foreground">
              {container.mount_id}
            </span>
          </div>
          <div className="flex flex-wrap items-center gap-1.5">
            <span className="rounded-[5px] bg-secondary px-2 py-0.5 text-[11px] text-muted-foreground">
              {providerLabel}
            </span>
            {container.capabilities.map((cap) => (
              <span
                key={cap}
                className="rounded-[5px] border border-border/80 bg-muted/30 px-2 py-0.5 text-[11px] text-muted-foreground"
              >
                {CONTEXT_CAPABILITY_OPTIONS.find((o) => o.value === cap)?.label ?? cap}
              </span>
            ))}
            {container.default_write && (
              <span className="rounded-[5px] border border-amber-500/20 bg-amber-500/10 px-2 py-0.5 text-[11px] text-amber-600">
                默认写入
              </span>
            )}
          </div>
        </div>
        <div className="flex shrink-0 items-center gap-2">
          {!readOnly && (
            <button
              type="button"
              onClick={onEdit}
              className="rounded-[8px] border border-border bg-background px-3 py-1.5 text-xs text-foreground transition-colors hover:border-primary/30 hover:text-primary"
            >
              编辑
            </button>
          )}
          {!readOnly && (
            <button
              type="button"
              onClick={onToggleMenu}
              className="ml-1 inline-flex h-7 w-7 items-center justify-center rounded-[8px] border border-border text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
              aria-label="VFS Mount 操作"
            >
              ...
            </button>
          )}
        </div>
      </div>

      {menuOpen && !readOnly && (
        <div className="absolute right-4 top-12 z-10 min-w-28 rounded-[10px] border border-border bg-background p-1 shadow-lg">
          <button
            type="button"
            onClick={onDelete}
            className="w-full rounded-[8px] px-2.5 py-1.5 text-left text-xs text-destructive hover:bg-destructive/10"
          >
            删除
          </button>
        </div>
      )}
    </div>
  );
}

function ContextContainersEditorForm({
  value,
  domain,
  isSaving = false,
  readOnly = false,
  addLabel = "添加 VFS Mount",
  emptyText = "暂无 VFS Mount",
  onSave,
}: ContextContainersEditorProps) {
  const [draft, setDraft] = useState<ContextContainerDefinition[]>(() => value.map(cloneContainer));
  const [editingIndex, setEditingIndex] = useState<number | null>(null);
  const [menuIndex, setMenuIndex] = useState<number | null>(null);
  const mountProviders = useMountProviders();

  const editingContainer = editingIndex == null ? null : draft[editingIndex] ?? null;
  const isEditingNew = editingIndex != null && editingIndex >= value.length;
  const isEditingDirty = useMemo(() => {
    if (editingIndex == null) return false;
    const original = value[editingIndex];
    if (!original) return true;
    return JSON.stringify(draft[editingIndex]) !== JSON.stringify(original);
  }, [draft, editingIndex, value]);

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

  const handleCreate = () => {
    setDraft((current) => {
      const next = [...current, createDefaultContainer(domain, current)];
      setEditingIndex(next.length - 1);
      setMenuIndex(null);
      return next;
    });
  };

  const handleCancelEdit = () => {
    if (editingIndex == null) return;
    if (isEditingNew) {
      setDraft((current) => current.filter((_, index) => index !== editingIndex));
    } else {
      const original = value[editingIndex];
      if (original) {
        setDraft((current) =>
          current.map((container, index) =>
            index === editingIndex ? cloneContainer(original) : container,
          ),
        );
      }
    }
    setEditingIndex(null);
  };

  const handleSaveEdit = async () => {
    await onSave(draft.map(cloneContainer));
    setEditingIndex(null);
  };

  const handleDeleteAt = async (index: number) => {
    setMenuIndex(null);
    const next = draft.filter((_, currentIndex) => currentIndex !== index).map(cloneContainer);
    setDraft(next);
    await onSave(next);
  };

  return (
    <div className="space-y-3">
      <div className="flex flex-wrap items-center gap-3 border-b border-border/70">
        <button
          type="button"
          onClick={() => {
            if (editingIndex != null) handleCancelEdit();
          }}
          className={`border-b-2 px-1 pb-2 text-xs font-medium transition-colors ${
            editingIndex == null
              ? "border-primary text-foreground"
              : "border-transparent text-muted-foreground hover:text-foreground"
          }`}
        >
          Mount 列表
        </button>
        {editingContainer && (
          <button
            type="button"
            className="border-b-2 border-primary px-1 pb-2 text-xs font-medium text-foreground"
          >
            {isEditingNew ? "新建 VFS Mount" : `编辑：${editingContainer.display_name.trim() || editingContainer.mount_id}`}
          </button>
        )}
      </div>

      {editingIndex == null ? (
        <>
          {draft.length === 0 && (
            <p className="text-xs text-muted-foreground">{emptyText}</p>
          )}

          <div className="space-y-2">
            {draft.map((container, index) => (
              <ContainerSummaryCard
                key={`container-summary-${index}`}
                container={container}
                readOnly={readOnly}
                menuOpen={menuIndex === index}
                onEdit={() => !readOnly && setEditingIndex(index)}
                onToggleMenu={() => setMenuIndex((current) => current === index ? null : index)}
                onDelete={() => void handleDeleteAt(index)}
              />
            ))}
          </div>

          {!readOnly && (
            <button
              type="button"
              onClick={handleCreate}
              disabled={isSaving}
              className="rounded-[8px] border border-dashed border-border px-3 py-2 text-xs text-muted-foreground transition-colors hover:border-primary/30 hover:text-foreground disabled:opacity-40"
            >
              + {addLabel}
            </button>
          )}
        </>
      ) : editingContainer ? (
        <ContainerEditorItem
          container={editingContainer}
          index={editingIndex}
          isSaving={isSaving}
          isNew={isEditingNew}
          isDirty={isEditingDirty}
          mountProviders={mountProviders}
          onUpdate={(updater) => updateContainerAt(editingIndex, updater)}
          onSave={() => void handleSaveEdit()}
          onCancel={handleCancelEdit}
        />
      ) : null}
    </div>
  );
}

function ContainerEditorItem({
  container,
  index,
  isSaving,
  isNew,
  isDirty,
  mountProviders,
  onUpdate,
  onSave,
  onCancel,
}: {
  container: ContextContainerDefinition;
  index: number;
  isSaving: boolean;
  isNew: boolean;
  isDirty: boolean;
  mountProviders: ConfigurableProviderInfo[];
  onUpdate: (updater: (c: ContextContainerDefinition) => ContextContainerDefinition) => void;
  onSave: () => void;
  onCancel: () => void;
}) {
  const [showAdvanced, setShowAdvanced] = useState(false);

  return (
    <div className="space-y-3 rounded-[12px] border border-primary/20 bg-background/70 p-3">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="min-w-0">
          <p className="text-xs font-medium text-foreground">
            {isNew ? "新建 VFS Mount" : (container.display_name.trim() || `VFS Mount ${index + 1}`)}
          </p>
          <p className="mt-0.5 truncate font-mono text-[10px] text-muted-foreground">
            {container.mount_id}
          </p>
        </div>
        <span className="rounded-full border border-primary/20 bg-primary/10 px-2 py-0.5 text-[10px] text-primary">
          编辑中
        </span>
      </div>

      <div className="grid gap-2 md:grid-cols-2">
        <div className="space-y-1.5">
          <label className="text-[10px] text-muted-foreground">显示名称</label>
          <input
            value={container.display_name}
            onChange={(e) => onUpdate((c) => ({ ...c, display_name: e.target.value }))}
            placeholder="例如：项目说明、团队约束"
            className="agentdash-form-input"
          />
        </div>
        <div className="space-y-1.5">
          <label className="text-[10px] text-muted-foreground">VFS ID</label>
          <input
            value={container.mount_id}
            onChange={(e) => onUpdate((c) => ({ ...c, mount_id: e.target.value }))}
            placeholder="例如：project-notes"
            className="agentdash-form-input font-mono"
          />
        </div>
      </div>

      {/* 高级选项折叠区 */}
      <details
        open={showAdvanced}
        onToggle={(e) => setShowAdvanced((e.target as HTMLDetailsElement).open)}
      >
        <summary className="cursor-pointer text-[11px] text-muted-foreground hover:text-foreground">
          高级选项（Provider / 能力）
        </summary>
        <div className="mt-2 space-y-3">
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

        </div>
      </details>

      <div className="flex flex-wrap items-center justify-end gap-2 border-t border-border/70 pt-3">
        <button
          type="button"
          onClick={onCancel}
          disabled={isSaving}
          className="agentdash-button-secondary disabled:opacity-40"
        >
          {isNew ? "取消创建" : "取消"}
        </button>
        <button
          type="button"
          onClick={onSave}
          disabled={isSaving || (!isNew && !isDirty)}
          className="agentdash-button-primary disabled:cursor-not-allowed disabled:opacity-50"
        >
          {saveLabel(isSaving, isNew ? "创建 VFS Mount" : "保存 VFS Mount")}
        </button>
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
  return (
    <DisabledContainerIdsEditorForm
      key={JSON.stringify({
        value,
        available_ids: availableContainers.map((container) => container.mount_id),
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
        当前 Project 没有可继承 VFS Mount，因此没有可禁用项。
      </p>
    );
  }

  return (
    <div className="space-y-3">
      <div className="space-y-2">
        {availableContainers.map((container) => {
          const checked = draft.includes(container.mount_id);
          return (
            <label
              key={container.mount_id}
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
                      ? [...current, container.mount_id]
                      : current.filter((item) => item !== container.mount_id),
                  )
                }
                className="mt-0.5 h-4 w-4 rounded border-border"
              />
              <div className="min-w-0 flex-1">
                <div className="flex flex-wrap items-center gap-2">
                  <span className="font-medium text-foreground">{container.display_name || container.mount_id}</span>
                  <span className="rounded-[4px] bg-secondary px-1.5 py-0.5 text-[10px] text-muted-foreground">
                    {container.mount_id}
                  </span>
                </div>
                <p className="mt-1 text-[11px] text-muted-foreground">
                  勾选后会从 Project 默认 VFS Mount 集合中移除该项。
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
