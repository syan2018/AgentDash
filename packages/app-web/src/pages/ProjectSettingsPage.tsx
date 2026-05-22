import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import type { ReactNode } from "react";
import type {
  DirectoryGroup,
  DirectoryUser,
  BackendWorkspaceInventory,
  ProjectBackendAccess,
  Project,
  ProjectRole,
  ProjectSubjectGrant,
  Workspace,
} from "../types";
import { useCurrentUserStore } from "../stores/currentUserStore";
import { useProjectStore } from "../stores/projectStore";
import { useWorkspaceStore } from "../stores/workspaceStore";
import { useCoordinatorStore } from "../stores/coordinatorStore";
import { WorkspaceList } from "../features/workspace/workspace-list";
import { VfsBrowser } from "../features/vfs";
import { resolveVfsSurface } from "../services/vfs";
import type { ResolvedMountSummary } from "../types";
import {
  DangerConfirmDialog,
} from "@agentdash/ui";
import { UserAvatar } from "../components/ui/user-avatar";
import { fetchDirectoryGroups, fetchDirectoryUsers } from "../services/directory";
import {
  createProjectBackendAccess,
  listBackendWorkspaceInventory,
  listProjectBackendAccess,
  revokeProjectBackendAccess,
} from "../services/backendAccess";

type SettingsTab = "overview" | "context" | "workspace" | "management";

interface SettingsTabItem {
  key: SettingsTab;
  label: string;
  description: string;
}

