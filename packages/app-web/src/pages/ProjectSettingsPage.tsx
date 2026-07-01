import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useNavigate, useParams, useSearchParams } from "react-router-dom";
import type {
  DirectoryUser,
  CurrentUser,
  Project,
  ProjectConfig,
  ProjectRole,
  ProjectSubjectGrant,
  Workspace,
} from "../types";
import { useCurrentUserStore } from "../stores/currentUserStore";
import { useProjectStore } from "../stores/projectStore";
import { useWorkspaceStore } from "../stores/workspaceStore";
import { WorkspaceList } from "../features/workspace/workspace-list";
import { WorkspaceModulesPanel } from "../features/workspace-module/ui/WorkspaceModulesPanel";
import {
  DangerConfirmDialog,
} from "@agentdash/ui";
import { UserAvatar } from "../components/ui/user-avatar";
import {
  fetchDirectoryGroups,
  fetchDirectoryUsers,
  resolveDirectoryGroup,
  resolveDirectoryUser,
} from "../services/directory";
import {
  mergeDirectoryUsers,
  mergeDirectoryGroups,
  resolveUserLabel,
  resolveGroupLabel,
} from "../features/directory/directorySubjectUtils";
import { DirectorySubjectPicker } from "../features/directory/DirectorySubjectPicker";
import type {
  SelectedSubject,
  DirectoryResponseStatus,
} from "../features/directory/DirectorySubjectPicker";
import type { DirectoryGroupSummary } from "../features/directory/directorySubjectUtils";
import {
  SectionCard,
  ContentGroup,
  CollapsibleGroup,
  SettingsTabs,
  Toast,
} from "../features/project/settings/settings-ui";
import { SETTINGS_TABS, type SettingsTab } from "../features/project/settings/settings-tabs";
import { BackendAccessPanel } from "../features/project/settings/BackendAccessPanel";
import { ContextTab } from "../features/project/settings/ContextTab";

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

const DIRECTORY_SEARCH_LIMIT = 20;

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
  groups: DirectoryGroupSummary[],
): string {
  if (grant.subject_type === "user") {
    const user = users.find((item) => item.user_id === grant.subject_id);
    return user ? resolveUserLabel(user) : grant.subject_id;
  }
  const group = groups.find((item) => item.group_id === grant.subject_id);
  return group ? resolveGroupLabel(group) : grant.subject_id;
}

function findGrantUser(
  grant: ProjectSubjectGrant,
  users: DirectoryUser[],
): DirectoryUser | null {
  if (grant.subject_type !== "user") return null;
  return users.find((item) => item.user_id === grant.subject_id) ?? null;
}

function userSubjectLabel(user: Pick<DirectoryUser, "user_id" | "subject" | "email">): string {
  const subject = user.subject?.trim();
  if (subject && subject !== user.user_id) return subject;
  const email = user.email?.trim();
  if (email) return email.includes("@") ? email.split("@")[0] : email;
  return user.user_id;
}

function resolveUserAuditLabel(
  userId: string,
  users: DirectoryUser[],
  currentUser: CurrentUser | null,
): string {
  const user = users.find((item) => item.user_id === userId);
  if (user) return userSubjectLabel(user);
  if (currentUser?.user_id === userId) return userSubjectLabel(currentUser);
  return userId;
}

function statusFrom(r: { source?: string; is_projection_only: boolean }): DirectoryResponseStatus {
  return { source: r.source, is_projection_only: r.is_projection_only };
}

