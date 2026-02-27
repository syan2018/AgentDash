import { useState } from "react";
import type { Workspace, WorkspaceStatus, WorkspaceType } from "../../types";
import { useWorkspaceStore } from "../../stores/workspaceStore";
import {
  DangerConfirmDialog,
  DetailMenu,
  DetailPanel,
  DetailSection,
} from "../../components/ui/detail-panel";

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

interface WorkspaceDetailDrawerProps {
  open: boolean;
  projectId: string;
  mode: "create" | "detail";
  workspace: Workspace | null;
  onClose: () => void;
}

function WorkspaceDetailDrawer({
  open,
  projectId,
  mode,
  workspace,
  onClose,
}: WorkspaceDetailDrawerProps) {
  const {
    createWorkspace,
    updateWorkspace,
    updateStatus,
    deleteWorkspace,
    pickDirectory,
    error,
  } = useWorkspaceStore();

  const [name, setName] = useState(mode === "detail" && workspace ? workspace.name : "");
  const [workspaceType, setWorkspaceType] = useState<WorkspaceType>(
    mode === "detail" && workspace ? workspace.workspace_type : "static",
  );
  const [containerRef, setContainerRef] = useState(
    mode === "detail" && workspace ? workspace.container_ref : "",
  );
  const [workspaceStatus, setWorkspaceStatus] = useState<WorkspaceStatus>(
    mode === "detail" && workspace ? workspace.status : "pending",
  );
  const [formMessage, setFormMessage] = useState<string | null>(null);
  const [isPickingDirectory, setIsPickingDirectory] = useState(false);
  const [isDeleteConfirmOpen, setIsDeleteConfirmOpen] = useState(false);
  const [deleteConfirmValue, setDeleteConfirmValue] = useState("");

  const handleSave = async () => {
    const trimmedName = name.trim();
    const trimmedPath = containerRef.trim();

    if (!trimmedName) {
      setFormMessage("请填写工作空间名称");
      return;
    }
    if (!trimmedPath && workspaceType !== "ephemeral") {
      setFormMessage("静态目录/Git Worktree 类型必须填写本地绝对路径");
      return;
    }
    if (trimmedPath && !isLikelyAbsolutePath(trimmedPath)) {
      setFormMessage("请填写本地绝对路径（例如 C:\\repo\\project 或 /Users/me/project）");
      return;
    }

    setFormMessage(null);

    if (mode === "create") {
      const created = await createWorkspace(projectId, trimmedName, {
        workspace_type: workspaceType,
        container_ref: trimmedPath || undefined,
      });
      if (!created) return;
      onClose();
      return;
    }

    if (!workspace) return;
    const updated = await updateWorkspace(workspace.id, projectId, {
      name: trimmedName,
      container_ref: trimmedPath || "",
      workspace_type: workspaceType,
    });
    if (!updated) return;

    if (workspace.status !== workspaceStatus) {
      await updateStatus(workspace.id, workspaceStatus);
    }

    onClose();
  };

  const handleDelete = async () => {
    if (!workspace) return;
    if (deleteConfirmValue.trim() !== workspace.name) {
      setFormMessage("请输入完整工作空间名后再删除");
      return;
    }
    await deleteWorkspace(workspace.id, projectId);
    setIsDeleteConfirmOpen(false);
    onClose();
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
      setFormMessage(`已选择目录：${pickedPath}`);
    } finally {
      setIsPickingDirectory(false);
    }
  };

  const title = mode === "create" ? "新建工作空间" : "工作空间详情";
  const subtitle =
    mode === "create"
      ? "创建后由后端自动检测并保存 Git 信息"
      : workspace
        ? `ID: ${workspace.id}`
        : undefined;

  return (
    <>
      <DetailPanel
        open={open}
        title={title}
        subtitle={subtitle}
        onClose={onClose}
        widthClassName="max-w-2xl"
        headerExtra={
          mode === "detail" && workspace ? (
            <DetailMenu
              items={[
                {
                  key: "delete",
                  label: "删除工作空间",
                  danger: true,
                  onSelect: () => setIsDeleteConfirmOpen(true),
                },
              ]}
            />
          ) : undefined
        }
      >
        <div className="space-y-4 p-5">
          <DetailSection title="基础信息">
            <input
              value={name}
              onChange={(event) => setName(event.target.value)}
              placeholder="工作空间名称"
              className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm outline-none ring-ring focus:ring-1"
            />
            <select
              value={workspaceType}
              onChange={(event) => setWorkspaceType(event.target.value as WorkspaceType)}
              className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm outline-none ring-ring focus:ring-1"
            >
              <option value="static">静态目录</option>
              <option value="git_worktree">Git Worktree</option>
              <option value="ephemeral">临时环境</option>
            </select>
            {mode === "detail" && (
              <select
                value={workspaceStatus}
                onChange={(event) => setWorkspaceStatus(event.target.value as WorkspaceStatus)}
                className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm outline-none ring-ring focus:ring-1"
              >
                <option value="pending">待创建</option>
                <option value="preparing">准备中</option>
                <option value="ready">就绪</option>
                <option value="active">运行中</option>
                <option value="archived">已归档</option>
                <option value="error">异常</option>
              </select>
            )}

            <div className="flex gap-2">
              <input
                value={containerRef}
                onChange={(event) => setContainerRef(event.target.value)}
                placeholder="目录绝对路径（ephemeral 可留空）"
                className="flex-1 rounded border border-border bg-background px-2 py-1.5 text-sm outline-none ring-ring focus:ring-1"
              />
              <button
                type="button"
                onClick={() => void handleBrowseDirectory()}
                disabled={isPickingDirectory}
                className="shrink-0 rounded border border-border bg-secondary px-2 py-1.5 text-xs text-foreground hover:bg-secondary/70 disabled:opacity-50"
              >
                {isPickingDirectory ? "选择中" : "浏览目录"}
              </button>
            </div>
          </DetailSection>

          {workspace?.git_config && (
            <DetailSection title="Git 信息">
              <p className="text-xs text-muted-foreground">
                仓库:{" "}
                <span className="font-mono text-foreground">
                  {workspace.git_config.source_repo}
                </span>
              </p>
              <p className="text-xs text-muted-foreground">
                分支:{" "}
                <span className="font-mono text-foreground">{workspace.git_config.branch}</span>
              </p>
              <p className="text-xs text-muted-foreground">
                Commit:{" "}
                <span className="font-mono text-foreground">
                  {workspace.git_config.commit_hash ?? "HEAD"}
                </span>
              </p>
            </DetailSection>
          )}

          {(formMessage || error) && (
            <p className="text-xs text-muted-foreground">{formMessage || error}</p>
          )}

          <div className="flex items-center justify-end border-t border-border pt-3">
            <button
              type="button"
              onClick={() => void handleSave()}
              className="rounded bg-primary px-3 py-1.5 text-sm text-primary-foreground"
            >
              {mode === "create" ? "创建工作空间" : "保存变更"}
            </button>
          </div>
        </div>
      </DetailPanel>

      {workspace && (
        <DangerConfirmDialog
          open={isDeleteConfirmOpen}
          title="删除工作空间"
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
      )}
    </>
  );
}

