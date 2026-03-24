import { useCallback, useEffect, useMemo, useState } from "react";
import { useLocation, useNavigate, useParams } from "react-router-dom";
import type {
  AgentBinding,
  ContextSourceKind,
  ContextSourceRef,
  MountDerivationPolicy,
  ProjectConfig,
  SessionComposition,
  Story,
  StoryNavigationState,
  StoryStatus,
  StoryPriority,
  StoryType,
  Task,
  Workspace,
} from "../types";
import { StorySessionPanel } from "../features/story/story-session-panel";
import { StoryStatusBadge, StoryPriorityBadge, StoryTypeBadge } from "../components/ui/status-badge";
import { TaskList } from "../features/task/task-list";
import { TaskDrawer } from "../features/task/task-drawer";
import { AgentBindingFields } from "../features/task/agent-binding-fields";
import {
  createDefaultAgentBinding,
  hasAgentBindingSelection,
  normalizeAgentBinding,
  resolveDefaultWorkspaceId,
} from "../features/task/agent-binding";
import { useStoryStore } from "../stores/storyStore";
import { useProjectStore } from "../stores/projectStore";
import { useWorkspaceStore } from "../stores/workspaceStore";
import type { AddressEntry } from "../services/addressSpaces";
import { useAddressSpacePicker, AddressEntryPickerInline } from "../features/context-source";
import {
  DangerConfirmDialog,
  DetailMenu,
  DetailSection,
} from "../components/ui/detail-panel";
import {
  ContextContainersEditor,
  DisabledContainerIdsEditor,
  MountPolicyEditor,
  SessionCompositionEditor,
} from "../components/context-config-editor";
import {
  createDefaultMountPolicy,
  createDefaultSessionComposition,
} from "../components/context-config-defaults";
import { AddressSpaceBrowser } from "../features/address-space";

// Story 优先级选项
const priorityOptions: { value: StoryPriority; label: string }[] = [
  { value: "p0", label: "P0 - 紧急" },
  { value: "p1", label: "P1 - 高" },
  { value: "p2", label: "P2 - 中" },
  { value: "p3", label: "P3 - 低" },
];

// Story 类型选项
const storyTypeOptions: { value: StoryType; label: string; icon: string }[] = [
  { value: "feature", label: "功能", icon: "✨" },
  { value: "bugfix", label: "缺陷", icon: "🐛" },
  { value: "refactor", label: "重构", icon: "♻️" },
  { value: "docs", label: "文档", icon: "📝" },
  { value: "test", label: "测试", icon: "🧪" },
  { value: "other", label: "其他", icon: "📦" },
];

type TabKey = "tasks" | "sessions" | "review";

// ─── Context Source 辅助 ─────────────────────────────

const SOURCE_KIND_META: Record<ContextSourceKind, { label: string; icon: string; color: string }> = {
  file: { label: "文件", icon: "📄", color: "text-blue-600" },
  manual_text: { label: "文本", icon: "📝", color: "text-emerald-600" },
  project_snapshot: { label: "快照", icon: "📸", color: "text-violet-600" },
  http_fetch: { label: "HTTP", icon: "🌐", color: "text-orange-600" },
  mcp_resource: { label: "MCP", icon: "🔌", color: "text-pink-600" },
  entity_ref: { label: "实体", icon: "🔗", color: "text-cyan-600" },
};

function sourceKindMeta(kind: ContextSourceKind) {
  return SOURCE_KIND_META[kind] ?? { label: kind, icon: "📎", color: "text-muted-foreground" };
}

function buildFileContextSource(address: string, label: string, index: number): ContextSourceRef {
  return {
    kind: "file",
    locator: address.trim(),
    label: label.trim() || null,
    slot: "references",
    priority: 1000 - index,
    required: false,
    max_chars: null,
    delivery: "resource",
  };
}

function buildManualTextSource(text: string, label: string, index: number): ContextSourceRef {
  return {
    kind: "manual_text",
    locator: text.trim(),
    label: label.trim() || null,
    slot: "references",
    priority: 900 - index,
    required: false,
    max_chars: null,
    delivery: "inline",
  };
}

function contextSummary(sourceRefs: ContextSourceRef[]) {
  const counts = new Map<ContextSourceKind, number>();
  for (const ref of sourceRefs) {
    counts.set(ref.kind, (counts.get(ref.kind) ?? 0) + 1);
  }
  return counts;
}

// ─── Story 覆盖层辅助编辑器 ───────────────────────────

function OptionalMountPolicyOverrideEditor({
  value,
  isSaving,
  onSave,
  onClear,
}: {
  value: MountDerivationPolicy | null;
  isSaving: boolean;
  onSave: (next: MountDerivationPolicy) => Promise<unknown>;
  onClear: () => Promise<unknown>;
}) {
  const [isCreating, setIsCreating] = useState(false);

  if (!value && !isCreating) {
    return (
      <div className="space-y-2">
        <p className="text-xs text-muted-foreground">
          当前没有显式挂载策略覆盖，将直接继承 Project 默认挂载策略。
        </p>
        <button
          type="button"
          onClick={() => setIsCreating(true)}
          className="agentdash-button-secondary"
        >
          新建挂载策略覆盖
        </button>
      </div>
    );
  }

  return (
    <div className="space-y-2">
      <MountPolicyEditor
        value={value ?? createDefaultMountPolicy()}
        isSaving={isSaving}
        onSave={onSave}
      />
      <div className="flex items-center gap-2">
        {value ? (
          <button
            type="button"
            onClick={() => {
              void onClear().then(() => {
                setIsCreating(false);
              });
            }}
            disabled={isSaving}
            className="agentdash-button-secondary"
          >
            清空覆盖
          </button>
        ) : (
          <button
            type="button"
            onClick={() => setIsCreating(false)}
            disabled={isSaving}
            className="agentdash-button-secondary"
          >
            取消
          </button>
        )}
      </div>
    </div>
  );
}

