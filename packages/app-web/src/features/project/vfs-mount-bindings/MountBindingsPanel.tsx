import { useCallback, useEffect, useMemo, useState } from "react";

import { useProjectStore } from "../../../stores/projectStore";
import {
  createProjectVfsMountBinding,
  deleteProjectVfsMountBinding,
  listProjectFilespaces,
  listProjectVfsMountBindings,
  updateProjectVfsMountBinding,
} from "../../../services/projectFilespaces";
import type {
  ProjectFilespace,
  ProjectVfsMountBinding,
  ProjectVfsMountSource,
} from "../../../types";

type Capability = "read" | "list" | "search" | "write";

const VFS_CAPS: Array<{ key: Capability; label: string }> = [
  { key: "read", label: "Read" },
  { key: "list", label: "List" },
  { key: "search", label: "Search" },
  { key: "write", label: "Write" },
];

interface BindingDraft {
  mount_id: string;
  display_name: string;
  capabilities: Capability[];
  default_write: boolean;
  source_kind: "filespace" | "external_service";
  filespace_id: string;
  service_id: string;
  root_ref: string;
}

const EMPTY_DRAFT: BindingDraft = {
  mount_id: "",
  display_name: "",
  capabilities: ["read", "list", "search"],
  default_write: false,
  source_kind: "filespace",
  filespace_id: "",
  service_id: "",
  root_ref: "",
};

