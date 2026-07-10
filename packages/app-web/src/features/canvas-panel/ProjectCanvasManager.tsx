import { useCallback, useEffect, useMemo, useState } from "react";

import type { CanvasDefinitionDto } from "../../generated/interaction-contracts";
import {
  archiveCanvas,
  copyCanvasToPersonal,
  createCanvas,
  createDefaultCanvasSourceBundle,
  fetchProjectCanvases,
  publishCanvasToProject,
  promoteCanvasToExtension,
  unpublishCanvas,
} from "../../services/canvas";
import { CanvasRuntimePanel } from "./CanvasRuntimePanel";

export interface ProjectCanvasManagerProps {
  projectId: string;
  projectName: string;
  onExtensionRuntimeRefresh?: (projectId: string) => Promise<void>;
}

type View = "mine" | "shared";

export function ProjectCanvasManager({
  projectId,
  projectName,
  onExtensionRuntimeRefresh,
}: ProjectCanvasManagerProps) {
  const [definitions, setDefinitions] = useState<CanvasDefinitionDto[]>([]);
  const [view, setView] = useState<View>("mine");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [title, setTitle] = useState("");
  const [description, setDescription] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async (preferredId?: string) => {
    setError(null);
    try {
      const next = await fetchProjectCanvases(projectId);
      setDefinitions(next);
      setSelectedId((current) => {
        const candidate = preferredId ?? current;
        return candidate && next.some((item) => item.definition_id === candidate)
          ? candidate
          : next[0]?.definition_id ?? null;
      });
    } catch (loadError) {
      setError(loadError instanceof Error ? loadError.message : "Canvas 列表加载失败");
    }
  }, [projectId]);

  useEffect(() => {
    queueMicrotask(() => {
      void load();
    });
  }, [load]);

  const visible = useMemo(() => definitions.filter((definition) => (
    view === "mine" ? definition.owner.kind === "user" : definition.owner.kind === "project"
  )), [definitions, view]);
  const selected = definitions.find((item) => item.definition_id === selectedId) ?? visible[0] ?? null;

  const create = useCallback(async () => {
    if (!title.trim()) return;
    setBusy(true);
    setError(null);
    try {
      const source_bundle = await createDefaultCanvasSourceBundle();
      const created = await createCanvas(projectId, {
        title: title.trim(),
        description: description.trim(),
        source_bundle,
        initial_state: {},
        state_schema: { type: "object" },
        command_definitions: [],
        component_bindings: [],
        resource_slots: [],
      });
      setTitle("");
      setDescription("");
      setView("mine");
      await load(created.definition_id);
    } catch (createError) {
      setError(createError instanceof Error ? createError.message : "Canvas 创建失败");
    } finally {
      setBusy(false);
    }
  }, [description, load, projectId, title]);

  const runAction = useCallback(async (action: "publish" | "copy" | "archive" | "unpublish" | "promote") => {
    if (!selected) return;
    setBusy(true);
    setError(null);
    try {
      if (action === "publish") {
        await publishCanvasToProject(selected.definition_id, { source_revision_id: selected.current_revision_id });
      } else if (action === "copy") {
        const copy = await copyCanvasToPersonal(selected.definition_id, { source_revision_id: selected.current_revision_id });
        setView("mine");
        await load(copy.definition_id);
        return;
      } else if (action === "unpublish") {
        await unpublishCanvas(selected.definition_id);
      } else if (action === "promote") {
        await promoteCanvasToExtension(selected.definition_id, {
          source_revision_id: selected.current_revision_id,
          package_version: null,
          asset_version: null,
          extension_key: null,
          display_name: selected.title,
          overwrite: true,
        });
        await onExtensionRuntimeRefresh?.(projectId);
      } else {
        await archiveCanvas(selected.definition_id);
      }
      await load();
    } catch (actionError) {
      setError(actionError instanceof Error ? actionError.message : "Canvas 操作失败");
    } finally {
      setBusy(false);
    }
  }, [load, onExtensionRuntimeRefresh, projectId, selected]);

  return (
    <div className="grid gap-4 xl:grid-cols-[300px_minmax(0,1fr)]">
      <aside className="space-y-3 rounded-[12px] border border-border bg-background p-4">
        <div>
          <h3 className="font-semibold text-foreground">{projectName} · Canvas</h3>
          <p className="text-xs text-muted-foreground">Immutable definition revisions 与独立 Interaction runtime。</p>
        </div>
        <div className="grid grid-cols-2 gap-1">
          <button className="agentdash-button-secondary" onClick={() => setView("mine")}>我的</button>
          <button className="agentdash-button-secondary" onClick={() => setView("shared")}>项目共用</button>
        </div>
        {view === "mine" && (
          <div className="space-y-2 rounded-[8px] border border-border p-3">
            <input className="agentdash-form-input" value={title} onChange={(event) => setTitle(event.target.value)} placeholder="Canvas 标题" />
            <textarea className="agentdash-form-input min-h-20" value={description} onChange={(event) => setDescription(event.target.value)} placeholder="用途说明" />
            <button className="agentdash-button-primary w-full" disabled={busy} onClick={() => void create()}>创建 Canvas</button>
          </div>
        )}
        {error && <div className="text-xs text-destructive">{error}</div>}
        <div className="space-y-2">
          {visible.map((definition) => (
            <button
              key={definition.definition_id}
              className={`w-full rounded-[8px] border p-3 text-left ${selected?.definition_id === definition.definition_id ? "border-foreground/30" : "border-border"}`}
              onClick={() => setSelectedId(definition.definition_id)}
            >
              <p className="text-sm font-medium">{definition.title}</p>
              <p className="text-xs text-muted-foreground">revision {definition.revision_number}</p>
            </button>
          ))}
        </div>
        {selected && (
          <div className="grid grid-cols-2 gap-2">
            {selected.access.can_publish && <button className="agentdash-button-secondary" disabled={busy} onClick={() => void runAction("publish")}>发布</button>}
            {selected.access.can_copy && selected.owner.kind === "project" && <button className="agentdash-button-secondary" disabled={busy} onClick={() => void runAction("copy")}>复制</button>}
            {selected.access.can_manage_shared && <button className="agentdash-button-secondary" disabled={busy} onClick={() => void runAction("unpublish")}>取消发布</button>}
            {(selected.access.can_edit_source || selected.access.can_manage_shared) && <button className="agentdash-button-secondary" disabled={busy} onClick={() => void runAction("archive")}>归档</button>}
            {(selected.access.can_edit_source || selected.access.can_manage_shared) && <button className="agentdash-button-secondary" disabled={busy} onClick={() => void runAction("promote")}>生成扩展</button>}
          </div>
        )}
      </aside>
      <main className="min-h-[560px] rounded-[12px] border border-border bg-background">
        {selected
          ? <CanvasRuntimePanel projectId={projectId} definitionId={selected.definition_id} />
          : <div className="p-6 text-sm text-muted-foreground">选择或创建一个 Canvas。</div>}
      </main>
    </div>
  );
}

export default ProjectCanvasManager;