function OptionalSessionCompositionEditor({
  value,
  isSaving,
  onSave,
  onClear,
}: {
  value: SessionComposition | null;
  isSaving: boolean;
  onSave: (next: SessionComposition) => Promise<unknown>;
  onClear: () => Promise<unknown>;
}) {
  const [isCreating, setIsCreating] = useState(false);

  if (!value && !isCreating) {
    return (
      <div className="space-y-2">
        <p className="text-xs text-muted-foreground">
          当前没有为这个 Story 配置显式会话编排，将仅使用会话内置的默认协作阶段提示。
        </p>
        <button
          type="button"
          onClick={() => setIsCreating(true)}
          className="agentdash-button-secondary"
        >
          新建会话编排
        </button>
      </div>
    );
  }

  return (
    <div className="space-y-2">
      <SessionCompositionEditor
        value={value ?? createDefaultSessionComposition()}
        isSaving={isSaving}
        onSave={onSave}
      />
      <div className="flex items-center gap-2">
        {value ? (
          <button
            type="button"
            onClick={() => {
              void onClear().then(() => {
                setIsCreating(false);
              });
            }}
            disabled={isSaving}
            className="agentdash-button-secondary"
          >
            清空配置
          </button>
        ) : (
          <button
            type="button"
            onClick={() => setIsCreating(false)}
            disabled={isSaving}
            className="agentdash-button-secondary"
          >
            取消
          </button>
        )}
      </div>
    </div>
  );
}

// 状态流转操作按钮组
interface StoryStatusActionsProps {
  currentStatus: StoryStatus;
  onStatusChange: (status: StoryStatus) => void;
}

function StoryStatusActions({ currentStatus, onStatusChange }: StoryStatusActionsProps) {
  // 根据当前状态定义可用的流转操作
  const getAvailableActions = (status: StoryStatus): Array<{ label: string; status: StoryStatus; variant: "primary" | "secondary" | "danger" }> => {
    switch (status) {
      case "draft":
        return [
          { label: "标记就绪", status: "ready", variant: "primary" },
          { label: "取消", status: "cancelled", variant: "danger" },
        ];
      case "ready":
        return [
          { label: "开始执行", status: "running", variant: "primary" },
          { label: "退回草稿", status: "draft", variant: "secondary" },
          { label: "取消", status: "cancelled", variant: "danger" },
        ];
      case "running":
        return [
          { label: "提交验收", status: "review", variant: "primary" },
          { label: "标记失败", status: "failed", variant: "danger" },
        ];
      case "review":
        return [
          { label: "验收通过", status: "completed", variant: "primary" },
          { label: "退回执行", status: "running", variant: "secondary" },
          { label: "验收不通过", status: "failed", variant: "danger" },
        ];
      case "completed":
        return [
          { label: "重新打开", status: "ready", variant: "secondary" },
        ];
      case "failed":
        return [
          { label: "重新执行", status: "running", variant: "primary" },
          { label: "关闭", status: "cancelled", variant: "secondary" },
        ];
      case "cancelled":
        return [
          { label: "重新打开", status: "draft", variant: "primary" },
        ];
      default:
        return [];
    }
  };

  const actions = getAvailableActions(currentStatus);

  if (actions.length === 0) return null;

  return (
    <DetailSection title="状态流转">
      <div className="flex flex-wrap gap-2">
        {actions.map((action) => {
          // 低饱和度配色
          const variantClasses = {
            primary: "bg-primary/10 text-primary hover:bg-primary/20 border border-primary/30",
            secondary: "bg-muted text-muted-foreground hover:bg-muted/80 border border-border",
            danger: "bg-destructive/10 text-destructive hover:bg-destructive/20 border border-destructive/30",
          };
          return (
            <button
              key={action.status}
              type="button"
              onClick={() => onStatusChange(action.status)}
              className={`rounded-[10px] px-3 py-1.5 text-xs font-medium transition-colors ${variantClasses[action.variant]}`}
            >
              {action.label}
            </button>
          );
        })}
      </div>
      <div className="mt-2 flex items-center gap-2 text-xs text-muted-foreground">
        <span>当前状态:</span>
        <StoryStatusBadge status={currentStatus} />
      </div>
    </DetailSection>
  );
}

interface CreateTaskPanelProps {
  story: Story;
  storyId: string;
  workspaces: Workspace[];
  projectConfig?: ProjectConfig;
  onCreated: () => void;
}

