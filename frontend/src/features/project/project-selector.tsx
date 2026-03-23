import { useState } from "react";
import type {
  BackendConfig,
  Project,
  ProjectConfig,
  Workspace,
} from "../../types";
import { useProjectStore } from "../../stores/projectStore";
import { useCoordinatorStore } from "../../stores/coordinatorStore";
import { useWorkspaceStore } from "../../stores/workspaceStore";
import { WorkspaceList } from "../workspace/workspace-list";
import {
  DangerConfirmDialog,
  DetailMenu,
  DetailPanel,
  DetailSection,
} from "../../components/ui/detail-panel";
import {
  ContextContainersEditor,
  MountPolicyEditor,
  SessionCompositionEditor,
} from "../../components/context-config-editor";
import {
  createDefaultMountPolicy,
  createDefaultSessionComposition,
} from "../../components/context-config-defaults";
import { ProjectWorkflowPanel } from "../workflow/project-workflow-panel";
import { AgentPresetEditor } from "./agent-preset-editor";

interface ProjectSelectorProps {
  projects: Project[];
  currentProjectId: string | null;
  onSelect: (id: string) => void;
}

interface ProjectCreateDrawerProps {
  open: boolean;
  backends: BackendConfig[];
  onClose: () => void;
}

function ProjectCreateDrawer({ open, backends, onClose }: ProjectCreateDrawerProps) {
  const { createProject, error } = useProjectStore();
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [backendId, setBackendId] = useState(backends[0]?.id ?? "");
  const effectiveBackendId = backendId || backends[0]?.id || "";

  const handleCreate = async () => {
    if (!name.trim() || !effectiveBackendId) return;
    const created = await createProject(name.trim(), description.trim(), effectiveBackendId);
    if (!created) return;
    onClose();
  };

  return (
    <DetailPanel
      open={open}
      title="新建项目"
      subtitle="创建 Project 并绑定后端"
      onClose={onClose}
      widthClassName="max-w-2xl"
    >
      <div className="space-y-4 p-5">
        <DetailSection title="基础信息">
          <input
            value={name}
            onChange={(event) => setName(event.target.value)}
            placeholder="项目名称"
            className="agentdash-form-input"
          />
          <input
            value={description}
            onChange={(event) => setDescription(event.target.value)}
            placeholder="描述（可选）"
            className="agentdash-form-input"
          />
          <select
            value={effectiveBackendId}
            onChange={(event) => setBackendId(event.target.value)}
            className="agentdash-form-select"
          >
            <option value="">选择后端</option>
            {backends.map((backend) => (
              <option key={backend.id} value={backend.id}>
                {backend.name}
              </option>
            ))}
          </select>
        </DetailSection>

        {error && <p className="text-xs text-destructive">创建失败：{error}</p>}

        <div className="flex items-center justify-end border-t border-border pt-3">
          <button
            type="button"
            onClick={() => void handleCreate()}
            disabled={!name.trim() || !effectiveBackendId}
            className="agentdash-button-primary"
          >
            创建项目
          </button>
        </div>
      </div>
    </DetailPanel>
  );
}

// ─── 上下文编排 Tab ──────────────────────────────────

function ProjectContextTab({ project, onError }: { project: Project; onError: (msg: string | null) => void }) {
  const { updateProject } = useProjectStore();
  const [isSaving, setIsSaving] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const containers = project.config.context_containers ?? [];
  const mountPolicy = project.config.mount_policy ?? createDefaultMountPolicy();
  const composition = project.config.session_composition ?? createDefaultSessionComposition();

  const persistProjectContext = async (
    payload: Parameters<typeof updateProject>[1],
    successMessage: string,
  ) => {
    setIsSaving(true);
    setMessage(null);
    onError(null);
    try {
      const result = await updateProject(project.id, payload);
      if (!result) {
        onError("保存失败");
        return;
      }
      setMessage(successMessage);
    } finally {
      setIsSaving(false);
    }
  };

  return (
    <div className="space-y-4">
      <DetailSection title="上下文容器">
        <p className="text-xs text-muted-foreground">
          这里维护 Project 默认层：容器 provider、capabilities、session exposure 与 allowed_agent_types 都从这里起步。
        </p>
        <ContextContainersEditor
          value={containers}
          isSaving={isSaving}
          emptyText="暂无项目级容器"
          onSave={(next) => persistProjectContext({ context_containers: next }, "已保存 Project 容器")}
        />
      </DetailSection>

      <DetailSection title="挂载策略">
        <p className="text-xs text-muted-foreground">
          这部分决定默认是否挂本地 workspace，以及 workspace mount 能暴露哪些能力。
        </p>
        <MountPolicyEditor
          value={mountPolicy}
          isSaving={isSaving}
          onSave={(next) => persistProjectContext({ mount_policy: next }, "已保存挂载策略")}
        />
      </DetailSection>

      <DetailSection title="会话编排默认配置">
        <p className="text-xs text-muted-foreground">
          Persona、workflow 与 required_context_blocks 会作为默认会话编排，后续允许 Story 只覆盖其中一部分。
        </p>
        <SessionCompositionEditor
          value={composition}
          isSaving={isSaving}
          onSave={(next) => persistProjectContext({ session_composition: next }, "已保存默认会话编排")}
        />
      </DetailSection>

      {message && <p className="text-xs text-emerald-600">{message}</p>}
    </div>
  );
}

