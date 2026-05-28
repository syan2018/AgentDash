import { useCallback, useEffect, useMemo, useState } from "react";

import {
  AssetCard,
  CardMenu,
  CreateButton,
  DangerConfirmDialog,
  DismissibleNotice,
  type DismissibleNoticeData,
  OriginBadge as UiOriginBadge,
} from "@agentdash/ui";
import { buildAssetMenuItems } from "../_shared/assetMenu";

import { useProjectStore } from "../../../stores/projectStore";
import { useCurrentUserStore } from "../../../stores/currentUserStore";
import {
  createProjectVfsMount,
  deleteProjectVfsMount,
  listProjectVfsMounts,
  updateProjectVfsMount,
} from "../../../services/projectVfsMounts";
import type {
  LibraryAssetDto,
  ProjectVfsMount,
  ProjectVfsMountContent,
} from "../../../types";
import { VfsBrowser } from "../../vfs";
import { PublishedBadge } from "../_shared/PublishedBadge";
import { SelectProjectEmpty } from "../_shared/SelectProjectEmpty";
import { resolveOriginBadge } from "../_shared/origin-badge-tone";
import { useLibraryPublishedAssets } from "../_shared/useLibraryPublishedAssets";
import { PublishLibraryAssetDialog } from "../publish/PublishLibraryAssetDialog";

type DetailMode =
  | { kind: "closed" }
  | { kind: "create" }
  | { kind: "edit"; mountId: string };

type ContentKind = "inline" | "external_service";

type Capability = ProjectVfsMount["capabilities"][number];

interface VfsMountFormState {
  mount_id: string;
  display_name: string;
  description: string;
  content_kind: ContentKind;
  service_id: string;
  root_ref: string;
  capabilities: Capability[];
}

const ALL_CAPABILITIES: Capability[] = ["read", "write", "list", "search"];
const DEFAULT_INLINE_CAPABILITIES: Capability[] = ["read", "write", "list", "search"];
const DEFAULT_EXTERNAL_CAPABILITIES: Capability[] = ["read", "list", "search"];

const EMPTY_FORM: VfsMountFormState = {
  mount_id: "",
  display_name: "",
  description: "",
  content_kind: "inline",
  service_id: "",
  root_ref: "",
  capabilities: DEFAULT_INLINE_CAPABILITIES,
};

function formFromMount(item: ProjectVfsMount): VfsMountFormState {
  if (item.content.kind === "inline") {
    return {
      mount_id: item.mount_id,
      display_name: item.display_name,
      description: item.description ?? "",
      content_kind: "inline",
      service_id: "",
      root_ref: "",
      capabilities: item.capabilities,
    };
  }
  return {
    mount_id: item.mount_id,
    display_name: item.display_name,
    description: item.description ?? "",
    content_kind: "external_service",
    service_id: item.content.service_id,
    root_ref: item.content.root_ref,
    capabilities: item.capabilities,
  };
}

function buildContent(form: VfsMountFormState): ProjectVfsMountContent {
  if (form.content_kind === "inline") return { kind: "inline" };
  return {
    kind: "external_service",
    service_id: form.service_id.trim(),
    root_ref: form.root_ref.trim(),
  };
}