const SETTINGS_TABS: SettingsTabItem[] = [
  { key: "overview", label: "概览", description: "项目身份、摘要与基础信息" },
  { key: "context", label: "VFS 资源", description: "项目级 VFS Mount、解析结果与 runtime preview" },
  { key: "workspace", label: "工作空间", description: "默认 workspace 与运行落点" },
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

function resolveDirectoryUserLabel(user: DirectoryUser): string {
  return user.display_name?.trim() || user.email?.trim() || user.user_id;
}

function findGrantUser(
  grant: ProjectSubjectGrant,
  users: DirectoryUser[],
): DirectoryUser | null {
  if (grant.subject_type !== "user") return null;
  return users.find((item) => item.user_id === grant.subject_id) ?? null;
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
  const [mounts, setMounts] = useState<ResolvedMountSummary[]>([]);
  const [defaultMountId, setDefaultMountId] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    void (async () => {
      try {
        const result = await resolveVfsSurface({
          source_type: "project_preview",
          project_id: projectId,
        });
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
      <div className="rounded-[8px] border border-destructive/20 bg-destructive/5 px-3 py-2 text-xs text-destructive">
        {error}
      </div>
    );
  }

  if (mounts.length === 0) {
    return (
      <p className="rounded-[8px] border border-dashed border-border px-4 py-4 text-center text-sm text-muted-foreground">
        当前配置下没有可用的 VFS Mount。请先配置工作空间或项目级 VFS Mount。
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
                          ? "bg-success"
                          : online === false
                            ? "bg-destructive"
                            : "bg-muted-foreground/30"
                      }`}
                      title={online === true ? "Backend 在线" : online === false ? "Backend 离线" : "状态未知"}
                    />
                  ) : (
                    // eslint-disable-next-line no-restricted-syntax -- 状态指示圆点
                    <span className="inline-block h-2 w-2 shrink-0 rounded-full bg-info" />
                  )}

                  <p className="truncate text-sm font-medium text-foreground">
                    {mount.display_name}
                  </p>

                  {isDefault && (
                    <span className="inline-flex items-center rounded-[8px] border border-primary/25 bg-primary/10 px-2 py-0.5 text-[10px] font-medium text-primary">
                      默认
                    </span>
                  )}
                  {mount.default_write && (
                    <span className="inline-flex items-center rounded-[8px] border border-warning/25 bg-warning/10 px-2 py-0.5 text-[10px] font-medium text-warning">
                      默认写入
                    </span>
                  )}
                  <span className="rounded-[8px] border border-border bg-muted/40 px-2 py-0.5 text-[10px] text-muted-foreground">
                    {providerLabel}
                  </span>
                </div>

                <p className="mt-1 truncate font-mono text-xs text-muted-foreground">
                  {mount.id}
                </p>
              </div>

              {/* 能力标签 */}
              <div className="flex shrink-0 flex-wrap justify-end gap-1">
                {mount.capabilities.map((cap) => (
                  <span
                    key={cap}
                    className="rounded-[8px] border border-border bg-background px-2 py-0.5 text-[10px] text-muted-foreground"
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
}: {
  project: Project;
}) {
  return (
    <>
      <SectionCard
        title="Project VFS Mount"
        description="Project 级 VFS 挂载点（Inline 文件 / 外部服务）已归入 Assets，CRUD 在资产流程中统一管理。"
      >
        <Link to="/dashboard/assets/vfs-mount" className="agentdash-button-secondary inline-flex">
          打开 VFS Mount 资产
        </Link>
      </SectionCard>

      <SectionCard
        title="解析后的 VFS Mount"
        description="基于当前 Workspace 与项目级 VFS 配置派生出的运行时挂载点概览。"
      >
        <MountOverviewList projectId={project.id} refreshKey={0} />
      </SectionCard>

      <SectionCard
        title="Runtime Preview"
        description="VFS 预览明确作为派生结果展示，用来解释当前默认配置会解析出什么挂载。"
      >
        <VfsBrowser source={{ source_type: "project_preview", project_id: project.id }} />
      </SectionCard>
    </>
  );
}

const ACCESS_STATUS_LABELS: Record<ProjectBackendAccess["status"], string> = {
  active: "已启用",
  paused: "已暂停",
  revoked: "已撤销",
};

const INVENTORY_STATUS_LABELS: Record<BackendWorkspaceInventory["status"], string> = {
  available: "可用",
  stale: "过期",
  offline: "离线",
  error: "异常",
};

function BackendAccessPanel({
  projectId,
  canEdit,
  inventoryRefreshKey = 0,
}: {
  projectId: string;
  canEdit: boolean;
  inventoryRefreshKey?: number;
}) {
  const backends = useCoordinatorStore((state) => state.backends);
  const fetchBackends = useCoordinatorStore((state) => state.fetchBackends);
  const [accesses, setAccesses] = useState<ProjectBackendAccess[]>([]);
  const [inventoriesByAccessId, setInventoriesByAccessId] = useState<Record<string, BackendWorkspaceInventory[]>>({});
  const [expandedInventoryAccessIds, setExpandedInventoryAccessIds] = useState<Record<string, boolean>>({});
  const [loadingInventoryAccessIds, setLoadingInventoryAccessIds] = useState<Record<string, boolean>>({});
  const [selectedBackendId, setSelectedBackendId] = useState("");
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const hasObservedBackendRuntimeRef = useRef(false);

  const load = useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      const nextAccesses = await listProjectBackendAccess(projectId);
      setAccesses(nextAccesses);
    } catch (loadError) {
      setError((loadError as Error).message);
    } finally {
      setIsLoading(false);
    }
  }, [projectId]);

  useEffect(() => {
    void fetchBackends();
    void load();
  }, [fetchBackends, load]);

  const reloadExpandedInventories = useCallback(async () => {
    const expandedAccessIds = Object.entries(expandedInventoryAccessIds)
      .filter(([, expanded]) => expanded)
      .map(([accessId]) => accessId);
    if (expandedAccessIds.length === 0) return;

    setError(null);
    for (const accessId of expandedAccessIds) {
      setLoadingInventoryAccessIds((current) => ({ ...current, [accessId]: true }));
    }
    try {
      const inventoryEntries = await Promise.all(
        expandedAccessIds.map(async (accessId) => [
          accessId,
          await listBackendWorkspaceInventory(projectId, accessId),
        ] as const),
      );
      setInventoriesByAccessId((current) => ({
        ...current,
        ...Object.fromEntries(inventoryEntries),
      }));
    } catch (inventoryError) {
      setError((inventoryError as Error).message);
    } finally {
      for (const accessId of expandedAccessIds) {
        setLoadingInventoryAccessIds((current) => ({ ...current, [accessId]: false }));
      }
    }
  }, [expandedInventoryAccessIds, projectId]);

  useEffect(() => {
    if (inventoryRefreshKey === 0) return;
    void load();
    void reloadExpandedInventories();
  }, [inventoryRefreshKey, load, reloadExpandedInventories]);

  const authorizedBackendIds = useMemo(
    () => new Set(accesses.map((access) => access.backend_id)),
    [accesses],
  );
  const selectableBackends = useMemo(
    () => backends.filter((backend) => !authorizedBackendIds.has(backend.id)),
    [authorizedBackendIds, backends],
  );
  const backendRuntimeSignature = useMemo(
    () => backends
      .map((backend) => [
        backend.id,
        backend.online ? "online" : "offline",
        backend.runtime_health?.status ?? "",
        backend.runtime_health?.updated_at ?? "",
      ].join(":"))
      .join("|"),
    [backends],
  );

  useEffect(() => {
    if (!backendRuntimeSignature) return;
    if (!hasObservedBackendRuntimeRef.current) {
      hasObservedBackendRuntimeRef.current = true;
      return;
    }
    void load();
    void reloadExpandedInventories();
  }, [backendRuntimeSignature, load, reloadExpandedInventories]);

  useEffect(() => {
    if (selectedBackendId && selectableBackends.some((backend) => backend.id === selectedBackendId)) return;
    setSelectedBackendId(selectableBackends[0]?.id ?? "");
  }, [selectableBackends, selectedBackendId]);

  const backendName = (backendId: string) => backends.find((backend) => backend.id === backendId)?.name ?? backendId;

  const handleAddAccess = async () => {
    if (!selectedBackendId) {
      setError("请选择 backend");
      return;
    }
    setError(null);
    try {
      const access = await createProjectBackendAccess(projectId, {
        backend_id: selectedBackendId,
      });
      setAccesses((current) => {
        const next = current.filter((item) => item.id !== access.id);
        return [...next, access].sort((a, b) => b.priority - a.priority);
      });
    } catch (addError) {
      setError((addError as Error).message);
    }
  };

  const handleLoadInventory = async (access: ProjectBackendAccess) => {
    const isExpanded = expandedInventoryAccessIds[access.id] === true;
    if (isExpanded) {
      setExpandedInventoryAccessIds((current) => ({ ...current, [access.id]: false }));
      return;
    }
    setExpandedInventoryAccessIds((current) => ({ ...current, [access.id]: true }));
    setError(null);
    setLoadingInventoryAccessIds((current) => ({ ...current, [access.id]: true }));
    try {
      const items = await listBackendWorkspaceInventory(projectId, access.id);
      setInventoriesByAccessId((current) => ({ ...current, [access.id]: items }));
    } catch (inventoryError) {
      setError((inventoryError as Error).message);
    } finally {
      setLoadingInventoryAccessIds((current) => ({ ...current, [access.id]: false }));
    }
  };

  const handleRevoke = async (access: ProjectBackendAccess) => {
    setError(null);
    try {
      await revokeProjectBackendAccess(projectId, access.id);
      setAccesses((current) => current.filter((item) => item.id !== access.id));
    } catch (revokeError) {
      setError((revokeError as Error).message);
    }
  };

  return (
    <div className="space-y-6">
      <ContentGroup title="Backend Access">
        <div className="grid gap-3 md:grid-cols-[minmax(0,1fr)_auto]">
          <select
            value={selectedBackendId}
            onChange={(event) => setSelectedBackendId(event.target.value)}
            disabled={!canEdit}
            className="agentdash-form-select disabled:cursor-not-allowed disabled:opacity-60"
          >
            <option value="">选择 backend</option>
            {selectableBackends.map((backend) => (
              <option key={backend.id} value={backend.id}>
                {backend.name} {backend.online ? "(online)" : "(offline)"}
              </option>
            ))}
          </select>
          <button
            type="button"
            onClick={() => void handleAddAccess()}
            disabled={!canEdit || !selectedBackendId}
            className="agentdash-button-secondary disabled:cursor-not-allowed disabled:opacity-60"
          >
            绑定 Backend
          </button>
        </div>

        {isLoading && <p className="text-xs text-muted-foreground">正在加载 backend access...</p>}
        {accesses.length === 0 && !isLoading && (
          <p className="rounded-[12px] border border-dashed border-border px-4 py-4 text-sm text-muted-foreground">
            当前 Project 还没有 backend 访问权限。
          </p>
        )}

        <div className="space-y-3">
          {accesses.map((access) => {
            const inventory = inventoriesByAccessId[access.id] ?? [];
            const inventoryExpanded = expandedInventoryAccessIds[access.id] === true;
            const inventoryLoading = loadingInventoryAccessIds[access.id] === true;
            return (
              <div key={access.id} className="rounded-[12px] border border-border bg-background px-4 py-4">
                <div className="flex flex-wrap items-start justify-between gap-3">
                  <div className="min-w-0">
                    <div className="flex flex-wrap items-center gap-2">
                      <p className="truncate text-sm font-medium text-foreground">{backendName(access.backend_id)}</p>
                      <span className="rounded-[8px] border border-border bg-secondary/40 px-2 py-0.5 text-[10px] text-muted-foreground">
                        {ACCESS_STATUS_LABELS[access.status]}
                      </span>
                      <span className="rounded-[8px] border border-border bg-background px-2 py-0.5 text-[10px] text-muted-foreground">
                        priority {access.priority}
                      </span>
                    </div>
                    <p className="mt-1 truncate font-mono text-xs text-muted-foreground">{access.backend_id}</p>
                  </div>
                  <div className="flex flex-wrap gap-2">
                    <button
                      type="button"
                      onClick={() => void handleLoadInventory(access)}
                      className="inline-flex items-center gap-1.5 rounded-[8px] border border-border bg-background px-3 py-1.5 text-xs text-muted-foreground hover:bg-secondary"
                    >
                      <span className="text-[10px]">{inventoryExpanded ? "▾" : "▸"}</span>
                      <span>{inventoryExpanded ? "收起 Inventory" : "展开 Inventory"}</span>
                    </button>
                    <button
                      type="button"
                      onClick={() => void handleRevoke(access)}
                      disabled={!canEdit}
                      className="rounded-[8px] border border-destructive/25 bg-destructive/5 px-3 py-1.5 text-xs text-destructive hover:bg-destructive/10 disabled:cursor-not-allowed disabled:opacity-50"
                    >
                      解绑
                    </button>
                  </div>
                </div>

                {inventoryExpanded && (
                  <div className="mt-3 space-y-2 border-t border-border/70 pt-3">
                    {inventoryLoading ? (
                      <p className="rounded-[8px] border border-border bg-muted/25 px-3 py-3 text-xs text-muted-foreground">
                        正在加载 inventory...
                      </p>
                    ) : inventory.length === 0 ? (
                      <p className="rounded-[8px] border border-dashed border-border px-3 py-3 text-xs text-muted-foreground">
                        当前还没有可用目录快照。等待 backend 上报，或在 Workspace 详情中登记新的可用目录。
                      </p>
                    ) : (
                      inventory.map((item) => (
                        <div key={item.id} className="grid gap-2 rounded-[8px] bg-muted/25 px-3 py-2 text-xs md:grid-cols-[120px_minmax(0,1fr)_100px]">
                          <span className="text-muted-foreground">{item.identity_kind}</span>
                          <span className="truncate font-mono text-foreground">{item.root_ref}</span>
                          <span className="text-muted-foreground">{INVENTORY_STATUS_LABELS[item.status]}</span>
                        </div>
                      ))
                    )}
                  </div>
                )}
              </div>
            );
          })}
        </div>
      </ContentGroup>

      {error && (
        <div className="rounded-[12px] border border-destructive/40 bg-destructive/10 px-4 py-3 text-sm text-destructive">
          {error}
        </div>
      )}
    </div>
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
  const [stallTimeoutMs, setStallTimeoutMs] = useState("");
  const [workspaceInventoryRefreshKey, setWorkspaceInventoryRefreshKey] = useState(0);
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
    setStallTimeoutMs(project.config.scheduling?.stall_timeout_ms != null ? String(project.config.scheduling.stall_timeout_ms) : "");
    setDeleteConfirmValue("");
    setShareTargetType("user");
    setSelectedUserId("");
    setSelectedGroupId("");
    setGrantRole("viewer");
    setActiveTab("overview");
    setWorkspaceInventoryRefreshKey(0);
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
  };

  const saveScheduling = async () => {
    if (!canEditProject) {
      setError("当前权限不允许修改调度配置");
      return;
    }
    const scheduling: Record<string, unknown> = {};
    if (stallTimeoutMs.trim()) {
      const n = Number(stallTimeoutMs.trim());
      if (!Number.isFinite(n) || n < 0) { setError("超时值必须是非负整数"); return; }
      scheduling.stall_timeout_ms = n;
    }
    const result = await updateProjectConfig(project.id, {
      default_agent_type: project.config.default_agent_type ?? null,
      default_workspace_id: project.config.default_workspace_id ?? null,
      agent_presets: project.config.agent_presets ?? [],
      context_containers: contextContainers,
      scheduling,
    });
    if (!result) { setError("调度配置保存失败"); return; }
    setError(null);
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
    });
    if (!result) {
      setError("默认工作空间保存失败");
      return;
    }
    setError(null);
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
                  <p className="max-w-3xl text-sm leading-6 text-muted-foreground">
                    设置页按概览、VFS 资源、工作空间和管理动作分栏收纳，让逻辑 workspace、运行时派生结果和项目级配置分开表达。
                  </p>
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
                        disabled={!canEditProject}
                        className="agentdash-button-secondary"
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
                        disabled={!canEditProject}
                        className="agentdash-button-secondary"
                      >
                        保存调度配置
                      </button>
                    </div>
                  </ContentGroup>
                </SectionCard>
              )}

              {activeTab === "context" && (
                <ContextTabContent
                  project={project}
                />
              )}

              {activeTab === "workspace" && (
                <>
                  <SectionCard
                    title="Backend Access"
                    description="Project 绑定可使用的 backend；可用目录由 backend 上报或在 Workspace 详情中登记。"
                  >
                    <BackendAccessPanel
                      projectId={project.id}
                      canEdit={canEditProject}
                      inventoryRefreshKey={workspaceInventoryRefreshKey}
                    />
                  </SectionCard>

                  <SectionCard
                    title="工作空间"
                    description="逻辑 Workspace 只表达身份；物理 backend/root 落点由可用目录确认。"
                  >
                    <WorkspaceList
                      projectId={project.id}
                      workspaces={workspaces}
                      defaultWorkspaceId={project.config.default_workspace_id}
                      canManageBindings={canManageSharing}
                      onSetDefault={canEditProject ? (wsId) => void saveDefaultWorkspace(wsId) : undefined}
                      onInventoryChanged={() => setWorkspaceInventoryRefreshKey((key) => key + 1)}
                    />
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
                                    {resolveDirectoryUserLabel(user)}
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
                            {grants.map((grant) => {
                              const grantUser = findGrantUser(grant, directoryUsers);
                              const subjectLabel = resolveGrantSubjectLabel(grant, directoryUsers, directoryGroups);
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
                                      <p className="mt-1 text-xs text-muted-foreground">
                                        subject_id: {grant.subject_id} · granted_by: {grant.granted_by_user_id}
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
