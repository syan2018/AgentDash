import { useEffect, useMemo, useState } from "react";
import type { ProjectConfig, Story, Task, Workspace } from "../../types";
import { StoryStatusBadge } from "../../components/ui/status-badge";
import { TaskList } from "../task/task-list";
import { useStoryStore } from "../../stores/storyStore";

interface StoryDrawerProps {
  story: Story | null;
  tasks: Task[];
  workspaces: Workspace[];
  projectConfig?: ProjectConfig;
  onClose: () => void;
  onOpenTask: (task: Task) => void;
}

type DrawerTab = "context" | "tasks" | "review";

function ContextPanel({ story }: { story: Story }) {
  const ctx = story.context;
  const hasContent = ctx.prd_doc || ctx.spec_refs.length > 0 || ctx.resource_list.length > 0;

  return (
    <div className="space-y-4 p-6">
      <h4 className="text-sm font-medium text-foreground">上下文</h4>

      {!hasContent ? (
        <p className="rounded-md border border-dashed border-border px-3 py-6 text-center text-sm text-muted-foreground">
          暂无上下文条目
        </p>
      ) : (
        <>
          {ctx.prd_doc && (
            <div className="rounded-md border border-border bg-card p-3">
              <p className="mb-1 text-xs font-medium text-muted-foreground">PRD 文档</p>
              <pre className="whitespace-pre-wrap text-sm text-foreground">{ctx.prd_doc}</pre>
            </div>
          )}

          {ctx.spec_refs.length > 0 && (
            <div className="rounded-md border border-border bg-card p-3">
              <p className="mb-2 text-xs font-medium text-muted-foreground">规格引用</p>
              <ul className="space-y-1">
                {ctx.spec_refs.map((ref, i) => (
                  <li key={i} className="text-sm text-foreground">
                    <span className="mr-2 text-muted-foreground">·</span>{ref}
                  </li>
                ))}
              </ul>
            </div>
          )}

          {ctx.resource_list.length > 0 && (
            <div className="rounded-md border border-border bg-card p-3">
              <p className="mb-2 text-xs font-medium text-muted-foreground">资源列表</p>
              {ctx.resource_list.map((res, i) => (
                <div key={i} className="mb-1 flex items-center gap-2">
                  <span className="rounded bg-secondary px-2 py-0.5 text-[10px] uppercase text-muted-foreground">
                    {res.resource_type}
                  </span>
                  <span className="text-sm text-foreground">{res.name}</span>
                  <span className="text-xs text-muted-foreground">{res.uri}</span>
                </div>
              ))}
            </div>
          )}
        </>
      )}
    </div>
  );
}

function ReviewPanel({ story, tasks }: { story: Story; tasks: Task[] }) {
  const successCount = tasks.filter((task) => task.status === "succeeded").length;
  const failedCount = tasks.filter((task) => task.status === "failed").length;
  return (
    <div className="space-y-4 p-6">
      <h4 className="text-sm font-medium text-foreground">验收</h4>
      <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
        <div className="rounded-md border border-border bg-card p-3">
          <p className="text-xs text-muted-foreground">Story 状态</p>
          <p className="mt-1 text-sm font-medium text-foreground">{story.status}</p>
        </div>
        <div className="rounded-md border border-border bg-card p-3">
          <p className="text-xs text-muted-foreground">成功任务数</p>
          <p className="mt-1 text-sm font-medium text-success">{successCount}</p>
        </div>
        <div className="rounded-md border border-border bg-card p-3">
          <p className="text-xs text-muted-foreground">失败任务数</p>
          <p className="mt-1 text-sm font-medium text-destructive">{failedCount}</p>
        </div>
      </div>
      <p className="text-sm text-muted-foreground">{story.description || "暂无 Story 描述"}</p>
    </div>
  );
}

