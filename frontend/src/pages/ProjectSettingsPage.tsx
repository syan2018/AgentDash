import { useEffect, useMemo, useRef, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import type { ReactNode } from "react";
import type {
  ContextContainerDefinition,
  DirectoryGroup,
  DirectoryUser,
  Project,
  ProjectRole,
  ProjectSubjectGrant,
  Workspace,
} from "../types";
import { useCurrentUserStore } from "../stores/currentUserStore";
import { useProjectStore } from "../stores/projectStore";
import { useWorkspaceStore } from "../stores/workspaceStore";
import { WorkspaceList } from "../features/workspace/workspace-list";
import { AddressSpaceBrowser } from "../features/address-space";
import {
  ContextContainersEditor,
} from "../components/context-config-editor";
import { previewAddressSpace, type MountSummary } from "../services/addressSpaces";
import {
  DangerConfirmDialog,
} from "../components/ui/detail-panel";
import { fetchDirectoryGroups, fetchDirectoryUsers } from "../services/directory";

type SettingsTab = "overview" | "context" | "workspace" | "management";

interface SettingsTabItem {
  key: SettingsTab;
  label: string;
  description: string;
}

const SETTINGS_TABS: SettingsTabItem[] = [
  { key: "overview", label: "概览", description: "项目身份、摘要与基础信息" },
  { key: "context", label: "上下文资源", description: "context containers 与挂载策略" },
  { key: "workspace", label: "工作空间", description: "默认 workspace、bindings 与 runtime preview" },
  { key: "management", label: "管理动作", description: "共享、模板、clone 与删除" },
];

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
  if (project.access.via_admin_bypass) return "管理员旁路";
  if (project.access.role) return PROJECT_ROLE_LABELS[project.access.role];
  if (project.access.via_template_visibility) return "模板访客";
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

function SectionCard({
  title,
  description,
  children,
}: {
  title: string;
  description?: string;
  children: ReactNode;
}) {
  return (
    <section className="space-y-5">
      <div className="space-y-1.5">
        <h2 className="text-xl font-semibold tracking-[-0.025em] text-foreground">{title}</h2>
        {description && <p className="max-w-3xl text-sm leading-6 text-muted-foreground">{description}</p>}
      </div>
      {children}
    </section>
  );
}

function ContentGroup({
  title,
  description,
  children,
}: {
  title: string;
  description?: string;
  children: ReactNode;
}) {
  return (
    <section className="space-y-4 border-t border-border/70 pt-6 first:border-t-0 first:pt-0">
      <div className="space-y-1">
        <h3 className="text-sm font-semibold uppercase tracking-[0.14em] text-foreground">{title}</h3>
        {description && <p className="text-sm leading-6 text-muted-foreground">{description}</p>}
      </div>
      {children}
    </section>
  );
}

function TabButton({
  tab,
  activeTab,
  onClick,
}: {
  tab: SettingsTabItem;
  activeTab: SettingsTab;
  onClick: (tab: SettingsTab) => void;
}) {
  const isActive = activeTab === tab.key;

  return (
    <button
      type="button"
      onClick={() => onClick(tab.key)}
      className={`flex h-full min-w-0 flex-col justify-between rounded-[14px] border px-4 py-3 text-left transition-colors ${
        isActive
          ? "border-foreground/10 bg-background text-foreground shadow-sm"
          : "border-transparent bg-transparent text-muted-foreground hover:border-border/80 hover:bg-background/70 hover:text-foreground"
      }`}
    >
      <p className="truncate text-sm font-medium">{tab.label}</p>
      <p className="mt-1 text-xs leading-5 opacity-80">{tab.description}</p>
    </button>
  );
}

const PROVIDER_LABELS: Record<string, string> = {
  relay_fs: "工作区文件",
  inline_fs: "内联文件",
  lifecycle_vfs: "Lifecycle 记录",
  canvas_fs: "Canvas",
  external_service: "外部服务",
};

const CAPABILITY_LABELS: Record<string, string> = {
  read: "读",
  write: "写",
  list: "列",
  search: "搜",
  exec: "执行",
};

function MountOverviewList({ projectId, refreshKey }: { projectId: string; refreshKey?: number }) {
  const [mounts, setMounts] = useState<MountSummary[]>([]);
  const [defaultMountId, setDefaultMountId] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    void (async () => {
      try {
        const result = await previewAddressSpace({ projectId, target: "project" });
        if (cancelled) return;
        setMounts(result.mounts);
        setDefaultMountId(result.default_mount_id ?? null);
      } catch (err) {
        if (!cancelled) setError(err instanceof Error ? err.message : String(err));
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();
    return () => { cancelled = true; };
  }, [projectId, refreshKey]);

  if (loading) {
    return (
      <p className="py-6 text-center text-xs text-muted-foreground">
        正在加载 Mount 概览…
      </p>
    );
  }

  if (error) {
    return (
      <div className="rounded-[10px] border border-destructive/20 bg-destructive/5 px-3 py-2 text-xs text-destructive">
        {error}
      </div>
    );
  }

  if (mounts.length === 0) {
    return (
      <p className="rounded-[10px] border border-dashed border-border px-4 py-4 text-center text-sm text-muted-foreground">
        当前配置下没有可用的 Mount。请先配置工作空间或上下文容器。
      </p>
    );
  }

  return (
    <div className="space-y-2">
      {mounts.map((mount) => {
        const isDefault = mount.id === defaultMountId;
        const providerLabel = PROVIDER_LABELS[mount.provider] ?? mount.provider;
        const online = mount.backend_online;

        return (
          <div
            key={mount.id}
            className={`rounded-[12px] border px-4 py-3 ${
              isDefault
                ? "border-primary/25 bg-primary/[0.03]"
                : "border-border bg-background"
            }`}
          >
            <div className="flex items-start justify-between gap-3">
              <div className="min-w-0 flex-1">
                <div className="flex flex-wrap items-center gap-2">
                  {/* 状态指示点 */}
                  {mount.provider === "relay_fs" ? (
                    <span
                      className={`inline-block h-2 w-2 shrink-0 rounded-full ${
                        online === true
                          ? "bg-emerald-500"
                          : online === false
                            ? "bg-red-400"
                            : "bg-muted-foreground/30"
                      }`}
                      title={online === true ? "Backend 在线" : online === false ? "Backend 离线" : "状态未知"}
                    />
                  ) : (
                    <span className="inline-block h-2 w-2 shrink-0 rounded-full bg-blue-400" />
                  )}

                  <p className="truncate text-sm font-medium text-foreground">
                    {mount.display_name}
                  </p>

                  {isDefault && (
                    <span className="inline-flex items-center rounded-full border border-primary/25 bg-primary/10 px-2 py-0.5 text-[10px] font-medium text-primary">
                      默认
                    </span>
                  )}
                  {mount.default_write && (
                    <span className="inline-flex items-center rounded-full border border-amber-500/25 bg-amber-500/10 px-2 py-0.5 text-[10px] font-medium text-amber-600">
                      默认写入
                    </span>
                  )}
                  <span className="rounded-full border border-border bg-muted/40 px-2 py-0.5 text-[10px] text-muted-foreground">
                    {providerLabel}
                  </span>
                </div>

                <p className="mt-1 truncate font-mono text-xs text-muted-foreground">
                  {mount.root_ref}
                </p>
              </div>

              {/* 能力标签 */}
              <div className="flex shrink-0 flex-wrap justify-end gap-1">
                {mount.capabilities.map((cap) => (
                  <span
                    key={cap}
                    className="rounded-full border border-border bg-background px-2 py-0.5 text-[10px] text-muted-foreground"
                  >
                    {CAPABILITY_LABELS[cap] ?? cap}
                  </span>
                ))}
              </div>
            </div>

            {mount.file_count != null && (
              <p className="mt-1 text-[10px] text-muted-foreground">
                {mount.file_count} 个文件
              </p>
            )}
          </div>
        );
      })}
    </div>
  );
}

function ContextTabContent({
  project,
  contextContainers,
  canEdit,
  onSave,
}: {
  project: Project;
  contextContainers: ContextContainerDefinition[];
  canEdit: boolean;
  onSave: (next: ContextContainerDefinition[]) => Promise<unknown>;
}) {
  const [mountRefreshKey, setMountRefreshKey] = useState(0);

  const handleContainerSave = async (next: ContextContainerDefinition[]) => {
    await onSave(next);
    setMountRefreshKey((k) => k + 1);
  };

  return (
    <>
      <SectionCard
        title="上下文容器"
        description="项目级上下文容器，可以是内联文件或外部服务。Agent 会话运行时会根据可见性规则自动挂载。"
      >
        <ContextContainersEditor
          value={contextContainers}
          domain="project"
          emptyText="暂无项目级容器"
          isSaving={false}
          readOnly={!canEdit}
          onSave={handleContainerSave}
        />
      </SectionCard>

      <SectionCard
        title="当前可用 Mount"
        description="基于当前 Workspace 和上下文容器配置，系统派生出的所有挂载点及其能力概览。"
      >
        <MountOverviewList projectId={project.id} refreshKey={mountRefreshKey} />
      </SectionCard>
    </>
  );
}

export function ProjectSettingsPage() {
  const navigate = useNavigate();
  const { projectId } = useParams<{ projectId: string }>();
  const currentUser = useCurrentUserStore((state) => state.currentUser);
  const {
    projects,
    currentProjectId,
    grantsByProjectId,
    selectProject,
    updateProject,
    updateProjectConfig,
    fetchProjectGrants,
    grantProjectUser,
    revokeProjectUser,
    grantProjectGroup,
    revokeProjectGroup,
    cloneProject,
    deleteProject,
  } = useProjectStore();
  const { fetchWorkspaces, workspacesByProjectId } = useWorkspaceStore();

  const [activeTab, setActiveTab] = useState<SettingsTab>("overview");
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [templateVisibility, setTemplateVisibility] = useState<Project["visibility"]>("private");
  const [templateFlag, setTemplateFlag] = useState(false);
  const [cloneName, setCloneName] = useState("");
  const [directoryUsers, setDirectoryUsers] = useState<DirectoryUser[]>([]);
  const [directoryGroups, setDirectoryGroups] = useState<DirectoryGroup[]>([]);
  const [isDirectoryLoading, setIsDirectoryLoading] = useState(false);
  const [shareTargetType, setShareTargetType] = useState<"user" | "group">("user");
  const [selectedUserId, setSelectedUserId] = useState("");
  const [selectedGroupId, setSelectedGroupId] = useState("");
  const [grantRole, setGrantRole] = useState<ProjectRole>("viewer");
  const [deleteConfirmValue, setDeleteConfirmValue] = useState("");
  const [isDeleteConfirmOpen, setIsDeleteConfirmOpen] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!projectId) return;
    if (currentProjectId !== projectId) {
      selectProject(projectId);
    }
    void fetchWorkspaces(projectId);
  }, [currentProjectId, fetchWorkspaces, projectId, selectProject]);

  const project = useMemo(
    () => projects.find((item) => item.id === projectId) ?? null,
    [projectId, projects],
  );
  const workspaces: Workspace[] = projectId ? (workspacesByProjectId[projectId] ?? []) : [];
  const grants = project ? (grantsByProjectId[project.id] ?? []) : [];
  const loadedProjectIdRef = useRef<string | null>(null);

  useEffect(() => {
    if (!project) return;
    if (loadedProjectIdRef.current === project.id) return;
    loadedProjectIdRef.current = project.id;
    setName(project.name);
    setDescription(project.description);
    setTemplateVisibility(project.visibility);
    setTemplateFlag(project.is_template);
    setCloneName(`${project.name}（副本）`);
    setDeleteConfirmValue("");
    setShareTargetType("user");
    setSelectedUserId("");
    setSelectedGroupId("");
    setGrantRole("viewer");
    setActiveTab("overview");
    setMessage(null);
    setError(null);
  }, [project]);

  useEffect(() => {
    if (activeTab !== "management" || !project?.access.can_manage_sharing) return;
    let cancelled = false;

    void (async () => {
      setIsDirectoryLoading(true);
      try {
        await fetchProjectGrants(project.id);
        const [users, groups] = await Promise.all([
          fetchDirectoryUsers(),
          fetchDirectoryGroups(),
        ]);
        if (cancelled) return;

        setDirectoryUsers(users);
        setDirectoryGroups(groups);

        const firstUser = users.find((item) => item.user_id !== currentUser?.user_id) ?? users[0];
        if (firstUser) {
          setSelectedUserId((prev) => prev || firstUser.user_id);
        }
        if (groups[0]) {
          setSelectedGroupId((prev) => prev || groups[0].group_id);
        }
      } catch (loadError) {
        if (!cancelled) {
          setError((loadError as Error).message);
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
    fetchProjectGrants,
    project?.access.can_manage_sharing,
    project?.id,
  ]);

  if (!project) {
    return (
      <div className="flex h-full items-center justify-center">
        <div className="text-center">
          <p className="text-sm text-muted-foreground">未找到对应的 Project。</p>
          <button
            type="button"
            onClick={() => navigate("/dashboard/agent")}
            className="mt-3 rounded-[10px] border border-border bg-background px-4 py-2 text-sm text-foreground transition-colors hover:bg-secondary"
          >
            返回 Dashboard
          </button>
        </div>
      </div>
    );
  }

  const canEditProject = project.access.can_edit;
  const canManageSharing = project.access.can_manage_sharing;
  const contextContainers = project.config.context_containers ?? [];
  const availableUsers = directoryUsers.filter((item) => item.user_id !== currentUser?.user_id);
  const activeTabMeta = SETTINGS_TABS.find((item) => item.key === activeTab) ?? SETTINGS_TABS[0];

  const saveBaseInfo = async () => {
    if (!canEditProject) {
      setError("当前权限不允许编辑 Project 基础信息");
      return;
    }
    const trimmedName = name.trim();
    if (!trimmedName) {
      setError("项目名称不能为空");
      return;
    }
    const result = await updateProject(project.id, {
      name: trimmedName,
      description: description.trim(),
    });
    if (!result) {
      setError("基础信息保存失败");
      return;
    }
    setError(null);
    setMessage("已保存基础信息");
  };

  const saveDefaultWorkspace = async (workspaceId: string | null) => {
    if (!canEditProject) {
      setError("当前权限不允许修改默认工作空间");
      return;
    }
    const result = await updateProjectConfig(project.id, {
      default_agent_type: project.config.default_agent_type ?? null,
      default_workspace_id: workspaceId,
      agent_presets: project.config.agent_presets ?? [],
      context_containers: contextContainers,
      mount_policy: project.config.mount_policy,
    });
    if (!result) {
      setError("默认工作空间保存失败");
      return;
    }
    setError(null);
    setMessage(workspaceId ? "已设为默认工作空间" : "已取消默认工作空间");
  };

  const saveContext = async (payload: Parameters<typeof updateProject>[1]) => {
    if (!canEditProject) {
      setError("当前权限不允许修改上下文资源");
      return;
    }
    const result = await updateProject(project.id, payload);
    if (!result) {
      setError("上下文资源保存失败");
      return;
    }
    setError(null);
    setMessage("已保存上下文资源");
  };

  const saveTemplateSettings = async () => {
    if (!canManageSharing) {
      setError("当前权限不允许修改模板与可见性策略");
      return;
    }
    if (templateVisibility === "template_visible" && !templateFlag) {
      setError("template_visible 仅适用于模板 Project，请先开启模板标记");
      return;
    }
    const result = await updateProject(project.id, {
      visibility: templateVisibility,
      is_template: templateFlag,
    });
    if (!result) {
      setError("模板策略保存失败");
      return;
    }
    setError(null);
    setMessage("已保存模板策略");
  };

  const submitGrant = async () => {
    if (!canManageSharing) {
      setError("当前权限不允许管理共享");
      return;
    }

    const subjectId = shareTargetType === "user" ? selectedUserId : selectedGroupId;
    if (!subjectId) {
      setError(shareTargetType === "user" ? "请选择用户" : "请选择用户组");
      return;
    }

    const savedGrant = shareTargetType === "user"
      ? await grantProjectUser(project.id, subjectId, grantRole)
      : await grantProjectGroup(project.id, subjectId, grantRole);
    if (!savedGrant) {
      setError("共享授权保存失败");
      return;
    }

    setError(null);
    setMessage(`已更新${shareTargetType === "user" ? "用户" : "用户组"}授权`);
  };

  const revokeGrant = async (grant: ProjectSubjectGrant) => {
    const revoked = grant.subject_type === "user"
      ? await revokeProjectUser(project.id, grant.subject_id)
      : await revokeProjectGroup(project.id, grant.subject_id);
    if (!revoked) {
      setError("撤销授权失败");
      return;
    }
    setError(null);
    setMessage("已撤销授权");
  };

  const handleCloneProject = async () => {
    const cloned = await cloneProject(project.id, {
      name: cloneName.trim() || undefined,
    });
    if (!cloned) {
      setError("克隆 Project 失败");
      return;
    }
    setError(null);
    setMessage(`已克隆私有 Project：${cloned.name}`);
    selectProject(cloned.id);
    navigate(`/projects/${cloned.id}/settings`);
  };

  const handleDeleteProject = async () => {
    if (!canManageSharing) {
      setError("当前权限不允许删除 Project");
      return;
    }
    if (deleteConfirmValue.trim() !== project.name) {
      setError("请输入完整项目名后再删除");
      return;
    }
    const deleted = await deleteProject(project.id);
    if (!deleted) {
      setError("删除失败，请查看错误信息后重试");
      return;
    }
    navigate("/dashboard/agent");
  };

  return (
    <>
      <div className="h-full overflow-y-auto">
        <div className="mx-auto max-w-6xl space-y-5 px-6 py-8">
          <div className="rounded-[24px] border border-border bg-background px-6 py-6">
            <div className="flex flex-col gap-5 lg:flex-row lg:items-start lg:justify-between">
              <div className="space-y-3">
                <button
                  type="button"
                  onClick={() => navigate("/dashboard/agent")}
                  className="inline-flex items-center gap-2 rounded-[10px] border border-border bg-background px-3 py-2 text-sm text-foreground transition-colors hover:bg-secondary"
                >
                  返回
                </button>
                <div className="space-y-2">
                  <p className="text-[11px] uppercase tracking-[0.16em] text-muted-foreground">Project Settings</p>
                  <h1 className="text-[2rem] font-semibold tracking-[-0.03em] text-foreground">{project.name}</h1>
                  <p className="max-w-3xl text-sm leading-6 text-muted-foreground">
                    设置页按概览、上下文资源、工作空间和管理动作分栏收纳，让逻辑 workspace、运行时派生结果和项目级配置分开表达。
                  </p>
                </div>
              </div>

              <div className="flex max-w-[22rem] flex-wrap gap-2 lg:justify-end">
                <span className="rounded-full border border-border bg-secondary/20 px-3 py-1 text-xs text-foreground">
                  权限：{describeProjectAccess(project)}
                </span>
                <span className="rounded-full border border-border bg-secondary/20 px-3 py-1 text-xs text-muted-foreground">
                  可编辑：{canEditProject ? "是" : "否"}
                </span>
                <span className="rounded-full border border-border bg-secondary/20 px-3 py-1 text-xs text-muted-foreground">
                  可管理共享：{canManageSharing ? "是" : "否"}
                </span>
                {project.is_template && (
                  <span className="rounded-full border border-amber-200 bg-amber-50 px-3 py-1 text-xs text-amber-700">
                    模板
                  </span>
                )}
              </div>
            </div>
          </div>

          <div className="rounded-[22px] border border-border bg-muted/20 p-2">
            <div className="grid gap-2 sm:grid-cols-2 xl:grid-cols-4">
              {SETTINGS_TABS.map((tab) => (
                <TabButton
                  key={tab.key}
                  tab={tab}
                  activeTab={activeTab}
                  onClick={setActiveTab}
                />
              ))}
            </div>
          </div>

          <div className="rounded-[24px] border border-border bg-background px-6 py-6 md:px-8 md:py-8">
            <div className="space-y-2 border-b border-border/70 pb-5">
              <p className="text-[11px] uppercase tracking-[0.16em] text-muted-foreground">
                {activeTabMeta.label}
              </p>
              <p className="max-w-3xl text-sm leading-6 text-muted-foreground">
                {activeTabMeta.description}
              </p>
            </div>

            <div className="space-y-8 pt-6">
              {activeTab === "overview" && (
                <SectionCard
                  title="概览"
                  description="只放项目本身的基础信息与访问摘要，不再掺杂运行时资源和工作空间绑定。"
                >
                  <ContentGroup title="基础信息">
                    <div className="grid gap-3 md:grid-cols-2">
                      <input
                        value={name}
                        onChange={(event) => setName(event.target.value)}
                        disabled={!canEditProject}
                        className="agentdash-form-input disabled:cursor-not-allowed disabled:opacity-70"
                        placeholder="项目名称"
                      />
                      <input
                        value={description}
                        onChange={(event) => setDescription(event.target.value)}
                        disabled={!canEditProject}
                        className="agentdash-form-input disabled:cursor-not-allowed disabled:opacity-70"
                        placeholder="项目描述"
                      />
                    </div>
                    <div className="flex justify-end">
                      <button
                        type="button"
                        onClick={() => void saveBaseInfo()}
                        disabled={!canEditProject}
                        className="agentdash-button-secondary"
                      >
                        保存基础信息
                      </button>
                    </div>
                  </ContentGroup>

                  <ContentGroup title="访问摘要">
                    <div className="flex flex-wrap gap-2">
                      <span className="rounded-full border border-border bg-secondary/35 px-3 py-1 text-xs text-muted-foreground">
                        visibility: {PROJECT_VISIBILITY_LABELS[project.visibility]}
                      </span>
                      <span className="rounded-full border border-border bg-secondary/35 px-3 py-1 text-xs text-muted-foreground">
                        default workspace: {project.config.default_workspace_id ?? "未设置"}
                      </span>
                      <span className="rounded-full border border-border bg-secondary/35 px-3 py-1 text-xs text-muted-foreground">
                        workspaces: {workspaces.length}
                      </span>
                      <span className="rounded-full border border-border bg-secondary/35 px-3 py-1 text-xs text-muted-foreground">
                        grants: {grants.length}
                      </span>
                    </div>
                  </ContentGroup>
                </SectionCard>
              )}

              {activeTab === "context" && (
                <ContextTabContent
                  project={project}
                  contextContainers={contextContainers}
                  canEdit={canEditProject}
                  onSave={async (next) => {
                    await saveContext({ context_containers: next });
                  }}
                />
              )}

              {activeTab === "workspace" && (
                <>
                  <SectionCard
                    title="工作空间"
                    description="逻辑 Workspace、bindings 以及 backend 快捷入口。点击卡片上的「设为默认」可指定项目默认 Workspace。"
                  >
                    <WorkspaceList
                      projectId={project.id}
                      workspaces={workspaces}
                      defaultWorkspaceId={project.config.default_workspace_id}
                      onSetDefault={canEditProject ? (wsId) => void saveDefaultWorkspace(wsId) : undefined}
                    />
                  </SectionCard>

                  <SectionCard
                    title="Runtime Preview"
                    description="Address Space 预览明确作为派生结果展示，用来解释当前默认配置会解析出什么挂载。"
                  >
                    <AddressSpaceBrowser preview={{ projectId: project.id, target: "project" }} />
                  </SectionCard>
                </>
              )}

              {activeTab === "management" && (
                <>
                  <SectionCard
                    title="共享管理"
                    description="Project 的共享记录独立于 Workspace。这里专门处理用户/用户组授权。"
                  >
                    <ContentGroup title="共享策略">
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
                              onClick={() => void submitGrant()}
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
                    </ContentGroup>

                    {canManageSharing && (
                      <ContentGroup title="当前授权列表">
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
                                  onClick={() => void revokeGrant(grant)}
                                  className="rounded-[8px] border border-destructive/25 bg-destructive/5 px-3 py-1.5 text-xs text-destructive transition-colors hover:bg-destructive/10"
                                >
                                  撤销
                                </button>
                              </div>
                            ))}
                          </div>
                        )}
                      </ContentGroup>
                    )}
                  </SectionCard>

                  <SectionCard
                    title="模板与复制"
                    description="模板策略、可见性和 clone 动作放在一起，和 workspace/runtime 设置分离。"
                  >
                    <ContentGroup title="模板策略">
                      {canManageSharing ? (
                        <>
                          <label className="flex items-center gap-2 text-sm text-foreground">
                            <input
                              type="checkbox"
                              checked={templateFlag}
                              onChange={(event) => setTemplateFlag(event.target.checked)}
                            />
                            标记为模板 Project
                          </label>

                          <select
                            value={templateVisibility}
                            onChange={(event) => setTemplateVisibility(event.target.value as Project["visibility"])}
                            className="agentdash-form-select"
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
                              onClick={() => void saveTemplateSettings()}
                              className="agentdash-button-secondary"
                            >
                              保存模板策略
                            </button>
                          </div>
                        </>
                      ) : (
                        <p className="text-xs leading-6 text-muted-foreground">
                          当前可见性：{PROJECT_VISIBILITY_LABELS[project.visibility]}。模板标记由 owner 或管理员维护；如果它已经是模板，你仍然可以在下方 clone 出自己的私有副本。
                        </p>
                      )}
                    </ContentGroup>

                    <ContentGroup
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
                    </ContentGroup>
                  </SectionCard>

                  <SectionCard
                    title="危险操作"
                    description="删除动作单独隔离，避免和普通配置保存按钮放在同一块区域里。"
                  >
                    <ContentGroup title="删除 Project">
                      <p className="text-sm text-muted-foreground">
                        删除后会级联移除 Project 下的 stories、tasks、workspaces 及其绑定记录，不可恢复。
                      </p>
                      <div className="flex justify-end">
                        <button
                          type="button"
                          onClick={() => setIsDeleteConfirmOpen(true)}
                          disabled={!canManageSharing}
                          className="rounded-[10px] border border-destructive/25 bg-destructive/5 px-4 py-2 text-sm text-destructive transition-colors hover:bg-destructive/10 disabled:cursor-not-allowed disabled:opacity-60"
                        >
                          删除 Project
                        </button>
                      </div>
                      {!canManageSharing && (
                        <p className="text-xs text-muted-foreground">
                          只有 owner 或管理员旁路身份可以删除 Project。
                        </p>
                      )}
                    </ContentGroup>
                  </SectionCard>
                </>
              )}

              {(message || error) && (
                <div className={`rounded-[12px] border px-4 py-3 text-sm ${
                  error
                    ? "border-destructive/40 bg-destructive/10 text-destructive"
                    : "border-emerald-400/40 bg-emerald-50 text-emerald-700"
                }`}>
                  {error ?? message}
                </div>
              )}
            </div>
          </div>
        </div>
      </div>

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
