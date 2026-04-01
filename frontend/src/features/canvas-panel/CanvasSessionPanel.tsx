import { useCallback, useEffect, useMemo, useState } from "react";
import {
  fetchCanvas,
  fetchCanvasRuntimeSnapshot,
  updateCanvas,
} from "../../services/canvas";
import type { Canvas, CanvasDataBinding, CanvasFile, CanvasRuntimeSnapshot } from "../../types";
import { CanvasBindingsEditor } from "./CanvasBindingsEditor";
import { CanvasFilesEditor } from "./CanvasFilesEditor";
import { CanvasRuntimePreview } from "./CanvasRuntimePreview";

export interface CanvasSessionPanelProps {
  canvasId: string | null;
  sessionId: string | null;
  onClose: () => void;
}

type DetailTab = "bindings" | "files" | "snapshot";

export function CanvasSessionPanel({ canvasId, sessionId, onClose }: CanvasSessionPanelProps) {
  const [canvas, setCanvas] = useState<Canvas | null>(null);
  const [snapshot, setSnapshot] = useState<CanvasRuntimeSnapshot | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [selectedFilePath, setSelectedFilePath] = useState<string | null>(null);
  const [isSavingBindings, setIsSavingBindings] = useState(false);
  const [bindingsError, setBindingsError] = useState<string | null>(null);
  const [isSavingFiles, setIsSavingFiles] = useState(false);
  const [filesError, setFilesError] = useState<string | null>(null);

  const [isDetailOpen, setIsDetailOpen] = useState(false);
  const [activeDetailTab, setActiveDetailTab] = useState<DetailTab>("snapshot");

  const loadCanvasData = useCallback(async () => {
    if (!canvasId) {
      setCanvas(null);
      setSnapshot(null);
      setError(null);
      setBindingsError(null);
      setSelectedFilePath(null);
      return;
    }

    setIsLoading(true);
    setError(null);
    try {
      const [nextCanvas, nextSnapshot] = await Promise.all([
        fetchCanvas(canvasId),
        fetchCanvasRuntimeSnapshot(canvasId, sessionId),
      ]);
      setCanvas(nextCanvas);
      setSnapshot(nextSnapshot);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Canvas 加载失败");
      setCanvas(null);
      setSnapshot(null);
      setBindingsError(null);
      setFilesError(null);
    } finally {
      setIsLoading(false);
    }
  }, [canvasId, sessionId]);

  useEffect(() => {
    void loadCanvasData();
  }, [loadCanvasData]);

  useEffect(() => {
    const nextFilePath = snapshot?.entry ?? snapshot?.files[0]?.path ?? null;
    setSelectedFilePath((current) => {
      if (!snapshot) {
        return null;
      }
      if (current && snapshot.files.some((file) => file.path === current)) {
        return current;
      }
      return nextFilePath;
    });
  }, [snapshot]);

  const selectedFile = useMemo(() => {
    if (!snapshot || !selectedFilePath) {
      return null;
    }
    return snapshot.files.find((file) => file.path === selectedFilePath) ?? null;
  }, [selectedFilePath, snapshot]);

  const handleBindingsSave = useCallback(async (bindings: CanvasDataBinding[]) => {
    if (!canvasId) {
      return;
    }

    setIsSavingBindings(true);
    setBindingsError(null);
    try {
      const nextCanvas = await updateCanvas(canvasId, { bindings });
      setCanvas(nextCanvas);
      const nextSnapshot = await fetchCanvasRuntimeSnapshot(canvasId, sessionId);
      setSnapshot(nextSnapshot);
      setFilesError(null);
    } catch (err) {
      setBindingsError(err instanceof Error ? err.message : "保存绑定失败");
      throw err;
    } finally {
      setIsSavingBindings(false);
    }
  }, [canvasId, sessionId]);

  const handleFilesSave = useCallback(async (input: {
    entryFile: string;
    files: CanvasFile[];
  }) => {
    if (!canvasId) {
      return;
    }

    setIsSavingFiles(true);
    setFilesError(null);
    try {
      const nextCanvas = await updateCanvas(canvasId, {
        entry_file: input.entryFile,
        files: input.files,
      });
      setCanvas(nextCanvas);
      const nextSnapshot = await fetchCanvasRuntimeSnapshot(canvasId, sessionId);
      setSnapshot(nextSnapshot);
    } catch (err) {
      setFilesError(err instanceof Error ? err.message : "保存文件失败");
      throw err;
    } finally {
      setIsSavingFiles(false);
    }
  }, [canvasId, sessionId]);

  if (!canvasId) {
    return null;
  }

  return (
    <aside className="flex h-full w-full flex-col overflow-hidden bg-background">
      {/* ── 顶部：标题 + 操作 ── */}
      <header className="flex items-center justify-between border-b border-border px-4 py-2.5">
        <div className="flex min-w-0 items-center gap-3">
          <div className="min-w-0">
            <h3 className="truncate text-sm font-semibold text-foreground">
              {canvas?.title || canvasId}
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
          <div className="m-4 rounded-[10px] border border-destructive/30 bg-destructive/10 px-3 py-2 text-xs text-destructive">
            {error}
          </div>
        )}

        {!isLoading && !error && snapshot && (
          <CanvasRuntimePreview snapshot={snapshot} />
        )}

        {!isLoading && !error && !snapshot && (
          <div className="flex flex-1 items-center justify-center px-4 text-sm text-muted-foreground">
            当前还没有可展示的 Canvas 运行时快照。
          </div>
        )}
      </div>

      {/* ── 底部：可折叠的详情面板 ── */}
      {!isLoading && !error && snapshot && (
        <div className="shrink-0 border-t border-border">
          {/* 底部 tab 栏 */}
          <div className="flex items-center justify-between bg-secondary/20 px-3 py-1.5">
            <div className="flex items-center gap-1">
              {(["snapshot", "bindings", "files"] as const).map((tab) => (
                <button
                  key={tab}
                  type="button"
                  onClick={() => {
                    if (activeDetailTab === tab && isDetailOpen) {
                      setIsDetailOpen(false);
                    } else {
                      setActiveDetailTab(tab);
                      setIsDetailOpen(true);
                    }
                  }}
                  className={[
                    "rounded-[6px] px-2 py-1 text-[11px] font-medium transition-colors",
                    activeDetailTab === tab && isDetailOpen
                      ? "bg-foreground text-background"
                      : "text-muted-foreground hover:bg-secondary hover:text-foreground",
                  ].join(" ")}
                >
                  {tab === "snapshot" ? "快照文件" : tab === "bindings" ? "数据绑定" : "资产编辑"}
                </button>
              ))}
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

          {/* 折叠内容 */}
          {isDetailOpen && (
            <div className="max-h-[45vh] overflow-y-auto px-3 py-3">
              {activeDetailTab === "snapshot" && (
                <SnapshotFileViewer
                  snapshot={snapshot}
                  selectedFilePath={selectedFilePath}
                  selectedFile={selectedFile}
                  onSelectFile={setSelectedFilePath}
                />
              )}

              {activeDetailTab === "bindings" && (
                <div className="space-y-3">
                  <section className="space-y-2 rounded-[10px] border border-border bg-secondary/20 p-3">
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

                  <CanvasBindingsEditor
                    value={canvas?.bindings ?? []}
                    isSaving={isSavingBindings}
                    error={bindingsError}
                    onSave={handleBindingsSave}
                  />
                </div>
              )}

              {activeDetailTab === "files" && (
                <CanvasFilesEditor
                  value={canvas?.files ?? []}
                  entryFile={canvas?.entry_file ?? ""}
                  isSaving={isSavingFiles}
                  error={filesError}
                  onSave={handleFilesSave}
                />
              )}
            </div>
          )}
        </div>
      )}
    </aside>
  );
}