function CreateTaskDialog({
  open,
  storyId,
  workspaces,
  projectConfig,
  onClose,
  onCreated,
}: {
  open: boolean;
  storyId: string;
  workspaces: Workspace[];
  projectConfig?: ProjectConfig;
  onClose: () => void;
  onCreated: (task: Task) => void;
}) {
  const { createTask, error } = useStoryStore();
  const [title, setTitle] = useState("");
  const [description, setDescription] = useState("");
  const [workspaceId, setWorkspaceId] = useState("");
  const [agentType, setAgentType] = useState("");
  const [presetName, setPresetName] = useState("");
  const [isSubmitting, setIsSubmitting] = useState(false);

  useEffect(() => {
    if (!open) return;

    const defaultWorkspace = projectConfig?.default_workspace_id ?? "";
    const defaultAgent = projectConfig?.default_agent_type ?? "";
    setTitle("");
    setDescription("");
    setWorkspaceId(defaultWorkspace);
    setAgentType(defaultAgent);
    setPresetName("");
  }, [open, projectConfig, storyId]);

  const handlePresetChange = (value: string) => {
    setPresetName(value);
    if (!value || !projectConfig) return;
    const preset = projectConfig.agent_presets.find((item) => item.name === value);
    if (preset) {
      setAgentType(preset.agent_type);
    }
  };

  const handleSubmit = async () => {
    if (!title.trim()) return;
    setIsSubmitting(true);
    try {
      const task = await createTask(storyId, {
        title: title.trim(),
        description: description.trim() || undefined,
        workspace_id: workspaceId || null,
        agent_binding: {
          agent_type: agentType.trim() || null,
          preset_name: presetName || null,
        },
      });
      if (!task) return;

      onClose();
      onCreated(task);
    } finally {
      setIsSubmitting(false);
    }
  };

  if (!open) return null;

  return (
    <>
      <div className="fixed inset-0 z-40 bg-foreground/30 backdrop-blur-[1px]" onClick={onClose} />
      <div className="fixed inset-0 z-50 flex items-center justify-center p-4">
        <div className="w-full max-w-3xl rounded-xl border border-border bg-background shadow-xl">
          <header className="flex items-center justify-between border-b border-border px-4 py-3">
            <h4 className="text-base font-semibold text-foreground">创建 Task</h4>
            <button
              type="button"
              onClick={onClose}
              className="rounded px-2 py-1 text-sm text-muted-foreground hover:bg-secondary"
            >
              关闭
            </button>
          </header>

          <div className="space-y-3 p-4">
            <div className="grid grid-cols-1 gap-2 lg:grid-cols-2">
              <input
                value={title}
                onChange={(e) => setTitle(e.target.value)}
                placeholder="Task 标题"
                className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm outline-none ring-ring focus:ring-1"
              />
              <select
                value={workspaceId}
                onChange={(e) => setWorkspaceId(e.target.value)}
                className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm outline-none ring-ring focus:ring-1"
              >
                <option value="">不绑定 Workspace</option>
                {workspaces.map((workspace) => (
                  <option key={workspace.id} value={workspace.id}>
                    {workspace.name}
                  </option>
                ))}
              </select>
            </div>

            <textarea
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              rows={3}
              placeholder="描述（可选）"
              className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm outline-none ring-ring focus:ring-1"
            />

            <div className="grid grid-cols-1 gap-2 lg:grid-cols-2">
              <input
                value={agentType}
                onChange={(e) => setAgentType(e.target.value)}
                placeholder="Agent 类型（可选，留空则使用项目默认）"
                className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm outline-none ring-ring focus:ring-1"
              />
              <select
                value={presetName}
                onChange={(e) => handlePresetChange(e.target.value)}
                className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm outline-none ring-ring focus:ring-1"
              >
                <option value="">不使用预设</option>
                {(projectConfig?.agent_presets ?? []).map((preset) => (
                  <option key={preset.name} value={preset.name}>
                    {preset.name}
                  </option>
                ))}
              </select>
            </div>

            {error && (
              <p className="text-xs text-destructive">创建失败：{error}</p>
            )}

            <div className="flex items-center justify-end gap-2 border-t border-border pt-2">
              <button
                type="button"
                onClick={onClose}
                className="rounded border border-border bg-secondary px-3 py-1.5 text-sm text-foreground hover:bg-secondary/70"
              >
                取消
              </button>
              <button
                type="button"
                onClick={() => void handleSubmit()}
                disabled={isSubmitting || !title.trim()}
                className="rounded bg-primary px-3 py-1.5 text-sm text-primary-foreground disabled:opacity-50"
              >
                {isSubmitting ? "创建中..." : "创建 Task"}
              </button>
            </div>
          </div>
        </div>
      </div>
    </>
  );
}

export function StoryDrawer({
  story,
  tasks,
  workspaces,
  projectConfig,
  onClose,
  onOpenTask,
}: StoryDrawerProps) {
  const [activeTab, setActiveTab] = useState<DrawerTab>("context");
  const [isCreateTaskOpen, setIsCreateTaskOpen] = useState(false);

  const sortedTasks = useMemo(
    () =>
      [...tasks].sort(
        (a, b) => new Date(b.updated_at).getTime() - new Date(a.updated_at).getTime(),
      ),
    [tasks],
  );

  if (!story) return null;

  return (
    <>
      <div className="fixed inset-0 z-20 bg-foreground/10 backdrop-blur-[1px]" onClick={onClose} />
      <aside className="fixed inset-y-0 right-0 z-30 flex w-full max-w-[80rem] flex-col border-l border-border bg-background shadow-xl">
        <header className="flex items-center justify-between border-b border-border px-6 py-4">
          <div className="min-w-0">
            <div className="mb-1">
              <StoryStatusBadge status={story.status} />
            </div>
            <h3 className="truncate text-lg font-semibold text-foreground">{story.title}</h3>
          </div>
          <button type="button" onClick={onClose} className="rounded-md px-2 py-1 text-sm text-muted-foreground hover:bg-secondary">
            关闭
          </button>
        </header>

        <div className="flex border-b border-border bg-card">
          {[
            { key: "context", label: "上下文" },
            { key: "tasks", label: "任务列表" },
            { key: "review", label: "验收" },
          ].map((tab) => (
            <button
              key={tab.key}
              type="button"
              onClick={() => setActiveTab(tab.key as DrawerTab)}
              className={`px-5 py-3 text-sm ${
                activeTab === tab.key
                  ? "border-b-2 border-primary text-primary"
                  : "text-muted-foreground hover:text-foreground"
              }`}
            >
              {tab.label}
            </button>
          ))}
        </div>

        <div className="flex-1 overflow-y-auto">
          {activeTab === "context" && <ContextPanel story={story} />}
          {activeTab === "tasks" && (
            <div className="space-y-4 p-6">
              <div className="flex items-center justify-between">
                <h4 className="text-sm font-medium text-foreground">任务列表</h4>
                <button
                  type="button"
                  onClick={() => setIsCreateTaskOpen(true)}
                  className="rounded bg-primary px-3 py-1.5 text-sm text-primary-foreground"
                >
                  + 新建 Task
                </button>
              </div>
              <TaskList tasks={sortedTasks} onTaskClick={onOpenTask} />
            </div>
          )}
          {activeTab === "review" && <ReviewPanel story={story} tasks={sortedTasks} />}
        </div>
      </aside>

      <CreateTaskDialog
        open={isCreateTaskOpen}
        storyId={story.id}
        workspaces={workspaces}
        projectConfig={projectConfig}
        onClose={() => setIsCreateTaskOpen(false)}
        onCreated={onOpenTask}
      />
    </>
  );
}
