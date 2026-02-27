import { useState } from "react";
import type { BackendConfig, Project, ProjectConfig, Workspace } from "../../types";
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
            className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm outline-none ring-ring focus:ring-1"
          />
          <input
            value={description}
            onChange={(event) => setDescription(event.target.value)}
            placeholder="描述（可选）"
            className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm outline-none ring-ring focus:ring-1"
          />
          <select
            value={effectiveBackendId}
            onChange={(event) => setBackendId(event.target.value)}
            className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm outline-none ring-ring focus:ring-1"
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
            className="rounded bg-primary px-3 py-1.5 text-sm text-primary-foreground disabled:opacity-50"
          >
            创建项目
          </button>
        </div>
      </div>
    </DetailPanel>
  );
}

type ProjectDetailTab = "base" | "config" | "workspaces";

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
  const [agentPresetsJson, setAgentPresetsJson] = useState(
    JSON.stringify(project?.config.agent_presets ?? [], null, 2),
  );
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
    let parsedPresets: ProjectConfig["agent_presets"] = [];

    try {
      const parsed = JSON.parse(agentPresetsJson.trim() || "[]") as unknown;
      if (!Array.isArray(parsed)) {
        setFormError("agent_presets 必须是数组");
        return;
      }
      parsedPresets = parsed.map((item, index) => {
        if (!item || typeof item !== "object") {
          throw new Error(`第 ${index + 1} 项必须是对象`);
        }
        const preset = item as Record<string, unknown>;
        const presetName = typeof preset.name === "string" ? preset.name.trim() : "";
        const presetAgentType =
          typeof preset.agent_type === "string" ? preset.agent_type.trim() : "";

        if (!presetName || !presetAgentType) {
          throw new Error(`第 ${index + 1} 项缺少 name 或 agent_type`);
        }

        return {
          name: presetName,
          agent_type: presetAgentType,
          config:
            preset.config && typeof preset.config === "object"
              ? (preset.config as Record<string, unknown>)
              : {},
        };
      });
    } catch (parseErr) {
      setFormError(
        parseErr instanceof Error ? parseErr.message : "agent_presets JSON 解析失败",
      );
      return;
    }

    const saved = await updateProjectConfig(project.id, {
      default_agent_type: defaultAgentType.trim() || null,
      default_workspace_id: defaultWorkspaceId || null,
      agent_presets: parsedPresets,
    });
    if (!saved) return;

    setFormError(null);
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
          <div className="flex border-b border-border bg-card">
            <button
              type="button"
              onClick={() => setActiveTab("base")}
              className={`px-5 py-3 text-sm ${
                activeTab === "base"
                  ? "border-b-2 border-primary text-primary"
                  : "text-muted-foreground hover:text-foreground"
              }`}
            >
              基础信息
            </button>
            <button
              type="button"
              onClick={() => setActiveTab("config")}
              className={`px-5 py-3 text-sm ${
                activeTab === "config"
                  ? "border-b-2 border-primary text-primary"
                  : "text-muted-foreground hover:text-foreground"
              }`}
            >
              项目配置
            </button>
            <button
              type="button"
              onClick={() => setActiveTab("workspaces")}
              className={`px-5 py-3 text-sm ${
                activeTab === "workspaces"
                  ? "border-b-2 border-primary text-primary"
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
                className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm outline-none ring-ring focus:ring-1"
              />
              <input
                value={editDescription}
                onChange={(event) => setEditDescription(event.target.value)}
                placeholder="项目描述"
                className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm outline-none ring-ring focus:ring-1"
              />
              <select
                value={editBackendId}
                onChange={(event) => setEditBackendId(event.target.value)}
                className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm outline-none ring-ring focus:ring-1"
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
                  className="rounded border border-border bg-secondary px-3 py-1.5 text-sm text-foreground hover:bg-secondary/70"
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
                className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm outline-none ring-ring focus:ring-1"
              />
              <select
                value={defaultWorkspaceId}
                onChange={(event) => setDefaultWorkspaceId(event.target.value)}
                className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm outline-none ring-ring focus:ring-1"
              >
                <option value="">默认 Workspace（可选）</option>
                {currentWorkspaces.map((workspace) => (
                  <option key={workspace.id} value={workspace.id}>
                    {workspace.name}
                  </option>
                ))}
              </select>
              <textarea
                value={agentPresetsJson}
                onChange={(event) => setAgentPresetsJson(event.target.value)}
                rows={8}
                placeholder='agent_presets JSON，例如 [{"name":"默认","agent_type":"claude-code","config":{}}]'
                className="w-full rounded border border-border bg-background px-2 py-1.5 text-xs font-mono outline-none ring-ring focus:ring-1"
              />
              <div className="flex justify-end">
                <button
                  type="button"
                  onClick={() => void handleSaveConfig()}
                  className="rounded bg-primary px-3 py-1.5 text-sm text-primary-foreground"
                >
                  保存配置
                </button>
              </div>
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
      <div className="space-y-2">
        <div className="flex items-center justify-between px-2">
          <p className="text-xs uppercase tracking-wider text-muted-foreground">项目</p>
          <button
            type="button"
            onClick={() => setIsCreateOpen(true)}
            className="rounded px-1.5 py-0.5 text-xs text-muted-foreground hover:bg-secondary hover:text-foreground"
          >
            + 新建
          </button>
        </div>

        {projects.length === 0 && (
          <p className="px-2 py-2 text-sm text-muted-foreground">暂无项目</p>
        )}

        {projects.map((project) => {
          const isActive = currentProjectId === project.id;
          const isFocused = focusedProjectId === project.id;
          const showDetail = isActive || isFocused;

          return (
            <div
              key={project.id}
              className={`flex items-center justify-between rounded-md px-3 py-2 text-sm ${
                isActive ? "bg-primary text-primary-foreground" : "hover:bg-secondary/50"
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
                className={`min-w-0 flex-1 text-left ${
                  isActive ? "text-primary-foreground" : "text-foreground"
                }`}
              >
                <p className="truncate font-medium">{project.name}</p>
                <p className={`truncate text-xs ${isActive ? "opacity-85" : "text-muted-foreground"}`}>
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
                  className={`ml-2 h-6 w-6 rounded text-sm leading-none ${
                    isActive
                      ? "text-primary-foreground/90 hover:bg-primary-foreground/15"
                      : "text-muted-foreground hover:bg-secondary hover:text-foreground"
                  }`}
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