function CreateTaskPanel({
  story,
  storyId,
  workspaces,
  projectConfig,
  onCreated,
}: CreateTaskPanelProps) {
  const { createTask, error } = useStoryStore();
  const [isExpanded, setIsExpanded] = useState(false);
  const [title, setTitle] = useState("");
  const [description, setDescription] = useState("");
  const [workspaceId, setWorkspaceId] = useState(() => resolveDefaultWorkspaceId(projectConfig, workspaces));
  const [agentBinding, setAgentBinding] = useState<AgentBinding>(() => createDefaultAgentBinding(projectConfig));
  const [selectedContextIndexes, setSelectedContextIndexes] = useState<number[]>([]);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [formMessage, setFormMessage] = useState<string | null>(null);
  const availableContexts = story.context.source_refs;

  useEffect(() => {
    if (isExpanded) return;
    setWorkspaceId(resolveDefaultWorkspaceId(projectConfig, workspaces));
    setAgentBinding(createDefaultAgentBinding(projectConfig));
    setSelectedContextIndexes([]);
    setFormMessage(null);
  }, [isExpanded, projectConfig, workspaces]);

  useEffect(() => {
    if (!isExpanded) {
      setSelectedContextIndexes([]);
    }
  }, [isExpanded, story.id]);

  const toggleContextSelection = (index: number) => {
    setSelectedContextIndexes((current) =>
      current.includes(index) ? current.filter((item) => item !== index) : [...current, index].sort((a, b) => a - b),
    );
  };

  const handleSubmit = async () => {
    if (!title.trim()) return;
    if (!hasAgentBindingSelection(agentBinding, projectConfig)) {
      setFormMessage("请指定 Agent 类型或预设，或先在 Project 配置中设置 default_agent_type");
      return;
    }
    setIsSubmitting(true);
    setFormMessage(null);
    try {
      const selectedContexts = selectedContextIndexes
        .map((index) => availableContexts[index])
        .filter((item): item is ContextSourceRef => Boolean(item));
      const task = await createTask(storyId, {
        title: title.trim(),
        description: description.trim() || undefined,
        workspace_id: workspaceId || null,
        agent_binding: normalizeAgentBinding({
          ...agentBinding,
          context_sources: selectedContexts,
        }),
      });
      if (!task) return;

      onCreated();
      // 重置表单并收起
      setTitle("");
      setDescription("");
      setWorkspaceId(resolveDefaultWorkspaceId(projectConfig, workspaces));
      setAgentBinding(createDefaultAgentBinding(projectConfig));
      setSelectedContextIndexes([]);
      setIsExpanded(false);
    } finally {
      setIsSubmitting(false);
    }
  };

  if (!isExpanded) {
    return (
      <button
        type="button"
        onClick={() => setIsExpanded(true)}
        className="flex w-full items-center justify-center gap-2 rounded-[12px] border border-dashed border-border bg-secondary/25 py-3.5 text-sm text-muted-foreground transition-colors hover:border-primary/25 hover:bg-secondary/40 hover:text-foreground"
      >
        <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4v16m8-8H4" />
        </svg>
        添加 Task
      </button>
    );
  }

  return (
    <div className="rounded-[12px] border border-border bg-secondary/35 p-4">
      <div className="mb-3 flex items-center justify-between">
        <span className="text-sm font-medium">新建 Task</span>
        <button
          type="button"
          onClick={() => setIsExpanded(false)}
          className="rounded-[8px] border border-border bg-background px-2 py-1 text-xs text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
        >
          取消
        </button>
      </div>

      <div className="space-y-3">
        <input
          value={title}
          onChange={(event) => setTitle(event.target.value)}
          placeholder="Task 标题"
          autoFocus
          className="agentdash-form-input"
        />

        <select
          value={workspaceId}
          onChange={(event) => setWorkspaceId(event.target.value)}
          className="agentdash-form-select"
        >
          <option value="">Workspace</option>
          {workspaces.map((workspace) => (
            <option key={workspace.id} value={workspace.id}>
              {workspace.name}
            </option>
          ))}
        </select>

        <textarea
          value={description}
          onChange={(event) => setDescription(event.target.value)}
          rows={2}
          placeholder="描述（可选）"
          className="agentdash-form-textarea"
        />

        <AgentBindingFields
          value={agentBinding}
          projectConfig={projectConfig}
          onChange={setAgentBinding}
        />

        {availableContexts.length > 0 && (
          <div className="rounded-[12px] border border-border bg-background p-3.5">
            <div className="mb-2 flex items-center justify-between gap-2">
              <div>
                <p className="text-xs font-medium text-muted-foreground">关联 Story 上下文</p>
                <p className="mt-1 text-xs text-muted-foreground">
                  勾选后会把这些上下文源分配给 Task Agent，并在执行时由后端解析注入。
                </p>
              </div>
              <span className="rounded-full border border-border bg-secondary/50 px-2 py-0.5 text-[10px] text-muted-foreground">
                已选 {selectedContextIndexes.length}
              </span>
            </div>

            <div className="space-y-2">
              {availableContexts.map((context, index) => {
                const checked = selectedContextIndexes.includes(index);
                return (
                  <label
                    key={`${context.label ?? "context"}-${index}`}
                    className={`flex cursor-pointer items-start gap-3 rounded-[10px] border px-3 py-2 transition-colors ${
                      checked
                        ? "border-primary/40 bg-primary/5"
                        : "border-border bg-secondary/20 hover:bg-secondary/35"
                    }`}
                  >
                    <input
                      type="checkbox"
                      checked={checked}
                      onChange={() => toggleContextSelection(index)}
                      className="mt-1 h-4 w-4 rounded border-border"
                    />
                    <div className="min-w-0 flex-1">
                      <div className="flex flex-wrap items-center gap-2">
                        {(() => { const m = sourceKindMeta(context.kind); return (
                          <span className={`rounded-full border border-current/20 px-1.5 py-0.5 text-[10px] font-medium ${m.color}`}>
                            {m.icon} {m.label}
                          </span>
                        ); })()}
                        <span className="text-sm font-medium text-foreground">
                          {context.label?.trim() || `上下文 ${index + 1}`}
                        </span>
                      </div>
                      <p className="mt-1 truncate text-xs leading-5 text-muted-foreground">
                        {context.locator}
                      </p>
                    </div>
                  </label>
                );
              })}
            </div>
          </div>
        )}

        <div className="flex items-center justify-between">
          {formMessage || error ? (
            <p className="text-xs text-destructive">{formMessage || error}</p>
          ) : (
            <div />
          )}
          <button
            type="button"
            onClick={() => void handleSubmit()}
            disabled={isSubmitting || !title.trim()}
            className="agentdash-button-primary"
          >
            {isSubmitting ? "创建中..." : "创建"}
          </button>
        </div>
      </div>
    </div>
  );
}