export function MountBindingsPanel({ projectId }: { projectId: string }) {
  const bindingsRevision = useProjectStore((s) => s.vfsMountBindingsRevision[projectId] ?? 0);
  const bumpBindingsRevision = useProjectStore((s) => s.bumpVfsMountBindingsRevision);

  const [bindings, setBindings] = useState<ProjectVfsMountBinding[]>([]);
  const [filespaces, setFilespaces] = useState<ProjectFilespace[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [confirmDelete, setConfirmDelete] = useState<ProjectVfsMountBinding | null>(null);
  const [isCreateOpen, setIsCreateOpen] = useState(false);

  const filespaceById = useMemo(() => {
    const map = new Map<string, ProjectFilespace>();
    for (const fs of filespaces) map.set(fs.id, fs);
    return map;
  }, [filespaces]);

  const load = useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      const [list, fs] = await Promise.all([
        listProjectVfsMountBindings(projectId),
        listProjectFilespaces(projectId),
      ]);
      setBindings(list);
      setFilespaces(fs);
    } catch (err) {
      setError(err instanceof Error ? err.message : "加载 Mount Binding 失败");
    } finally {
      setIsLoading(false);
    }
  }, [projectId]);

  useEffect(() => {
    void load();
  }, [load, bindingsRevision]);

  const handleUpdate = useCallback(
    async (binding: ProjectVfsMountBinding, patch: Partial<ProjectVfsMountBinding>) => {
      setBusyId(binding.id);
      try {
        const next = { ...binding, ...patch };
        const saved = await updateProjectVfsMountBinding(projectId, binding.id, {
          mount_id: next.mount_id,
          display_name: next.display_name,
          source: next.source,
          capabilities: next.capabilities,
          default_write: next.default_write,
        });
        setBindings((prev) => prev.map((b) => (b.id === saved.id ? saved : b)));
        bumpBindingsRevision(projectId);
      } catch (err) {
        setError(err instanceof Error ? err.message : "保存 Mount Binding 失败");
      } finally {
        setBusyId(null);
      }
    },
    [projectId, bumpBindingsRevision],
  );

  const handleDelete = useCallback(async () => {
    if (!confirmDelete) return;
    setBusyId(confirmDelete.id);
    try {
      await deleteProjectVfsMountBinding(projectId, confirmDelete.id);
      setBindings((prev) => prev.filter((b) => b.id !== confirmDelete.id));
      bumpBindingsRevision(projectId);
      setConfirmDelete(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : "解绑失败");
    } finally {
      setBusyId(null);
    }
  }, [confirmDelete, projectId, bumpBindingsRevision]);

  const handleCreate = useCallback(
    async (draft: BindingDraft) => {
      let source: ProjectVfsMountSource;
      if (draft.source_kind === "filespace") {
        if (!draft.filespace_id) throw new Error("请选择 Filespace");
        source = { kind: "filespace", filespace_id: draft.filespace_id };
      } else {
        if (!draft.service_id.trim()) throw new Error("service_id 不能为空");
        if (!draft.root_ref.trim()) throw new Error("root_ref 不能为空");
        source = {
          kind: "external_service",
          service_id: draft.service_id.trim(),
          root_ref: draft.root_ref.trim(),
        };
      }
      const saved = await createProjectVfsMountBinding(projectId, {
        mount_id: draft.mount_id.trim(),
        display_name: draft.display_name.trim() || draft.mount_id.trim(),
        source,
        capabilities: draft.capabilities,
        default_write: draft.default_write,
      });
      setBindings((prev) => [...prev, saved]);
      bumpBindingsRevision(projectId);
    },
    [projectId, bumpBindingsRevision],
  );

  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between gap-3">
        <p className="text-xs text-muted-foreground">
          Project VFS Mount Binding：把 Filespace 或外部服务挂到具体 mount id；Agent VFS 能力分配的来源即此列表。
        </p>
        <button
          type="button"
          onClick={() => setIsCreateOpen(true)}
          className="agentdash-button-secondary"
        >
          新建 Mount
        </button>
      </div>

      {error && (
        <div className="rounded-[8px] border border-destructive/20 bg-destructive/5 px-3 py-2 text-xs text-destructive">
          {error}
        </div>
      )}

      {isLoading ? (
        <p className="rounded-[8px] border border-dashed border-border px-4 py-4 text-center text-xs text-muted-foreground">
          正在加载 Mount Binding…
        </p>
      ) : bindings.length === 0 ? (
        <p className="rounded-[8px] border border-dashed border-border px-4 py-4 text-center text-sm text-muted-foreground">
          当前 Project 还没有 VFS Mount Binding。新建 Filespace 时会自动创建一个，也可在此手动追加。
        </p>
      ) : (
        <ul className="space-y-2">
          {bindings.map((binding) => (
            <BindingRow
              key={binding.id}
              binding={binding}
              filespace={
                binding.source.kind === "filespace"
                  ? filespaceById.get(binding.source.filespace_id) ?? null
                  : null
              }
              busy={busyId === binding.id}
              onPatch={(patch) => void handleUpdate(binding, patch)}
              onRequestDelete={() => setConfirmDelete(binding)}
            />
          ))}
        </ul>
      )}

      {confirmDelete && (
        <ConfirmUnbindDialog
          binding={confirmDelete}
          busy={busyId === confirmDelete.id}
          onCancel={() => setConfirmDelete(null)}
          onConfirm={() => void handleDelete()}
        />
      )}

      {isCreateOpen && (
        <CreateBindingDialog
          filespaces={filespaces}
          existingMountIds={bindings.map((b) => b.mount_id)}
          onClose={() => setIsCreateOpen(false)}
          onSubmit={async (draft) => {
            await handleCreate(draft);
            setIsCreateOpen(false);
          }}
        />
      )}
    </div>
  );
}

