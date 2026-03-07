import { useEffect, useMemo, useState } from "react";
import { useLocation, useNavigate, useParams } from "react-router-dom";
import type {
  AgentBinding,
  ContextSourceRef,
  ProjectConfig,
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
import { FilePickerPopup } from "../features/file-reference";
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
import { listAddressEntries, listAddressSpaces, type AddressSpaceDescriptor } from "../services/addressSpaces";
import type { FileEntry } from "../services/workspaceFiles";
import {
  DangerConfirmDialog,
  DetailMenu,
  DetailSection,
} from "../components/ui/detail-panel";

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

type TabKey = "context" | "tasks" | "sessions" | "review";

interface StoryFileContextDraft {
  label: string;
  relPath: string;
  priority: number;
}

function isFileContextSource(source: ContextSourceRef): boolean {
  return source.kind === "file";
}

function getStoryFileContexts(story: Story): ContextSourceRef[] {
  return story.context.source_refs.filter(isFileContextSource);
}

function toStoryFileContextDraft(source: ContextSourceRef, index: number): StoryFileContextDraft {
  return {
    label: source.label?.trim() ?? "",
    relPath: source.locator,
    priority: Number.isFinite(source.priority) ? source.priority : 1000 - index,
  };
}

function buildStoryFileContextSource(draft: StoryFileContextDraft, index: number): ContextSourceRef {
  return {
    kind: "file",
    locator: draft.relPath.trim(),
    label: draft.label.trim() || null,
    slot: "references",
    priority: Number.isFinite(draft.priority) ? draft.priority : 1000 - index,
    required: false,
    max_chars: null,
    delivery: "resource",
  };
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
  const availableContexts = useMemo(() => getStoryFileContexts(story), [story]);

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
                  勾选后会把这些文件引用分配给 Task Agent，并在执行时由后端解析。
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
                        <span className="text-sm font-medium text-foreground">
                          {context.label?.trim() || `上下文 ${index + 1}`}
                        </span>
                      </div>
                      <p className="mt-1 whitespace-pre-wrap break-words text-xs leading-5 text-muted-foreground">
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
  const fileContexts = useMemo(() => getStoryFileContexts(story), [story]);
  const defaultWorkspaceId = useMemo(
    () => resolveDefaultWorkspaceId(projectConfig, workspaces),
    [projectConfig, workspaces],
  );
  const [isEditing, setIsEditing] = useState(false);
  const [drafts, setDrafts] = useState<StoryFileContextDraft[]>(() =>
    fileContexts.map((item, index) => toStoryFileContextDraft(item, index)),
  );
  const [message, setMessage] = useState<string | null>(null);
  const [isSaving, setIsSaving] = useState(false);
  const [fileSpace, setFileSpace] = useState<AddressSpaceDescriptor | null>(null);
  const [fileSpaceError, setFileSpaceError] = useState<string | null>(null);
  const [pickerOpen, setPickerOpen] = useState(false);
  const [pickerQuery, setPickerQuery] = useState("");
  const [pickerFiles, setPickerFiles] = useState<FileEntry[]>([]);
  const [pickerLoading, setPickerLoading] = useState(false);
  const [pickerError, setPickerError] = useState<string | null>(null);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [activeDraftIndex, setActiveDraftIndex] = useState<number | null>(null);
  const hasLegacyContent = ctx.prd_doc || ctx.spec_refs.length > 0 || ctx.resource_list.length > 0;

  useEffect(() => {
    if (isEditing) return;
    setDrafts(fileContexts.map((item, index) => toStoryFileContextDraft(item, index)));
  }, [fileContexts, isEditing]);

  useEffect(() => {
    setDrafts(fileContexts.map((item, index) => toStoryFileContextDraft(item, index)));
    setMessage(null);
    setIsEditing(false);
    setPickerOpen(false);
    setPickerQuery("");
    setPickerFiles([]);
    setPickerError(null);
    setSelectedIndex(0);
    setActiveDraftIndex(null);
  }, [story.id]);

  useEffect(() => {
    let cancelled = false;

    async function loadAvailableSpaces() {
      if (!defaultWorkspaceId) {
        setFileSpace(null);
        setFileSpaceError("当前 Project 尚未配置默认工作空间，暂时无法快捷选择文件。");
        return;
      }

      try {
        setFileSpaceError(null);
        const result = await listAddressSpaces({
          storyId: story.id,
          workspaceId: defaultWorkspaceId,
        });
        if (cancelled) return;
        const workspaceFileSpace = result.spaces.find((item) => item.id === "workspace_file") ?? null;
        setFileSpace(workspaceFileSpace);
        if (!workspaceFileSpace) {
          setFileSpaceError("当前环境未暴露工作空间文件寻址能力。");
        }
      } catch (err) {
        if (cancelled) return;
        setFileSpace(null);
        setFileSpaceError(err instanceof Error ? err.message : "加载寻址空间失败");
      }
    }

    void loadAvailableSpaces();
    return () => {
      cancelled = true;
    };
  }, [defaultWorkspaceId, story.id]);

  const updateDraft = (index: number, patch: Partial<StoryFileContextDraft>) => {
    setDrafts((current) => current.map((item, itemIndex) => (itemIndex === index ? { ...item, ...patch } : item)));
  };

  const addDraft = () => {
    setDrafts((current) => [
      ...current,
      { label: "", relPath: "", priority: 1000 - current.length },
    ]);
  };

  const removeDraft = (index: number) => {
    setDrafts((current) => current.filter((_, itemIndex) => itemIndex !== index));
  };

  const loadPickerFiles = async (query: string) => {
    if (!fileSpace || !defaultWorkspaceId) {
      setPickerFiles([]);
      setPickerError("当前没有可用的工作空间文件寻址能力");
      return;
    }

    setPickerLoading(true);
    setPickerError(null);
    try {
      const result = await listAddressEntries(fileSpace.id, {
        storyId: story.id,
        workspaceId: defaultWorkspaceId,
        query,
      });
      setPickerFiles(result.entries.filter((item) => item.isText));
      setSelectedIndex(0);
    } catch (err) {
      setPickerFiles([]);
      setPickerError(err instanceof Error ? err.message : "加载文件列表失败");
    } finally {
      setPickerLoading(false);
    }
  };

  const openDraftPicker = (index: number) => {
    setActiveDraftIndex(index);
    setPickerOpen(true);
    setPickerQuery("");
    setSelectedIndex(0);
    void loadPickerFiles("");
  };

  const closeDraftPicker = () => {
    setPickerOpen(false);
    setPickerQuery("");
    setPickerFiles([]);
    setPickerError(null);
    setSelectedIndex(0);
    setActiveDraftIndex(null);
  };

  const movePickerSelection = (delta: number) => {
    setSelectedIndex((current) => {
      const len = pickerFiles.length;
      if (len === 0) return 0;
      return (current + delta + len) % len;
    });
  };

  const handlePickerQueryChange = (query: string) => {
    setPickerQuery(query);
    void loadPickerFiles(query);
  };

  const handleDraftFileSelected = (file: FileEntry) => {
    if (activeDraftIndex == null) return;
    const fallbackLabel = file.relPath.split("/").pop() ?? file.relPath;
    setDrafts((current) =>
      current.map((item, index) =>
        index === activeDraftIndex
          ? {
              ...item,
              relPath: file.relPath,
              label: item.label.trim() ? item.label : fallbackLabel,
            }
          : item,
      ),
    );
    closeDraftPicker();
  };

  const handleSaveContexts = async () => {
    const normalizedDrafts = drafts
      .map((item) => ({
        ...item,
        label: item.label.trim(),
        relPath: item.relPath.trim(),
      }))
      .filter((item) => item.relPath);
    const nonFileSources = ctx.source_refs.filter((item) => !isFileContextSource(item));
    const nextSourceRefs = [
      ...nonFileSources,
      ...normalizedDrafts.map((item, index) => buildStoryFileContextSource(item, index)),
    ];

    setIsSaving(true);
    setMessage(null);
    try {
      const updated = await updateStory(story.id, {
        context_source_refs: nextSourceRefs,
      });
      if (!updated) {
        setMessage(error ?? "保存上下文失败");
        return;
      }
      setMessage("文件引用已保存到 Story，可用于伴生会话与 Task 分配");
      setIsEditing(false);
    } finally {
      setIsSaving(false);
    }
  };

  return (
    <DetailSection title="上下文">
      <div className="space-y-3">
        {!hasLegacyContent && fileContexts.length === 0 && !isEditing ? (
          <p className="rounded-[12px] border border-dashed border-border bg-secondary/25 px-3 py-6 text-center text-sm text-muted-foreground">
            暂无上下文条目
          </p>
        ) : null}

        {ctx.prd_doc && (
            <div className="rounded-[12px] border border-border bg-background p-3.5">
              <p className="mb-2 text-xs font-medium text-muted-foreground">PRD 文档</p>
              <pre className="whitespace-pre-wrap text-sm leading-6 text-foreground">{ctx.prd_doc}</pre>
            </div>
        )}

        {ctx.spec_refs.length > 0 && (
            <div className="rounded-[12px] border border-border bg-background p-3.5">
              <p className="mb-2 text-xs font-medium text-muted-foreground">规格引用</p>
              <ul className="space-y-1.5">
                {ctx.spec_refs.map((ref, index) => (
                  <li key={index} className="text-sm text-foreground">
                    <span className="mr-2 text-muted-foreground">·</span>
                    {ref}
                  </li>
                ))}
              </ul>
            </div>
        )}

        {ctx.resource_list.length > 0 && (
            <div className="rounded-[12px] border border-border bg-background p-3.5">
              <p className="mb-2 text-xs font-medium text-muted-foreground">资源列表</p>
              <div className="space-y-2">
                {ctx.resource_list.map((resource, index) => (
                  <div
                    key={index}
                    className="flex flex-wrap items-center gap-2 rounded-[10px] border border-border bg-secondary/35 px-3 py-2"
                  >
                    <span className="rounded-full border border-border bg-background px-2 py-0.5 text-[10px] uppercase text-muted-foreground">
                      {resource.resource_type}
                    </span>
                    <span className="text-sm font-medium text-foreground">{resource.name}</span>
                    <span className="min-w-0 break-all text-xs text-muted-foreground">{resource.uri}</span>
                  </div>
                ))}
              </div>
            </div>
        )}

        <div className="rounded-[12px] border border-border bg-background p-3.5">
            <div className="mb-3 flex items-center justify-between gap-2">
              <div>
                <p className="text-xs font-medium text-muted-foreground">工作区文件引用</p>
                <p className="mt-1 text-xs text-muted-foreground">
                  这些文件会暂存在 Story 上，伴生会话会自动解析它们，创建 Task 时也可按需分配给 Task Agent。
                </p>
                {fileSpace?.root && (
                  <p className="mt-1 break-all text-[11px] text-muted-foreground/80">
                    当前寻址空间：{fileSpace.label} · {fileSpace.root}
                  </p>
                )}
                {!fileSpace?.root && fileSpaceError && (
                  <p className="mt-1 text-[11px] text-amber-600">{fileSpaceError}</p>
                )}
              </div>
              {!isEditing ? (
                <button
                  type="button"
                  onClick={() => setIsEditing(true)}
                  className="rounded-[8px] border border-border bg-background px-2 py-1 text-xs text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
                >
                  编辑
                </button>
              ) : null}
            </div>

            {!isEditing ? (
              fileContexts.length > 0 ? (
                <div className="space-y-2">
                  {fileContexts.map((context, index) => (
                    <div
                      key={`${context.locator}-${index}`}
                      className="rounded-[10px] border border-border bg-secondary/25 px-3 py-2"
                    >
                      <div className="flex flex-wrap items-center gap-2">
                        <span className="text-sm font-medium text-foreground">
                          {context.label?.trim() || `文件引用 ${index + 1}`}
                        </span>
                      </div>
                      <p className="mt-1 whitespace-pre-wrap break-words text-sm leading-6 text-foreground">
                        {context.locator}
                      </p>
                    </div>
                  ))}
                </div>
              ) : (
                <p className="rounded-[10px] border border-dashed border-border bg-secondary/20 px-3 py-4 text-sm text-muted-foreground">
                  暂无文件引用。
                </p>
              )
            ) : (
              <div className="space-y-3">
                {drafts.length > 0 ? (
                  drafts.map((draft, index) => (
                    <div key={index} className="rounded-[10px] border border-border bg-secondary/20 p-3">
                      <div className="mb-2 flex items-center justify-between gap-2">
                        <span className="text-xs text-muted-foreground">文件引用 {index + 1}</span>
                        <button
                          type="button"
                          onClick={() => removeDraft(index)}
                          className="rounded-[8px] border border-border bg-background px-2 py-1 text-xs text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
                        >
                          删除
                        </button>
                      </div>

                      <div className="space-y-2">
                        <input
                          value={draft.label}
                          onChange={(event) => updateDraft(index, { label: event.target.value })}
                          placeholder="引用标题（可选）"
                          className="agentdash-form-input"
                        />
                        <div className="relative flex gap-2">
                          {pickerOpen && activeDraftIndex === index && (
                            <FilePickerPopup
                              open={pickerOpen}
                              query={pickerQuery}
                              files={pickerFiles}
                              loading={pickerLoading}
                              error={pickerError}
                              selectedIndex={selectedIndex}
                              placeholder={fileSpace?.selector?.placeholder ?? "搜索工作空间文件"}
                              emptyText={pickerQuery ? "没有匹配的工作空间文件" : "当前工作空间暂无可选文本文件"}
                              onQueryChange={handlePickerQueryChange}
                              onSelect={handleDraftFileSelected}
                              onClose={closeDraftPicker}
                              onMoveSelection={movePickerSelection}
                              onConfirmSelection={() => {
                                const file = pickerFiles[selectedIndex];
                                if (!file) return;
                                handleDraftFileSelected(file);
                              }}
                            />
                          )}
                          <input
                            value={draft.relPath}
                            onChange={(event) => updateDraft(index, { relPath: event.target.value })}
                            placeholder="例如: crates/agentdash-api/src/routes/stories.rs"
                            className="agentdash-form-input flex-1"
                          />
                          <button
                            type="button"
                            onClick={() => openDraftPicker(index)}
                            disabled={!fileSpace}
                            className="rounded-[10px] border border-border bg-background px-3 py-2 text-xs text-foreground transition-colors hover:bg-secondary disabled:cursor-not-allowed disabled:opacity-50"
                          >
                            选择文件
                          </button>
                        </div>
                      </div>
                    </div>
                  ))
                ) : (
                  <p className="rounded-[10px] border border-dashed border-border bg-secondary/20 px-3 py-4 text-sm text-muted-foreground">
                    还没有文件引用，点击下方按钮新增。
                  </p>
                )}

                <div className="flex flex-wrap items-center gap-2">
                  <button type="button" onClick={addDraft} className="agentdash-button-secondary">
                    新增文件引用
                  </button>
                  <button
                    type="button"
                    onClick={() => {
                      setDrafts(fileContexts.map((item, index) => toStoryFileContextDraft(item, index)));
                      setIsEditing(false);
                      setMessage(null);
                      closeDraftPicker();
                    }}
                    className="agentdash-button-secondary"
                  >
                    取消
                  </button>
                  <button
                    type="button"
                    onClick={() => void handleSaveContexts()}
                    disabled={isSaving}
                    className="agentdash-button-primary"
                  >
                    {isSaving ? "保存中..." : "保存上下文"}
                  </button>
                </div>
              </div>
            )}

            {message && <p className="mt-3 text-xs text-emerald-600">{message}</p>}
            {!message && error && isEditing && <p className="mt-3 text-xs text-destructive">{error}</p>}
        </div>
      </div>
    </DetailSection>
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

  const [activeTab, setActiveTab] = useState<TabKey>("context");
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
    { key: "context", label: "上下文" },
    { key: "tasks", label: "任务列表" },
    { key: "sessions", label: "会话" },
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
          <div className="flex-1 overflow-y-auto p-6">
            {activeTab === "context" && (
              <ContextPanel
                story={story}
                workspaces={workspaces}
                projectConfig={currentProject?.config}
              />
            )}
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
            {activeTab === "sessions" && (
              <DetailSection title="伴随会话" description="绑定到当前 Story 的协作与规划会话。">
                <StorySessionPanel story={story} showTitle={false} />
              </DetailSection>
            )}
            {activeTab === "review" && <ReviewPanel story={story} tasks={sortedTasks} />}
          </div>
        </div>
      </div>

      <TaskDrawer
        key={selectedTask?.id ?? "no-task-selected"}
        task={selectedTask}
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
