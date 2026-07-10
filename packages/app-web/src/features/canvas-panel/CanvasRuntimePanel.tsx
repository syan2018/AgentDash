import { useCallback, useEffect, useState } from "react";
import {
  fetchCanvas,
  fetchAgentRunCanvasRuntimeSnapshot,
  fetchCanvasByMountId,
  buildStandaloneCanvasPreviewSnapshot,
  upsertAgentRunCanvasRuntimeBinding,
  updateCanvas,
  type AgentRunCanvasBridgeIdentity,
} from "../../services/canvas";
import type { Canvas, CanvasRuntimeSnapshot } from "../../types";
import { CanvasFilesEditor, type CanvasFilesEditorSaveInput } from "./CanvasFilesEditor";
import { CanvasRuntimeBindingsEditor, type CanvasRuntimeBindingDraft } from "./CanvasRuntimeBindingsEditor";
import { CanvasRuntimePreview } from "./CanvasRuntimePreview";
import { areCanvasRuntimeSnapshotsEquivalent } from "./CanvasRuntimePreview.runtime";

export interface CanvasRuntimePanelProps {
  canvasId: string | null;
  canvasMountId?: string | null;
  projectId?: string | null;
  agentRunBridge?: AgentRunCanvasBridgeIdentity | null;
  showBridgeUnavailable?: boolean;
  onAgentRunWorkspaceRefresh?: (() => Promise<unknown>) | null;
  refreshRevision?: number;
  onClose: () => void;
  /** 打开该 Canvas 对应 mount 的资源浏览 Tab */
  onBrowseFiles?: (mountId: string) => void;
}

type CanvasDetailMode = "bindings" | "files";

