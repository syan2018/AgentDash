import { useEffect, useState } from "react";
import type {
  DirectoryGroup,
  DirectoryUser,
  Project,
  ProjectConfig,
  ProjectRole,
  ProjectSubjectGrant,
  Workspace,
} from "../../types";
import { useProjectStore } from "../../stores/projectStore";
import { useCoordinatorStore } from "../../stores/coordinatorStore";
import { useCurrentUserStore } from "../../stores/currentUserStore";
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
import { AddressSpaceBrowser } from "../address-space";
import { fetchDirectoryGroups, fetchDirectoryUsers } from "../../services/directory";

interface ProjectSelectorProps {
  projects: Project[];
  currentProjectId: string | null;
  onSelect: (id: string) => void;
}

interface ProjectCreateDrawerProps {
  open: boolean;
  onClose: () => void;
}

const PROJECT_ROLE_OPTIONS: Array<{ value: ProjectRole; label: string }> = [
  { value: "owner", label: "Owner" },
  { value: "editor", label: "Editor" },
  { value: "viewer", label: "Viewer" },
];

const PROJECT_ROLE_LABELS: Record<ProjectRole, string> = {
  owner: "Owner",
  editor: "Editor",
  viewer: "Viewer",
};

const PROJECT_VISIBILITY_LABELS: Record<Project["visibility"], string> = {
  private: "私有",
  template_visible: "模板可见",
};

function describeProjectAccess(project: Project): string {
  if (project.access.via_admin_bypass) {
    return "管理员旁路";
  }
  if (project.access.role) {
    return PROJECT_ROLE_LABELS[project.access.role];
  }
  if (project.access.via_template_visibility) {
    return "模板访客";
  }
  return "仅查看";
}

function resolveGrantSubjectLabel(
  grant: ProjectSubjectGrant,
  users: DirectoryUser[],
  groups: DirectoryGroup[],
): string {
  if (grant.subject_type === "user") {
    const user = users.find((item) => item.user_id === grant.subject_id);
    return user?.display_name?.trim() || user?.email?.trim() || grant.subject_id;
  }

  const group = groups.find((item) => item.group_id === grant.subject_id);
  return group?.display_name?.trim() || grant.subject_id;
}