function SnapshotFileViewer({
  snapshot,
  selectedFilePath,
  selectedFile,
  onSelectFile,
}: {
  snapshot: CanvasRuntimeSnapshot;
  selectedFilePath: string | null;
  selectedFile: { path: string; file_type: string; content: string } | null;
  onSelectFile: (path: string) => void;
}) {
  if (snapshot.files.length === 0) {
    return (
      <p className="text-xs text-muted-foreground">当前没有可运行文件。</p>
    );
  }

  return (
    <div className="space-y-3">
      <div className="flex flex-wrap gap-1.5">
        {snapshot.files.map((file) => {
          const isSelected = file.path === selectedFilePath;
          return (
            <button
              key={file.path}
              type="button"
              onClick={() => onSelectFile(file.path)}
              className={[
                "rounded-[6px] border px-2 py-0.5 text-[11px] transition-colors",
                isSelected
                  ? "border-foreground bg-foreground text-background"
                  : "border-border bg-background text-muted-foreground hover:bg-secondary hover:text-foreground",
              ].join(" ")}
            >
              {file.path}
            </button>
          );
        })}
      </div>

      {selectedFile && (
        <div className="space-y-2 rounded-[10px] border border-border bg-background p-3">
          <div className="flex items-center justify-between gap-3">
            <p className="min-w-0 truncate text-xs font-medium text-foreground">
              {selectedFile.path}
            </p>
            <span className="shrink-0 text-[11px] text-muted-foreground">
              {selectedFile.file_type}
            </span>
          </div>
          <pre className="max-h-[200px] overflow-auto rounded-[8px] border border-border bg-slate-950 px-3 py-2 text-[11px] leading-5 text-slate-100">
            <code>{selectedFile.content}</code>
          </pre>
        </div>
      )}
    </div>
  );
}

export default CanvasSessionPanel;
