import { useState } from "react";
import type { Workspace, WorkspaceType, WorkspaceStatus } from "../../types";
import { useWorkspaceStore } from "../../stores/workspaceStore";

const statusConfig: Record<WorkspaceStatus, { label: string; cls: string }> = {
  pending: { label: "待创建", cls: "bg-muted text-muted-foreground" },
  preparing: { label: "准备中", cls: "bg-info/15 text-info" },
  ready: { label: "就绪", cls: "bg-success/15 text-success" },
  active: { label: "运行中", cls: "bg-primary/15 text-primary" },
  archived: { label: "已归档", cls: "bg-muted text-muted-foreground" },
  error: { label: "异常", cls: "bg-destructive/15 text-destructive" },
};

const typeLabels: Record<WorkspaceType, string> = {
  git_worktree: "Git Worktree",
  static: "静态目录",
  ephemeral: "临时环境",
};

const isLikelyAbsolutePath = (value: string) =>
  /^[a-zA-Z]:[\\/]/.test(value) || value.startsWith("/") || value.startsWith("\\\\");

function WorkspaceStatusBadge({ status }: { status: WorkspaceStatus }) {
  const cfg = statusConfig[status];
  return (
    <span className={`inline-flex rounded-full px-2 py-0.5 text-[10px] font-medium ${cfg.cls}`}>
      {status === "active" && (
        <span className="mr-1 mt-0.5 inline-block h-1.5 w-1.5 animate-pulse rounded-full bg-current" />
      )}
      {cfg.label}
    </span>
  );
}

function CreateWorkspacePanel({
  projectId,
  onDone,
  onCancel,
}: {
  projectId: string;
  onDone: () => void;
  onCancel: () => void;
}) {
  const { createWorkspace, pickDirectory, error } = useWorkspaceStore();
  const [name, setName] = useState("");
  const [wsType, setWsType] = useState<WorkspaceType>("static");
  const [containerRef, setContainerRef] = useState("");
  const [isPickingDirectory, setIsPickingDirectory] = useState(false);
  const [formMessage, setFormMessage] = useState<string | null>(null);

  const handleCreate = async () => {
    if (!name.trim()) {
      setFormMessage("请填写工作空间名称");
      return;
    }

    const path = containerRef.trim();
    if (!path && wsType !== "ephemeral") {
      setFormMessage("静态目录/Git Worktree 类型必须填写本地绝对路径");
      return;
    }
    if (path && !isLikelyAbsolutePath(path)) {
      setFormMessage("请填写本地绝对路径（例如 C:\\repo\\project 或 /Users/me/project）");
      return;
    }

    setFormMessage(null);
    const workspace = await createWorkspace(projectId, name.trim(), {
      workspace_type: wsType,
      container_ref: path || undefined,
    });
    if (!workspace) return;

    setName("");
    setContainerRef("");
    setFormMessage(null);
    onDone();
  };

  const handleBrowseDirectory = async () => {
    setIsPickingDirectory(true);
    setFormMessage(null);
    try {
      const pickedPath = await pickDirectory(containerRef);
      if (!pickedPath) {
        setFormMessage("已取消目录选择");
        return;
      }
      setContainerRef(pickedPath);
      setFormMessage(`已从后端选择并回填目录：${pickedPath}`);
    } finally {
      setIsPickingDirectory(false);
    }
  };

  return (
    <div className="space-y-3">
      <input
        value={name}
        onChange={(e) => setName(e.target.value)}
        placeholder="工作空间名称"
        className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm outline-none ring-ring focus:ring-1"
      />

      <select
        value={wsType}
        onChange={(e) => setWsType(e.target.value as WorkspaceType)}
        className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm outline-none ring-ring focus:ring-1"
      >
        <option value="static">静态目录</option>
        <option value="git_worktree">Git Worktree</option>
        <option value="ephemeral">临时环境</option>
      </select>

      <div className="flex gap-1">
        <input
          value={containerRef}
          onChange={(e) => setContainerRef(e.target.value)}
          placeholder="目录绝对路径（如 C:\\repo\\project 或 /workspace/project）"
          className="flex-1 rounded border border-border bg-background px-2 py-1.5 text-sm outline-none ring-ring focus:ring-1"
        />
        <button
          type="button"
          onClick={() => void handleBrowseDirectory()}
          disabled={isPickingDirectory}
          className="shrink-0 rounded border border-border bg-secondary px-2 py-1.5 text-xs text-foreground hover:bg-secondary/70 disabled:opacity-50"
          title="浏览目录"
        >
          {isPickingDirectory ? "选择中" : "📁"}
        </button>
      </div>

      <p className="text-[11px] text-muted-foreground">
        路径选择与 Git 识别均以后端为准；创建时会自动检测并保存 Git 信息。
      </p>
      {formMessage && <p className="text-xs text-muted-foreground">{formMessage}</p>}
      {error && <p className="text-xs text-destructive">创建失败：{error}</p>}

      <div className="flex items-center justify-end gap-2 border-t border-border pt-2">
        <button
          type="button"
          onClick={onCancel}
          className="rounded border border-border bg-secondary px-3 py-1.5 text-sm text-foreground hover:bg-secondary/70"
        >
          取消
        </button>
        <button
          type="button"
          onClick={() => void handleCreate()}
          disabled={!name.trim()}
          className="rounded bg-primary px-3 py-1.5 text-sm text-primary-foreground disabled:opacity-50"
        >
          创建工作空间
        </button>
      </div>
    </div>
  );
}