function ContextPanel({
  story,
  workspaces,
  projectConfig,
}: {
  story: Story;
  workspaces: Workspace[];
  projectConfig?: ProjectConfig;
}) {
  const ctx = story.context;
  const { updateStory, error } = useStoryStore();
  const sourceRefs = ctx.source_refs;
  const defaultWorkspaceId = useMemo(
    () => resolveDefaultWorkspaceId(projectConfig, workspaces),
    [projectConfig, workspaces],
  );

  const [message, setMessage] = useState<string | null>(null);
  const [isSaving, setIsSaving] = useState(false);
  const [addingText, setAddingText] = useState(false);
  const [newTextLabel, setNewTextLabel] = useState("");
  const [newTextContent, setNewTextContent] = useState("");
  const inheritedProjectContainers = projectConfig?.context_containers ?? [];

  const filePicker = useAddressSpacePicker({
    spaceId: "workspace_file",
    workspaceId: defaultWorkspaceId,
    resetKey: story.id,
  });

  useEffect(() => {
    setMessage(null);
    setAddingText(false);
    setNewTextLabel("");
    setNewTextContent("");
  }, [story.id]);

  const hasLegacyContent = ctx.prd_doc || ctx.spec_refs.length > 0 || ctx.resource_list.length > 0;

  const persistStoryContext = useCallback(async (
    payload: Parameters<typeof updateStory>[1],
    successMsg: string,
  ) => {
    setIsSaving(true);
    setMessage(null);
    try {
      const updated = await updateStory(story.id, payload);
      if (!updated) {
        setMessage(error ?? "保存失败");
        return false;
      }
      setMessage(successMsg);
      return true;
    } finally {
      setIsSaving(false);
    }
  }, [updateStory, story.id, error]);

  const persistSourceRefs = useCallback(
    async (nextRefs: ContextSourceRef[], successMsg: string) =>
      persistStoryContext({ context_source_refs: nextRefs }, successMsg),
    [persistStoryContext],
  );

  const handleRemoveSource = useCallback(async (index: number) => {
    const next = sourceRefs.filter((_, i) => i !== index);
    await persistSourceRefs(next, "已删除");
  }, [sourceRefs, persistSourceRefs]);

  const handleQuickAddFile = useCallback(async (entry: AddressEntry) => {
    const label = entry.address.split("/").pop() ?? entry.address;
    const newRef = buildFileContextSource(entry.address, label, sourceRefs.length);
    const next = [...sourceRefs, newRef];
    filePicker.closePicker();
    await persistSourceRefs(next, `已添加文件：${entry.label}`);
  }, [sourceRefs, persistSourceRefs, filePicker]);

  const closeTextForm = useCallback(() => {
    setAddingText(false);
    setNewTextLabel("");
    setNewTextContent("");
  }, []);

  const handleAddManualText = useCallback(async () => {
    const content = newTextContent.trim();
    if (!content) return;
    const newRef = buildManualTextSource(content, newTextLabel, sourceRefs.length);
    const next = [...sourceRefs, newRef];
    const ok = await persistSourceRefs(next, "已添加文本上下文");
    if (ok) closeTextForm();
  }, [newTextContent, newTextLabel, sourceRefs, persistSourceRefs, closeTextForm]);

  return (
    <div className="space-y-2">

      {/* Legacy：PRD */}
      {ctx.prd_doc && (
        <div>
          <p className="mb-1 text-[11px] font-medium uppercase tracking-wide text-muted-foreground/70">PRD</p>
          <pre className="max-h-32 overflow-y-auto whitespace-pre-wrap rounded-[8px] bg-background/60 px-2.5 py-2 text-xs leading-5 text-foreground">{ctx.prd_doc}</pre>
        </div>
      )}

      {/* Legacy：规格引用 */}
      {ctx.spec_refs.length > 0 && (
        <div>
          <p className="mb-1 text-[11px] font-medium uppercase tracking-wide text-muted-foreground/70">规格引用</p>
          <div className="flex flex-wrap gap-1.5">
            {ctx.spec_refs.map((ref, index) => (
              <span key={index} className="rounded-[6px] bg-background/60 px-2 py-1 text-xs text-foreground">{ref}</span>
            ))}
          </div>
        </div>
      )}

      {/* Legacy：资源列表 */}
      {ctx.resource_list.length > 0 && (
        <div>
          <p className="mb-1 text-[11px] font-medium uppercase tracking-wide text-muted-foreground/70">资源</p>
          {ctx.resource_list.map((resource, index) => (
            <div key={index} className="flex items-center gap-2 py-1 text-xs">
              <span className="rounded-[4px] bg-muted px-1.5 py-0.5 text-[10px] uppercase text-muted-foreground">{resource.resource_type}</span>
              <span className="font-medium text-foreground">{resource.name}</span>
              <span className="truncate text-muted-foreground">{resource.uri}</span>
            </div>
          ))}
        </div>
      )}

      {/* 条目列表 */}
      {sourceRefs.length > 0 && (
        <div className="space-y-1">
          {sourceRefs.map((ref, index) => {
            const meta = sourceKindMeta(ref.kind);
            return (
              <div
                key={`${ref.kind}-${ref.locator}-${index}`}
                className="group flex items-center gap-2 rounded-[8px] px-2 py-1.5 transition-colors hover:bg-background/60"
              >
                <span className="shrink-0 text-sm">{meta.icon}</span>
                <span className={`shrink-0 text-[10px] font-medium ${meta.color}`}>{meta.label}</span>
                {ref.label?.trim() && (
                  <span className="shrink-0 text-xs font-medium text-foreground">{ref.label}</span>
                )}
                <span className="min-w-0 flex-1 truncate text-xs text-muted-foreground" title={ref.locator}>
                  {ref.kind === "manual_text"
                    ? (ref.locator.length > 80 ? ref.locator.slice(0, 80) + "…" : ref.locator)
                    : ref.locator}
                </span>
                <button
                  type="button"
                  onClick={() => void handleRemoveSource(index)}
                  disabled={isSaving}
                  className="shrink-0 rounded-[4px] p-0.5 text-muted-foreground/40 opacity-0 transition-all hover:text-destructive group-hover:opacity-100 disabled:opacity-50"
                  title="删除"
                >
                  <svg className="h-3 w-3" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2.5}>
                    <path strokeLinecap="round" strokeLinejoin="round" d="M6 18L18 6M6 6l12 12" />
                  </svg>
                </button>
              </div>
            );
          })}
        </div>
      )}

      <div className="space-y-3 rounded-[12px] border border-border bg-secondary/20 p-3">
        <div>
          <p className="text-[11px] font-medium uppercase tracking-wide text-muted-foreground/70">
            Story 追加容器
          </p>
          <p className="mt-1 text-xs text-muted-foreground">
            这里追加的是 Story 覆盖层容器，只影响当前 Story 派生出来的会话。
          </p>
        </div>
        <ContextContainersEditor
          value={ctx.context_containers}
          isSaving={isSaving}
          addLabel="添加 Story 容器"
          emptyText="暂无 Story 级容器"
          onSave={(next) => persistStoryContext({ context_containers: next }, "已保存 Story 容器")}
        />
      </div>

      <div className="space-y-3 rounded-[12px] border border-border bg-secondary/20 p-3">
        <div>
          <p className="text-[11px] font-medium uppercase tracking-wide text-muted-foreground/70">
            禁用 Project 容器
          </p>
          <p className="mt-1 text-xs text-muted-foreground">
            这里不是删除 Project 默认容器，而是只在当前 Story 的继承链里把它们摘掉。
          </p>
        </div>
        <DisabledContainerIdsEditor
          value={ctx.disabled_container_ids}
          availableContainers={inheritedProjectContainers}
          isSaving={isSaving}
          onSave={(next) => persistStoryContext({ disabled_container_ids: next }, "已保存禁用容器列表")}
        />
      </div>

      <div className="space-y-3 rounded-[12px] border border-border bg-secondary/20 p-3">
        <div>
          <p className="text-[11px] font-medium uppercase tracking-wide text-muted-foreground/70">
            挂载策略覆盖
          </p>
          <p className="mt-1 text-xs text-muted-foreground">
            只有当前 Story 需要偏离 Project 默认挂载规则时，才在这里建立 override。
          </p>
        </div>
        <OptionalMountPolicyOverrideEditor
          value={ctx.mount_policy_override ?? null}
          isSaving={isSaving}
          onSave={(next) => persistStoryContext({ mount_policy_override: next }, "已保存挂载策略覆盖")}
          onClear={() => persistStoryContext({ clear_mount_policy_override: true }, "已清空挂载策略覆盖")}
        />
      </div>

      <div className="space-y-3 rounded-[12px] border border-border bg-secondary/20 p-3">
        <div>
          <p className="text-[11px] font-medium uppercase tracking-wide text-muted-foreground/70">
            会话编排
          </p>
          <p className="mt-1 text-xs text-muted-foreground">
            这里定义 Story 自己的 persona / workflow / required_context_blocks，不再继承 Project 级默认值。
          </p>
        </div>
        <OptionalSessionCompositionEditor
          value={ctx.session_composition ?? null}
          isSaving={isSaving}
          onSave={(next) => persistStoryContext({ session_composition: next }, "已保存会话编排")}
          onClear={() => persistStoryContext({ clear_session_composition: true }, "已清空会话编排")}
        />
      </div>

      {/* Address Space 预览 */}
      <div className="space-y-2 rounded-[12px] border border-border bg-secondary/20 p-3">
        <div>
          <p className="text-[11px] font-medium uppercase tracking-wide text-muted-foreground/70">
            地址空间预览
          </p>
          <p className="mt-1 text-xs text-muted-foreground">
            以下是当前 Story 配置下 Agent 将看到的挂载视图。
          </p>
        </div>
        <AddressSpaceBrowser
          preview={{ projectId: story.project_id, storyId: story.id, target: "story" }}
        />
      </div>

      {/* 空态 */}
      {sourceRefs.length === 0
        && !hasLegacyContent
        && ctx.context_containers.length === 0
        && ctx.disabled_container_ids.length === 0
        && !ctx.mount_policy_override
        && !ctx.session_composition && (
        <p className="py-3 text-center text-xs text-muted-foreground/70">
          暂无上下文源
        </p>
      )}

      {/* 内联添加区（文件选择器 / 文本表单，互斥） */}
      {filePicker.pickerOpen && (
        <AddressEntryPickerInline
          open={filePicker.pickerOpen}
          query={filePicker.pickerQuery}
          entries={filePicker.pickerEntries}
          loading={filePicker.pickerLoading}
          error={filePicker.pickerError}
          selectedIndex={filePicker.selectedIndex}
          placeholder={filePicker.space?.selector?.placeholder ?? "搜索文件…"}
          emptyText={filePicker.pickerQuery ? "没有匹配的文件" : "暂无可选文件"}
          onQueryChange={filePicker.updatePickerQuery}
          onSelect={(entry) => void handleQuickAddFile(entry)}
          onClose={filePicker.closePicker}
          onMoveSelection={filePicker.moveSelection}
          onConfirmSelection={() => {
            const entry = filePicker.confirmSelection();
            if (entry) void handleQuickAddFile(entry);
          }}
        />
      )}
      {addingText && (
        <div className="rounded-[8px] border border-border bg-background/80">
          <div className="flex items-center gap-2 px-2.5 py-1.5">
            <span className="text-xs text-muted-foreground">Aa</span>
            <input
              value={newTextLabel}
              onChange={(e) => setNewTextLabel(e.target.value)}
              onKeyDown={(e) => { if (e.key === "Escape") closeTextForm(); }}
              placeholder="标签（可选）"
              className="flex-1 bg-transparent text-xs text-foreground outline-none placeholder:text-muted-foreground/60"
              autoFocus
            />
            <button
              type="button"
              onClick={closeTextForm}
              className="rounded-[4px] p-0.5 text-muted-foreground/50 transition-colors hover:text-foreground"
            >
              <svg className="h-3 w-3" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2.5}>
                <path strokeLinecap="round" strokeLinejoin="round" d="M6 18L18 6M6 6l12 12" />
              </svg>
            </button>
          </div>
          <div className="border-t border-border px-2.5 py-1.5">
            <textarea
              value={newTextContent}
              onChange={(e) => setNewTextContent(e.target.value)}
              onKeyDown={(e) => { if (e.key === "Escape") closeTextForm(); }}
              placeholder="输入文本内容…"
              rows={3}
              className="w-full resize-y bg-transparent text-xs leading-5 text-foreground outline-none placeholder:text-muted-foreground/60"
            />
          </div>
          <div className="flex items-center gap-2 border-t border-border px-2.5 py-1">
            <button
              type="button"
              onClick={() => void handleAddManualText()}
              disabled={isSaving || !newTextContent.trim()}
              className="rounded-[6px] bg-primary px-2.5 py-1 text-[11px] font-medium text-primary-foreground transition-colors hover:bg-primary/90 disabled:opacity-40"
            >
              {isSaving ? "保存中…" : "添加"}
            </button>
            <span className="text-[10px] text-muted-foreground/50">Esc 取消</span>
          </div>
        </div>
      )}

      {/* 操作栏（两个都没展开时才显示按钮） */}
      <div className="flex items-center gap-2 pt-1">
        {!filePicker.pickerOpen && !addingText && (
          <>
            <button
              type="button"
              onClick={() => { filePicker.openPicker(); }}
              disabled={!filePicker.isAvailable || isSaving}
              className="inline-flex items-center gap-1 rounded-[6px] px-2 py-1 text-xs text-muted-foreground transition-colors hover:bg-background/60 hover:text-foreground disabled:cursor-not-allowed disabled:opacity-40"
            >
              <svg className="h-3 w-3" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                <path strokeLinecap="round" strokeLinejoin="round" d="M12 4v16m8-8H4" />
              </svg>
              文件
            </button>
            <button
              type="button"
              onClick={() => { setAddingText(true); }}
              disabled={isSaving}
              className="inline-flex items-center gap-1 rounded-[6px] px-2 py-1 text-xs text-muted-foreground transition-colors hover:bg-background/60 hover:text-foreground disabled:cursor-not-allowed disabled:opacity-40"
            >
              <svg className="h-3 w-3" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                <path strokeLinecap="round" strokeLinejoin="round" d="M12 4v16m8-8H4" />
              </svg>
              文本
            </button>
          </>
        )}
        {filePicker.spaceError && !filePicker.space && (
          <span className="text-[10px] text-amber-600">{filePicker.spaceError}</span>
        )}
        {message && <span className="ml-auto text-[11px] text-emerald-600">{message}</span>}
        {!message && error && <span className="ml-auto text-[11px] text-destructive">{error}</span>}
      </div>
    </div>
  );
}