function ProjectCreateDrawer({ open, onClose }: ProjectCreateDrawerProps) {
  const { createProject, error } = useProjectStore();
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");

  const handleCreate = async () => {
    if (!name.trim()) return;
    const created = await createProject(name.trim(), description.trim());
    if (!created) return;
    onClose();
  };

  return (
    <DetailPanel
      open={open}
      title="新建项目"
      subtitle="创建 Project"
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
        </DetailSection>

        {error && <p className="text-xs text-destructive">创建失败：{error}</p>}

        <div className="flex items-center justify-end border-t border-border pt-3">
          <button
            type="button"
            onClick={() => void handleCreate()}
            disabled={!name.trim()}
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

function ProjectContextTab({
  project,
  canEdit,
  onError,
}: {
  project: Project;
  canEdit: boolean;
  onError: (msg: string | null) => void;
}) {
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
    if (!canEdit) return;
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

  if (!canEdit) {
    return (
      <DetailSection title="上下文编排">
        <p className="text-xs leading-6 text-muted-foreground">
          当前账号对这个 Project 只有只读权限。上下文容器、挂载策略和会话编排仍由 owner/editor 维护，这里暂不开放写入。
        </p>
      </DetailSection>
    );
  }

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

type ProjectDetailTab =
  | "base"
  | "config"
  | "context"
  | "workflow"
  | "workspaces"
  | "sharing"
  | "template";

interface ProjectDetailDrawerProps {
  open: boolean;
  project: Project | null;
  currentWorkspaces: Workspace[];
  onClose: () => void;
}

function ProjectDetailDrawer({
  open,
  project,
  currentWorkspaces,
  onClose,
}: ProjectDetailDrawerProps) {
  const {
    updateProject,
    updateProjectConfig,
    deleteProject,
    cloneProject,
    fetchProjectGrants,
    grantProjectUser,
    revokeProjectUser,
    grantProjectGroup,
    revokeProjectGroup,
    grantsByProjectId,
    error,
  } = useProjectStore();
  const currentUser = useCurrentUserStore((state) => state.currentUser);
  const [activeTab, setActiveTab] = useState<ProjectDetailTab>("base");
  const [editName, setEditName] = useState("");
  const [editDescription, setEditDescription] = useState("");
  const [defaultAgentType, setDefaultAgentType] = useState("");
  const [defaultWorkspaceId, setDefaultWorkspaceId] = useState("");
  const [templateVisibility, setTemplateVisibility] = useState<Project["visibility"]>("private");
  const [templateFlag, setTemplateFlag] = useState(false);
  const [cloneName, setCloneName] = useState("");
  const [isPresetSaving, setIsPresetSaving] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);
  const [successMessage, setSuccessMessage] = useState<string | null>(null);
  const [isDeleteConfirmOpen, setIsDeleteConfirmOpen] = useState(false);
  const [deleteConfirmValue, setDeleteConfirmValue] = useState("");
  const [directoryUsers, setDirectoryUsers] = useState<DirectoryUser[]>([]);
  const [directoryGroups, setDirectoryGroups] = useState<DirectoryGroup[]>([]);
  const [isDirectoryLoading, setIsDirectoryLoading] = useState(false);
  const [shareTargetType, setShareTargetType] = useState<"user" | "group">("user");
  const [selectedUserId, setSelectedUserId] = useState("");
  const [selectedGroupId, setSelectedGroupId] = useState("");
  const [grantRole, setGrantRole] = useState<ProjectRole>("viewer");

  useEffect(() => {
    if (!project) return;
    setEditName(project.name);
    setEditDescription(project.description);
    setDefaultAgentType(project.config.default_agent_type ?? "");
    setDefaultWorkspaceId(project.config.default_workspace_id ?? "");
    setTemplateVisibility(project.visibility);
    setTemplateFlag(project.is_template);
    setCloneName(`${project.name}（副本）`);
    setActiveTab("base");
    setFormError(null);
    setSuccessMessage(null);
    setShareTargetType("user");
    setSelectedUserId("");
    setSelectedGroupId("");
    setGrantRole("viewer");
  }, [
    project?.id,
    project?.name,
    project?.description,
    project?.config.default_agent_type,
    project?.config.default_workspace_id,
    project?.visibility,
    project?.is_template,
  ]);

  useEffect(() => {
    if (!project || activeTab !== "sharing" || !project.access.can_manage_sharing) return;
    let cancelled = false;
    void (async () => {
      await fetchProjectGrants(project.id);
      if (directoryUsers.length > 0 || directoryGroups.length > 0) {
        return;
      }

      setIsDirectoryLoading(true);
      try {
        const [users, groups] = await Promise.all([
          fetchDirectoryUsers(),
          fetchDirectoryGroups(),
        ]);
        if (cancelled) return;

        setDirectoryUsers(users);
        setDirectoryGroups(groups);
        if (!selectedUserId && users.length > 0) {
          const firstUser = users.find((item) => item.user_id !== currentUser?.user_id) ?? users[0];
          setSelectedUserId(firstUser?.user_id ?? "");
        }
        if (!selectedGroupId && groups.length > 0) {
          setSelectedGroupId(groups[0].group_id);
        }
      } catch (loadError) {
        if (!cancelled) {
          setFormError((loadError as Error).message);
        }
      } finally {
        if (!cancelled) {
          setIsDirectoryLoading(false);
        }
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [
    activeTab,
    currentUser?.user_id,
    directoryGroups.length,
    directoryUsers.length,
    fetchProjectGrants,
    project,
    selectedGroupId,
    selectedUserId,
  ]);

  if (!project) return null;

  const canEditProject = project.access.can_edit;
  const canManageSharing = project.access.can_manage_sharing;
  const grants = grantsByProjectId[project.id] ?? [];
  const availableUsers = directoryUsers.filter((item) => item.user_id !== currentUser?.user_id);

  const handleSaveBase = async () => {
    if (!canEditProject) {
      setFormError("当前权限不允许编辑 Project 基础信息");
      return;
    }

    const trimmedName = editName.trim();
    if (!trimmedName) {
      setFormError("项目名称不能为空");
      return;
    }

    const saved = await updateProject(project.id, {
      name: trimmedName,
      description: editDescription.trim(),
    });
    if (!saved) return;

    setFormError(null);
    setSuccessMessage("已保存基础信息");
  };

  const handleSaveConfig = async () => {
    if (!canEditProject) {
      setFormError("当前权限不允许修改项目默认配置");
      return;
    }

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
    setSuccessMessage("已保存默认配置");
  };

  const handleSavePresets = async (nextPresets: ProjectConfig["agent_presets"]) => {
    if (!canEditProject) {
      setFormError("当前权限不允许维护 Agent 预设");
      return;
    }

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
      if (!saved) {
        setFormError("保存预设失败");
        return;
      }
      setFormError(null);
      setSuccessMessage("已保存 Agent 预设");
    } finally {
      setIsPresetSaving(false);
    }
  };

  const handleDeleteProject = async () => {
    if (!canManageSharing) {
      setFormError("当前权限不允许删除 Project");
      return;
    }

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

  const handleSaveTemplateSettings = async () => {
    if (!canManageSharing) {
      setFormError("当前权限不允许修改模板与可见性策略");
      return;
    }
    if (templateVisibility === "template_visible" && !templateFlag) {
      setFormError("template_visible 仅适用于模板 Project，请先开启模板标记");
      return;
    }

    const saved = await updateProject(project.id, {
      visibility: templateVisibility,
      is_template: templateFlag,
    });
    if (!saved) return;

    setFormError(null);
    setSuccessMessage("已保存模板策略");
  };

  const handleCloneProject = async () => {
    const cloned = await cloneProject(project.id, {
      name: cloneName.trim() || undefined,
    });
    if (!cloned) return;

    setFormError(null);
    setSuccessMessage(`已克隆私有 Project：${cloned.name}`);
  };

  const handleGrantSubmit = async () => {
    if (!canManageSharing) {
      setFormError("当前权限不允许管理共享");
      return;
    }

    const subjectId = shareTargetType === "user" ? selectedUserId : selectedGroupId;
    if (!subjectId) {
      setFormError(shareTargetType === "user" ? "请选择用户" : "请选择用户组");
      return;
    }

    const savedGrant = shareTargetType === "user"
      ? await grantProjectUser(project.id, subjectId, grantRole)
      : await grantProjectGroup(project.id, subjectId, grantRole);
    if (!savedGrant) return;

    setFormError(null);
    setSuccessMessage(`已更新${shareTargetType === "user" ? "用户" : "用户组"}授权`);
  };

  const handleRevokeGrant = async (grant: ProjectSubjectGrant) => {
    const revoked = grant.subject_type === "user"
      ? await revokeProjectUser(project.id, grant.subject_id)
      : await revokeProjectGroup(project.id, grant.subject_id);
    if (!revoked) return;

    setFormError(null);
    setSuccessMessage("已撤销授权");
  };

  return (
    <>
      <DetailPanel
        open={open}
        title="项目详情"
        subtitle={`ID: ${project.id} · ${describeProjectAccess(project)}`}
        onClose={onClose}
        widthClassName="max-w-4xl"
        headerExtra={
          <DetailMenu
            items={[
              {
                key: "delete",
                label: "删除项目",
                danger: true,
                disabled: !canManageSharing,
                onSelect: () => setIsDeleteConfirmOpen(true),
              },
            ]}
          />
        }
      >
        <div className="space-y-4 p-5">
          <div className="flex flex-wrap border-b border-border bg-secondary/35 px-2 pt-2">
            {[
              ["base", "基础信息"],
              ["config", "项目配置"],
              ["context", "上下文编排"],
              ["workflow", "Workflow"],
              ["workspaces", "工作空间"],
              ["sharing", "共享管理"],
              ["template", "模板策略"],
            ].map(([key, label]) => (
              <button
                key={key}
                type="button"
                onClick={() => setActiveTab(key as ProjectDetailTab)}
                className={`rounded-t-[10px] px-5 py-3 text-sm transition-colors ${
                  activeTab === key
                    ? "border border-border border-b-background bg-background font-medium text-foreground"
                    : "text-muted-foreground hover:text-foreground"
                }`}
              >
                {label}
              </button>
            ))}
          </div>

          {activeTab === "base" && (
            <div className="space-y-4">
              <DetailSection title="权限摘要">
                <div className="flex flex-wrap gap-2">
                  <span className="rounded-full border border-border bg-background px-2.5 py-1 text-xs text-foreground">
                    当前身份：{describeProjectAccess(project)}
                  </span>
                  <span className="rounded-full border border-border bg-background px-2.5 py-1 text-xs text-muted-foreground">
                    可编辑：{project.access.can_edit ? "是" : "否"}
                  </span>
                  <span className="rounded-full border border-border bg-background px-2.5 py-1 text-xs text-muted-foreground">
                    可管理共享：{project.access.can_manage_sharing ? "是" : "否"}
                  </span>
                  {project.access.via_template_visibility && (
                    <span className="rounded-full border border-amber-200 bg-amber-50 px-2.5 py-1 text-xs text-amber-700">
                      通过模板可见进入
                    </span>
                  )}
                  {project.access.via_admin_bypass && (
                    <span className="rounded-full border border-emerald-200 bg-emerald-50 px-2.5 py-1 text-xs text-emerald-700">
                      管理员旁路
                    </span>
                  )}
                </div>
              </DetailSection>

              <DetailSection title="基础信息">
                <input
                  value={editName}
                  onChange={(event) => setEditName(event.target.value)}
                  placeholder="项目名称"
                  disabled={!canEditProject}
                  className="agentdash-form-input disabled:cursor-not-allowed disabled:opacity-70"
                />
                <input
                  value={editDescription}
                  onChange={(event) => setEditDescription(event.target.value)}
                  placeholder="项目描述"
                  disabled={!canEditProject}
                  className="agentdash-form-input disabled:cursor-not-allowed disabled:opacity-70"
                />
                <div className="flex justify-end">
                  <button
                    type="button"
                    onClick={() => void handleSaveBase()}
                    disabled={!canEditProject}
                    className="agentdash-button-secondary"
                  >
                    保存基础信息
                  </button>
                </div>
              </DetailSection>
            </div>
          )}

          {activeTab === "config" && (
            <DetailSection title="项目配置">
              {!canEditProject && (
                <p className="text-xs text-muted-foreground">
                  当前账号对该 Project 仅有只读权限，这里展示的是现有默认配置。
                </p>
              )}
              <input
                value={defaultAgentType}
                onChange={(event) => setDefaultAgentType(event.target.value)}
                placeholder="默认 Agent 类型（可选）"
                disabled={!canEditProject}
                className="agentdash-form-input disabled:cursor-not-allowed disabled:opacity-70"
              />
              <select
                value={defaultWorkspaceId}
                onChange={(event) => setDefaultWorkspaceId(event.target.value)}
                disabled={!canEditProject}
                className="agentdash-form-select disabled:cursor-not-allowed disabled:opacity-70"
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
                  disabled={!canEditProject}
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
              canEdit={canEditProject}
              onError={setFormError}
            />
          )}

          {activeTab === "workflow" && (
            <DetailSection title="Workflow 平台">
              {!canEditProject && (
                <p className="text-xs text-muted-foreground">
                  你当前可以查看 workflow 绑定现状，但如需改动仍需要 owner/editor 权限。
                </p>
              )}
              <ProjectWorkflowPanel projectId={project.id} />
            </DetailSection>
          )}

          {activeTab === "workspaces" && (
            <DetailSection title="工作空间">
              <WorkspaceList projectId={project.id} workspaces={currentWorkspaces} />

              <div className="mt-4 border-t border-border pt-4">
                <p className="mb-1 text-[11px] font-medium uppercase tracking-[0.14em] text-muted-foreground/70">
                  Address Space 预览
                </p>
                <p className="mb-3 text-xs text-muted-foreground">
                  以下是当前项目配置下 Agent 会话将看到的挂载视图（基于默认工作空间 + 上下文容器）。
                </p>
                <AddressSpaceBrowser
                  preview={{ projectId: project.id, target: "project" }}
                />
              </div>
            </DetailSection>
          )}

          {activeTab === "sharing" && (
            <div className="space-y-4">
              <DetailSection
                title="共享策略"
                description="Project 默认私有，可按用户或用户组授予 owner/editor/viewer。"
              >
                {canManageSharing ? (
                  <>
                    <div className="grid gap-3 md:grid-cols-[120px_minmax(0,1fr)_140px_auto]">
                      <select
                        value={shareTargetType}
                        onChange={(event) => setShareTargetType(event.target.value as "user" | "group")}
                        className="agentdash-form-select"
                      >
                        <option value="user">用户</option>
                        <option value="group">用户组</option>
                      </select>

                      {shareTargetType === "user" ? (
                        <select
                          value={selectedUserId}
                          onChange={(event) => setSelectedUserId(event.target.value)}
                          className="agentdash-form-select"
                        >
                          <option value="">选择用户</option>
                          {availableUsers.map((user) => (
                            <option key={user.user_id} value={user.user_id}>
                              {user.display_name?.trim() || user.email?.trim() || user.user_id}
                            </option>
                          ))}
                        </select>
                      ) : (
                        <select
                          value={selectedGroupId}
                          onChange={(event) => setSelectedGroupId(event.target.value)}
                          className="agentdash-form-select"
                        >
                          <option value="">选择用户组</option>
                          {directoryGroups.map((group) => (
                            <option key={group.group_id} value={group.group_id}>
                              {group.display_name?.trim() || group.group_id}
                            </option>
                          ))}
                        </select>
                      )}

                      <select
                        value={grantRole}
                        onChange={(event) => setGrantRole(event.target.value as ProjectRole)}
                        className="agentdash-form-select"
                      >
                        {PROJECT_ROLE_OPTIONS.map((option) => (
                          <option key={option.value} value={option.value}>
                            {option.label}
                          </option>
                        ))}
                      </select>

                      <button
                        type="button"
                        onClick={() => void handleGrantSubmit()}
                        className="agentdash-button-primary"
                      >
                        保存授权
                      </button>
                    </div>

                    {isDirectoryLoading && (
                      <p className="text-xs text-muted-foreground">正在加载身份目录...</p>
                    )}
                  </>
                ) : (
                  <p className="text-xs leading-6 text-muted-foreground">
                    当前只有 owner 或管理员旁路身份可以查看并管理共享记录。
                  </p>
                )}
              </DetailSection>

              {canManageSharing && (
                <DetailSection title="当前授权列表">
                  {grants.length === 0 ? (
                    <p className="text-xs text-muted-foreground">
                      当前还没有额外共享记录，只有创建者 owner 授权。
                    </p>
                  ) : (
                    <div className="space-y-2">
                      {grants.map((grant) => (
                        <div
                          key={`${grant.subject_type}:${grant.subject_id}`}
                          className="flex flex-wrap items-center justify-between gap-3 rounded-[10px] border border-border bg-background px-3 py-3"
                        >
                          <div className="min-w-0">
                            <div className="flex flex-wrap items-center gap-2">
                              <span className="rounded-full border border-border bg-secondary/50 px-2 py-1 text-[11px] uppercase text-muted-foreground">
                                {grant.subject_type}
                              </span>
                              <span className="text-sm font-medium text-foreground">
                                {resolveGrantSubjectLabel(grant, directoryUsers, directoryGroups)}
                              </span>
                              <span className="rounded-full border border-border bg-background px-2 py-1 text-[11px] text-muted-foreground">
                                {PROJECT_ROLE_LABELS[grant.role]}
                              </span>
                            </div>
                            <p className="mt-1 text-xs text-muted-foreground">
                              subject_id: {grant.subject_id} · granted_by: {grant.granted_by_user_id}
                            </p>
                          </div>
                          <button
                            type="button"
                            onClick={() => void handleRevokeGrant(grant)}
                            className="rounded-[8px] border border-destructive/25 bg-destructive/5 px-3 py-1.5 text-xs text-destructive transition-colors hover:bg-destructive/10"
                          >
                            撤销
                          </button>
                        </div>
                      ))}
                    </div>
                  )}
                </DetailSection>
              )}
            </div>
          )}

          {activeTab === "template" && (
            <div className="space-y-4">
              <DetailSection
                title="模板与可见性"
                description="模板 Project 可被克隆为用户私有副本；template_visible 只适用于模板。"
              >
                <label className="flex items-center gap-2 text-sm text-foreground">
                  <input
                    type="checkbox"
                    checked={templateFlag}
                    onChange={(event) => setTemplateFlag(event.target.checked)}
                    disabled={!canManageSharing}
                  />
                  标记为模板 Project
                </label>

                <select
                  value={templateVisibility}
                  onChange={(event) => setTemplateVisibility(event.target.value as Project["visibility"])}
                  disabled={!canManageSharing}
                  className="agentdash-form-select disabled:cursor-not-allowed disabled:opacity-70"
                >
                  {Object.entries(PROJECT_VISIBILITY_LABELS).map(([value, label]) => (
                    <option key={value} value={value}>
                      {label}
                    </option>
                  ))}
                </select>

                <div className="flex justify-end">
                  <button
                    type="button"
                    onClick={() => void handleSaveTemplateSettings()}
                    disabled={!canManageSharing}
                    className="agentdash-button-secondary"
                  >
                    保存模板策略
                  </button>
                </div>
              </DetailSection>

              <DetailSection
                title="克隆为私有 Project"
                description="clone 不会复制原 Project 的共享记录，也不会复制 workspaces / stories / tasks / sessions。"
              >
                {project.is_template ? (
                  <>
                    <input
                      value={cloneName}
                      onChange={(event) => setCloneName(event.target.value)}
                      placeholder="新 Project 名称"
                      className="agentdash-form-input"
                    />
                    <p className="text-xs text-muted-foreground">
                      当前会复制项目基础配置与 workflow assignments，并清空默认 workspace，避免引用源模板下的工作空间。
                    </p>
                    <div className="flex justify-end">
                      <button
                        type="button"
                        onClick={() => void handleCloneProject()}
                        className="agentdash-button-primary"
                      >
                        克隆为我的私有 Project
                      </button>
                    </div>
                  </>
                ) : (
                  <p className="text-xs text-muted-foreground">
                    当前 Project 还不是模板，无法作为标准私有化来源被 clone。
                  </p>
                )}
              </DetailSection>
            </div>
          )}

          {(successMessage || formError || error) && (
            <p className={successMessage && !formError && !error ? "text-xs text-emerald-600" : "text-xs text-destructive"}>
              {successMessage && !formError && !error ? successMessage : `操作失败：${formError || error}`}
            </p>
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
                  {project.description || `ID: ${project.id}`}
                </p>
                <div className="mt-2 flex flex-wrap gap-1.5">
                  <span className="rounded-full border border-border bg-background px-2 py-0.5 text-[10px] text-muted-foreground">
                    {describeProjectAccess(project)}
                  </span>
                  {project.is_template && (
                    <span className="rounded-full border border-amber-200 bg-amber-50 px-2 py-0.5 text-[10px] text-amber-700">
                      模板
                    </span>
                  )}
                  <span className="rounded-full border border-border bg-background px-2 py-0.5 text-[10px] text-muted-foreground">
                    {PROJECT_VISIBILITY_LABELS[project.visibility]}
                  </span>
                </div>
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
        onClose={() => setIsCreateOpen(false)}
      />

      <ProjectDetailDrawer
        key={`project-detail-${isDetailOpen ? "open" : "closed"}-${detailProject?.id ?? "none"}`}
        open={isDetailOpen}
        project={detailProject}
        currentWorkspaces={detailWorkspaces}
        onClose={() => {
          setIsDetailOpen(false);
          setDetailProjectId(null);
        }}
      />
    </>
  );
}