export function CanvasRuntimePanel({
  canvasId,
  canvasMountId,
  projectId,
  agentRunBridge = null,
  showBridgeUnavailable = false,
  onAgentRunWorkspaceRefresh = null,
  refreshRevision = 0,
  onClose,
  onBrowseFiles,
}: CanvasRuntimePanelProps) {
  const [canvas, setCanvas] = useState<Canvas | null>(null);
  const [snapshot, setSnapshot] = useState<CanvasRuntimeSnapshot | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [isSavingBindings, setIsSavingBindings] = useState(false);
  const [bindingsError, setBindingsError] = useState<string | null>(null);
  const [isSavingFiles, setIsSavingFiles] = useState(false);
  const [filesError, setFilesError] = useState<string | null>(null);

  const [isDetailOpen, setIsDetailOpen] = useState(false);
  const [detailMode, setDetailMode] = useState<CanvasDetailMode>("bindings");

  const loadCanvasData = useCallback(async () => {
    if (!canvasId && (!canvasMountId || !projectId)) {
      setCanvas(null);
      setSnapshot(null);
      setError(null);
      setBindingsError(null);
      setFilesError(null);
      return;
    }

    setIsLoading(true);
    setError(null);
    setBindingsError(null);
    setFilesError(null);
    try {
      const canvasRequest = canvasId
        ? fetchCanvas(canvasId)
        : fetchCanvasByMountId(projectId ?? "", canvasMountId ?? "");
      const nextCanvas = await canvasRequest;
      const nextSnapshot = agentRunBridge
        ? await fetchAgentRunCanvasRuntimeSnapshot(agentRunBridge)
        : buildStandaloneCanvasPreviewSnapshot(nextCanvas);
      setCanvas(nextCanvas);
      setSnapshot((current) =>
        areCanvasRuntimeSnapshotsEquivalent(current, nextSnapshot) ? current : nextSnapshot,
      );
    } catch (err) {
      setError(err instanceof Error ? err.message : "Canvas 加载失败");
      setCanvas(null);
      setSnapshot(null);
      setBindingsError(null);
      setFilesError(null);
    } finally {
      setIsLoading(false);
    }
  }, [agentRunBridge, canvasId, canvasMountId, projectId]);

  useEffect(() => {
    void loadCanvasData();
  }, [loadCanvasData, refreshRevision]);

  const handleBindingUpsert = useCallback(async (binding: CanvasRuntimeBindingDraft) => {
    if (!agentRunBridge) {
      const accessError = new Error("当前视图没有 AgentRun runtime bridge，不能保存运行期绑定。");
      setBindingsError(accessError.message);
      throw accessError;
    }
    setIsSavingBindings(true);
    setBindingsError(null);
    try {
      const nextSnapshot = await upsertAgentRunCanvasRuntimeBinding(agentRunBridge, binding);
      setSnapshot((current) =>
        areCanvasRuntimeSnapshotsEquivalent(current, nextSnapshot) ? current : nextSnapshot,
      );
    } catch (err) {
      setBindingsError(err instanceof Error ? err.message : "保存运行期绑定失败");
      throw err;
    } finally {
      setIsSavingBindings(false);
    }
  }, [agentRunBridge]);

  const handleFilesSave = useCallback(async (input: CanvasFilesEditorSaveInput) => {
    if (!canvas) {
      return;
    }
    if (canvas.access.can_edit_source !== true) {
      const accessError = new Error("当前 Canvas 源为只读，不能保存源文件。");
      setFilesError(accessError.message);
      throw accessError;
    }

    setIsSavingFiles(true);
    setFilesError(null);
    try {
      const nextCanvas = await updateCanvas(canvas.canvas_id, {
        entry_file: input.entryFile,
        files: input.files,
      });
      setCanvas(nextCanvas);
      const nextSnapshot = agentRunBridge
        ? await fetchAgentRunCanvasRuntimeSnapshot(agentRunBridge)
        : buildStandaloneCanvasPreviewSnapshot(nextCanvas);
      setSnapshot((current) =>
        areCanvasRuntimeSnapshotsEquivalent(current, nextSnapshot) ? current : nextSnapshot,
      );
    } catch (err) {
      setFilesError(err instanceof Error ? err.message : "保存源文件失败");
      throw err;
    } finally {
      setIsSavingFiles(false);
    }
  }, [agentRunBridge, canvas]);

  const toggleDetailMode = useCallback((mode: CanvasDetailMode) => {
    setDetailMode(mode);
    setIsDetailOpen((current) => (current && detailMode === mode ? false : true));
  }, [detailMode]);

  const vfsMountId = canvas?.vfs_mount_id ?? snapshot?.vfs_mount_id ?? null;
  const canEditSource = canvas?.access.can_edit_source === true;

  if (!canvasId && !canvasMountId) {
    return null;
  }

  return (
    <aside className="flex h-full w-full flex-col overflow-hidden bg-background">
      {/* ── 顶部：标题 + 操作 ── */}
      <header className="flex items-center justify-between border-b border-border px-4 py-2.5">
        <div className="flex min-w-0 items-center gap-3">
          <div className="min-w-0">
            <h3 className="truncate text-sm font-semibold text-foreground">
              {canvas?.title || canvasMountId || canvasId}
            </h3>
          </div>
          {snapshot && (
            <div className="flex shrink-0 items-center gap-1.5 text-[11px] text-muted-foreground">
              <span className="rounded-[6px] border border-border bg-secondary/60 px-1.5 py-0.5">
                {snapshot.files.length} 文件
              </span>
              <span className="rounded-[6px] border border-border bg-secondary/60 px-1.5 py-0.5">
                {snapshot.bindings.length} 绑定
              </span>
              <span className="rounded-[6px] border border-border bg-secondary/60 px-1.5 py-0.5">
                {snapshot.entry || "无入口"}
              </span>
              {canvas && !canEditSource && (
                <span className="rounded-[6px] border border-border bg-secondary/60 px-1.5 py-0.5">
                  只读源
                </span>
              )}
            </div>
          )}
        </div>
        <div className="flex shrink-0 items-center gap-1.5">
          <button
            type="button"
            onClick={() => void loadCanvasData()}
            className="rounded-[8px] border border-border bg-background px-2 py-1 text-xs text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
          >
            刷新
          </button>
          <button
            type="button"
            onClick={onClose}
            className="rounded-[8px] border border-border bg-background px-2 py-1 text-xs text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
          >
            关闭
          </button>
        </div>
      </header>

      {/* ── 中部：Canvas 预览（主体，占满剩余空间） ── */}
      <div className="relative flex min-h-0 flex-1 flex-col">
        {isLoading && (
          <div className="flex flex-1 items-center justify-center text-sm text-muted-foreground">
            正在加载 Canvas 运行时快照...
          </div>
        )}
        {error && (
          <div className="m-4 rounded-[8px] border border-destructive/30 bg-destructive/10 px-3 py-2 text-xs text-destructive">
            {error}
          </div>
        )}

        {!isLoading && !error && snapshot && (
          <CanvasRuntimePreview
            snapshot={snapshot}
            agentRunBridge={agentRunBridge}
            showBridgeUnavailable={showBridgeUnavailable}
            onAgentRunWorkspaceRefresh={onAgentRunWorkspaceRefresh}
          />
        )}

        {!isLoading && !error && !snapshot && (
          <div className="flex flex-1 items-center justify-center px-4 text-sm text-muted-foreground">
            当前还没有可展示的 Canvas 运行时快照。
          </div>
        )}
      </div>

      {/* ── 底部：操作栏 ── */}
      {!isLoading && !error && snapshot && (
        <div className="shrink-0 border-t border-border">
          <div className="flex items-center justify-between bg-secondary/20 px-3 py-1.5">
            <div className="flex items-center gap-1">
              {onBrowseFiles && vfsMountId && (
                <button
                  type="button"
                  onClick={() => onBrowseFiles(vfsMountId)}
                  className="rounded-[6px] px-2 py-1 text-[11px] font-medium text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
                >
                  浏览文件
                </button>
              )}
              <button
                type="button"
                onClick={() => toggleDetailMode("bindings")}
                className={[
                  "rounded-[6px] px-2 py-1 text-[11px] font-medium transition-colors",
                  isDetailOpen && detailMode === "bindings"
                    ? "bg-foreground text-background"
                    : "text-muted-foreground hover:bg-secondary hover:text-foreground",
                ].join(" ")}
              >
                数据绑定
              </button>
              <button
                type="button"
                onClick={() => toggleDetailMode("files")}
                className={[
                  "rounded-[6px] px-2 py-1 text-[11px] font-medium transition-colors",
                  isDetailOpen && detailMode === "files"
                    ? "bg-foreground text-background"
                    : "text-muted-foreground hover:bg-secondary hover:text-foreground",
                ].join(" ")}
              >
                源文件
              </button>
            </div>
            <button
              type="button"
              onClick={() => setIsDetailOpen((v) => !v)}
              className="text-muted-foreground transition-transform hover:text-foreground"
            >
              <svg
                xmlns="http://www.w3.org/2000/svg"
                width="14"
                height="14"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth="2"
                strokeLinecap="round"
                strokeLinejoin="round"
                className={`transition-transform ${isDetailOpen ? "" : "rotate-180"}`}
              >
                <path d="m6 9 6 6 6-6" />
              </svg>
            </button>
          </div>

          {isDetailOpen && (
            <div className="max-h-[45vh] overflow-y-auto px-3 py-3">
              <div className="space-y-3">
                {!canEditSource && (
                  <div className="rounded-[8px] border border-border bg-secondary/20 px-3 py-2 text-xs text-muted-foreground">
                    当前 Canvas 源为只读，预览和读取保持可用。
                  </div>
                )}

                {detailMode === "bindings" && (
                  <>
                    <section className="space-y-2 rounded-[8px] border border-border bg-secondary/20 p-3">
                      <p className="text-[11px] uppercase tracking-[0.12em] text-muted-foreground">当前绑定状态</p>
                      {snapshot.bindings.length === 0 && (
                        <p className="text-xs text-muted-foreground">当前没有数据绑定。</p>
                      )}
                      {snapshot.bindings.map((binding) => (
                        <div key={`${binding.alias}:${binding.source_uri}`} className="rounded-[8px] border border-border bg-background px-2 py-2 text-xs">
                          <p className="font-medium text-foreground">{binding.alias}</p>
                          <p className="break-all text-muted-foreground">source: {binding.source_uri}</p>
                          <p className="text-muted-foreground">path: {binding.data_path}</p>
                          <p className="text-muted-foreground">resolved: {binding.resolved ? "yes" : "no"}</p>
                        </div>
                      ))}
                    </section>

                    <CanvasRuntimeBindingsEditor
                      value={snapshot.bindings}
                      isSaving={isSavingBindings}
                      error={bindingsError}
                      readOnly={!agentRunBridge}
                      onUpsert={handleBindingUpsert}
                    />
                  </>
                )}

                {detailMode === "files" && (
                  <CanvasFilesEditor
                    value={canvas?.files ?? []}
                    entryFile={canvas?.entry_file ?? ""}
                    isSaving={isSavingFiles}
                    error={filesError}
                    readOnly={!canEditSource}
                    onSave={handleFilesSave}
                  />
                )}
              </div>
            </div>
          )}
        </div>
      )}
    </aside>
  );
}

export default CanvasRuntimePanel;
