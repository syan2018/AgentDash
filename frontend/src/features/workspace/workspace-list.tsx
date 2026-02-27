import { useState } from "react";
import type { Workspace, WorkspaceType, WorkspaceStatus } from "../../types";
import { useWorkspaceStore } from "../../stores/workspaceStore";

// ─── 状态徽标 ─────────────────────────────────────────

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

function WorkspaceStatusBadge({ status }: { status: WorkspaceStatus }) {
  const cfg = statusConfig[status];
  return (
    <span className={`inline-flex rounded-full px-2 py-0.5 text-[10px] font-medium ${cfg.cls}`}>
      {status === "active" && <span className="mr-1 mt-0.5 inline-block h-1.5 w-1.5 animate-pulse rounded-full bg-current" />}
      {cfg.label}
    </span>
  );
}

// ─── 创建面板 ─────────────────────────────────────────

function CreateWorkspacePanel({ projectId, onDone }: { projectId: string; onDone: () => void }) {
  const { createWorkspace, detectGitInfo } = useWorkspaceStore();
  const [name, setName] = useState("");
  const [wsType, setWsType] = useState<WorkspaceType>("static");
  const [containerRef, setContainerRef] = useState("");
  const [sourceRepo, setSourceRepo] = useState("");
  const [branch, setBranch] = useState("main");
  const [commitHash, setCommitHash] = useState("");
  const [isDetectingGit, setIsDetectingGit] = useState(false);
  const [detectResult, setDetectResult] = useState<string | null>(null);

  const handleCreate = async () => {
    if (!name.trim()) return;
    await createWorkspace(projectId, name.trim(), {
      workspace_type: wsType,
      container_ref: containerRef.trim(),
      git_config: wsType === "git_worktree"
        ? {
          source_repo: sourceRepo,
          branch,
          commit_hash: commitHash.trim() || undefined,
        }
        : undefined,
    });
    setName("");
    setContainerRef("");
    setSourceRepo("");
    setBranch("main");
    setCommitHash("");
    setDetectResult(null);
    onDone();
  };

  const handleDetectGit = async (pathOverride?: string) => {
    const targetPath = (pathOverride ?? containerRef).trim();
    if (!targetPath) {
      setDetectResult("请先填写目录路径");
      return;
    }

    setIsDetectingGit(true);
    setDetectResult(null);

    try {
      const detected = await detectGitInfo(targetPath);
      if (!detected) {
        setDetectResult("Git 识别失败，请检查路径后重试");
        return;
      }

      if (!detected.is_git_repo) {
        setDetectResult("该目录不是 Git 仓库，可继续手动填写配置");
        return;
      }

      setWsType("git_worktree");
      setSourceRepo(detected.source_repo ?? targetPath);
      setBranch(detected.branch ?? "main");
      setCommitHash(detected.commit_hash ?? "");
      setDetectResult("Git 信息识别成功，已自动回填");
    } finally {
      setIsDetectingGit(false);
    }
  };

  const handleBrowseDirectory = () => {
    const input = document.createElement("input");
    input.type = "file";
    const directoryInput = input as HTMLInputElement & { webkitdirectory?: boolean };
    directoryInput.webkitdirectory = true;
    input.addEventListener("change", () => {
      const files = input.files;
      if (files && files.length > 0) {
        const firstFile = files[0] as File & { path?: string };
        const firstPath = firstFile.webkitRelativePath;
        const topDir = firstPath.split("/")[0];

        let detectedPath = topDir;
        if (typeof firstFile.path === "string" && firstFile.path.length > 0 && firstPath) {
          const relativePath = firstPath.replace(/\//g, "\\");
          if (firstFile.path.endsWith(relativePath)) {
            detectedPath = firstFile.path
              .slice(0, firstFile.path.length - relativePath.length)
              .replace(/[\\/]$/, "");
          } else {
            detectedPath = firstFile.path;
          }
        }

        setContainerRef(detectedPath);
        void handleDetectGit(detectedPath);
      }
    });
    input.click();
  };

  return (
    <div className="space-y-2 rounded-md border border-border bg-background p-2">
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
          placeholder="目录路径（如 /workspace/project）"
          className="flex-1 rounded border border-border bg-background px-2 py-1.5 text-sm outline-none ring-ring focus:ring-1"
        />
        <button
          type="button"
          onClick={handleBrowseDirectory}
          className="shrink-0 rounded border border-border bg-secondary px-2 py-1.5 text-xs text-foreground hover:bg-secondary/70"
          title="浏览目录"
        >
          📁
        </button>
        <button
          type="button"
          onClick={() => void handleDetectGit()}
          disabled={isDetectingGit || !containerRef.trim()}
          className="shrink-0 rounded border border-border bg-secondary px-2 py-1.5 text-xs text-foreground hover:bg-secondary/70 disabled:opacity-50"
          title="识别 Git 信息"
        >
          {isDetectingGit ? "识别中" : "识别Git"}
        </button>
      </div>

      {detectResult && (
        <p className="text-xs text-muted-foreground">{detectResult}</p>
      )}

      {wsType === "git_worktree" && (
        <>
          <input
            value={sourceRepo}
            onChange={(e) => setSourceRepo(e.target.value)}
            placeholder="源仓库路径"
            className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm outline-none ring-ring focus:ring-1"
          />
          <input
            value={branch}
            onChange={(e) => setBranch(e.target.value)}
            placeholder="分支名（默认 main）"
            className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm outline-none ring-ring focus:ring-1"
          />
          <input
            value={commitHash}
            onChange={(e) => setCommitHash(e.target.value)}
            placeholder="提交哈希（可选）"
            className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm outline-none ring-ring focus:ring-1"
          />
        </>
      )}

      <button
        type="button"
        onClick={() => void handleCreate()}
        disabled={!name.trim()}
        className="w-full rounded bg-primary px-3 py-1.5 text-sm text-primary-foreground disabled:opacity-50"
      >
        创建工作空间
      </button>
    </div>
  );
}

// ─── 列表组件 ─────────────────────────────────────────

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
          onClick={() => setShowCreate(!showCreate)}
          className="rounded px-1.5 py-0.5 text-xs text-muted-foreground hover:bg-secondary hover:text-foreground"
        >
          {showCreate ? "取消" : "+ 新建"}
        </button>
      </div>

      {showCreate && (
        <CreateWorkspacePanel projectId={projectId} onDone={() => setShowCreate(false)} />
      )}

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
          </div>
          <WorkspaceStatusBadge status={ws.status} />
        </div>
      ))}
    </div>
  );
}
