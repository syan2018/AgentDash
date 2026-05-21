import { useCallback, useEffect, useMemo, useState } from "react";
import { useProjectStore } from "../../../stores/projectStore";
import { useCurrentUserStore } from "../../../stores/currentUserStore";
import type { ProjectFilespace } from "../../../types";
import {
  createProjectFilespace,
  deleteProjectFilespace,
  listProjectFilespaces,
} from "../../../services/projectFilespaces";
import { VfsBrowser } from "../../vfs";
import { Notice, type NoticeData } from "../_shared/Notice";
import { PublishLibraryAssetDialog } from "../publish/PublishLibraryAssetDialog";

export function FilespaceCategoryPanel() {
  const projectId = useProjectStore((s) => s.currentProjectId);
  const currentUserId = useCurrentUserStore((s) => s.currentUser?.user_id ?? null);
  const [items, setItems] = useState<ProjectFilespace[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [isCreating, setIsCreating] = useState(false);
  const [newKey, setNewKey] = useState("");
  const [newName, setNewName] = useState("");
  const [notice, setNotice] = useState<NoticeData | null>(null);
  const [publishTarget, setPublishTarget] = useState<ProjectFilespace | null>(null);

  const selected = useMemo(
    () => items.find((item) => item.id === selectedId) ?? items[0] ?? null,
    [items, selectedId],
  );

  const load = useCallback(async () => {
    if (!projectId) return;
    setIsLoading(true);
    try {
      const next = await listProjectFilespaces(projectId);
      setItems(next);
      setSelectedId((current) => current && next.some((item) => item.id === current) ? current : next[0]?.id ?? null);
    } catch (err) {
      setNotice({ tone: "danger", message: err instanceof Error ? err.message : "加载 Filespace 失败" });
    } finally {
      setIsLoading(false);
    }
  }, [projectId]);

  useEffect(() => {
    void load();
  }, [load]);

  const create = async () => {
    if (!projectId) return;
    const key = newKey.trim();
    const displayName = newName.trim() || key;
    if (!key) {
      setNotice({ tone: "danger", message: "key 不能为空" });
      return;
    }
    setIsCreating(true);
    try {
      const created = await createProjectFilespace(projectId, {
        key,
        display_name: displayName,
      });
      setItems((prev) => [...prev, created]);
      setSelectedId(created.id);
      setNewKey("");
      setNewName("");
      setNotice({ tone: "success", message: "Filespace 已创建" });
    } catch (err) {
      setNotice({ tone: "danger", message: err instanceof Error ? err.message : "创建 Filespace 失败" });
    } finally {
      setIsCreating(false);
    }
  };

  const remove = async (item: ProjectFilespace) => {
    if (!projectId) return;
    if (!window.confirm(`删除 Filespace「${item.display_name}」？`)) return;
    try {
      await deleteProjectFilespace(projectId, item.id);
      setItems((prev) => prev.filter((candidate) => candidate.id !== item.id));
      setSelectedId(null);
      setNotice({ tone: "success", message: "Filespace 已删除" });
    } catch (err) {
      setNotice({ tone: "danger", message: err instanceof Error ? err.message : "删除 Filespace 失败" });
    }
  };

  if (!projectId) {
    return <div className="p-6 text-sm text-muted-foreground">请选择项目</div>;
  }

  return (
    <div className="flex h-full min-h-0 flex-col">
      <header className="flex shrink-0 items-center justify-between border-b border-border px-6 py-4">
        <div>
          <h2 className="text-sm font-semibold text-foreground">Filespace</h2>
          <p className="mt-0.5 text-xs text-muted-foreground">Project 级可复用 VFS 文件空间</p>
        </div>
        <button type="button" onClick={load} className="agentdash-button-secondary" disabled={isLoading}>
          刷新
        </button>
      </header>

      <div className="grid min-h-0 flex-1 grid-cols-[280px_minmax(0,1fr)]">
        <aside className="min-h-0 overflow-y-auto border-r border-border p-4">
          <Notice data={notice} onDismiss={() => setNotice(null)} />
          <div className="rounded-[8px] border border-border bg-card/40 p-3">
            <div className="space-y-2">
              <input
                value={newKey}
                onChange={(event) => setNewKey(event.target.value)}
                placeholder="key"
                className="agentdash-input w-full"
              />
              <input
                value={newName}
                onChange={(event) => setNewName(event.target.value)}
                placeholder="显示名称"
                className="agentdash-input w-full"
              />
              <button
                type="button"
                onClick={create}
                className="agentdash-button-primary w-full"
                disabled={isCreating}
              >
                新建 Filespace
              </button>
            </div>
          </div>

          <div className="mt-4 space-y-2">
            {items.map((item) => (
              <button
                key={item.id}
                type="button"
                onClick={() => setSelectedId(item.id)}
                className={`w-full rounded-[8px] border p-3 text-left transition-colors ${
                  selected?.id === item.id
                    ? "border-primary/30 bg-primary/10"
                    : "border-border bg-card/30 hover:bg-secondary/40"
                }`}
              >
                <div className="truncate text-sm font-medium text-foreground">{item.display_name}</div>
                <div className="truncate text-xs text-muted-foreground">{item.key}</div>
              </button>
            ))}
          </div>
        </aside>

        <section className="min-h-0">
          {selected ? (
            <div className="flex h-full min-h-0 flex-col">
              <div className="flex shrink-0 items-center justify-between border-b border-border px-4 py-3">
                <div className="min-w-0">
                  <div className="truncate text-sm font-medium text-foreground">{selected.display_name}</div>
                  <div className="truncate text-xs text-muted-foreground">{selected.key}</div>
                </div>
                <div className="flex gap-2">
                  <button
                    type="button"
                    onClick={() => setPublishTarget(selected)}
                    className="agentdash-button-secondary"
                  >
                    发布
                  </button>
                  <button
                    type="button"
                    onClick={() => void remove(selected)}
                    className="agentdash-button-secondary"
                  >
                    删除
                  </button>
                </div>
              </div>
              <VfsBrowser
                source={{ source_type: "project_filespace", project_id: projectId, filespace_id: selected.id }}
                browserHeightClassName="min-h-0 flex-1"
                className="flex h-full flex-col"
              />
            </div>
          ) : (
            <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
              {isLoading ? "正在加载 Filespace..." : "尚未创建 Filespace"}
            </div>
          )}
        </section>
      </div>

      {publishTarget && (
        <PublishLibraryAssetDialog
          projectId={projectId}
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
            setPublishTarget(null);
            setNotice({ tone: "success", message });
          }}
        />
      )}
    </div>
  );
}