function ReviewPanel({ story, tasks }: { story: Story; tasks: Task[] }) {
  const successCount = tasks.filter((task) => task.status === "completed").length;
  const failedCount = tasks.filter((task) => task.status === "failed").length;
  const runningCount = tasks.filter((task) => task.status === "running").length;
  const pendingCount = tasks.filter((task) => task.status === "pending" || task.status === "assigned").length;

  return (
    <DetailSection title="验收">
      <div className="space-y-4">
        <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
          <div className="rounded-[12px] border border-border bg-background p-3.5">
            <p className="text-xs text-muted-foreground">类型</p>
            <div className="mt-1">
              <StoryTypeBadge type={story.story_type} />
            </div>
          </div>
          <div className="rounded-[12px] border border-border bg-background p-3.5">
            <p className="text-xs text-muted-foreground">优先级</p>
            <div className="mt-1">
              <StoryPriorityBadge priority={story.priority} showLabel />
            </div>
          </div>
          <div className="rounded-[12px] border border-border bg-background p-3.5">
            <p className="text-xs text-muted-foreground">状态</p>
            <div className="mt-1">
              <StoryStatusBadge status={story.status} />
            </div>
          </div>
          <div className="rounded-[12px] border border-border bg-background p-3.5">
            <p className="text-xs text-muted-foreground">任务总数</p>
            <p className="mt-1 text-sm font-medium text-foreground">{tasks.length}</p>
          </div>
        </div>

        {story.tags.length > 0 && (
          <div className="space-y-2">
            <p className="text-xs text-muted-foreground">标签</p>
            <div className="flex flex-wrap gap-1.5">
              {story.tags.map((tag, index) => (
                <span
                  key={index}
                  className="inline-flex items-center rounded-full border border-border bg-background px-2.5 py-1 text-xs text-muted-foreground"
                >
                  {tag}
                </span>
              ))}
            </div>
          </div>
        )}

        <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
          <div className="rounded-[12px] border border-border bg-background p-3.5">
            <p className="text-xs text-muted-foreground">待执行</p>
            <p className="mt-1 text-sm font-medium text-muted-foreground">{pendingCount}</p>
          </div>
          <div className="rounded-[12px] border border-border bg-background p-3.5">
            <p className="text-xs text-muted-foreground">执行中</p>
            <p className="mt-1 text-sm font-medium text-primary">{runningCount}</p>
          </div>
          <div className="rounded-[12px] border border-border bg-background p-3.5">
            <p className="text-xs text-muted-foreground">成功</p>
            <p className="mt-1 text-sm font-medium text-success">{successCount}</p>
          </div>
          <div className="rounded-[12px] border border-border bg-background p-3.5">
            <p className="text-xs text-muted-foreground">失败</p>
            <p className="mt-1 text-sm font-medium text-destructive">{failedCount}</p>
          </div>
        </div>

        <div className="rounded-[12px] border border-border bg-background p-3.5">
          <p className="mb-2 text-xs font-medium text-muted-foreground">描述</p>
          <p className="text-sm leading-6 text-foreground">{story.description || "暂无 Story 描述"}</p>
        </div>
      </div>
    </DetailSection>
  );
}