interface WorkspaceListProps {
  projectId: string;
  workspaces: Workspace[];
}

export function WorkspaceList({ projectId, workspaces }: WorkspaceListProps) {
  const [isCreateOpen, setIsCreateOpen] = useState(false);
  const [selectedWorkspace, setSelectedWorkspace] = useState<Workspace | null>(null);

  return (
    <>
      <div className="space-y-1">
        <div className="flex items-center justify-between px-2">
          <p className="text-xs uppercase tracking-wider text-muted-foreground">工作空间</p>
          <button
            type="button"
            onClick={() => setIsCreateOpen(true)}
            className="rounded px-1.5 py-0.5 text-xs text-muted-foreground hover:bg-secondary hover:text-foreground"
          >
            + 新建
          </button>
        </div>

        {workspaces.length === 0 && (
          <p className="px-2 py-2 text-sm text-muted-foreground">暂无工作空间</p>
        )}

        {workspaces.map((workspace) => (
          <button
            key={workspace.id}
            type="button"
            onClick={() => setSelectedWorkspace(workspace)}
            className="flex w-full items-center justify-between rounded-md px-3 py-2 text-left text-sm hover:bg-secondary/50"
          >
            <div className="min-w-0 flex-1">
              <p className="truncate font-medium text-foreground">{workspace.name}</p>
              <p className="truncate text-xs text-muted-foreground">
                {typeLabels[workspace.workspace_type]} · {workspace.container_ref || "未指定路径"}
              </p>
              {workspace.git_config && (
                <p className="truncate text-xs text-muted-foreground">
                  Git: {workspace.git_config.branch} · {workspace.git_config.source_repo}
                </p>
              )}
            </div>
            <div className="ml-2 flex items-center gap-2">
              <WorkspaceStatusBadge status={workspace.status} />
              <span className="text-xs text-muted-foreground">详情</span>
            </div>
          </button>
        ))}
      </div>

      <WorkspaceDetailDrawer
        key={`workspace-create-${isCreateOpen ? "open" : "closed"}-${projectId}`}
        open={isCreateOpen}
        projectId={projectId}
        mode="create"
        workspace={null}
        onClose={() => setIsCreateOpen(false)}
      />

      <WorkspaceDetailDrawer
        key={`workspace-detail-${selectedWorkspace?.id ?? "none"}-${selectedWorkspace ? "open" : "closed"}`}
        open={Boolean(selectedWorkspace)}
        projectId={projectId}
        mode="detail"
        workspace={selectedWorkspace}
        onClose={() => setSelectedWorkspace(null)}
      />
    </>
  );
}
