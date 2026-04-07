/**
 * Story 详情页的大型子面板：上下文管理面板 + 验收面板
 *
 * 从 StoryPage 提取，减少页面组件体积。
 */
import { useCallback, useEffect, useMemo, useState } from "react";
import type {
  ContextSourceRef,
  ProjectConfig,
  SessionComposition,
  Story,
  Task,
  Workspace,
} from "../../types";
import type { AddressEntry } from "../../services/addressSpaces";
import { StoryStatusBadge, StoryPriorityBadge, StoryTypeBadge } from "../../components/ui/status-badge";
import { DetailSection } from "../../components/ui/detail-panel";
import {
  ContextContainersEditor,
  DisabledContainerIdsEditor,
  SessionCompositionEditor,
} from "../../components/context-config-editor";
import {
  createDefaultSessionComposition,
} from "../../components/context-config-defaults";
import { AddressSpaceBrowser } from "../address-space";
import { resolveDefaultWorkspaceId } from "../task/agent-binding";
import { useAddressSpacePicker, AddressEntryPickerInline } from "../context-source";
import { useStoryStore } from "../../stores/storyStore";
import { sourceKindMeta } from "./context-source-utils";

// ─── Context source builder helpers ────────────────────

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

// ─── Override Editors ──────────────────────────────────

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

// ─── ContextPanel ──────────────────────────────────────

export function ContextPanel({
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
          domain="story"
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

      {/* 操作栏 */}
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

// ─── ReviewPanel ───────────────────────────────────────

export function ReviewPanel({ story, tasks }: { story: Story; tasks: Task[] }) {
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