type ProjectDetailTab = "base" | "config" | "context" | "workflow" | "workspaces";

interface ProjectDetailDrawerProps {
  open: boolean;
  project: Project | null;
  backends: BackendConfig[];
  currentWorkspaces: Workspace[];
  onClose: () => void;
}

function ProjectDetailDrawer({
  open,
  project,
  backends,
  currentWorkspaces,
  onClose,
}: ProjectDetailDrawerProps) {
  const { updateProject, updateProjectConfig, deleteProject, error } = useProjectStore();
  const [activeTab, setActiveTab] = useState<ProjectDetailTab>("base");
  const [editName, setEditName] = useState(project?.name ?? "");
  const [editDescription, setEditDescription] = useState(project?.description ?? "");
  const [editBackendId, setEditBackendId] = useState(project?.backend_id ?? "");
  const [defaultAgentType, setDefaultAgentType] = useState(
    project?.config.default_agent_type ?? "",
  );
  const [defaultWorkspaceId, setDefaultWorkspaceId] = useState(
    project?.config.default_workspace_id ?? "",
  );
  const [isPresetSaving, setIsPresetSaving] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);
  const [isDeleteConfirmOpen, setIsDeleteConfirmOpen] = useState(false);
  const [deleteConfirmValue, setDeleteConfirmValue] = useState("");

  if (!project) return null;

  const handleSaveBase = async () => {
    const trimmedName = editName.trim();
    const trimmedBackendId = editBackendId.trim();
    if (!trimmedName || !trimmedBackendId) {
      setFormError("项目名称和后端不能为空");
      return;
    }

    const saved = await updateProject(project.id, {
      name: trimmedName,
      description: editDescription.trim(),
      backend_id: trimmedBackendId,
    });
    if (!saved) return;

    setFormError(null);
  };

  const handleSaveConfig = async () => {
    const saved = await updateProjectConfig(project.id, {
      default_agent_type: defaultAgentType.trim() || null,
      default_workspace_id: defaultWorkspaceId || null,
      agent_presets: project.config.agent_presets ?? [],
      context_containers: project.config.context_containers ?? [],
      mount_policy: project.config.mount_policy ?? { include_local_workspace: true, local_workspace_capabilities: [] },
      session_composition: project.config.session_composition ?? { workflow_steps: [], required_context_blocks: [] },
    });
    if (!saved) return;
    setFormError(null);
  };

  const handleSavePresets = async (nextPresets: ProjectConfig["agent_presets"]) => {
    setIsPresetSaving(true);
    try {
      const saved = await updateProjectConfig(project.id, {
        default_agent_type: project.config.default_agent_type ?? null,
        default_workspace_id: project.config.default_workspace_id ?? null,
        agent_presets: nextPresets,
        context_containers: project.config.context_containers ?? [],
        mount_policy: project.config.mount_policy ?? { include_local_workspace: true, local_workspace_capabilities: [] },
        session_composition: project.config.session_composition ?? { workflow_steps: [], required_context_blocks: [] },
      });
      if (!saved) setFormError("保存预设失败");
      else setFormError(null);
    } finally {
      setIsPresetSaving(false);
    }
  };

  const handleDeleteProject = async () => {
    if (deleteConfirmValue.trim() !== project.name) {
      setFormError("请输入完整项目名后再删除");
      return;
    }
    const deleted = await deleteProject(project.id);
    if (!deleted) {
      setFormError("删除失败，请查看错误信息后重试");
      return;
    }
    setIsDeleteConfirmOpen(false);
    onClose();
  };

  return (
    <>
      <DetailPanel
        open={open}
        title="项目详情"
        subtitle={`ID: ${project.id}`}
        onClose={onClose}
        widthClassName="max-w-3xl"
        headerExtra={
          <DetailMenu
            items={[
              {
                key: "delete",
                label: "删除项目",
                danger: true,
                onSelect: () => setIsDeleteConfirmOpen(true),
              },
            ]}
          />
        }
      >
        <div className="space-y-4 p-5">
          <div className="flex border-b border-border bg-secondary/35 px-2 pt-2">
            <button
              type="button"
              onClick={() => setActiveTab("base")}
              className={`rounded-t-[10px] px-5 py-3 text-sm transition-colors ${
                activeTab === "base"
                  ? "border border-border border-b-background bg-background font-medium text-foreground"
                  : "text-muted-foreground hover:text-foreground"
              }`}
            >
              基础信息
            </button>
            <button
              type="button"
              onClick={() => setActiveTab("config")}
              className={`rounded-t-[10px] px-5 py-3 text-sm transition-colors ${
                activeTab === "config"
                  ? "border border-border border-b-background bg-background font-medium text-foreground"
                  : "text-muted-foreground hover:text-foreground"
              }`}
            >
              项目配置
            </button>
            <button
              type="button"
              onClick={() => setActiveTab("context")}
              className={`rounded-t-[10px] px-5 py-3 text-sm transition-colors ${
                activeTab === "context"
                  ? "border border-border border-b-background bg-background font-medium text-foreground"
                  : "text-muted-foreground hover:text-foreground"
              }`}
            >
              上下文编排
            </button>
            <button
              type="button"
              onClick={() => setActiveTab("workflow")}
              className={`rounded-t-[10px] px-5 py-3 text-sm transition-colors ${
                activeTab === "workflow"
                  ? "border border-border border-b-background bg-background font-medium text-foreground"
                  : "text-muted-foreground hover:text-foreground"
              }`}
            >
              Workflow
            </button>
            <button
              type="button"
              onClick={() => setActiveTab("workspaces")}
              className={`rounded-t-[10px] px-5 py-3 text-sm transition-colors ${
                activeTab === "workspaces"
                  ? "border border-border border-b-background bg-background font-medium text-foreground"
                  : "text-muted-foreground hover:text-foreground"
              }`}
            >
              工作空间
            </button>
          </div>

          {activeTab === "base" && (
            <DetailSection title="基础信息">
              <input
                value={editName}
                onChange={(event) => setEditName(event.target.value)}
                placeholder="项目名称"
                className="agentdash-form-input"
              />
              <input
                value={editDescription}
                onChange={(event) => setEditDescription(event.target.value)}
                placeholder="项目描述"
                className="agentdash-form-input"
              />
              <select
                value={editBackendId}
                onChange={(event) => setEditBackendId(event.target.value)}
                className="agentdash-form-select"
              >
                <option value="">选择后端</option>
                {backends.map((backend) => (
                  <option key={backend.id} value={backend.id}>
                    {backend.name}
                  </option>
                ))}
              </select>
              <div className="flex justify-end">
                <button
                  type="button"
                  onClick={() => void handleSaveBase()}
                  className="agentdash-button-secondary"
                >
                  保存基础信息
                </button>
              </div>
            </DetailSection>
          )}

          {activeTab === "config" && (
            <DetailSection title="项目配置">
              <input
                value={defaultAgentType}
                onChange={(event) => setDefaultAgentType(event.target.value)}
                placeholder="默认 Agent 类型（可选）"
                className="agentdash-form-input"
              />
              <select
                value={defaultWorkspaceId}
                onChange={(event) => setDefaultWorkspaceId(event.target.value)}
                className="agentdash-form-select"
              >
                <option value="">默认 Workspace（可选）</option>
                {currentWorkspaces.map((workspace) => (
                  <option key={workspace.id} value={workspace.id}>
                    {workspace.name}
                  </option>
                ))}
              </select>
              <div className="flex justify-end">
                <button
                  type="button"
                  onClick={() => void handleSaveConfig()}
                  className="agentdash-button-primary"
                >
                  保存默认配置
                </button>
              </div>

              <div className="mt-2 border-t border-border pt-3">
                <p className="mb-2 text-sm font-medium text-foreground">Agent 预设</p>
                <AgentPresetEditor
                  presets={project.config.agent_presets ?? []}
                  onSave={handleSavePresets}
                  isSaving={isPresetSaving}
                />
              </div>
            </DetailSection>
          )}

          {activeTab === "context" && (
            <ProjectContextTab
              project={project}
              onError={setFormError}
            />
          )}

          {activeTab === "workflow" && (
            <DetailSection title="Workflow 平台">
              <ProjectWorkflowPanel projectId={project.id} />
            </DetailSection>
          )}

          {activeTab === "workspaces" && (
            <DetailSection title="工作空间">
              <WorkspaceList projectId={project.id} workspaces={currentWorkspaces} />
            </DetailSection>
          )}

          {(formError || error) && (
            <p className="text-xs text-destructive">保存失败：{formError || error}</p>
          )}
        </div>
      </DetailPanel>

      <DangerConfirmDialog
        open={isDeleteConfirmOpen}
        title="删除项目"
        description="项目删除后不可恢复，请确认。"
        expectedValue={project.name}
        inputValue={deleteConfirmValue}
        onInputValueChange={setDeleteConfirmValue}
        confirmLabel="确认删除"
        onClose={() => {
          setIsDeleteConfirmOpen(false);
          setDeleteConfirmValue("");
        }}
        onConfirm={() => void handleDeleteProject()}
      />
    </>
  );
}