export function StoryPage() {
  const { storyId } = useParams<{ storyId: string }>();
  const location = useLocation();
  const navigate = useNavigate();
  const { projects } = useProjectStore();
  const {
    stories,
    tasksByStoryId,
    fetchStoryById,
    fetchTasks,
    updateStory,
    deleteStory,
    error,
  } = useStoryStore();
  const { workspacesByProjectId } = useWorkspaceStore();

  const [activeTab, setActiveTab] = useState<TabKey>("sessions");
  const [isContextExpanded, setIsContextExpanded] = useState(false);
  const [isDeleteConfirmOpen, setIsDeleteConfirmOpen] = useState(false);
  const [deleteConfirmValue, setDeleteConfirmValue] = useState("");
  const [formMessage, setFormMessage] = useState<string | null>(null);
  const [isEditingBasicInfo, setIsEditingBasicInfo] = useState(false);
  const [selectedTaskId, setSelectedTaskId] = useState<string | null>(null);
  const [isStoryLoading, setIsStoryLoading] = useState(false);
  const routeState = useMemo(
    () => (location.state as StoryNavigationState | null) ?? null,
    [location.state],
  );
  const openTaskIdFromRoute = routeState?.open_task_id?.trim() ?? "";

  // 获取当前 Story
  const story = useMemo(() => stories.find((s) => s.id === storyId) || null, [stories, storyId]);

  // 编辑表单状态 - 使用 key 模式在 storyId 变化时重置
  const [editTitle, setEditTitle] = useState(story?.title ?? "");
  const [editDescription, setEditDescription] = useState(story?.description ?? "");
  const [editStatus, setEditStatus] = useState<StoryStatus>(story?.status ?? "draft");
  const [editPriority, setEditPriority] = useState<StoryPriority>(story?.priority ?? "p2");
  const [editStoryType, setEditStoryType] = useState<StoryType>(story?.story_type ?? "feature");
  const [editTags, setEditTags] = useState<string>(story?.tags.join(", ") ?? "");

  // 当 storyId 变化时重置表单（通过 key 属性实现，这里作为备份）
  useEffect(() => {
    if (story) {
      setEditTitle(story.title);
      setEditDescription(story.description || "");
      setEditStatus(story.status);
      setEditPriority(story.priority);
      setEditStoryType(story.story_type);
      setEditTags(story.tags.join(", "));
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [storyId]);

  // 获取 Story 相关数据
  const tasks = useMemo(() => (storyId ? tasksByStoryId[storyId] ?? [] : []), [tasksByStoryId, storyId]);
  const sortedTasks = useMemo(
    () => [...tasks].sort((a, b) => new Date(b.updated_at).getTime() - new Date(a.updated_at).getTime()),
    [tasks]
  );
  const selectedTask = useMemo(
    () => sortedTasks.find((task) => task.id === selectedTaskId) ?? null,
    [sortedTasks, selectedTaskId]
  );

  const currentProject = useMemo(() => {
    if (!story) return null;
    return projects.find((p) => p.id === story.project_id) || null;
  }, [story, projects]);

  const workspaces = useMemo(() => {
    if (!story) return [];
    return workspacesByProjectId[story.project_id] ?? [];
  }, [story, workspacesByProjectId]);

  // 按 ID 精准加载 Story，避免按项目循环覆盖导致“Story 不存在”闪烁
  useEffect(() => {
    if (!storyId) return;
    if (story?.id === storyId) {
      setIsStoryLoading(false);
      return;
    }

    let cancelled = false;
    setIsStoryLoading(true);
    void (async () => {
      await fetchStoryById(storyId);
      if (!cancelled) {
        setIsStoryLoading(false);
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [fetchStoryById, story?.id, storyId]);

  // 加载 Tasks
  useEffect(() => {
    if (storyId && !tasksByStoryId[storyId]) {
      void fetchTasks(storyId);
    }
  }, [storyId, tasksByStoryId, fetchTasks]);

  useEffect(() => {
    if (!openTaskIdFromRoute) return;
    if (selectedTaskId === openTaskIdFromRoute) return;

    const matched = sortedTasks.some((task) => task.id === openTaskIdFromRoute);
    if (!matched) return;

    setSelectedTaskId(openTaskIdFromRoute);
    navigate(location.pathname, { replace: true, state: null });
  }, [location.pathname, navigate, openTaskIdFromRoute, selectedTaskId, sortedTasks]);


  const handleSaveStory = async () => {
    if (!story) return;
    const trimmedTitle = editTitle.trim();
    if (!trimmedTitle) {
      setFormMessage("Story 标题不能为空");
      return;
    }

    // 解析标签
    const parsedTags = editTags
      .split(",")
      .map((t) => t.trim())
      .filter((t) => t.length > 0);

    const updated = await updateStory(story.id, {
      title: trimmedTitle,
      description: editDescription,
      status: editStatus,
      priority: editPriority,
      story_type: editStoryType,
      tags: parsedTags,
    });
    if (!updated) return;

    setFormMessage(null);
    setIsEditingBasicInfo(false);
  };

  const handleDeleteStory = async () => {
    if (!story) return;
    if (deleteConfirmValue.trim() !== story.title) {
      setFormMessage("请输入完整 Story 标题后再删除");
      return;
    }
    await deleteStory(story.id);
    setIsDeleteConfirmOpen(false);
    navigate("/");
  };

  const handleTaskCreated = () => {
    // Task 创建成功后刷新列表
    if (storyId) {
      void fetchTasks(storyId);
    }
  };

  const handleTaskUpdated = (updated: Task) => {
    setSelectedTaskId(updated.id);
    if (storyId) {
      void fetchTasks(storyId);
    }
  };

  const handleTaskDeleted = (taskId: string) => {
    if (selectedTaskId === taskId) {
      setSelectedTaskId(null);
    }
    if (storyId) {
      void fetchTasks(storyId);
    }
  };

  if (!story && isStoryLoading) {
    return (
      <div className="flex h-full items-center justify-center">
        <div className="text-center">
          <div className="mx-auto h-6 w-6 animate-spin rounded-full border-2 border-primary border-t-transparent" />
          <p className="mt-3 text-sm text-muted-foreground">正在加载 Story...</p>
        </div>
      </div>
    );
  }

  if (!story) {
    return (
      <div className="flex h-full items-center justify-center">
        <div className="text-center">
          <h2 className="text-xl font-semibold text-foreground">Story 不存在</h2>
          <p className="mt-2 text-sm text-muted-foreground">该 Story 可能已被删除或无法访问</p>
          <button
            type="button"
            onClick={() => navigate("/")}
            className="mt-4 rounded bg-primary px-4 py-2 text-sm text-primary-foreground"
          >
            返回看板
          </button>
        </div>
      </div>
    );
  }

  const tabs = [
    { key: "sessions", label: "会话" },
    { key: "tasks", label: "任务列表" },
    { key: "review", label: "验收" },
  ] as const;

  return (
    <div className="flex h-full flex-col overflow-hidden">
      {/* 页面头部 */}
      <header className="flex h-14 shrink-0 items-center justify-between border-b border-border bg-background px-6">
        <div className="flex items-center gap-3">
          <button
            type="button"
            onClick={() => navigate("/")}
            className="rounded-[10px] border border-border bg-background px-2.5 py-1.5 text-sm text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
          >
            ← 返回看板
          </button>
          <div className="flex items-center gap-2.5">
            <span className="agentdash-panel-header-tag">Story</span>
            <div>
            <h1 className="text-sm font-semibold text-foreground">{story.title}</h1>
            <p className="text-xs text-muted-foreground">ID: {story.id}</p>
            </div>
          </div>
        </div>
        <div className="flex items-center gap-2">
          <StoryTypeBadge type={story.story_type} />
          <StoryPriorityBadge priority={story.priority} showLabel />
          <StoryStatusBadge status={story.status} />
          <DetailMenu
            items={[
              {
                key: "delete",
                label: "删除 Story",
                danger: true,
                onSelect: () => setIsDeleteConfirmOpen(true),
              },
            ]}
          />
        </div>
      </header>

      {/* 页面内容 */}
      <div className="flex flex-1 overflow-hidden">
        {/* 左侧：Story 编辑 */}
        <div className="w-80 shrink-0 overflow-y-auto border-r border-border bg-background p-4">
          {/* Story 基本信息 */}
          <DetailSection
            title="基本信息"
            extra={
              !isEditingBasicInfo && (
                <button
                  type="button"
                  onClick={() => setIsEditingBasicInfo(true)}
                  className="rounded-[8px] border border-border bg-background px-2 py-1 text-xs text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
                >
                  编辑
                </button>
              )
            }
          >
            {isEditingBasicInfo ? (
              <div className="space-y-3">
                <div>
                  <label className="agentdash-form-label">标题</label>
                  <input
                    value={editTitle}
                    onChange={(event) => setEditTitle(event.target.value)}
                    placeholder="Story 标题"
                    autoFocus
                    className="agentdash-form-input"
                  />
                </div>
                <div>
                  <label className="agentdash-form-label">描述</label>
                  <textarea
                    value={editDescription}
                    onChange={(event) => setEditDescription(event.target.value)}
                    rows={3}
                    placeholder="Story 描述"
                    className="agentdash-form-textarea"
                  />
                </div>
                <div>
                  <label className="agentdash-form-label">类型</label>
                  <select
                    value={editStoryType}
                    onChange={(event) => setEditStoryType(event.target.value as StoryType)}
                    className="agentdash-form-select"
                  >
                    {storyTypeOptions.map((opt) => (
                      <option key={opt.value} value={opt.value}>
                        {opt.icon} {opt.label}
                      </option>
                    ))}
                  </select>
                </div>
                <div>
                  <label className="agentdash-form-label">优先级</label>
                  <select
                    value={editPriority}
                    onChange={(event) => setEditPriority(event.target.value as StoryPriority)}
                    className="agentdash-form-select"
                  >
                    {priorityOptions.map((opt) => (
                      <option key={opt.value} value={opt.value}>
                        {opt.label}
                      </option>
                    ))}
                  </select>
                </div>
                <div>
                  <label className="agentdash-form-label">标签（逗号分隔）</label>
                  <input
                    value={editTags}
                    onChange={(event) => setEditTags(event.target.value)}
                    placeholder="例如: frontend, api, urgent"
                    className="agentdash-form-input"
                  />
                </div>
                <div className="flex gap-2">
                  <button
                    type="button"
                    onClick={() => {
                      setIsEditingBasicInfo(false);
                      // 重置为原始值
                      if (story) {
                        setEditTitle(story.title);
                        setEditDescription(story.description || "");
                        setEditStatus(story.status);
                        setEditPriority(story.priority);
                        setEditStoryType(story.story_type);
                        setEditTags(story.tags.join(", "));
                      }
                    }}
                    className="agentdash-button-secondary flex-1"
                  >
                    取消
                  </button>
                  <button
                    type="button"
                    onClick={() => void handleSaveStory()}
                    className="agentdash-button-primary flex-1"
                  >
                    保存
                  </button>
                </div>
              </div>
            ) : (
              <div className="space-y-3.5">
                <div>
                  <span className="text-xs text-muted-foreground">标题</span>
                  <p className="mt-1 text-sm font-medium">{story.title}</p>
                </div>
                <div>
                  <span className="text-xs text-muted-foreground">描述</span>
                  <p className="mt-1 text-sm leading-6 text-foreground">
                    {story.description || <span className="text-muted-foreground">暂无描述</span>}
                  </p>
                </div>
                <div>
                  <span className="text-xs text-muted-foreground">类型</span>
                  <div className="mt-1.5">
                    <StoryTypeBadge type={story.story_type} />
                  </div>
                </div>
                <div>
                  <span className="text-xs text-muted-foreground">优先级</span>
                  <div className="mt-1.5">
                    <StoryPriorityBadge priority={story.priority} showLabel />
                  </div>
                </div>
                {story.tags.length > 0 && (
                  <div>
                    <span className="text-xs text-muted-foreground">标签</span>
                    <div className="mt-1.5 flex flex-wrap gap-1.5">
                      {story.tags.map((tag, index) => (
                        <span
                          key={index}
                          className="inline-flex items-center rounded-full border border-border bg-background px-2.5 py-1 text-xs text-muted-foreground"
                        >
                          {tag}
                        </span>
                      ))}
                    </div>
                  </div>
                )}
              </div>
            )}
          </DetailSection>

          {/* 状态流转操作 */}
          <div className="mt-3">
            <StoryStatusActions
              currentStatus={story.status}
              onStatusChange={(status) => void updateStory(story.id, { status })}
            />
          </div>

          {/* 创建 Task */}
          <div className="mt-3">
            <CreateTaskPanel
              story={story}
              storyId={story.id}
              workspaces={workspaces}
              projectConfig={currentProject?.config}
              onCreated={handleTaskCreated}
            />
          </div>

          {(formMessage || error) && <p className="mt-3 text-xs text-destructive">{formMessage || error}</p>}
        </div>

        {/* 右侧：Tab 内容 */}
        <div className="flex flex-1 flex-col overflow-hidden bg-background">
          {/* 可折叠上下文面板 */}
          <div className="shrink-0 border-b border-border">
            <button
              type="button"
              onClick={() => setIsContextExpanded((v) => !v)}
              className="flex w-full items-center justify-between px-5 py-2.5 text-xs text-muted-foreground transition-colors hover:bg-secondary/25"
            >
              <div className="flex items-center gap-2">
                <svg
                  className={`h-3.5 w-3.5 transition-transform ${isContextExpanded ? "rotate-90" : ""}`}
                  fill="none"
                  viewBox="0 0 24 24"
                  stroke="currentColor"
                >
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 5l7 7-7 7" />
                </svg>
                <span className="font-medium">上下文</span>
                {Array.from(contextSummary(story.context.source_refs)).map(([kind, count]) => {
                  const meta = sourceKindMeta(kind);
                  return (
                    <span key={kind} className={`rounded-full border border-current/20 px-1.5 py-0.5 text-[10px] font-medium ${meta.color}`}>
                      {meta.icon} {count} {meta.label}
                    </span>
                  );
                })}
                {story.context.prd_doc && (
                  <span className="rounded-full border border-border bg-secondary/50 px-1.5 py-0.5 text-[10px]">
                    PRD
                  </span>
                )}
                {story.context.context_containers.length > 0 && (
                  <span className="rounded-full border border-violet-400/30 bg-violet-500/10 px-1.5 py-0.5 text-[10px] font-medium text-violet-600">
                    📦 {story.context.context_containers.length} 容器
                  </span>
                )}
                {story.context.session_composition && (
                  <span className="rounded-full border border-cyan-400/30 bg-cyan-500/10 px-1.5 py-0.5 text-[10px] font-medium text-cyan-600">
                    🎭 会话编排
                  </span>
                )}
              </div>
            </button>
            {isContextExpanded && (
              <div className="max-h-[40vh] overflow-y-auto border-t border-border bg-secondary/10 px-5 py-4">
                <ContextPanel
                  story={story}
                  workspaces={workspaces}
                  projectConfig={currentProject?.config}
                />
              </div>
            )}
          </div>

          {/* Tab 导航 */}
          <div className="flex border-b border-border bg-secondary/35 px-2 pt-2">
            {tabs.map((tab) => (
              <button
                key={tab.key}
                type="button"
                onClick={() => setActiveTab(tab.key)}
                className={`rounded-t-[10px] px-5 py-3 text-sm transition-colors ${
                  activeTab === tab.key
                    ? "border border-border border-b-background bg-background font-medium text-foreground"
                    : "text-muted-foreground hover:text-foreground"
                }`}
              >
                {tab.label}
              </button>
            ))}
          </div>

          {/* Tab 内容 */}
          {activeTab === "sessions" ? (
            <div className="flex-1 overflow-hidden">
              <StorySessionPanel story={story} />
            </div>
          ) : (
            <div className="flex-1 overflow-y-auto p-6">
              {activeTab === "tasks" && (
                <DetailSection title="任务列表">
                  <TaskList
                    tasks={sortedTasks}
                    onTaskClick={(task) => {
                      setSelectedTaskId(task.id);
                    }}
                  />
                </DetailSection>
              )}
              {activeTab === "review" && <ReviewPanel story={story} tasks={sortedTasks} />}
            </div>
          )}
        </div>
      </div>

      <TaskDrawer
        key={selectedTask?.id ?? "no-task-selected"}
        task={selectedTask}
        projectId={story.project_id}
        workspaces={workspaces}
        projectConfig={currentProject?.config}
        onTaskUpdated={handleTaskUpdated}
        onTaskDeleted={handleTaskDeleted}
        onClose={() => setSelectedTaskId(null)}
      />

      {/* 删除确认对话框 */}
      <DangerConfirmDialog
        open={isDeleteConfirmOpen}
        title="删除 Story"
        description="Story 删除后其下 Task 会一起删除。"
        expectedValue={story.title}
        inputValue={deleteConfirmValue}
        onInputValueChange={setDeleteConfirmValue}
        confirmLabel="确认删除"
        onClose={() => {
          setIsDeleteConfirmOpen(false);
          setDeleteConfirmValue("");
        }}
        onConfirm={() => void handleDeleteStory()}
      />
    </div>
  );
}