function BindingRow({
  binding,
  filespace,
  busy,
  onPatch,
  onRequestDelete,
}: {
  binding: ProjectVfsMountBinding;
  filespace: ProjectFilespace | null;
  busy: boolean;
  onPatch: (patch: Partial<ProjectVfsMountBinding>) => void;
  onRequestDelete: () => void;
}) {
  const sourceLabel =
    binding.source.kind === "filespace"
      ? `Filespace · ${filespace?.display_name ?? filespace?.key ?? binding.source.filespace_id}`
      : `External · ${binding.source.service_id} ${binding.source.root_ref}`;

  const toggleCap = (cap: Capability) => {
    const exists = binding.capabilities.includes(cap);
    const next = exists
      ? binding.capabilities.filter((c) => c !== cap)
      : [...binding.capabilities, cap];
    const default_write = next.includes("write") ? binding.default_write : false;
    onPatch({ capabilities: next, default_write });
  };

  return (
    <li className="rounded-[8px] border border-border bg-background p-3">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <span className="truncate text-sm font-medium text-foreground">
              {binding.display_name}
            </span>
            <span className="rounded-[6px] border border-border bg-secondary/40 px-1.5 py-0.5 font-mono text-[11px] text-muted-foreground">
              {binding.mount_id}
            </span>
          </div>
          <p className="mt-1 truncate text-xs text-muted-foreground">{sourceLabel}</p>
        </div>
        <button
          type="button"
          onClick={onRequestDelete}
          disabled={busy}
          className="rounded-[8px] border border-destructive/25 bg-destructive/5 px-3 py-1 text-xs text-destructive hover:bg-destructive/10 disabled:cursor-not-allowed disabled:opacity-50"
        >
          解绑
        </button>
      </div>

      <div className="mt-3 flex flex-wrap gap-1.5">
        {VFS_CAPS.map((cap) => {
          const active = binding.capabilities.includes(cap.key);
          return (
            <button
              key={cap.key}
              type="button"
              disabled={busy}
              onClick={() => toggleCap(cap.key)}
              className={`rounded-[6px] border px-2 py-0.5 text-[11px] transition-colors ${
                active
                  ? "border-primary/30 bg-primary/10 text-foreground"
                  : "border-border bg-background text-muted-foreground hover:text-foreground"
              }`}
            >
              {cap.label}
            </button>
          );
        })}
        <label className="ml-auto flex items-center gap-1.5 text-[11px] text-muted-foreground">
          <input
            type="checkbox"
            checked={binding.default_write}
            disabled={busy || !binding.capabilities.includes("write")}
            onChange={(e) => onPatch({ default_write: e.currentTarget.checked })}
          />
          default write
        </label>
      </div>
    </li>
  );
}

function ConfirmUnbindDialog({
  binding,
  busy,
  onCancel,
  onConfirm,
}: {
  binding: ProjectVfsMountBinding;
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
        <h3 className="text-sm font-semibold text-foreground">确认解绑</h3>
        <p className="mt-2 text-xs leading-5 text-muted-foreground">
          解绑 mount <span className="font-mono text-foreground">{binding.mount_id}</span>？
          {binding.source.kind === "filespace"
            ? "底层 Filespace 资产保留，不会被删除。"
            : "底层 external service 配置不受影响。"}
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
            {busy ? "解绑中…" : "解绑"}
          </button>
        </div>
      </div>
    </div>
  );
}