interface WorkspaceListProps {
  projectId: string;
  workspaces: Workspace[];
}

export function WorkspaceList({ projectId, workspaces }: WorkspaceListProps) {
  const [showCreate, setShowCreate] = useState(false);

  return (
    <div className="space-y-1">
      <div className="flex items-center justify-between px-2">
        <p className="text-xs uppercase tracking-wider text-muted-foreground">工作空间</p>
        <button
          type="button"
          onClick={() => setShowCreate(true)}
          className="rounded px-1.5 py-0.5 text-xs text-muted-foreground hover:bg-secondary hover:text-foreground"
        >
          + 新建
        </button>
      </div>

      {workspaces.length === 0 && !showCreate && (
        <p className="px-2 py-2 text-sm text-muted-foreground">暂无工作空间</p>
      )}

      {workspaces.map((ws) => (
        <div
          key={ws.id}
          className="flex items-center justify-between rounded-md px-3 py-2 text-sm hover:bg-secondary/50"
        >
          <div className="min-w-0 flex-1">
            <p className="truncate font-medium text-foreground">{ws.name}</p>
            <p className="truncate text-xs text-muted-foreground">
              {typeLabels[ws.workspace_type]} · {ws.container_ref || "未指定路径"}
            </p>
            {ws.git_config && (
              <p className="truncate text-xs text-muted-foreground">
                Git: {ws.git_config.branch} · {ws.git_config.source_repo}
              </p>
            )}
          </div>
          <WorkspaceStatusBadge status={ws.status} />
        </div>
      ))}

      {showCreate && (
        <>
          <div
            className="fixed inset-0 z-40 bg-foreground/30 backdrop-blur-[1px]"
            onClick={() => setShowCreate(false)}
          />
          <div className="fixed inset-0 z-50 flex items-center justify-center p-4">
            <div className="w-full max-w-2xl rounded-xl border border-border bg-background shadow-xl">
              <div className="flex items-center justify-between border-b border-border px-4 py-3">
                <h3 className="text-base font-semibold text-foreground">新建工作空间</h3>
                <button
                  type="button"
                  onClick={() => setShowCreate(false)}
                  className="rounded px-2 py-1 text-sm text-muted-foreground hover:bg-secondary"
                >
                  关闭
                </button>
              </div>
              <div className="p-4">
                <CreateWorkspacePanel
                  projectId={projectId}
                  onDone={() => setShowCreate(false)}
                  onCancel={() => setShowCreate(false)}
                />
              </div>
            </div>
          </div>
        </>
      )}
    </div>
  );
}