export function ProjectSettingsPage() {
  const navigate = useNavigate();
  const { projectId } = useParams<{ projectId: string }>();
  const [searchParams, setSearchParams] = useSearchParams();
  const currentUser = useCurrentUserStore((state) => state.currentUser);
  const {
    projects,
    currentProjectId,
    grantsByProjectId,
    selectProject,
    fetchProjects,
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

  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [templateVisibility, setTemplateVisibility] = useState<Project["visibility"]>("private");
  const [templateFlag, setTemplateFlag] = useState(false);
  const [cloneName, setCloneName] = useState("");
  const [directoryUsers, setDirectoryUsers] = useState<DirectoryUser[]>([]);
  const [directoryGroups, setDirectoryGroups] = useState<DirectoryGroupSummary[]>([]);
  const [directoryUsersStatus, setDirectoryUsersStatus] = useState<DirectoryResponseStatus | null>(null);
  const [directoryGroupsStatus, setDirectoryGroupsStatus] = useState<DirectoryResponseStatus | null>(null);
  const [isDirectoryLoading, setIsDirectoryLoading] = useState(false);
  const [pickerSelections, setPickerSelections] = useState<SelectedSubject[]>([]);
  const [grantRole, setGrantRole] = useState<ProjectRole>("viewer");
  const [deleteConfirmValue, setDeleteConfirmValue] = useState("");
  const [isDeleteConfirmOpen, setIsDeleteConfirmOpen] = useState(false);
  const [stallTimeoutMs, setStallTimeoutMs] = useState("");
  const [workspaceInventoryRefreshKey, setWorkspaceInventoryRefreshKey] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const dismissNotice = useCallback(() => setNotice(null), []);

  const tabParam = searchParams.get("tab");
  const activeTab: SettingsTab = SETTINGS_TABS.some((item) => item.key === tabParam)
    ? (tabParam as SettingsTab)
    : "overview";

  const handleTabChange = useCallback(
    (tab: SettingsTab) => {
      setError(null);
      setSearchParams(
        (prev) => {
          const next = new URLSearchParams(prev);
          next.set("tab", tab);
          return next;
        },
        { replace: true },
      );
    },
    [setSearchParams],
  );

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
  const grants = useMemo(
    () => (project ? (grantsByProjectId[project.id] ?? []) : []),
    [grantsByProjectId, project],
  );
  const grantedUserIds = useMemo(
    () => new Set(grants.filter((grant) => grant.subject_type === "user").map((grant) => grant.subject_id)),
    [grants],
  );
  const grantedGroupIds = useMemo(
    () => new Set(grants.filter((grant) => grant.subject_type === "group").map((grant) => grant.subject_id)),
    [grants],
  );
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
    setStallTimeoutMs(project.config.scheduling?.stall_timeout_ms != null ? String(project.config.scheduling.stall_timeout_ms) : "");
    setDeleteConfirmValue("");
    setPickerSelections([]);
    setGrantRole("viewer");
    setDirectoryUsers([]);
    setDirectoryGroups([]);
    setDirectoryUsersStatus(null);
    setDirectoryGroupsStatus(null);
    setWorkspaceInventoryRefreshKey(0);
    setError(null);
    setNotice(null);
  }, [project]);

  const rememberDirectoryUsers = useCallback((items: DirectoryUser[]) => {
    setDirectoryUsers((current) => mergeDirectoryUsers(current, items));
  }, []);

  const rememberDirectoryGroups = useCallback((items: DirectoryGroupSummary[]) => {
    setDirectoryGroups((current) => mergeDirectoryGroups(current, items));
  }, []);

  const loadDirectorySnapshot = useCallback(async () => {
    const [usersResponse, groupsResponse] = await Promise.all([
      fetchDirectoryUsers({ limit: DIRECTORY_SEARCH_LIMIT }),
      fetchDirectoryGroups({ limit: DIRECTORY_SEARCH_LIMIT }),
    ]);
    rememberDirectoryUsers(usersResponse.items);
    rememberDirectoryGroups(groupsResponse.items);
    setDirectoryUsersStatus(statusFrom(usersResponse));
    setDirectoryGroupsStatus(statusFrom(groupsResponse));
  }, [rememberDirectoryGroups, rememberDirectoryUsers]);

  const hydrateDirectorySubjectsForGrants = useCallback(async (grantItems: ProjectSubjectGrant[]) => {
    const userIds = Array.from(new Set(
      grantItems
        .filter((grant) => grant.subject_type === "user")
        .map((grant) => grant.subject_id),
    ));
    const groupIds = Array.from(new Set(
      grantItems
        .filter((grant) => grant.subject_type === "group")
        .map((grant) => grant.subject_id),
    ));
    const [userResults, groupResults] = await Promise.all([
      Promise.allSettled(userIds.map((key) => resolveDirectoryUser({ key }))),
      Promise.allSettled(groupIds.map((key) => resolveDirectoryGroup({ key }))),
    ]);

    const resolvedUsers = userResults
      .filter((result): result is PromiseFulfilledResult<Awaited<ReturnType<typeof resolveDirectoryUser>>> =>
        result.status === "fulfilled",
      )
      .map((result) => result.value.item);
    const resolvedGroups = groupResults
      .filter((result): result is PromiseFulfilledResult<Awaited<ReturnType<typeof resolveDirectoryGroup>>> =>
        result.status === "fulfilled",
      )
      .map((result) => result.value.item);
    rememberDirectoryUsers(resolvedUsers);
    rememberDirectoryGroups(resolvedGroups);
  }, [rememberDirectoryGroups, rememberDirectoryUsers]);

  useEffect(() => {
    if (activeTab !== "management" || !project?.access.can_manage_sharing) return;
    let cancelled = false;

    void (async () => {
      setIsDirectoryLoading(true);
      try {
        const grantItems = await fetchProjectGrants(project.id);
        await Promise.all([
          loadDirectorySnapshot(),
          hydrateDirectorySubjectsForGrants(grantItems),
        ]);
        if (cancelled) return;
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
    fetchProjectGrants,
    hydrateDirectorySubjectsForGrants,
    loadDirectorySnapshot,
    project?.access.can_manage_sharing,
    project?.id,
  ]);

  const refreshWorkspaceBindings = useCallback(async () => {
    if (!projectId) return;
    await fetchWorkspaces(projectId);
    setWorkspaceInventoryRefreshKey((key) => key + 1);
  }, [fetchWorkspaces, projectId]);

  if (!project) {
    return (
      <div className="flex h-full items-center justify-center">
        <div className="text-center">
          <p className="text-sm text-muted-foreground">未找到对应的 Project。</p>
          <button
            type="button"
            onClick={() => navigate("/dashboard/agent")}
            className="mt-3 rounded-[8px] border border-border bg-background px-4 py-2 text-sm text-foreground transition-colors hover:bg-secondary"
          >
            返回 Dashboard
          </button>
        </div>
      </div>
    );
  }

  const canEditProject = project.access.can_edit;
  const canManageSharing = project.access.can_manage_sharing;
  const pendingSelections = pickerSelections.filter((s) =>
    s.type === "user" ? !grantedUserIds.has(s.id) : !grantedGroupIds.has(s.id),
  );
  const activeTabMeta = SETTINGS_TABS.find((item) => item.key === activeTab) ?? SETTINGS_TABS[0];

  const currentStallTimeout = project.config.scheduling?.stall_timeout_ms;
  const baseInfoDirty = name !== project.name || description !== project.description;
  const schedulingDirty =
    stallTimeoutMs.trim() !== (currentStallTimeout != null ? String(currentStallTimeout) : "");
  const templateDirty =
    templateFlag !== project.is_template || templateVisibility !== project.visibility;

  // 所有 config 写入都从完整的 project.config 整体展开，再覆盖目标字段。
  // 后端是整体替换语义，逐字段手抄会漏字段导致其它配置被清空。
  const persistConfig = (patch: Partial<ProjectConfig>) =>
    updateProjectConfig(project.id, { ...project.config, ...patch });

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
    try {
      await updateProject(project.id, {
        name: trimmedName,
        description: description.trim(),
      });
      setError(null);
      setNotice("基础信息已保存");
    } catch (e) {
      setError((e as Error).message || "基础信息保存失败");
    }
  };

  const saveScheduling = async () => {
    if (!canEditProject) {
      setError("当前权限不允许修改调度配置");
      return;
    }
    let stallTimeout: number | null = null;
    if (stallTimeoutMs.trim()) {
      const n = Number(stallTimeoutMs.trim());
      if (!Number.isFinite(n) || n < 0) { setError("超时值必须是非负整数"); return; }
      stallTimeout = n;
    }
    try {
      await persistConfig({ scheduling: { stall_timeout_ms: stallTimeout } });
      setError(null);
      setNotice("调度配置已保存");
    } catch (e) {
      setError((e as Error).message || "调度配置保存失败");
    }
  };

  const saveDefaultWorkspace = async (workspaceId: string | null) => {
    if (!canEditProject) {
      setError("当前权限不允许修改默认工作空间");
      return;
    }
    try {
      await persistConfig({ default_workspace_id: workspaceId });
      setError(null);
      setNotice("默认工作空间已更新");
    } catch (e) {
      setError((e as Error).message || "默认工作空间保存失败");
    }
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
    try {
      await updateProject(project.id, {
        visibility: templateVisibility,
        is_template: templateFlag,
      });
      setError(null);
      setNotice("模板策略已保存");
    } catch (e) {
      setError((e as Error).message || "模板策略保存失败");
    }
  };

  const submitBatchGrant = async () => {
    if (!canManageSharing) {
      setError("当前权限不允许管理共享");
      return;
    }
    if (pendingSelections.length === 0) {
      setError("请先选择要授权的用户或用户组");
      return;
    }

    try {
      const results = await Promise.allSettled(
        pendingSelections.map((s) =>
          s.type === "user"
            ? grantProjectUser(project.id, s.id, grantRole)
            : grantProjectGroup(project.id, s.id, grantRole),
        ),
      );
      const failed = results.filter((r) => r.status === "rejected");
      if (failed.length > 0) {
        setError(`${pendingSelections.length - failed.length} 项授权成功，${failed.length} 项失败`);
      } else {
        setError(null);
        setNotice(`已授权 ${pendingSelections.length} 项`);
      }
      const refreshedGrantItems = await fetchProjectGrants(project.id);
      await Promise.all([
        fetchProjects(),
        loadDirectorySnapshot(),
        hydrateDirectorySubjectsForGrants(refreshedGrantItems),
      ]);
      setPickerSelections([]);
    } catch (grantError) {
      setError((grantError as Error).message);
    }
  };

  const revokeGrant = async (grant: ProjectSubjectGrant) => {
    const revoked = grant.subject_type === "user"
      ? await revokeProjectUser(project.id, grant.subject_id)
      : await revokeProjectGroup(project.id, grant.subject_id);
    if (!revoked) {
      setError("撤销授权失败");
      return;
    }
    await Promise.all([
      fetchProjectGrants(project.id),
      fetchProjects(),
    ]);
    setError(null);
    setNotice("已撤销授权");
  };

  const handleCloneProject = async () => {
    try {
      const cloned = await cloneProject(project.id, {
        name: cloneName.trim() || undefined,
      });
      if (!cloned) {
        setError("克隆 Project 失败");
        return;
      }
      setError(null);
      selectProject(cloned.id);
      navigate(`/projects/${cloned.id}/settings`);
    } catch (e) {
      setError((e as Error).message || "克隆 Project 失败");
    }
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
    try {
      const deleted = await deleteProject(project.id);
      if (!deleted) {
        setError("删除失败，请查看错误信息后重试");
        return;
      }
      navigate("/dashboard/agent");
    } catch (e) {
      setError((e as Error).message || "删除失败，请查看错误信息后重试");
    }
  };

  return (
    <>
      <div className="h-full overflow-y-auto">
        <div className="mx-auto max-w-6xl space-y-5 px-6 py-8">
          <div className="rounded-[12px] border border-border bg-background px-6 py-6">
            <div className="flex flex-col gap-5 lg:flex-row lg:items-start lg:justify-between">
              <div className="space-y-3">
                <button
                  type="button"
                  onClick={() => navigate("/dashboard/agent")}
                  className="inline-flex items-center gap-2 rounded-[8px] border border-border bg-background px-3 py-2 text-sm text-foreground transition-colors hover:bg-secondary"
                >
                  返回
                </button>
                <div className="space-y-2">
                  <p className="text-[11px] uppercase tracking-[0.16em] text-muted-foreground">Project Settings</p>
                  <h1 className="text-[2rem] font-semibold tracking-[-0.03em] text-foreground">{project.name}</h1>
                </div>
              </div>

              <div className="flex max-w-[22rem] flex-wrap gap-2 lg:justify-end">
                <span className="rounded-[8px] border border-border bg-secondary/20 px-3 py-1 text-xs text-foreground">
                  权限：{describeProjectAccess(project)}
                </span>
                <span className="rounded-[8px] border border-border bg-secondary/20 px-3 py-1 text-xs text-muted-foreground">
                  可编辑：{canEditProject ? "是" : "否"}
                </span>
                <span className="rounded-[8px] border border-border bg-secondary/20 px-3 py-1 text-xs text-muted-foreground">
                  可管理共享：{canManageSharing ? "是" : "否"}
                </span>
                {project.is_template && (
                  <span className="rounded-[8px] border border-warning/30 bg-warning/10 px-3 py-1 text-xs text-warning">
                    模板
                  </span>
                )}
              </div>
            </div>
          </div>

          <div className="rounded-[12px] border border-border bg-muted/20 p-2">
            <SettingsTabs tabs={SETTINGS_TABS} activeTab={activeTab} onChange={handleTabChange} />
          </div>

          <div className="rounded-[12px] border border-border bg-background px-6 py-6 md:px-8 md:py-8">
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
                        disabled={!canEditProject || !baseInfoDirty}
                        className="agentdash-button-secondary disabled:cursor-not-allowed disabled:opacity-60"
                      >
                        保存基础信息
                      </button>
                    </div>
                  </ContentGroup>

                  <ContentGroup title="访问摘要">
                    <div className="flex flex-wrap gap-2">
                      <span className="rounded-[8px] border border-border bg-secondary/35 px-3 py-1 text-xs text-muted-foreground">
                        visibility: {PROJECT_VISIBILITY_LABELS[project.visibility]}
                      </span>
                      <span className="rounded-[8px] border border-border bg-secondary/35 px-3 py-1 text-xs text-muted-foreground">
                        default workspace: {project.config.default_workspace_id ?? "未设置"}
                      </span>
                      <span className="rounded-[8px] border border-border bg-secondary/35 px-3 py-1 text-xs text-muted-foreground">
                        workspaces: {workspaces.length}
                      </span>
                      <span className="rounded-[8px] border border-border bg-secondary/35 px-3 py-1 text-xs text-muted-foreground">
                        grants: {grants.length}
                      </span>
                    </div>
                  </ContentGroup>

                  <ContentGroup
                    title="调度安全网"
                    description="平台级安全限制，防止 Agent 失控运行。留空则使用系统默认值。"
                  >
                    <div className="grid gap-3 md:grid-cols-2">
                      <div>
                        <label className="agentdash-form-label">Session 无活动超时 (毫秒)</label>
                        <input
                          type="number"
                          value={stallTimeoutMs}
                          onChange={(e) => setStallTimeoutMs(e.target.value)}
                          disabled={!canEditProject}
                          placeholder="默认 300000 (5 分钟)，0 = 禁用"
                          min={0}
                          className="agentdash-form-input disabled:cursor-not-allowed disabled:opacity-70"
                        />
                      </div>
                    </div>
                    <div className="flex justify-end">
                      <button
                        type="button"
                        onClick={() => void saveScheduling()}
                        disabled={!canEditProject || !schedulingDirty}
                        className="agentdash-button-secondary disabled:cursor-not-allowed disabled:opacity-60"
                      >
                        保存调度配置
                      </button>
                    </div>
                  </ContentGroup>
                </SectionCard>
              )}

              {activeTab === "context" && (
                <ContextTab project={project} />
              )}

              {activeTab === "workspace" && (
                <>
                  <BackendAccessPanel
                    projectId={project.id}
                    canEdit={canEditProject}
                    workspaces={workspaces}
                    inventoryRefreshKey={workspaceInventoryRefreshKey}
                    onWorkspacesChanged={refreshWorkspaceBindings}
                  />

                  <WorkspaceList
                    projectId={project.id}
                    workspaces={workspaces}
                    defaultWorkspaceId={project.config.default_workspace_id}
                    canManageBindings={canManageSharing}
                    onSetDefault={canEditProject ? (wsId) => void saveDefaultWorkspace(wsId) : undefined}
                    onInventoryChanged={() => setWorkspaceInventoryRefreshKey((key) => key + 1)}
                  />

                  <SectionCard
                    title="诊断 / 高级"
                    description="只读诊断，日常无需关注。"
                  >
                    <CollapsibleGroup
                      title="Workspace Modules"
                      hint="Canvas 与 Extension 贡献的协作模块：kind / 来源 / 状态 / operations 与 UI entries 数；unavailable 模块给出诊断。启停在各自的 Extension / Canvas 管理入口完成。"
                    >
                      <WorkspaceModulesPanel projectId={project.id} />
                    </CollapsibleGroup>
                  </SectionCard>

                </>
              )}

              {activeTab === "management" && (
                <>
                  <SectionCard
                    title="共享管理"
                    description="Project 的共享记录独立于 Workspace。这里专门处理用户/用户组授权。"
                  >
                    <ContentGroup title="添加授权">
                      {canManageSharing ? (
                        <>
                          <DirectorySubjectPicker
                            selections={pickerSelections}
                            onSelectionsChange={setPickerSelections}
                            knownUsers={directoryUsers}
                            knownGroups={directoryGroups}
                            userDirectoryStatus={directoryUsersStatus}
                            groupDirectoryStatus={directoryGroupsStatus}
                            grantedUserIds={grantedUserIds}
                            grantedGroupIds={grantedGroupIds}
                            currentUserId={currentUser?.user_id}
                            onUsersObserved={rememberDirectoryUsers}
                            onGroupsObserved={rememberDirectoryGroups}
                          />

                          <div className="flex flex-wrap items-center gap-3">
                            <select
                              value={grantRole}
                              onChange={(event) => setGrantRole(event.target.value as ProjectRole)}
                              className="agentdash-form-select w-auto"
                            >
                              {PROJECT_ROLE_OPTIONS.map((option) => (
                                <option key={option.value} value={option.value}>
                                  {option.label}
                                </option>
                              ))}
                            </select>

                            <button
                              type="button"
                              onClick={() => void submitBatchGrant()}
                              disabled={pendingSelections.length === 0}
                              className="agentdash-button-primary disabled:cursor-not-allowed disabled:opacity-50"
                            >
                              {pendingSelections.length > 0
                                ? `批量授权 (${pendingSelections.length})`
                                : "批量授权"}
                            </button>

                            {pendingSelections.length > 0 && (
                              <button
                                type="button"
                                onClick={() => setPickerSelections([])}
                                className="text-xs text-muted-foreground hover:text-foreground"
                              >
                                清空选择
                              </button>
                            )}
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
                            {grants.map((grant) => {
                              const grantUser = findGrantUser(grant, directoryUsers);
                              const subjectLabel = resolveGrantSubjectLabel(grant, directoryUsers, directoryGroups);
                              const subjectAuditLabel = grant.subject_type === "user"
                                ? resolveUserAuditLabel(grant.subject_id, directoryUsers, currentUser)
                                : grant.subject_id;
                              const grantorAuditLabel = resolveUserAuditLabel(
                                grant.granted_by_user_id,
                                directoryUsers,
                                currentUser,
                              );
                              return (
                                <div
                                  key={`${grant.subject_type}:${grant.subject_id}`}
                                  className="flex flex-wrap items-center justify-between gap-3 rounded-[8px] border border-border bg-background px-3 py-3"
                                >
                                  <div className="flex min-w-0 items-center gap-3">
                                    {grantUser && (
                                      <UserAvatar
                                        avatarUrl={grantUser.avatar_url}
                                        fallback={subjectLabel}
                                        sizeClassName="h-8 w-8"
                                      />
                                    )}
                                    <div className="min-w-0">
                                      <div className="flex flex-wrap items-center gap-2">
                                        <span className="rounded-[8px] border border-border bg-secondary/50 px-2 py-1 text-[11px] uppercase text-muted-foreground">
                                          {grant.subject_type}
                                        </span>
                                        <span className="text-sm font-medium text-foreground">
                                          {subjectLabel}
                                        </span>
                                        <span className="rounded-[8px] border border-border bg-background px-2 py-1 text-[11px] text-muted-foreground">
                                          {PROJECT_ROLE_LABELS[grant.role]}
                                        </span>
                                      </div>
                                      <p
                                        className="mt-1 text-xs text-muted-foreground"
                                        title={`subject_id: ${grant.subject_id} · granted_by: ${grant.granted_by_user_id}`}
                                      >
                                        {grant.subject_type === "user" ? "subject" : "group_id"}: {subjectAuditLabel}
                                        {" · granted_by: "}
                                        {grantorAuditLabel}
                                      </p>
                                    </div>
                                  </div>
                                  <button
                                    type="button"
                                    onClick={() => void revokeGrant(grant)}
                                    className="rounded-[8px] border border-destructive/25 bg-destructive/5 px-3 py-1.5 text-xs text-destructive transition-colors hover:bg-destructive/10"
                                  >
                                    撤销
                                  </button>
                                </div>
                              );
                            })}
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
                              disabled={!templateDirty}
                              className="agentdash-button-secondary disabled:cursor-not-allowed disabled:opacity-60"
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
                          className="rounded-[8px] border border-destructive/25 bg-destructive/5 px-4 py-2 text-sm text-destructive transition-colors hover:bg-destructive/10 disabled:cursor-not-allowed disabled:opacity-60"
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

              {error && (
                <div className="rounded-[12px] border border-destructive/40 bg-destructive/10 px-4 py-3 text-sm text-destructive">
                  {error}
                </div>
              )}
            </div>
          </div>
        </div>
      </div>

      {notice && <Toast message={notice} onDone={dismissNotice} />}

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