export function ProjectSelector({
  projects,
  currentProjectId,
  onSelect,
}: ProjectSelectorProps) {
  const [isCreateOpen, setIsCreateOpen] = useState(false);
  const [isDetailOpen, setIsDetailOpen] = useState(false);
  const [detailProjectId, setDetailProjectId] = useState<string | null>(null);
  const [focusedProjectId, setFocusedProjectId] = useState<string | null>(null);
  const { backends } = useCoordinatorStore();
  const { workspacesByProjectId } = useWorkspaceStore();

  const currentProject = projects.find((project) => project.id === currentProjectId) ?? null;
  const detailProject =
    projects.find((project) => project.id === detailProjectId) ?? currentProject;
  const detailWorkspaces = detailProject
    ? (workspacesByProjectId[detailProject.id] ?? [])
    : [];

  return (
    <>
      <div className="space-y-2 rounded-[12px] border border-border bg-secondary/35 p-2.5">
        <div className="flex items-center justify-between px-1">
          <p className="text-[11px] uppercase tracking-[0.14em] text-muted-foreground">项目</p>
          <button
            type="button"
            onClick={() => setIsCreateOpen(true)}
            className="rounded-[8px] border border-border bg-background px-2 py-1 text-xs text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
          >
            + 新建
          </button>
        </div>

        {projects.length === 0 && (
          <p className="rounded-[10px] border border-dashed border-border px-3 py-3 text-sm text-muted-foreground">暂无项目</p>
        )}

        {projects.map((project) => {
          const isActive = currentProjectId === project.id;
          const isFocused = focusedProjectId === project.id;
          const showDetail = isActive || isFocused;

          return (
            <div
              key={project.id}
              className={`flex items-center justify-between rounded-[10px] border px-3 py-2.5 text-sm transition-colors ${
                isActive
                  ? "border-primary/20 bg-background"
                  : "border-transparent bg-transparent hover:border-border hover:bg-background/80"
              }`}
              onMouseEnter={() => setFocusedProjectId(project.id)}
              onMouseLeave={() => setFocusedProjectId((value) => (value === project.id ? null : value))}
              onFocusCapture={() => setFocusedProjectId(project.id)}
              onBlurCapture={(event) => {
                const nextTarget = event.relatedTarget as Node | null;
                if (!nextTarget || !event.currentTarget.contains(nextTarget)) {
                  setFocusedProjectId((value) => (value === project.id ? null : value));
                }
              }}
            >
              <button
                type="button"
                onClick={() => onSelect(project.id)}
                className="min-w-0 flex-1 text-left text-foreground"
              >
                <p className="truncate font-medium">{project.name}</p>
                <p className="truncate text-xs text-muted-foreground">
                  {project.description || `后端: ${project.backend_id}`}
                </p>
              </button>
              {showDetail && (
                <button
                  type="button"
                  onClick={() => {
                    onSelect(project.id);
                    setDetailProjectId(project.id);
                    setIsDetailOpen(true);
                  }}
                  className="ml-2 inline-flex h-7 w-7 items-center justify-center rounded-[8px] border border-border bg-secondary text-sm leading-none text-muted-foreground transition-colors hover:text-foreground"
                  aria-label="查看项目详情"
                  title="查看项目详情"
                >
                  ⋯
                </button>
              )}
            </div>
          );
        })}
      </div>

      <ProjectCreateDrawer
        key={`project-create-${isCreateOpen ? "open" : "closed"}-${backends[0]?.id ?? "none"}`}
        open={isCreateOpen}
        backends={backends}
        onClose={() => setIsCreateOpen(false)}
      />

      <ProjectDetailDrawer
        key={`project-detail-${isDetailOpen ? "open" : "closed"}-${detailProject?.id ?? "none"}`}
        open={isDetailOpen}
        project={detailProject}
        backends={backends}
        currentWorkspaces={detailWorkspaces}
        onClose={() => {
          setIsDetailOpen(false);
          setDetailProjectId(null);
        }}
      />
    </>
  );
}
