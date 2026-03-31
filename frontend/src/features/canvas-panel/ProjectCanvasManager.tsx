import { useCallback, useEffect, useMemo, useState } from "react";
import {
  createCanvas,
  deleteCanvas,
  fetchProjectCanvases,
} from "../../services/canvas";
import type { Canvas } from "../../types";
import { CanvasSessionPanel } from "./CanvasSessionPanel";

export interface ProjectCanvasManagerProps {
  projectId: string;
  projectName: string;
}

const SELECTED_CANVAS_STORAGE_KEY_PREFIX = "agentdash:selected-canvas:";

export function ProjectCanvasManager({
  projectId,
  projectName,
}: ProjectCanvasManagerProps) {
  const [canvases, setCanvases] = useState<Canvas[]>([]);
  const [selectedCanvasId, setSelectedCanvasId] = useState<string | null>(null);
  const [createTitle, setCreateTitle] = useState("");
  const [createDescription, setCreateDescription] = useState("");
  const [isLoading, setIsLoading] = useState(false);
  const [isCreating, setIsCreating] = useState(false);
  const [deletingCanvasId, setDeletingCanvasId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);

  const storageKey = useMemo(
    () => `${SELECTED_CANVAS_STORAGE_KEY_PREFIX}${projectId}`,
    [projectId],
  );

  const loadCanvases = useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      const nextCanvases = await fetchProjectCanvases(projectId);
      setCanvases(nextCanvases);
      setSelectedCanvasId((current) => {
        const remembered = readRememberedCanvasId(storageKey);
        const preferredIds = [current, remembered].filter((value): value is string => Boolean(value));
        for (const preferredId of preferredIds) {
          if (nextCanvases.some((canvas) => canvas.id === preferredId)) {
            return preferredId;
          }
        }
        return nextCanvases[0]?.id ?? null;
      });
    } catch (loadError) {
      setError(loadError instanceof Error ? loadError.message : "Canvas 列表加载失败");
    } finally {
      setIsLoading(false);
    }
  }, [projectId, storageKey]);

  useEffect(() => {
    void loadCanvases();
  }, [loadCanvases]);

  useEffect(() => {
    setCreateTitle("");
    setCreateDescription("");
    setError(null);
    setMessage(null);
  }, [projectId]);

  useEffect(() => {
    writeRememberedCanvasId(storageKey, selectedCanvasId);
  }, [selectedCanvasId, storageKey]);

  const selectedCanvas = useMemo(
    () => canvases.find((canvas) => canvas.id === selectedCanvasId) ?? null,
    [canvases, selectedCanvasId],
  );

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
      setCanvases((prev) => [createdCanvas, ...prev]);
      setSelectedCanvasId(createdCanvas.id);
      setCreateTitle("");
      setCreateDescription("");
      setMessage(`已在 ${projectName} 下创建 Canvas：${createdCanvas.title}`);
    } catch (createError) {
      setError(createError instanceof Error ? createError.message : "创建 Canvas 失败");
    } finally {
      setIsCreating(false);
    }
  }, [createDescription, createTitle, projectId, projectName]);

  const handleDeleteCanvas = useCallback(async (canvas: Canvas) => {
    setDeletingCanvasId(canvas.id);
    setError(null);
    setMessage(null);
    try {
      await deleteCanvas(canvas.id);
      const currentIndex = canvases.findIndex((item) => item.id === canvas.id);
      const nextSelectedCanvasId =
        canvases[currentIndex + 1]?.id
        ?? canvases[currentIndex - 1]?.id
        ?? null;
      setCanvases((prev) => prev.filter((item) => item.id !== canvas.id));
      setSelectedCanvasId((current) => {
        if (current !== canvas.id) {
          return current;
        }
        return nextSelectedCanvasId;
      });
      setMessage(`已删除 Canvas：${canvas.title}`);
    } catch (deleteError) {
      setError(deleteError instanceof Error ? deleteError.message : "删除 Canvas 失败");
    } finally {
      setDeletingCanvasId(null);
    }
  }, [canvases]);

  return (
    <div className="grid gap-6 xl:grid-cols-[320px_minmax(0,1fr)]">
      <section className="space-y-4 rounded-[18px] border border-border bg-background p-4">
        <div className="space-y-1">
          <p className="text-[11px] uppercase tracking-[0.14em] text-muted-foreground">Canvas Assets</p>
          <h3 className="text-base font-semibold text-foreground">项目级 Canvas 列表</h3>
          <p className="text-sm leading-6 text-muted-foreground">
            在这里维护 {projectName} 的可视化资产，选中后可直接预览运行时与编辑绑定。
          </p>
        </div>

        <div className="space-y-3 rounded-[14px] border border-border bg-secondary/20 p-3">
          <div className="space-y-1">
            <p className="text-xs font-medium text-foreground">新建 Canvas</p>
            <p className="text-xs text-muted-foreground">
              默认会生成 `src/main.tsx`，可稍后继续编辑文件和数据绑定。
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
            className="min-h-[88px] w-full rounded-[12px] border border-border bg-background px-3 py-2 text-sm text-foreground outline-none transition-colors focus:border-foreground/30"
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

        {(message || error) && (
          <div className={`rounded-[12px] border px-3 py-2 text-xs ${
            error
              ? "border-destructive/40 bg-destructive/10 text-destructive"
              : "border-emerald-300/40 bg-emerald-50 text-emerald-700"
          }`}>
            {error ?? message}
          </div>
        )}

        <div className="space-y-2">
          <div className="flex items-center justify-between">
            <p className="text-xs font-medium text-foreground">已有 Canvas</p>
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
            <div className="rounded-[12px] border border-dashed border-border bg-secondary/10 px-3 py-4 text-sm text-muted-foreground">
              正在加载 Canvas 列表...
            </div>
          )}

          {!isLoading && canvases.length === 0 && (
            <div className="rounded-[12px] border border-dashed border-border bg-secondary/10 px-3 py-4 text-sm text-muted-foreground">
              当前项目还没有 Canvas，先创建一个试试看。
            </div>
          )}

          {!isLoading && canvases.length > 0 && (
            <div className="space-y-2">
              {canvases.map((canvas) => {
                const isSelected = canvas.id === selectedCanvasId;
                return (
                  <article
                    key={canvas.id}
                    className={`rounded-[14px] border p-3 transition-colors ${
                      isSelected
                        ? "border-foreground/15 bg-foreground/[0.03]"
                        : "border-border bg-background"
                    }`}
                  >
                    <button
                      type="button"
                      onClick={() => setSelectedCanvasId(canvas.id)}
                      className="w-full text-left"
                    >
                      <div className="flex items-start justify-between gap-3">
                        <div className="min-w-0">
                          <p className="truncate text-sm font-medium text-foreground">
                            {canvas.title}
                          </p>
                          <p className="mt-1 text-xs text-muted-foreground">
                            {canvas.description || "暂无描述"}
                          </p>
                        </div>
                        {isSelected && (
                          <span className="rounded-full border border-foreground/10 bg-background px-2 py-1 text-[11px] text-foreground">
                            当前
                          </span>
                        )}
                      </div>
                    </button>

                    <div className="mt-3 flex flex-wrap gap-2">
                      <span className="rounded-full border border-border bg-secondary/20 px-2 py-1 text-[11px] text-muted-foreground">
                        files: {canvas.files.length}
                      </span>
                      <span className="rounded-full border border-border bg-secondary/20 px-2 py-1 text-[11px] text-muted-foreground">
                        bindings: {canvas.bindings.length}
                      </span>
                    </div>

                    <div className="mt-3 flex items-center justify-between gap-3">
                      <p className="text-[11px] text-muted-foreground">
                        更新于 {formatDateTime(canvas.updated_at)}
                      </p>
                      <button
                        type="button"
                        onClick={() => void handleDeleteCanvas(canvas)}
                        disabled={deletingCanvasId === canvas.id}
                        className="rounded-[8px] border border-destructive/25 bg-destructive/5 px-2.5 py-1 text-xs text-destructive transition-colors hover:bg-destructive/10 disabled:cursor-not-allowed disabled:opacity-60"
                      >
                        {deletingCanvasId === canvas.id ? "删除中..." : "删除"}
                      </button>
                    </div>
                  </article>
                );
              })}
            </div>
          )}
        </div>
      </section>

      <section className="min-w-0 rounded-[18px] border border-border bg-background">
        {selectedCanvas ? (
          <div className="flex h-[920px] flex-col overflow-hidden rounded-[18px]">
            <div className="border-b border-border px-4 py-3">
              <div className="flex flex-wrap items-start justify-between gap-3">
                <div className="min-w-0">
                  <p className="text-[11px] uppercase tracking-[0.14em] text-muted-foreground">当前 Canvas</p>
                  <h3 className="truncate text-base font-semibold text-foreground">{selectedCanvas.title}</h3>
                  <p className="mt-1 text-sm text-muted-foreground">
                    {selectedCanvas.description || "暂无描述，可直接在右侧预览和绑定编辑中继续完善。"}
                  </p>
                </div>
                <div className="flex flex-wrap gap-2">
                  <span className="rounded-full border border-border bg-secondary/20 px-2 py-1 text-[11px] text-muted-foreground">
                    files: {selectedCanvas.files.length}
                  </span>
                  <span className="rounded-full border border-border bg-secondary/20 px-2 py-1 text-[11px] text-muted-foreground">
                    bindings: {selectedCanvas.bindings.length}
                  </span>
                  <span className="rounded-full border border-border bg-secondary/20 px-2 py-1 text-[11px] text-muted-foreground">
                    更新于 {formatDateTime(selectedCanvas.updated_at)}
                  </span>
                </div>
              </div>
            </div>
            <div className="min-h-0 flex-1">
              <CanvasSessionPanel
                canvasId={selectedCanvas.id}
                sessionId={null}
                onClose={() => setSelectedCanvasId(null)}
              />
            </div>
          </div>
        ) : (
          <div className="flex h-[920px] items-center justify-center rounded-[18px] bg-secondary/10 px-6 text-center">
            <div className="max-w-sm space-y-3">
              <p className="text-[11px] uppercase tracking-[0.14em] text-muted-foreground">Canvas Detail</p>
              <h3 className="text-lg font-semibold text-foreground">选择一个 Canvas 开始编辑</h3>
              <p className="text-sm leading-6 text-muted-foreground">
                右侧会展示运行时预览、绑定编辑和文件快照；如果还没有 Canvas，可以先在左侧创建一个。
              </p>
            </div>
          </div>
        )}
      </section>
    </div>
  );
}

function formatDateTime(value: string): string {
  const time = new Date(value);
  if (Number.isNaN(time.getTime())) {
    return value;
  }
  return time.toLocaleString("zh-CN", {
    hour12: false,
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
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