export function VfsMountCategoryPanel() {
  const currentProjectId = useProjectStore((s) => s.currentProjectId);
  const projects = useProjectStore((s) => s.projects);
  const bumpRevision = useProjectStore((s) => s.bumpVfsMountsRevision);
  const currentProject = useMemo(
    () => projects.find((p) => p.id === currentProjectId) ?? null,
    [currentProjectId, projects],
  );
  const currentUserId = useCurrentUserStore((s) => s.currentUser?.user_id ?? null);

  const [items, setItems] = useState<ProjectVfsMount[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [detail, setDetail] = useState<DetailMode>({ kind: "closed" });
  const [form, setForm] = useState<VfsMountFormState>(EMPTY_FORM);
  const [confirmDelete, setConfirmDelete] = useState<ProjectVfsMount | null>(null);
  const [publishTarget, setPublishTarget] = useState<ProjectVfsMount | null>(null);
  const { publishedByKey: publishedByMountId, reloadPublished } =
    useLibraryPublishedAssets("vfs_mount_template");
  const [notice, setNotice] = useState<DismissibleNoticeData | null>(null);

  const showSuccess = useCallback((message: string) => setNotice({ tone: "success", message }), []);
  const showError = useCallback((message: string) => setNotice({ tone: "danger", message }), []);
  const clearNotice = useCallback(() => setNotice(null), []);

  const load = useCallback(async () => {
    if (!currentProjectId) return;
    setIsLoading(true);
    clearNotice();
    try {
      setItems(await listProjectVfsMounts(currentProjectId));
    } catch (err) {
      showError(err instanceof Error ? err.message : "加载 VFS Mount 资产失败");
    } finally {
      setIsLoading(false);
    }
  }, [currentProjectId, clearNotice, showError]);

  useEffect(() => {
    void load();
  }, [load]);

  const openCreate = useCallback(() => {
    setForm(EMPTY_FORM);
    clearNotice();
    setDetail({ kind: "create" });
  }, [clearNotice]);

  const openEdit = useCallback((item: ProjectVfsMount) => {
    setForm(formFromMount(item));
    clearNotice();
    setDetail({ kind: "edit", mountId: item.mount_id });
  }, [clearNotice]);

  const closeDetail = useCallback(() => {
    setDetail({ kind: "closed" });
    setForm(EMPTY_FORM);
  }, []);

  const handleCreate = useCallback(async () => {
    if (!currentProjectId || detail.kind !== "create") return;
    const mountId = form.mount_id.trim();
    const displayName = form.display_name.trim() || mountId;
    if (!mountId) {
      showError("mount_id 不能为空");
      return;
    }
    if (items.some((existing) => existing.mount_id === mountId)) {
      showError(`mount_id "${mountId}" 已存在`);
      return;
    }
    if (form.content_kind === "external_service") {
      if (!form.service_id.trim() || !form.root_ref.trim()) {
        showError("ExternalService 必须填 service_id 与 root_ref");
        return;
      }
    }
    setIsSaving(true);
    try {
      const created = await createProjectVfsMount(currentProjectId, {
        mount_id: mountId,
        display_name: displayName,
        description: form.description.trim() || null,
        capabilities: form.capabilities,
        content: buildContent({ ...form, mount_id: mountId, display_name: displayName }),
      });
      setItems((prev) => [...prev, created]);
      bumpRevision(currentProjectId);
      showSuccess(`已创建 VFS Mount：${created.mount_id}`);
      setForm(formFromMount(created));
      setDetail({ kind: "edit", mountId: created.mount_id });
    } catch (err) {
      showError(err instanceof Error ? err.message : "创建 VFS Mount 失败");
    } finally {
      setIsSaving(false);
    }
  }, [currentProjectId, detail, form, items, bumpRevision, showError, showSuccess]);

  const handleSave = useCallback(async () => {
    if (!currentProjectId || detail.kind !== "edit") return;
    const newMountId = form.mount_id.trim();
    const displayName = form.display_name.trim() || newMountId;
    if (!newMountId) {
      showError("mount_id 不能为空");
      return;
    }
    if (
      items.some(
        (existing) =>
          existing.mount_id === newMountId && existing.mount_id !== detail.mountId,
      )
    ) {
      showError(`mount_id "${newMountId}" 已存在`);
      return;
    }
    if (form.content_kind === "external_service") {
      if (!form.service_id.trim() || !form.root_ref.trim()) {
        showError("ExternalService 必须填 service_id 与 root_ref");
        return;
      }
    }
    setIsSaving(true);
    try {
      const updated = await updateProjectVfsMount(currentProjectId, detail.mountId, {
        mount_id: newMountId,
        display_name: displayName,
        description: form.description.trim() || null,
        capabilities: form.capabilities,
        content: buildContent({ ...form, mount_id: newMountId, display_name: displayName }),
      });
      setItems((prev) =>
        prev.map((item) => (item.mount_id === detail.mountId ? updated : item)),
      );
      bumpRevision(currentProjectId);
      showSuccess(`已保存 VFS Mount：${updated.mount_id}`);
      setForm(formFromMount(updated));
      setDetail({ kind: "edit", mountId: updated.mount_id });
    } catch (err) {
      showError(err instanceof Error ? err.message : "保存 VFS Mount 失败");
    } finally {
      setIsSaving(false);
    }
  }, [currentProjectId, detail, form, items, bumpRevision, showError, showSuccess]);

  const handleDelete = useCallback(async () => {
    if (!currentProjectId || !confirmDelete) return;
    setBusyId(confirmDelete.mount_id);
    try {
      await deleteProjectVfsMount(currentProjectId, confirmDelete.mount_id);
      setItems((prev) => prev.filter((item) => item.mount_id !== confirmDelete.mount_id));
      bumpRevision(currentProjectId);
      showSuccess(`已删除 VFS Mount：${confirmDelete.mount_id}`);
      if (detail.kind === "edit" && detail.mountId === confirmDelete.mount_id) {
        closeDetail();
      }
      setConfirmDelete(null);
    } catch (err) {
      showError(err instanceof Error ? err.message : "删除 VFS Mount 失败");
    } finally {
      setBusyId(null);
    }
  }, [confirmDelete, currentProjectId, detail, bumpRevision, closeDetail, showError, showSuccess]);

  if (!currentProjectId || !currentProject) {
    return <SelectProjectEmpty assetLabel="VFS Mount 资产" />;
  }

  return (
    <div className="flex h-full flex-col gap-4 p-6">
      <header className="flex flex-wrap items-center justify-between gap-3">
        <div className="space-y-1">
          <h2 className="text-base font-semibold tracking-tight text-foreground">VFS Mount 资产</h2>
          <p className="text-xs text-muted-foreground">
            {items.length > 0
              ? `${items.length} 个 Project 级 VFS 挂载点 · Inline 文件 / External Service`
              : "0 个 VFS Mount · 可创建 Inline 文件挂载或 ExternalService 挂载"}
          </p>
        </div>
        <CreateButton entity="VFS Mount" onClick={openCreate} />
      </header>

      <DismissibleNotice notice={notice} onDismiss={clearNotice} />

      {isLoading ? (
        <div className="rounded-[8px] border border-dashed border-border px-6 py-10 text-center text-sm text-muted-foreground">
          正在加载 VFS Mount 资产…
        </div>
      ) : (
        <VfsMountGrid
          items={items}
          publishedByMountId={publishedByMountId}
          busyId={busyId}
          onEdit={openEdit}
          onPublish={setPublishTarget}
          onDelete={setConfirmDelete}
        />
      )}

      {detail.kind !== "closed" && (
        <VfsMountEditorDialog
          mode={detail}
          projectId={currentProjectId}
          form={form}
          onFormChange={setForm}
          isSaving={isSaving}
          onCreate={() => void handleCreate()}
          onSave={() => void handleSave()}
          onClose={closeDetail}
        />
      )}

      <DangerConfirmDialog
        open={confirmDelete != null}
        title="确认删除"
        description={
          confirmDelete
            ? `确定要删除 VFS Mount ${confirmDelete.mount_id} 吗？${
                confirmDelete.content.kind === "inline"
                  ? "其中的文件也会一并删除。"
                  : "此操作不可撤销。"
              }`
            : ""
        }
        confirmLabel={
          confirmDelete && busyId === confirmDelete.mount_id ? "删除中…" : "删除"
        }
        isConfirming={confirmDelete != null && busyId === confirmDelete.mount_id}
        onClose={() => setConfirmDelete(null)}
        onConfirm={() => void handleDelete()}
      />

      {publishTarget && (
        <PublishLibraryAssetDialog
          projectId={currentProjectId}
          assetKind="vfs_mount"
          projectAssetId={publishTarget.mount_id}
          defaults={{
            key: publishTarget.mount_id,
            display_name: publishTarget.display_name,
            description: publishTarget.description ?? null,
          }}
          currentUserId={currentUserId}
          onClose={() => setPublishTarget(null)}
          onPublished={(message) => {
            showSuccess(message);
            setPublishTarget(null);
            void load();
            reloadPublished();
          }}
        />
      )}
    </div>
  );
}

export default VfsMountCategoryPanel;

/* ─── Origin Badge ─── */

function VfsMountOriginBadge({ mount }: { mount: ProjectVfsMount }) {
  const installed = Boolean(mount.installed_source);
  const { label, tone } = resolveOriginBadge("user_authored", installed);
  return <UiOriginBadge label={label} tone={tone} url={null} />;
}

/* ─── Grid ─── */

function VfsMountGrid({
  items,
  publishedByMountId,
  busyId,
  onEdit,
  onPublish,
  onDelete,
}: {
  items: ProjectVfsMount[];
  publishedByMountId: Map<string, LibraryAssetDto>;
  busyId: string | null;
  onEdit: (item: ProjectVfsMount) => void;
  onPublish: (item: ProjectVfsMount) => void;
  onDelete: (item: ProjectVfsMount) => void;
}) {
  if (items.length === 0) {
    return (
      <div className="rounded-[8px] border border-dashed border-border bg-secondary/20 px-6 py-14 text-center">
        <p className="text-sm text-foreground">暂无 VFS Mount</p>
        <p className="mt-1.5 text-xs text-muted-foreground">
          点击右上角「+ VFS Mount」创建一个挂载点
        </p>
      </div>
    );
  }

  return (
    <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
      {items.map((item) => {
        const isInstalled = Boolean(item.installed_source);
        const canPublish = !isInstalled;
        const published = publishedByMountId.get(item.mount_id) ?? null;
        const isBusy = busyId === item.mount_id;
        const contentKind = item.content.kind;
        const menuItems = buildAssetMenuItems({
          primary: { label: "编辑", onSelect: () => onEdit(item) },
          publish: canPublish
            ? { published: Boolean(published), onSelect: () => onPublish(item) }
            : null,
          danger: { label: "删除", busy: isBusy, onSelect: () => onDelete(item) },
        });

        return (
          <AssetCard
            key={item.mount_id}
            onOpen={() => onEdit(item)}
            openTitle="编辑"
            title={item.display_name}
            subtitle={<span className="font-mono">{item.mount_id}</span>}
            description={item.description}
            headerRight={
              <>
                <span className="rounded-[8px] border border-border px-1.5 py-0.5 text-[10px] uppercase text-muted-foreground">
                  {contentKind === "inline" ? "Inline" : "External"}
                </span>
                {published && <PublishedBadge version={published.version} />}
                <VfsMountOriginBadge mount={item} />
                <CardMenu items={menuItems} />
              </>
            }
          />
        );
      })}
    </div>
  );
}

/* ─── Editor Dialog ─── */

function VfsMountEditorDialog({
  mode,
  projectId,
  form,
  onFormChange,
  isSaving,
  onCreate,
  onSave,
  onClose,
}: {
  mode: DetailMode;
  projectId: string;
  form: VfsMountFormState;
  onFormChange: (form: VfsMountFormState) => void;
  isSaving: boolean;
  onCreate: () => void;
  onSave: () => void;
  onClose: () => void;
}) {
  if (mode.kind === "closed") return null;
  const isCreate = mode.kind === "create";
  const editingMountId = mode.kind === "edit" ? mode.mountId : null;
  const updateField = <K extends keyof VfsMountFormState>(key: K, value: VfsMountFormState[K]) => {
    onFormChange({ ...form, [key]: value });
  };

  const isInline = form.content_kind === "inline";
  const showVfsBrowser = !isCreate && isInline;

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-3 sm:p-6"
      onClick={onClose}
    >
      <div
        className={`flex h-[90vh] ${
          showVfsBrowser ? "w-[min(94vw,1480px)]" : "w-[min(94vw,640px)]"
        } flex-col overflow-hidden rounded-[8px] border border-border bg-background shadow-xl`}
        onClick={(e) => e.stopPropagation()}
      >
        <header className="flex items-center justify-between border-b border-border px-5 py-4">
          <div>
            <h3 className="text-sm font-semibold text-foreground">
              {isCreate ? "新建 VFS Mount" : "编辑 VFS Mount"}
            </h3>
            <p className="mt-0.5 text-xs text-muted-foreground">
              {form.mount_id ? `mount: ${form.mount_id}` : "mount_id 决定 Project VFS 挂载点 ID"}
            </p>
          </div>
          <button type="button" onClick={onClose} className="agentdash-button-secondary">
            关闭
          </button>
        </header>

        <div
          className={`grid min-h-0 flex-1 grid-cols-1 ${
            showVfsBrowser ? "lg:grid-cols-[360px_minmax(0,1fr)]" : ""
          }`}
        >
          <section className="space-y-4 overflow-y-auto border-b border-border p-5 lg:border-b-0 lg:border-r">
            {isCreate && (
              <fieldset className="space-y-2">
                <legend className="agentdash-form-label">挂载内容</legend>
                <div className="grid grid-cols-2 gap-2">
                  <button
                    type="button"
                    onClick={() => updateField("content_kind", "inline")}
                    className={`rounded-[6px] border px-3 py-2 text-left text-xs ${
                      form.content_kind === "inline"
                        ? "border-primary bg-primary/10 text-foreground"
                        : "border-border text-muted-foreground hover:border-primary/40"
                    }`}
                  >
                    <p className="font-medium">Inline 文件</p>
                    <p className="mt-0.5 text-[11px]">云端存储的文件挂载</p>
                  </button>
                  <button
                    type="button"
                    onClick={() => {
                      onFormChange({
                        ...form,
                        content_kind: "external_service",
                        capabilities: DEFAULT_EXTERNAL_CAPABILITIES,
                      });
                    }}
                    className={`rounded-[6px] border px-3 py-2 text-left text-xs ${
                      form.content_kind === "external_service"
                        ? "border-primary bg-primary/10 text-foreground"
                        : "border-border text-muted-foreground hover:border-primary/40"
                    }`}
                  >
                    <p className="font-medium">External Service</p>
                    <p className="mt-0.5 text-[11px]">外部 Provider (service_id + root_ref)</p>
                  </button>
                </div>
              </fieldset>
            )}

            <label className="block space-y-1.5">
              <span className="agentdash-form-label">mount_id</span>
              <input
                value={form.mount_id}
                onChange={(e) => updateField("mount_id", e.target.value)}
                placeholder="my-mount"
                className="agentdash-form-input"
              />
            </label>

            <label className="block space-y-1.5">
              <span className="agentdash-form-label">显示名称</span>
              <input
                value={form.display_name}
                onChange={(e) => updateField("display_name", e.target.value)}
                placeholder="留空则使用 mount_id"
                className="agentdash-form-input"
              />
            </label>

            <label className="block space-y-1.5">
              <span className="agentdash-form-label">描述</span>
              <textarea
                value={form.description}
                onChange={(e) => updateField("description", e.target.value)}
                rows={3}
                className="agentdash-form-textarea"
                placeholder="可选"
              />
            </label>

            {form.content_kind === "external_service" && (
              <>
                <label className="block space-y-1.5">
                  <span className="agentdash-form-label">service_id</span>
                  <input
                    value={form.service_id}
                    onChange={(e) => updateField("service_id", e.target.value)}
                    placeholder="例如 lifecycle_vfs"
                    className="agentdash-form-input"
                  />
                </label>
                <label className="block space-y-1.5">
                  <span className="agentdash-form-label">root_ref</span>
                  <input
                    value={form.root_ref}
                    onChange={(e) => updateField("root_ref", e.target.value)}
                    placeholder="例如 lifecycle://run/..."
                    className="agentdash-form-input"
                  />
                </label>
              </>
            )}

            <fieldset className="space-y-2">
              <legend className="agentdash-form-label">capabilities</legend>
              <div className="flex flex-wrap gap-2">
                {ALL_CAPABILITIES.map((cap) => {
                  const checked = form.capabilities.includes(cap);
                  return (
                    <label
                      key={cap}
                      className={`flex cursor-pointer items-center gap-1.5 rounded-full border px-2.5 py-1 text-xs ${
                        checked ? "border-primary bg-primary/10" : "border-border"
                      }`}
                    >
                      <input
                        type="checkbox"
                        checked={checked}
                        onChange={(e) => {
                          const next = e.target.checked
                            ? [...form.capabilities, cap]
                            : form.capabilities.filter((c) => c !== cap);
                          onFormChange({
                            ...form,
                            capabilities: next,
                          });
                        }}
                      />
                      {cap}
                    </label>
                  );
                })}
              </div>
            </fieldset>

            <div className="flex justify-end">
              <button
                type="button"
                onClick={isCreate ? onCreate : onSave}
                disabled={isSaving}
                className="agentdash-button-primary"
              >
                {isSaving ? "保存中…" : isCreate ? "创建" : "保存"}
              </button>
            </div>
          </section>

          {showVfsBrowser && editingMountId && (
            <section className="min-h-0">
              <VfsBrowser
                source={{
                  source_type: "project_vfs_mount",
                  project_id: projectId,
                  mount_id: editingMountId,
                }}
                browserHeightClassName="min-h-0 flex-1"
                className="flex h-full flex-col"
              />
            </section>
          )}
        </div>
      </div>
    </div>
  );
}

