import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import {
  copyCanvasToPersonal,
  createCanvas,
  deleteCanvas,
  fetchProjectCanvases,
  promoteCanvasToExtension,
  publishCanvasToProject,
  unpublishCanvas,
} from "../../services/canvas";
import type { Canvas } from "../../types";
import { formatDateTime } from "../../lib/format";
import { CanvasRuntimePanel } from "./CanvasRuntimePanel";

export interface ProjectCanvasManagerProps {
  projectId: string;
  projectName: string;
  onExtensionRuntimeRefresh?: (projectId: string) => Promise<void>;
}

type CanvasView = "mine" | "shared";

interface CanvasSelectionState {
  mine: string | null;
  shared: string | null;
}

interface LoadCanvasesOptions {
  focusView?: CanvasView;
  selectCanvasId?: string | null;
}

const SELECTED_CANVAS_STORAGE_KEY_PREFIX = "agentdash:selected-canvas:";

export function ProjectCanvasManager({
  projectId,
  projectName,
  onExtensionRuntimeRefresh,
}: ProjectCanvasManagerProps) {
  const [canvases, setCanvases] = useState<Canvas[]>([]);
  const [selectedCanvasIds, setSelectedCanvasIds] = useState<CanvasSelectionState>({
    mine: null,
    shared: null,
  });
  const [activeView, setActiveView] = useState<CanvasView>("mine");
  const [createTitle, setCreateTitle] = useState("");
  const [createDescription, setCreateDescription] = useState("");
  const [isLoading, setIsLoading] = useState(false);
  const [isCreating, setIsCreating] = useState(false);
  const [deletingCanvasId, setDeletingCanvasId] = useState<string | null>(null);
  const [publishingCanvasId, setPublishingCanvasId] = useState<string | null>(null);
  const [copyingCanvasId, setCopyingCanvasId] = useState<string | null>(null);
  const [unpublishingCanvasId, setUnpublishingCanvasId] = useState<string | null>(null);
  const [promotingCanvasId, setPromotingCanvasId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const selectionWriteProjectRef = useRef(projectId);

  const mineCanvases = useMemo(
    () => canvases.filter((canvas) => canvas.scope === "personal"),
    [canvases],
  );
  const sharedCanvases = useMemo(
    () => canvases.filter((canvas) => canvas.scope === "project"),
    [canvases],
  );
  const visibleCanvases = activeView === "mine" ? mineCanvases : sharedCanvases;
  const selectedCanvasId = activeView === "mine" ? selectedCanvasIds.mine : selectedCanvasIds.shared;

  const loadCanvases = useCallback(async (options: LoadCanvasesOptions = {}) => {
    setIsLoading(true);
    setError(null);
    try {
      const nextCanvases = await fetchProjectCanvases(projectId, "all");
      const nextMineCanvases = nextCanvases.filter((canvas) => canvas.scope === "personal");
      const nextSharedCanvases = nextCanvases.filter((canvas) => canvas.scope === "project");

      setCanvases(nextCanvases);
      setSelectedCanvasIds((current) => ({
        mine: chooseCanvasId({
          canvases: nextMineCanvases,
          currentCanvasId: current.mine,
          preferredCanvasId: options.focusView === "mine" ? options.selectCanvasId : undefined,
          rememberedCanvasId: readRememberedCanvasId(getSelectionStorageKey(projectId, "mine")),
        }),
        shared: chooseCanvasId({
          canvases: nextSharedCanvases,
          currentCanvasId: current.shared,
          preferredCanvasId: options.focusView === "shared" ? options.selectCanvasId : undefined,
          rememberedCanvasId: readRememberedCanvasId(getSelectionStorageKey(projectId, "shared")),
        }),
      }));

      if (options.focusView) {
        setActiveView(options.focusView);
      }
    } catch (loadError) {
      setError(loadError instanceof Error ? loadError.message : "Canvas 列表加载失败");
    } finally {
      setIsLoading(false);
    }
  }, [projectId]);

  useEffect(() => {
    void loadCanvases();
  }, [loadCanvases]);

  useEffect(() => {
    setCanvases([]);
    setCreateTitle("");
    setCreateDescription("");
    setError(null);
    setMessage(null);
    setSelectedCanvasIds({
      mine: null,
      shared: null,
    });
    setActiveView("mine");
  }, [projectId]);

  useEffect(() => {
    if (selectionWriteProjectRef.current !== projectId) {
      selectionWriteProjectRef.current = projectId;
      return;
    }
    if (canvases.length === 0) {
      return;
    }
    writeRememberedCanvasId(getSelectionStorageKey(projectId, "mine"), selectedCanvasIds.mine);
    writeRememberedCanvasId(getSelectionStorageKey(projectId, "shared"), selectedCanvasIds.shared);
  }, [canvases.length, projectId, selectedCanvasIds]);

  const selectedCanvas = useMemo(
    () => visibleCanvases.find((canvas) => canvas.canvas_id === selectedCanvasId) ?? null,
    [selectedCanvasId, visibleCanvases],
  );

  const handleViewChange = useCallback((view: CanvasView) => {
    setActiveView(view);
    setSelectedCanvasIds((current) => {
      const canvasesForView = view === "mine" ? mineCanvases : sharedCanvases;
      const currentCanvasId = view === "mine" ? current.mine : current.shared;
      if (currentCanvasId && canvasesForView.some((canvas) => canvas.canvas_id === currentCanvasId)) {
        return current;
      }
      return withSelectedCanvasId(
        current,
        view,
        chooseCanvasId({
          canvases: canvasesForView,
          currentCanvasId: null,
          rememberedCanvasId: readRememberedCanvasId(getSelectionStorageKey(projectId, view)),
        }),
      );
    });
  }, [mineCanvases, projectId, sharedCanvases]);

  const handleSelectCanvas = useCallback((canvas: Canvas) => {
    if (!canvas.access.can_view) {
      return;
    }
    const view = getCanvasView(canvas);
    setActiveView(view);
    setSelectedCanvasIds((current) => withSelectedCanvasId(current, view, canvas.canvas_id));
  }, []);

  const handleCreateCanvas = useCallback(async () => {
    const title = createTitle.trim();
    if (!title) {
      setError("Canvas 标题不能为空");
      return;
    }

    setIsCreating(true);
    setError(null);
    setMessage(null);
    try {
      const createdCanvas = await createCanvas(projectId, {
        title,
        description: createDescription.trim() || undefined,
      });
      setCanvases((prev) => [createdCanvas, ...prev.filter((canvas) => canvas.canvas_id !== createdCanvas.canvas_id)]);
      setActiveView("mine");
      setSelectedCanvasIds((current) => withSelectedCanvasId(current, "mine", createdCanvas.canvas_id));
      setCreateTitle("");
      setCreateDescription("");
      setMessage(`已创建我的 Canvas：${createdCanvas.title}`);
    } catch (createError) {
      setError(createError instanceof Error ? createError.message : "创建 Canvas 失败");
    } finally {
      setIsCreating(false);
    }
  }, [createDescription, createTitle, projectId]);

  const handleDeleteCanvas = useCallback(async (canvas: Canvas) => {
    if (!canDeleteCanvas(canvas)) {
      setError("当前 Canvas 不允许删除");
      return;
    }

    const view = getCanvasView(canvas);
    const nextSelectedCanvasId = getNextSelectionAfterRemoval(
      view === "mine" ? mineCanvases : sharedCanvases,
      canvas.canvas_id,
    );

    setDeletingCanvasId(canvas.canvas_id);
    setError(null);
    setMessage(null);
    try {
      await deleteCanvas(canvas.canvas_id);
      await loadCanvases({
        focusView: view,
        selectCanvasId: nextSelectedCanvasId,
      });
      setMessage(`已删除 Canvas：${canvas.title}`);
    } catch (deleteError) {
      setError(deleteError instanceof Error ? deleteError.message : "删除 Canvas 失败");
    } finally {
      setDeletingCanvasId(null);
    }
  }, [loadCanvases, mineCanvases, sharedCanvases]);

  const handlePublishCanvas = useCallback(async (canvas: Canvas) => {
    if (!canvas.access.can_publish) {
      setError("当前 Canvas 不允许发布到项目共用");
      return;
    }

    setPublishingCanvasId(canvas.canvas_id);
    setError(null);
    setMessage(null);
    try {
      const publishedCanvas = await publishCanvasToProject(canvas.canvas_id);
      await loadCanvases({
        focusView: "mine",
        selectCanvasId: canvas.canvas_id,
      });
      setMessage(`已发布到项目共用：${publishedCanvas.title}`);
    } catch (publishError) {
      setError(publishError instanceof Error ? publishError.message : "发布到项目共用失败");
    } finally {
      setPublishingCanvasId(null);
    }
  }, [loadCanvases]);

  const handleCopyCanvas = useCallback(async (canvas: Canvas) => {
    if (!canvas.access.can_copy) {
      setError("当前 Canvas 不允许复制为我的 Canvas");
      return;
    }

    setCopyingCanvasId(canvas.canvas_id);
    setError(null);
    setMessage(null);
    try {
      const copiedCanvas = await copyCanvasToPersonal(canvas.canvas_id);
      await loadCanvases({
        focusView: "mine",
        selectCanvasId: copiedCanvas.canvas_id,
      });
      setMessage(`已复制为我的 Canvas：${copiedCanvas.title}`);
    } catch (copyError) {
      setError(copyError instanceof Error ? copyError.message : "复制 Canvas 失败");
    } finally {
      setCopyingCanvasId(null);
    }
  }, [loadCanvases]);

  const handleUnpublishCanvas = useCallback(async (canvas: Canvas) => {
    if (!canvas.access.can_manage_shared) {
      setError("当前 Canvas 不允许取消发布");
      return;
    }

    const nextSelectedCanvasId = getNextSelectionAfterRemoval(sharedCanvases, canvas.canvas_id);

    setUnpublishingCanvasId(canvas.canvas_id);
    setError(null);
    setMessage(null);
    try {
      await unpublishCanvas(canvas.canvas_id);
      await loadCanvases({
        focusView: "shared",
        selectCanvasId: nextSelectedCanvasId,
      });
      setMessage(`已取消发布：${canvas.title}`);
    } catch (unpublishError) {
      setError(unpublishError instanceof Error ? unpublishError.message : "取消发布失败");
    } finally {
      setUnpublishingCanvasId(null);
    }
  }, [loadCanvases, sharedCanvases]);

  const handlePromoteCanvas = useCallback(async (canvas: Canvas) => {
    if (!canvas.access.can_edit_source) {
      setError("当前 Canvas 源为只读，不能发布为插件");
      return;
    }

    setPromotingCanvasId(canvas.canvas_id);
    setError(null);
    setMessage(null);
    try {
      const result = await promoteCanvasToExtension(canvas.canvas_id, {
        display_name: canvas.title,
        overwrite: true,
      });
      await onExtensionRuntimeRefresh?.(projectId);
      setMessage(`已发布为 WorkspacePanel 插件：${result.extension_key}`);
    } catch (promoteError) {
      setError(promoteError instanceof Error ? promoteError.message : "发布 Canvas 插件失败");
    } finally {
      setPromotingCanvasId(null);
    }
  }, [onExtensionRuntimeRefresh, projectId]);

  return (
    <div className="grid gap-6 xl:grid-cols-[320px_minmax(0,1fr)]">
      <section className="space-y-4 rounded-[12px] border border-border bg-background p-4">
        <div className="space-y-1">
          <p className="text-[11px] uppercase tracking-wider text-muted-foreground">Canvas Assets</p>
          <h3 className="text-base font-semibold text-foreground">Canvas 资产</h3>
          <p className="text-sm leading-6 text-muted-foreground">
            {projectName} 的个人 Canvas 与项目共用 Canvas。
          </p>
        </div>

        <div className="grid grid-cols-2 gap-1 rounded-[8px] border border-border bg-secondary/20 p-1">
          <button
            type="button"
            onClick={() => handleViewChange("mine")}
            className={getViewButtonClassName(activeView === "mine")}
          >
            我的
            <span className="text-[10px] text-muted-foreground">{mineCanvases.length}</span>
          </button>
          <button
            type="button"
            onClick={() => handleViewChange("shared")}
            className={getViewButtonClassName(activeView === "shared")}
          >
            项目共用
            <span className="text-[10px] text-muted-foreground">{sharedCanvases.length}</span>
          </button>
        </div>

        {activeView === "mine" && (
          <div className="space-y-3 rounded-[8px] border border-border bg-secondary/20 p-3">
            <div className="space-y-1">
              <p className="text-xs font-medium text-foreground">新建 Canvas</p>
              <p className="text-xs text-muted-foreground">
                默认创建为我的个人 Canvas。
              </p>
            </div>

            <input
              value={createTitle}
              onChange={(event) => setCreateTitle(event.target.value)}
              placeholder="例如：运营指标看板"
              className="agentdash-form-input"
            />
            <textarea
              value={createDescription}
              onChange={(event) => setCreateDescription(event.target.value)}
              placeholder="简要描述这个 Canvas 的用途"
              rows={3}
              className="min-h-[88px] w-full rounded-[8px] border border-border bg-background px-3 py-2 text-sm text-foreground outline-none transition-colors focus:border-foreground/30"
            />
            <button
              type="button"
              onClick={() => void handleCreateCanvas()}
              disabled={isCreating}
              className="agentdash-button-primary w-full disabled:cursor-not-allowed disabled:opacity-60"
            >
              {isCreating ? "创建中..." : "创建 Canvas"}
            </button>
          </div>
        )}

        {(message || error) && (
          <div className={`rounded-[8px] border px-3 py-2 text-xs ${
            error
              ? "border-destructive/40 bg-destructive/10 text-destructive"
              : "border-success/40 bg-success/10 text-success"
          }`}>
            {error ?? message}
          </div>
        )}

        <div className="space-y-2">
          <div className="flex items-center justify-between gap-3">
            <p className="text-xs font-medium text-foreground">
              {activeView === "mine" ? "我的 Canvas" : "项目共用 Canvas"}
            </p>
            <button
              type="button"
              onClick={() => void loadCanvases()}
              disabled={isLoading}
              className="rounded-[8px] border border-border bg-background px-2 py-1 text-xs text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground disabled:cursor-not-allowed disabled:opacity-60"
            >
              刷新
            </button>
          </div>

          {isLoading && (
            <div className="rounded-[8px] border border-dashed border-border bg-secondary/10 px-3 py-4 text-sm text-muted-foreground">
              正在加载 Canvas 列表...
            </div>
          )}

          {!isLoading && visibleCanvases.length === 0 && (
            <div className="rounded-[8px] border border-dashed border-border bg-secondary/10 px-3 py-4 text-sm text-muted-foreground">
              {activeView === "mine"
                ? "当前项目还没有我的 Canvas。"
                : "当前项目还没有项目共用 Canvas。"}
            </div>
          )}

          {!isLoading && visibleCanvases.length > 0 && (
            <div className="space-y-2">
              {visibleCanvases.map((canvas) => {
                const isSelected = canvas.canvas_id === selectedCanvasId;
                return (
                  <article
                    key={canvas.canvas_id}
                    className={`rounded-[12px] border p-3 transition-colors ${
                      isSelected
                        ? "border-foreground/15 bg-foreground/[0.03]"
                        : "border-border bg-background"
                    }`}
                  >
                    <button
                      type="button"
                      onClick={() => handleSelectCanvas(canvas)}
                      disabled={!canvas.access.can_view}
                      className="w-full text-left disabled:cursor-not-allowed disabled:opacity-60"
                    >
                      <div className="flex items-start justify-between gap-3">
                        <div className="min-w-0">
                          <p className="truncate text-sm font-medium text-foreground">
                            {canvas.title}
                          </p>
                          <p className="mt-1 line-clamp-2 text-xs text-muted-foreground">
                            {canvas.description || "暂无描述"}
                          </p>
                        </div>
                        {isSelected && (
                          <span className="shrink-0 rounded-[8px] border border-foreground/10 bg-background px-2 py-1 text-[11px] text-foreground">
                            当前
                          </span>
                        )}
                      </div>
                    </button>

                    <div className="mt-3 flex flex-wrap gap-2">
                      <CanvasMetaBadge>{canvas.scope === "personal" ? "个人" : "项目共用"}</CanvasMetaBadge>
                      <CanvasMetaBadge>{canvas.access.can_edit_source ? "可编辑" : "只读"}</CanvasMetaBadge>
                      {canvas.shared_canvas_id && <CanvasMetaBadge>已发布</CanvasMetaBadge>}
                      {canvas.cloned_from_canvas_id && <CanvasMetaBadge>复制来源</CanvasMetaBadge>}
                      <CanvasMetaBadge>files: {canvas.files.length}</CanvasMetaBadge>
                    </div>

                    <div className="mt-3 flex flex-wrap items-center justify-between gap-2">
                      <p className="text-[11px] text-muted-foreground">
                        更新于 {formatDateTime(canvas.updated_at)}
                      </p>
                      <div className="flex flex-wrap justify-end gap-2">
                        <button
                          type="button"
                          onClick={() => handleSelectCanvas(canvas)}
                          disabled={!canvas.access.can_view}
                          className="whitespace-nowrap rounded-[8px] border border-border bg-background px-2.5 py-1 text-xs text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground disabled:cursor-not-allowed disabled:opacity-60"
                        >
                          {canvas.access.can_edit_source ? "编辑" : "打开预览"}
                        </button>
                        {activeView === "mine" && canvas.access.can_publish && (
                          <button
                            type="button"
                            onClick={() => void handlePublishCanvas(canvas)}
                            disabled={publishingCanvasId === canvas.canvas_id}
                            className="whitespace-nowrap rounded-[8px] border border-border bg-background px-2.5 py-1 text-xs text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground disabled:cursor-not-allowed disabled:opacity-60"
                          >
                            {publishingCanvasId === canvas.canvas_id ? "发布中..." : "发布到项目共用"}
                          </button>
                        )}
                        {activeView === "mine" && canvas.access.can_edit_source && (
                          <button
                            type="button"
                            onClick={() => void handlePromoteCanvas(canvas)}
                            disabled={promotingCanvasId === canvas.canvas_id}
                            className="whitespace-nowrap rounded-[8px] border border-border bg-background px-2.5 py-1 text-xs text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground disabled:cursor-not-allowed disabled:opacity-60"
                          >
                            {promotingCanvasId === canvas.canvas_id ? "发布中..." : "发布为插件"}
                          </button>
                        )}
                        {activeView === "shared" && canvas.access.can_copy && (
                          <button
                            type="button"
                            onClick={() => void handleCopyCanvas(canvas)}
                            disabled={copyingCanvasId === canvas.canvas_id}
                            className="whitespace-nowrap rounded-[8px] border border-border bg-background px-2.5 py-1 text-xs text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground disabled:cursor-not-allowed disabled:opacity-60"
                          >
                            {copyingCanvasId === canvas.canvas_id ? "复制中..." : "复制为我的"}
                          </button>
                        )}
                        {activeView === "shared" && canvas.access.can_manage_shared && (
                          <button
                            type="button"
                            onClick={() => void handleUnpublishCanvas(canvas)}
                            disabled={unpublishingCanvasId === canvas.canvas_id}
                            className="whitespace-nowrap rounded-[8px] border border-border bg-background px-2.5 py-1 text-xs text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground disabled:cursor-not-allowed disabled:opacity-60"
                          >
                            {unpublishingCanvasId === canvas.canvas_id ? "取消中..." : "取消发布"}
                          </button>
                        )}
                        {canDeleteCanvas(canvas) && (
                          <button
                            type="button"
                            onClick={() => void handleDeleteCanvas(canvas)}
                            disabled={deletingCanvasId === canvas.canvas_id}
                            className="whitespace-nowrap rounded-[8px] border border-destructive/25 bg-destructive/5 px-2.5 py-1 text-xs text-destructive transition-colors hover:bg-destructive/10 disabled:cursor-not-allowed disabled:opacity-60"
                          >
                            {deletingCanvasId === canvas.canvas_id ? "删除中..." : activeView === "shared" ? "删除共用源" : "删除"}
                          </button>
                        )}
                      </div>
                    </div>
                  </article>
                );
              })}
            </div>
          )}
        </div>
      </section>

      <section className="min-w-0 rounded-[12px] border border-border bg-background">
        {selectedCanvas ? (
          <div className="flex h-[920px] flex-col overflow-hidden rounded-[12px]">
            <div className="border-b border-border px-4 py-3">
              <div className="flex flex-wrap items-start justify-between gap-3">
                <div className="min-w-0">
                  <p className="text-[11px] uppercase tracking-wider text-muted-foreground">
                    {activeView === "mine" ? "我的 Canvas" : "项目共用 Canvas"}
                  </p>
                  <h3 className="truncate text-base font-semibold text-foreground">{selectedCanvas.title}</h3>
                  <p className="mt-1 text-sm text-muted-foreground">
                    {selectedCanvas.description || "暂无描述"}
                  </p>
                </div>
                <div className="flex flex-wrap justify-end gap-2">
                  {selectedCanvas.scope === "personal" && selectedCanvas.access.can_publish && (
                    <button
                      type="button"
                      onClick={() => void handlePublishCanvas(selectedCanvas)}
                      disabled={publishingCanvasId === selectedCanvas.canvas_id}
                      className="whitespace-nowrap rounded-[8px] border border-border bg-background px-2.5 py-1 text-xs text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground disabled:cursor-not-allowed disabled:opacity-60"
                    >
                      {publishingCanvasId === selectedCanvas.canvas_id ? "发布中..." : "发布到项目共用"}
                    </button>
                  )}
                  {selectedCanvas.scope === "personal" && selectedCanvas.access.can_edit_source && (
                    <button
                      type="button"
                      onClick={() => void handlePromoteCanvas(selectedCanvas)}
                      disabled={promotingCanvasId === selectedCanvas.canvas_id}
                      className="whitespace-nowrap rounded-[8px] border border-border bg-background px-2.5 py-1 text-xs text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground disabled:cursor-not-allowed disabled:opacity-60"
                    >
                      {promotingCanvasId === selectedCanvas.canvas_id ? "发布中..." : "发布为插件"}
                    </button>
                  )}
                  {selectedCanvas.scope === "project" && selectedCanvas.access.can_copy && (
                    <button
                      type="button"
                      onClick={() => void handleCopyCanvas(selectedCanvas)}
                      disabled={copyingCanvasId === selectedCanvas.canvas_id}
                      className="whitespace-nowrap rounded-[8px] border border-border bg-background px-2.5 py-1 text-xs text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground disabled:cursor-not-allowed disabled:opacity-60"
                    >
                      {copyingCanvasId === selectedCanvas.canvas_id ? "复制中..." : "复制为我的"}
                    </button>
                  )}
                  {selectedCanvas.scope === "project" && selectedCanvas.access.can_manage_shared && (
                    <button
                      type="button"
                      onClick={() => void handleUnpublishCanvas(selectedCanvas)}
                      disabled={unpublishingCanvasId === selectedCanvas.canvas_id}
                      className="whitespace-nowrap rounded-[8px] border border-border bg-background px-2.5 py-1 text-xs text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground disabled:cursor-not-allowed disabled:opacity-60"
                    >
                      {unpublishingCanvasId === selectedCanvas.canvas_id ? "取消中..." : "取消发布"}
                    </button>
                  )}
                  <CanvasMetaBadge>mount: {selectedCanvas.canvas_mount_id}</CanvasMetaBadge>
                  <CanvasMetaBadge>files: {selectedCanvas.files.length}</CanvasMetaBadge>
                  <CanvasMetaBadge>更新于 {formatDateTime(selectedCanvas.updated_at)}</CanvasMetaBadge>
                </div>
              </div>
            </div>
            <div className="min-h-0 flex-1">
              <CanvasRuntimePanel
                canvasId={selectedCanvas.canvas_id}
                onClose={() => setSelectedCanvasIds((current) => withSelectedCanvasId(current, activeView, null))}
              />
            </div>
          </div>
        ) : (
          <div className="flex h-[920px] items-center justify-center rounded-[12px] bg-secondary/10 px-6 text-center">
            <div className="max-w-sm space-y-3">
              <p className="text-[11px] uppercase tracking-wider text-muted-foreground">Canvas Detail</p>
              <h3 className="text-lg font-semibold text-foreground">
                {activeView === "mine" ? "选择我的 Canvas" : "选择项目共用 Canvas"}
              </h3>
              <p className="text-sm leading-6 text-muted-foreground">
                {activeView === "mine"
                  ? "个人 Canvas 会在这里展示预览、源文件和数据绑定。"
                  : "项目共用 Canvas 会在这里展示只读预览和绑定状态。"}
              </p>
            </div>
          </div>
        )}
      </section>
    </div>
  );
}

interface ChooseCanvasIdInput {
  canvases: Canvas[];
  currentCanvasId: string | null;
  rememberedCanvasId?: string | null;
  preferredCanvasId?: string | null;
}

function chooseCanvasId({
  canvases,
  currentCanvasId,
  rememberedCanvasId = null,
  preferredCanvasId = null,
}: ChooseCanvasIdInput): string | null {
  const preferredIds = [preferredCanvasId, currentCanvasId, rememberedCanvasId];
  for (const preferredId of preferredIds) {
    if (preferredId && canvases.some((canvas) => canvas.canvas_id === preferredId)) {
      return preferredId;
    }
  }
  return canvases[0]?.canvas_id ?? null;
}

function getCanvasView(canvas: Canvas): CanvasView {
  return canvas.scope === "project" ? "shared" : "mine";
}

function canDeleteCanvas(canvas: Canvas): boolean {
  return canvas.access.can_edit_source || canvas.access.can_manage_shared;
}

function getNextSelectionAfterRemoval(canvases: Canvas[], removedCanvasId: string): string | null {
  const currentIndex = canvases.findIndex((canvas) => canvas.canvas_id === removedCanvasId);
  if (currentIndex < 0) {
    return canvases[0]?.canvas_id ?? null;
  }
  return canvases[currentIndex + 1]?.canvas_id
    ?? canvases[currentIndex - 1]?.canvas_id
    ?? null;
}

function withSelectedCanvasId(
  current: CanvasSelectionState,
  view: CanvasView,
  canvasId: string | null,
): CanvasSelectionState {
  if (view === "mine") {
    return {
      ...current,
      mine: canvasId,
    };
  }
  return {
    ...current,
    shared: canvasId,
  };
}

function getSelectionStorageKey(projectId: string, view: CanvasView): string {
  return `${SELECTED_CANVAS_STORAGE_KEY_PREFIX}${projectId}:${view}`;
}

function getViewButtonClassName(isActive: boolean): string {
  return [
    "flex min-w-0 items-center justify-center gap-2 rounded-[6px] px-2 py-1.5 text-xs transition-colors",
    isActive
      ? "bg-background text-foreground shadow-sm"
      : "text-muted-foreground hover:bg-background/70 hover:text-foreground",
  ].join(" ");
}

interface CanvasMetaBadgeProps {
  children: ReactNode;
}

function CanvasMetaBadge({ children }: CanvasMetaBadgeProps) {
  return (
    <span className="rounded-[8px] border border-border bg-secondary/20 px-2 py-1 text-[11px] text-muted-foreground">
      {children}
    </span>
  );
}

function readRememberedCanvasId(storageKey: string): string | null {
  try {
    const value = localStorage.getItem(storageKey)?.trim();
    return value || null;
  } catch {
    return null;
  }
}

function writeRememberedCanvasId(storageKey: string, canvasId: string | null): void {
  try {
    if (canvasId) {
      localStorage.setItem(storageKey, canvasId);
      return;
    }
    localStorage.removeItem(storageKey);
  } catch {
    // 本地持久化失败不应影响 Canvas 主流程。
  }
}

export default ProjectCanvasManager;
