import { useCallback, useEffect, useMemo, useState } from "react";

import { CardMenu, CreateButton, OriginBadge as UiOriginBadge } from "@agentdash/ui";

import { useProjectStore } from "../../../stores/projectStore";
import { useCurrentUserStore } from "../../../stores/currentUserStore";
import { fetchLibraryAssets } from "../../../services/sharedLibrary";
import {
  createProjectFilespace,
  deleteProjectFilespace,
  listProjectFilespaces,
  updateProjectFilespace,
} from "../../../services/projectFilespaces";
import type { LibraryAssetDto, ProjectFilespace } from "../../../types";
import { VfsBrowser } from "../../vfs";
import { Notice, type NoticeData } from "../_shared/Notice";
import { PublishedBadge } from "../_shared/PublishedBadge";
import { resolveOriginBadge } from "../_shared/origin-badge-tone";
import { PublishLibraryAssetDialog } from "../publish/PublishLibraryAssetDialog";

type DetailMode =
  | { kind: "closed" }
  | { kind: "create" }
  | { kind: "edit"; filespaceId: string };

interface FilespaceFormState {
  key: string;
  display_name: string;
  description: string;
}

const EMPTY_FORM: FilespaceFormState = { key: "", display_name: "", description: "" };

function formFromFilespace(item: ProjectFilespace): FilespaceFormState {
  return {
    key: item.key,
    display_name: item.display_name,
    description: item.description ?? "",
  };
}