function CreateBindingDialog({
  filespaces,
  existingMountIds,
  onClose,
  onSubmit,
}: {
  filespaces: ProjectFilespace[];
  existingMountIds: string[];
  onClose: () => void;
  onSubmit: (draft: BindingDraft) => Promise<void>;
}) {
  const [draft, setDraft] = useState<BindingDraft>({
    ...EMPTY_DRAFT,
    filespace_id: filespaces[0]?.id ?? "",
  });
  const [error, setError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);

  const updateField = <K extends keyof BindingDraft>(key: K, value: BindingDraft[K]) => {
    setDraft((prev) => ({ ...prev, [key]: value }));
  };

  const toggleCap = (cap: Capability) => {
    const exists = draft.capabilities.includes(cap);
    const nextCaps = exists
      ? draft.capabilities.filter((c) => c !== cap)
      : [...draft.capabilities, cap];
    setDraft((prev) => ({
      ...prev,
      capabilities: nextCaps,
      default_write: nextCaps.includes("write") ? prev.default_write : false,
    }));
  };

  const submit = async () => {
    setError(null);
    const mountId = draft.mount_id.trim();
    if (!mountId) return setError("mount_id 不能为空");
    if (existingMountIds.includes(mountId)) return setError(`mount_id "${mountId}" 已存在`);
    setSubmitting(true);
    try {
      await onSubmit(draft);
    } catch (err) {
      setError(err instanceof Error ? err.message : "创建 Mount Binding 失败");
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-3" onClick={onClose}>
      <div
        className="w-[min(94vw,520px)] rounded-[8px] border border-border bg-background p-5 shadow-xl"
        onClick={(e) => e.stopPropagation()}
      >
        <header className="flex items-center justify-between">
          <h3 className="text-sm font-semibold text-foreground">新建 Project VFS Mount</h3>
          <button type="button" onClick={onClose} className="agentdash-button-secondary">
            关闭
          </button>
        </header>

        <div className="mt-4 space-y-3">
          <div className="flex gap-1.5">
            {(["filespace", "external_service"] as const).map((kind) => {
              const active = draft.source_kind === kind;
              return (
                <button
                  key={kind}
                  type="button"
                  onClick={() => updateField("source_kind", kind)}
                  className={`flex-1 rounded-[8px] border px-3 py-2 text-xs font-medium transition-colors ${
                    active
                      ? "border-primary/30 bg-primary/10 text-foreground"
                      : "border-border bg-background text-muted-foreground hover:text-foreground"
                  }`}
                >
                  {kind === "filespace" ? "Filespace 资产" : "外部服务"}
                </button>
              );
            })}
          </div>

          {draft.source_kind === "filespace" ? (
            <label className="block space-y-1.5">
              <span className="agentdash-form-label">Filespace</span>
              <select
                value={draft.filespace_id}
                onChange={(e) => updateField("filespace_id", e.target.value)}
                className="agentdash-form-select"
              >
                <option value="">选择 Filespace</option>
                {filespaces.map((fs) => (
                  <option key={fs.id} value={fs.id}>
                    {fs.display_name} ({fs.key})
                  </option>
                ))}
              </select>
            </label>
          ) : (
            <div className="grid gap-3 md:grid-cols-2">
              <label className="block space-y-1.5">
                <span className="agentdash-form-label">service_id</span>
                <input
                  value={draft.service_id}
                  onChange={(e) => updateField("service_id", e.target.value)}
                  className="agentdash-form-input"
                  placeholder="external_service id"
                />
              </label>
              <label className="block space-y-1.5">
                <span className="agentdash-form-label">root_ref</span>
                <input
                  value={draft.root_ref}
                  onChange={(e) => updateField("root_ref", e.target.value)}
                  className="agentdash-form-input"
                  placeholder="external root reference"
                />
              </label>
            </div>
          )}

          <div className="grid gap-3 md:grid-cols-2">
            <label className="block space-y-1.5">
              <span className="agentdash-form-label">mount_id</span>
              <input
                value={draft.mount_id}
                onChange={(e) => updateField("mount_id", e.target.value)}
                className="agentdash-form-input"
                placeholder="例如 docs"
              />
            </label>
            <label className="block space-y-1.5">
              <span className="agentdash-form-label">显示名称</span>
              <input
                value={draft.display_name}
                onChange={(e) => updateField("display_name", e.target.value)}
                className="agentdash-form-input"
                placeholder="留空则使用 mount_id"
              />
            </label>
          </div>

          <div>
            <p className="agentdash-form-label">capabilities</p>
            <div className="mt-1.5 flex flex-wrap items-center gap-1.5">
              {VFS_CAPS.map((cap) => {
                const active = draft.capabilities.includes(cap.key);
                return (
                  <button
                    key={cap.key}
                    type="button"
                    onClick={() => toggleCap(cap.key)}
                    className={`rounded-[6px] border px-2 py-0.5 text-[11px] transition-colors ${
                      active
                        ? "border-primary/30 bg-primary/10 text-foreground"
                        : "border-border bg-background text-muted-foreground"
                    }`}
                  >
                    {cap.label}
                  </button>
                );
              })}
              <label className="ml-auto flex items-center gap-1.5 text-[11px] text-muted-foreground">
                <input
                  type="checkbox"
                  checked={draft.default_write}
                  disabled={!draft.capabilities.includes("write")}
                  onChange={(e) => updateField("default_write", e.currentTarget.checked)}
                />
                default write
              </label>
            </div>
          </div>

          {error && (
            <p className="rounded-[6px] border border-destructive/20 bg-destructive/5 px-2 py-1.5 text-xs text-destructive">
              {error}
            </p>
          )}
        </div>

        <footer className="mt-5 flex justify-end gap-2">
          <button type="button" onClick={onClose} className="agentdash-button-secondary">
            取消
          </button>
          <button
            type="button"
            onClick={() => void submit()}
            disabled={submitting}
            className="agentdash-button-primary"
          >
            {submitting ? "创建中…" : "创建"}
          </button>
        </footer>
      </div>
    </div>
  );
}

export default MountBindingsPanel;