export function FilespaceCategoryPanel() {
  const currentProjectId = useProjectStore((s) => s.currentProjectId);
  const projects = useProjectStore((s) => s.projects);
  const bumpBindingsRevision = useProjectStore((s) => s.bumpVfsMountBindingsRevision);
  const currentProject = useMemo(
    () => projects.find((p) => p.id === currentProjectId) ?? null,
    [currentProjectId, projects],
  );
  const currentUserId = useCurrentUserStore((s) => s.currentUser?.user_id ?? null);

  const [items, setItems] = useState<ProjectFilespace[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [detail, setDetail] = useState<DetailMode>({ kind: "closed" });
  const [form, setForm] = useState<FilespaceFormState>(EMPTY_FORM);
  const [confirmDelete, setConfirmDelete] = useState<ProjectFilespace | null>(null);
  const [publishTarget, setPublishTarget] = useState<ProjectFilespace | null>(null);
  const [publishedAssets, setPublishedAssets] = useState<LibraryAssetDto[]>([]);
  const [publishedReloadTick, setPublishedReloadTick] = useState(0);
  const [notice, setNotice] = useState<NoticeData | null>(null);

  const showSuccess = useCallback((message: string) => setNotice({ tone: "success", message }), []);
  const showError = useCallback((message: string) => setNotice({ tone: "danger", message }), []);
  const clearNotice = useCallback(() => setNotice(null), []);

  const load = useCallback(async () => {
    if (!currentProjectId) return;
    setIsLoading(true);
    clearNotice();
    try {
      setItems(await listProjectFilespaces(currentProjectId));
    } catch (err) {
      showError(err instanceof Error ? err.message : "加载 Filespace 资产失败");
    } finally {
      setIsLoading(false);
    }
  }, [currentProjectId, clearNotice, showError]);

  useEffect(() => {
    void load();
  }, [load]);

  useEffect(() => {
    if (!currentUserId) {
      setPublishedAssets([]);
      return;
    }
    let cancelled = false;
    fetchLibraryAssets({ asset_type: "filespace_template", owner_id: currentUserId })
      .then((list) => {
        if (!cancelled) setPublishedAssets(list);
      })
      .catch(() => {
        if (!cancelled) setPublishedAssets([]);
      });
    return () => {
      cancelled = true;
    };
  }, [currentUserId, publishedReloadTick]);

  const publishedByKey = useMemo(() => {
    if (!currentUserId) return new Map<string, LibraryAssetDto>();
    const map = new Map<string, LibraryAssetDto>();
    for (const a of publishedAssets) {
      if (a.source === "user_authored") map.set(a.key, a);
    }
    return map;
  }, [publishedAssets, currentUserId]);

  const reloadPublished = useCallback(() => {
    setPublishedReloadTick((tick) => tick + 1);
  }, []);

  const openCreate = useCallback(() => {
    setForm(EMPTY_FORM);
    clearNotice();
    setDetail({ kind: "create" });
  }, [clearNotice]);

  const openEdit = useCallback((item: ProjectFilespace) => {
    setForm(formFromFilespace(item));
    clearNotice();
    setDetail({ kind: "edit", filespaceId: item.id });
  }, [clearNotice]);

  const closeDetail = useCallback(() => {
    setDetail({ kind: "closed" });
    setForm(EMPTY_FORM);
  }, []);

  const handleCreate = useCallback(async () => {
    if (!currentProjectId || detail.kind !== "create") return;
    const key = form.key.trim();
    const displayName = form.display_name.trim() || key;
    if (!key) {
      showError("key 不能为空");
      return;
    }
    if (items.some((existing) => existing.key === key)) {
      showError(`key "${key}" 已存在`);
      return;
    }
    setIsSaving(true);
    try {
      const created = await createProjectFilespace(currentProjectId, {
        key,
        display_name: displayName,
        description: form.description.trim() || null,
      });
      setItems((prev) => [...prev, created]);
      bumpBindingsRevision(currentProjectId);
      showSuccess(`已创建 Filespace：${created.key}`);
      setForm(formFromFilespace(created));
      setDetail({ kind: "edit", filespaceId: created.id });
    } catch (err) {
      showError(err instanceof Error ? err.message : "创建 Filespace 失败");
    } finally {
      setIsSaving(false);
    }
  }, [currentProjectId, detail, form, items, bumpBindingsRevision, showError, showSuccess]);

  const handleSaveMeta = useCallback(async () => {
    if (!currentProjectId || detail.kind !== "edit") return;
    const key = form.key.trim();
    const displayName = form.display_name.trim() || key;
    if (!key) {
      showError("key 不能为空");
      return;
    }
    if (items.some((existing) => existing.key === key && existing.id !== detail.filespaceId)) {
      showError(`key "${key}" 已存在`);
      return;
    }
    setIsSaving(true);
    try {
      const updated = await updateProjectFilespace(currentProjectId, detail.filespaceId, {
        key,
        display_name: displayName,
        description: form.description.trim() || null,
      });
      setItems((prev) => prev.map((item) => (item.id === updated.id ? updated : item)));
      showSuccess(`已保存 Filespace：${updated.key}`);
    } catch (err) {
      showError(err instanceof Error ? err.message : "保存 Filespace 失败");
    } finally {
      setIsSaving(false);
    }
  }, [currentProjectId, detail, form, items, showError, showSuccess]);

  const handleDelete = useCallback(async () => {
    if (!currentProjectId || !confirmDelete) return;
    setBusyId(confirmDelete.id);
    try {
      await deleteProjectFilespace(currentProjectId, confirmDelete.id);
      setItems((prev) => prev.filter((item) => item.id !== confirmDelete.id));
      bumpBindingsRevision(currentProjectId);
      showSuccess(`已删除 Filespace：${confirmDelete.key}`);
      if (detail.kind === "edit" && detail.filespaceId === confirmDelete.id) {
        closeDetail();
      }
      setConfirmDelete(null);
    } catch (err) {
      showError(err instanceof Error ? err.message : "删除 Filespace 失败");
    } finally {
      setBusyId(null);
    }
  }, [confirmDelete, currentProjectId, detail, bumpBindingsRevision, closeDetail, showError, showSuccess]);

  if (!currentProjectId || !currentProject) {
    return (
      <div className="flex h-full items-center justify-center p-6">
        <div className="text-center text-sm text-muted-foreground">请选择项目后查看 Filespace 资产</div>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col gap-4 p-6">
      <header className="flex flex-wrap items-center justify-between gap-3">
        <div className="space-y-1">
          <h2 className="text-base font-semibold tracking-tight text-foreground">Filespace 资产</h2>
          <p className="text-xs text-muted-foreground">
            {items.length > 0
              ? `${items.length} 个 Filespace · 可挂载到 Project VFS / 发布到资源市场`
              : "0 个 Filespace · 可挂载到 Project VFS / 发布到资源市场"}
          </p>
        </div>
        <CreateButton entity="Filespace" onClick={openCreate} />
      </header>

      <Notice notice={notice} onDismiss={clearNotice} />

      {isLoading ? (
        <div className="rounded-[8px] border border-dashed border-border px-6 py-10 text-center text-sm text-muted-foreground">
          正在加载 Filespace 资产…
        </div>
      ) : (
        <FilespaceGrid
          items={items}
          publishedByKey={publishedByKey}
          busyId={busyId}
          onEdit={openEdit}
          onPublish={setPublishTarget}
          onDelete={setConfirmDelete}
        />
      )}

      {detail.kind !== "closed" && (
        <FilespaceEditorDialog
          mode={detail}
          projectId={currentProjectId}
          form={form}
          onFormChange={setForm}
          isSaving={isSaving}
          onCreate={() => void handleCreate()}
          onSaveMeta={() => void handleSaveMeta()}
          onClose={closeDetail}
        />
      )}

      {confirmDelete && (
        <ConfirmDeleteDialog
          filespace={confirmDelete}
          busy={busyId === confirmDelete.id}
          onCancel={() => setConfirmDelete(null)}
          onConfirm={() => void handleDelete()}
        />
      )}

      {publishTarget && (
        <PublishLibraryAssetDialog
          projectId={currentProjectId}
          assetKind="filespace"
          projectAssetId={publishTarget.id}
          defaults={{
            key: publishTarget.key,
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

export default FilespaceCategoryPanel;

/* ─── Origin Badge ─── */

function FilespaceOriginBadge({ filespace }: { filespace: ProjectFilespace }) {
  const installed = Boolean(filespace.installed_source);
  const { label, tone } = resolveOriginBadge("user_authored", installed);
  return <UiOriginBadge label={label} tone={tone} url={null} />;
}

/* ─── Grid ─── */

function FilespaceGrid({
  items,
  publishedByKey,
  busyId,
  onEdit,
  onPublish,
  onDelete,
}: {
  items: ProjectFilespace[];
  publishedByKey: Map<string, LibraryAssetDto>;
  busyId: string | null;
  onEdit: (item: ProjectFilespace) => void;
  onPublish: (item: ProjectFilespace) => void;
  onDelete: (item: ProjectFilespace) => void;
}) {
  if (items.length === 0) {
    return (
      <div className="rounded-[8px] border border-dashed border-border bg-secondary/20 px-6 py-14 text-center">
        <p className="text-sm text-foreground">暂无 Filespace 资产</p>
        <p className="mt-1.5 text-xs text-muted-foreground">
          点击右上角「+ Filespace」创建一个 Project 级可复用文件空间
        </p>
      </div>
    );
  }

  return (
    <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
      {items.map((item) => {
        const isInstalled = Boolean(item.installed_source);
        const canPublish = !isInstalled;
        const published = publishedByKey.get(item.key) ?? null;
        const isBusy = busyId === item.id;
        const menuItems = [
          { key: "edit", label: "编辑", onSelect: () => onEdit(item) },
          ...(canPublish
            ? [
                {
                  key: "publish",
                  label: published ? "更新发布" : "发布到资源市场",
                  onSelect: () => onPublish(item),
                },
              ]
            : []),
          { key: "---", label: "", onSelect: () => {} },
          {
            key: "delete",
            label: isBusy ? "处理中…" : "删除",
            danger: true,
            onSelect: () => onDelete(item),
          },
        ];

        return (
          <article
            key={item.id}
            role="button"
            tabIndex={0}
            onClick={() => onEdit(item)}
            onKeyDown={(e) => {
              if (e.key === "Enter" || e.key === " ") {
                e.preventDefault();
                onEdit(item);
              }
            }}
            title="编辑"
            className="flex cursor-pointer flex-col rounded-[8px] border border-border bg-background p-3.5 text-left transition-colors hover:border-primary/25 hover:bg-secondary/30 focus:outline-none focus-visible:ring-2 focus-visible:ring-primary/40"
          >
            <header className="flex items-start justify-between gap-2">
              <div className="min-w-0">
                <p className="truncate text-sm font-medium leading-6 text-foreground">
                  {item.display_name}
                </p>
                <p className="mt-0.5 truncate font-mono text-xs text-muted-foreground">
                  {item.key}
                </p>
              </div>
              <div className="flex shrink-0 items-center gap-1">
                {published && <PublishedBadge version={published.version} />}
                <FilespaceOriginBadge filespace={item} />
                <CardMenu items={menuItems} />
              </div>
            </header>

            {item.description && (
              <p className="mt-1.5 line-clamp-2 text-xs leading-5 text-muted-foreground">
                {item.description}
              </p>
            )}
          </article>
        );
      })}
    </div>
  );
}

/* ─── Editor Dialog ─── */

function FilespaceEditorDialog({
  mode,
  projectId,
  form,
  onFormChange,
  isSaving,
  onCreate,
  onSaveMeta,
  onClose,
}: {
  mode: DetailMode;
  projectId: string;
  form: FilespaceFormState;
  onFormChange: (form: FilespaceFormState) => void;
  isSaving: boolean;
  onCreate: () => void;
  onSaveMeta: () => void;
  onClose: () => void;
}) {
  if (mode.kind === "closed") return null;
  const isCreate = mode.kind === "create";
  const filespaceId = mode.kind === "edit" ? mode.filespaceId : null;
  const updateField = <K extends keyof FilespaceFormState>(key: K, value: FilespaceFormState[K]) => {
    onFormChange({ ...form, [key]: value });
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-3 sm:p-6"
      onClick={onClose}
    >
      <div
        className="flex h-[90vh] w-[min(94vw,1480px)] flex-col overflow-hidden rounded-[8px] border border-border bg-background shadow-xl"
        onClick={(e) => e.stopPropagation()}
      >
        <header className="flex items-center justify-between border-b border-border px-5 py-4">
          <div>
            <h3 className="text-sm font-semibold text-foreground">
              {isCreate ? "新建 Filespace" : "编辑 Filespace"}
            </h3>
            <p className="mt-0.5 text-xs text-muted-foreground">
              {form.key ? `mount: ${form.key}` : "key 决定 Project VFS 默认 mount id"}
            </p>
          </div>
          <button type="button" onClick={onClose} className="agentdash-button-secondary">
            关闭
          </button>
        </header>

        <div className="grid min-h-0 flex-1 grid-cols-1 lg:grid-cols-[320px_minmax(0,1fr)]">
          <section className="space-y-4 overflow-y-auto border-b border-border p-5 lg:border-b-0 lg:border-r">
            <label className="block space-y-1.5">
              <span className="agentdash-form-label">key</span>
              <input
                value={form.key}
                onChange={(e) => updateField("key", e.target.value)}
                placeholder="my-filespace"
                className="agentdash-form-input"
              />
            </label>
            <label className="block space-y-1.5">
              <span className="agentdash-form-label">显示名称</span>
              <input
                value={form.display_name}
                onChange={(e) => updateField("display_name", e.target.value)}
                placeholder="留空则使用 key"
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
            <div className="flex justify-end">
              <button
                type="button"
                onClick={isCreate ? onCreate : onSaveMeta}
                disabled={isSaving}
                className="agentdash-button-primary"
              >
                {isSaving ? "保存中…" : isCreate ? "创建" : "保存"}
              </button>
            </div>
            {!isCreate && (
              <p className="text-[11px] text-muted-foreground/80">
                文件内容直接在右侧 VFS 浏览器内编辑、上传或删除，无需手动保存。
              </p>
            )}
          </section>

          <section className="min-h-0">
            {filespaceId ? (
              <VfsBrowser
                source={{
                  source_type: "project_filespace",
                  project_id: projectId,
                  filespace_id: filespaceId,
                }}
                browserHeightClassName="min-h-0 flex-1"
                className="flex h-full flex-col"
              />
            ) : (
              <div className="flex h-full items-center justify-center px-6 text-center text-xs text-muted-foreground">
                创建后即可在此浏览与编辑 Filespace 内的文件。
              </div>
            )}
          </section>
        </div>
      </div>
    </div>
  );
}

/* ─── Confirm Delete ─── */

function ConfirmDeleteDialog({
  filespace,
  busy,
  onCancel,
  onConfirm,
}: {
  filespace: ProjectFilespace;
  busy: boolean;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40" onClick={onCancel}>
      <div
        className="w-[420px] rounded-[8px] border border-border bg-background p-5 shadow-xl"
        onClick={(e) => e.stopPropagation()}
      >
        <h3 className="text-sm font-semibold text-foreground">确认删除</h3>
        <p className="mt-2 text-xs leading-5 text-muted-foreground">
          确定要删除 Filespace <span className="font-medium text-foreground">{filespace.key}</span> 吗？该 Filespace 的 mount binding 会一并解除。
        </p>
        <div className="mt-4 flex justify-end gap-2">
          <button type="button" onClick={onCancel} className="agentdash-button-secondary">
            取消
          </button>
          <button
            type="button"
            onClick={onConfirm}
            disabled={busy}
            className="agentdash-button-danger"
          >
            {busy ? "删除中…" : "删除"}
          </button>
        </div>
      </div>
    </div>
  );
}
